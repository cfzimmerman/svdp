//! Writes to ServWare, verified by read-back.
//!
//! **HTTP status is not evidence of success.** Spring re-renders a rejected form
//! as 200, and the client follows redirects, so the previous implementation
//! reported "logged $70" for writes ServWare had refused. Every write here is
//! confirmed by re-reading the request. See DECISIONS.md D9.

use crate::domain::policy::ConferenceConfig;
use crate::domain::policy::Slot;
use crate::servware::client::ServWareClient;
use crate::servware::detail;
use crate::servware::detail::RequestDetail;
use crate::servware::error::Result;
use crate::servware::error::ServWareError;

/// What actually happened to one write slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteOutcome {
    /// The write was made and confirmed by re-reading the request.
    Applied,
    /// This exact write was already present. Nothing was sent.
    AlreadyDone,
    /// Someone changed the request in ServWare; a human must decide.
    Conflict(String),
}

/// Mark a request complete and credit the visit to a volunteer.
///
/// Skips rather than overwrites when the request is already completed: the
/// county may have closed it for an unrelated reason, and blindly rewriting
/// would clobber their assignment and notes.
pub async fn mark_complete(
    client: &ServWareClient,
    config: &ConferenceConfig,
    request_id: u64,
    volunteer_id: &str,
    visit_date: &str,
    expected_version: Option<u64>,
) -> Result<WriteOutcome> {
    let before = detail::fetch(client, request_id).await?;

    if before.is_completed() {
        return Ok(WriteOutcome::AlreadyDone);
    }
    if let Some(expected) = expected_version {
        if let Some(actual) = before.form.get("version").and_then(|v| v.parse::<u64>().ok()) {
            if actual != expected {
                return Ok(WriteOutcome::Conflict(format!(
                    "it was edited in ServWare after you planned this delivery"
                )));
            }
        }
    }

    let after = before.form.overlay([
        ("status", "Completed".to_string()),
        ("requestAssignedToMemberId", volunteer_id.to_string()),
        ("visitAssignedToMemberId", volunteer_id.to_string()),
        ("homeVisitRequired", "true".to_string()),
        ("homeVisitCnt", "1".to_string()),
        ("visitCompleted", "true".to_string()),
        ("visitMileageInService", config.visit_mileage.clone()),
        ("visitScheduledDate", visit_date.to_string()),
        ("visitNotes", config.visit_notes_html.clone()),
    ])?;

    // Prove we are changing only what we named. Clobbering becomes impossible by
    // construction rather than by care.
    let changed: Vec<String> = before.form.diff(&after).into_iter().map(|(n, _, _)| n).collect();
    debug_assert!(
        changed.len() <= 9,
        "mark_complete touched unexpected fields: {changed:?}"
    );

    client.post_form(&detail::detail_path(request_id), &after.pairs()).await?;

    // Read-back verification: the only trustworthy signal.
    let confirmed = detail::fetch(client, request_id).await?;
    if !confirmed.is_completed() {
        return Err(ServWareError::WriteRejected {
            request_id,
            what: "completion".into(),
        });
    }
    Ok(WriteOutcome::Applied)
}

/// Add one assistance item, skipping if this tool already recorded it.
///
/// The form mirrors the captured browser POST field-for-field and in order --
/// 30 fields, not the 13 that `api.md` lists. See DECISIONS.md D9.
#[allow(clippy::too_many_arguments)]
pub async fn add_assistance_item(
    client: &ServWareClient,
    config: &ConferenceConfig,
    request_id: u64,
    client_id: u64,
    session: &str,
    slot: Slot,
    dollars: u32,
    date_provided: &str,
) -> Result<WriteOutcome> {
    let type_id = match slot {
        Slot::Food => &config.second_harvest.id,
        Slot::GiftCard => &config.gift_card.id,
        Slot::Complete => {
            return Err(ServWareError::Malformed(
                "completion is not an assistance item".into(),
            ));
        }
    };
    let tag = config.tag(session, slot);
    let before = detail::fetch(client, request_id).await?;

    // ServWare is the source of truth; the local receipt is only an optimization.
    // Re-read before every write, including the first.
    if before.has_item_tagged(&tag) {
        return Ok(WriteOutcome::AlreadyDone);
    }
    if !config.tag_assistance_notes {
        let kind = match slot {
            Slot::Food => &config.second_harvest.name_contains,
            _ => &config.gift_card.name_contains,
        };
        if !before.items_like(kind, date_provided).is_empty() {
            return Ok(WriteOutcome::Conflict(format!(
                "a {kind} item is already logged for {date_provided}"
            )));
        }
    }

    let notes = config.item_notes(session, slot, date_provided);
    let form = assistance_form(
        type_id,
        client_id,
        dollars,
        date_provided,
        &notes,
    );
    let path = format!("/app/assistancerequests/{request_id}/assistanceitems/new");
    client.post_form(&path, &form).await?;

    let confirmed = detail::fetch(client, request_id).await?;
    if config.tag_assistance_notes && !confirmed.has_item_tagged(&tag) {
        return Err(ServWareError::WriteRejected {
            request_id,
            what: format!("${dollars} {} entry", slot.as_str()),
        });
    }
    if !config.tag_assistance_notes
        && confirmed.assistance_items.len() <= before.assistance_items.len()
    {
        return Err(ServWareError::WriteRejected {
            request_id,
            what: format!("${dollars} {} entry", slot.as_str()),
        });
    }
    Ok(WriteOutcome::Applied)
}

/// The assistance-item form, in the exact field order the browser sends.
///
/// Pinned by a golden test; do not reorder or prune. `_pending` and
/// `_checkRequested` are Spring checkbox companions submitted without their
/// controls, meaning "unchecked".
pub fn assistance_form(
    type_id: &str,
    client_id: u64,
    dollars: u32,
    date_provided: &str,
    notes: &str,
) -> Vec<(String, String)> {
    let pair = |k: &str, v: &str| (k.to_string(), v.to_string());
    vec![
        pair("assistanceTypeId", type_id),
        pair("clientId", &client_id.to_string()),
        pair("housingProviderId", ""),
        pair("vendorId", ""),
        pair("utilityId", ""),
        pair("clientAccountId", ""),
        pair("clientAccountName", ""),
        pair("clientAccountNumber", ""),
        pair("clientAccountHolder", ""),
        pair("specialProgramId", ""),
        pair("inKindSubType", ""),
        pair("monetaryValue", &dollars.to_string()),
        pair("accountId", ""),
        pair("quantity", "1"),
        pair("dateProvided", date_provided),
        pair("voucherAsstId", ""),
        pair("_pending", "on"),
        pair("promisedDate", ""),
        pair("_checkRequested", "on"),
        pair("datePaid", ""),
        pair("checkNumber", ""),
        pair("payeeName", ""),
        pair("notes", notes),
        pair("councilPaymentValue", ""),
        pair("councilCheckConfNumber", ""),
        pair("districtPaymentValue", ""),
        pair("districtCheckConfNumber", ""),
        pair("otherPaymentValue", ""),
        pair("otherCheckConfNumber", ""),
        pair("action", "save"),
    ]
}

/// Convenience for callers that already hold the detail page.
pub fn already_recorded(detail: &RequestDetail, config: &ConferenceConfig, session: &str, slot: Slot) -> bool {
    detail.has_item_tagged(&config.tag(session, slot))
}

/// The live backend: the real thing behind `DeliveryBackend`.
pub struct ServWareBackend<'a> {
    pub client: &'a ServWareClient,
    pub config: &'a ConferenceConfig,
}

impl crate::domain::submit::DeliveryBackend for ServWareBackend<'_> {
    async fn ensure_item(
        &self,
        request_id: u64,
        client_id: u64,
        session_id: &str,
        slot: Slot,
        dollars: u32,
        date: &str,
    ) -> Result<WriteOutcome> {
        add_assistance_item(
            self.client, self.config, request_id, client_id, session_id, slot, dollars, date,
        )
        .await
    }

    async fn ensure_complete(
        &self,
        request_id: u64,
        volunteer_id: &str,
        date: &str,
        expected_version: Option<u64>,
    ) -> Result<WriteOutcome> {
        mark_complete(
            self.client, self.config, request_id, volunteer_id, date, expected_version,
        )
        .await
    }
}
