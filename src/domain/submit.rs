//! The submit engine: idempotent, resumable, and never silently lossy.

use crate::domain::policy::ConferenceConfig;
use crate::domain::policy::Slot;
use crate::domain::session::DeliverySession;
use crate::domain::session::SessionState;
use crate::domain::session::SlotState;
use crate::servware::error::ServWareError;
use crate::servware::write::WriteOutcome;

/// The writes the engine needs, abstracted so the resume and idempotency rules
/// can be tested against an in-memory fake rather than a live county database.
pub trait DeliveryBackend {
    /// Log one assistance item, skipping if this session already logged it.
    fn ensure_item(
        &self,
        request_id: u64,
        client_id: u64,
        session_id: &str,
        slot: Slot,
        dollars: u32,
        date: &str,
    ) -> impl std::future::Future<Output = Result<WriteOutcome, ServWareError>> + Send;

    /// Mark the request complete, skipping if it already is.
    fn ensure_complete(
        &self,
        request_id: u64,
        volunteer_id: &str,
        date: &str,
        expected_version: Option<u64>,
    ) -> impl std::future::Future<Output = Result<WriteOutcome, ServWareError>> + Send;
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct SubmitReport {
    pub applied: u32,
    pub skipped: u32,
    pub failed: u32,
    pub conflicts: u32,
    /// One human-readable line per delivery that did not fully succeed.
    pub attention: Vec<String>,
}

impl SubmitReport {
    pub fn is_clean(&self) -> bool {
        self.failed == 0 && self.conflicts == 0
    }
}

/// Submit every pending slot, continuing past failures.
///
/// A failure never aborts the batch: the previous implementation used `?` inside
/// the loop, so one bad row stranded every row after it *and* left no record of
/// which had succeeded. Here each slot records its own outcome, so a re-run
/// writes only what is still missing.
pub async fn submit<B: DeliveryBackend>(
    backend: &B,
    session: &mut DeliverySession,
    config: &ConferenceConfig,
    now: &str,
) -> Result<SubmitReport, String> {
    if !matches!(
        session.state,
        SessionState::Confirmed | SessionState::Submitting | SessionState::NeedsAttention
    ) {
        return Err(format!(
            "This delivery plan has not been confirmed yet (it is {:?}). \
             Show the plan to the volunteer, then confirm it.",
            session.state
        ));
    }

    let date = session.delivery_date.clone();
    let session_id = session.id.clone();
    let mut report = SubmitReport::default();

    for group in &mut session.groups {
        let volunteer = group.volunteer_id.clone();
        for delivery in &mut group.deliveries {
            for slot in delivery.pending_slots() {
                let result = match slot {
                    Slot::Complete => {
                        backend
                            .ensure_complete(
                                delivery.request_id,
                                &volunteer,
                                &date,
                                delivery.version,
                            )
                            .await
                    }
                    Slot::Food => {
                        backend
                            .ensure_item(
                                delivery.request_id,
                                delivery.client_id,
                                &session_id,
                                slot,
                                delivery.food_dollars,
                                &date,
                            )
                            .await
                    }
                    Slot::GiftCard => {
                        backend
                            .ensure_item(
                                delivery.request_id,
                                delivery.client_id,
                                &session_id,
                                slot,
                                delivery.gift_card_dollars,
                                &date,
                            )
                            .await
                    }
                };

                let state = match result {
                    Ok(WriteOutcome::Applied) => {
                        report.applied += 1;
                        SlotState::Succeeded { at: now.to_string() }
                    }
                    Ok(WriteOutcome::AlreadyDone) => {
                        report.skipped += 1;
                        SlotState::Skipped
                    }
                    Ok(WriteOutcome::Conflict(reason)) => {
                        report.conflicts += 1;
                        report.attention.push(format!(
                            "{}: {} needs a decision — {reason}",
                            delivery.name,
                            slot.as_str()
                        ));
                        SlotState::Conflict { reason }
                    }
                    Err(e) => {
                        report.failed += 1;
                        report
                            .attention
                            .push(format!("{}: {}", delivery.name, e.user_message()));
                        SlotState::Failed { error: e.user_message() }
                    }
                };

                let stop = matches!(state, SlotState::Failed { .. } | SlotState::Conflict { .. });
                delivery.set_slot(slot, state);

                // Completion is the commit marker. If an assistance write failed,
                // do not mark the request complete -- it would leave the books
                // short while the request looks finished, and completion removes
                // it from the working list.
                if stop {
                    break;
                }
            }
        }
    }

    let _ = config;
    session.recompute_state();
    Ok(report)
}
