//! Resume and idempotency: the behaviour that keeps real money correct.
//!
//! Driven by an in-memory backend so a submission can be crashed, replayed, and
//! inspected without touching the county's database.

use std::collections::HashMap;
use std::sync::Mutex;

use svdp::domain::policy::ConferenceConfig;
use svdp::domain::policy::Slot;
use svdp::domain::session::*;
use svdp::domain::submit::DeliveryBackend;
use svdp::domain::submit::submit;
use svdp::servware::error::ServWareError;
use svdp::servware::write::WriteOutcome;

/// Records every write, and can be told to fail a particular slot.
#[derive(Default)]
struct FakeServWare {
    items: Mutex<HashMap<(u64, &'static str), u32>>,
    completed: Mutex<Vec<u64>>,
    /// (request_id, slot) -> how the write should behave.
    behaviour: Mutex<HashMap<(u64, &'static str), Behaviour>>,
    pub calls: Mutex<Vec<String>>,
}

#[derive(Clone)]
enum Behaviour {
    Fail,
    Conflict,
}

impl FakeServWare {
    fn fail(&self, request: u64, slot: Slot) {
        self.behaviour
            .lock().unwrap()
            .insert((request, slot.as_str()), Behaviour::Fail);
    }
    fn conflict(&self, request: u64, slot: Slot) {
        self.behaviour
            .lock().unwrap()
            .insert((request, slot.as_str()), Behaviour::Conflict);
    }
    fn clear_behaviour(&self) {
        self.behaviour.lock().unwrap().clear();
    }
    fn item_count(&self) -> usize {
        self.items.lock().unwrap().len()
    }
    fn dollars_logged(&self) -> u32 {
        self.items.lock().unwrap().values().sum()
    }
}

impl DeliveryBackend for FakeServWare {
    async fn ensure_item(
        &self,
        request_id: u64,
        _client_id: u64,
        _session_id: &str,
        slot: Slot,
        dollars: u32,
        _date: &str,
    ) -> Result<WriteOutcome, ServWareError> {
        self.calls
            .lock().unwrap()
            .push(format!("item {request_id} {}", slot.as_str()));
        match self.behaviour.lock().unwrap().get(&(request_id, slot.as_str())) {
            Some(Behaviour::Fail) => {
                return Err(ServWareError::WriteRejected {
                    request_id,
                    what: slot.as_str().into(),
                });
            }
            Some(Behaviour::Conflict) => {
                return Ok(WriteOutcome::Conflict("already logged today".into()));
            }
            None => {}
        }
        // The real backend reads before writing; the fake mirrors that.
        let key = (request_id, slot.as_str());
        if self.items.lock().unwrap().contains_key(&key) {
            return Ok(WriteOutcome::AlreadyDone);
        }
        self.items.lock().unwrap().insert(key, dollars);
        Ok(WriteOutcome::Applied)
    }

    async fn ensure_complete(
        &self,
        request_id: u64,
        _volunteer_id: &str,
        _date: &str,
        _expected_version: Option<u64>,
    ) -> Result<WriteOutcome, ServWareError> {
        self.calls.lock().unwrap().push(format!("complete {request_id}"));
        match self.behaviour.lock().unwrap().get(&(request_id, "complete")) {
            Some(Behaviour::Fail) => {
                return Err(ServWareError::WriteRejected {
                    request_id,
                    what: "completion".into(),
                });
            }
            Some(Behaviour::Conflict) => {
                return Ok(WriteOutcome::Conflict("closed by someone else".into()));
            }
            None => {}
        }
        if self.completed.lock().unwrap().contains(&request_id) {
            return Ok(WriteOutcome::AlreadyDone);
        }
        self.completed.lock().unwrap().push(request_id);
        Ok(WriteOutcome::Applied)
    }
}

fn delivery(request_id: u64, name: &str, household: u32, gift: u32) -> Delivery {
    Delivery {
        request_id,
        client_id: request_id * 10,
        name: name.into(),
        household_size: household,
        gift_card_dollars: gift,
        food_dollars: 70,
        version: Some(1),
        outcome: DeliveryOutcome::Delivered,
        food: SlotState::Pending,
        gift_card: SlotState::Pending,
        complete: SlotState::Pending,
    }
}

fn confirmed_session() -> DeliverySession {
    let mut s = DeliverySession::new("09/05/2026", "2026-09-05T18:00:00");
    s.groups = vec![
        Group {
            volunteer_id: "44270".into(),
            volunteer_name: "Ada Lovelace".into(),
            deliveries: vec![delivery(101, "Alvarez", 4, 80), delivery(102, "Baptiste", 2, 60)],
        },
        Group {
            volunteer_id: "44271".into(),
            volunteer_name: "Grace Hopper".into(),
            deliveries: vec![delivery(103, "Okafor", 6, 100)],
        },
    ];
    s.state = SessionState::Confirmed;
    s
}

const NOW: &str = "2026-09-05T19:00:00";

#[tokio::test]
async fn happy_path_writes_every_slot_once() {
    let fake = FakeServWare::default();
    let mut session = confirmed_session();
    let report = submit(&fake, &mut session, &ConferenceConfig::default(), NOW).await.unwrap();

    assert_eq!(report.applied, 9, "3 deliveries x 3 slots");
    assert_eq!(report.failed, 0);
    assert!(report.is_clean());
    assert_eq!(session.state, SessionState::Submitted);
    assert_eq!(fake.dollars_logged(), 70 * 3 + 80 + 60 + 100);
}

/// The defect that motivated the rewrite: re-running must not double-log money.
///
/// Two independent guards, both pinned here. The outer one refuses to resubmit a
/// finished session at all. The inner one is that every write re-reads ServWare
/// first, so even a forced re-run finds its own work and skips it.
#[tokio::test]
async fn resubmitting_writes_nothing_and_double_logs_no_money() {
    let fake = FakeServWare::default();
    let config = ConferenceConfig::default();
    let mut session = confirmed_session();

    submit(&fake, &mut session, &config, NOW).await.unwrap();
    let dollars_after_first = fake.dollars_logged();
    let items_after_first = fake.item_count();
    assert_eq!(session.state, SessionState::Submitted);

    // Outer guard: a finished session is refused.
    let refused = submit(&fake, &mut session, &config, NOW).await.unwrap_err();
    assert!(refused.contains("not been confirmed"), "{refused}");
    assert_eq!(fake.dollars_logged(), dollars_after_first);

    // Inner guard: force it past the state check and it still writes nothing,
    // because each slot re-reads ServWare before writing.
    session.state = SessionState::NeedsAttention;
    let forced = submit(&fake, &mut session, &config, NOW).await.unwrap();
    assert_eq!(forced.applied, 0, "no new writes even when forced");
    assert_eq!(fake.dollars_logged(), dollars_after_first, "no extra money logged");
    assert_eq!(fake.item_count(), items_after_first);
}

/// Belt and braces: if the session record were lost entirely, the backend's own
/// read-before-write still prevents a second charge.
#[tokio::test]
async fn a_lost_session_record_still_cannot_double_charge() {
    let fake = FakeServWare::default();
    let config = ConferenceConfig::default();

    let mut first = confirmed_session();
    submit(&fake, &mut first, &config, NOW).await.unwrap();
    let dollars = fake.dollars_logged();

    // Same plan, all slots Pending again, as if the receipt file vanished.
    let mut replayed = confirmed_session();
    let report = submit(&fake, &mut replayed, &config, NOW).await.unwrap();

    assert_eq!(report.applied, 0, "ServWare is the source of truth, not the receipt");
    assert_eq!(report.skipped, 9);
    assert_eq!(fake.dollars_logged(), dollars, "not one dollar more");
}

/// A crash after the food write must resume by writing only what is missing.
#[tokio::test]
async fn resume_writes_only_the_missing_slots() {
    let fake = FakeServWare::default();
    let config = ConferenceConfig::default();
    let mut session = confirmed_session();

    // First pass: gift card fails for one family.
    fake.fail(102, Slot::GiftCard);
    let first = submit(&fake, &mut session, &config, NOW).await.unwrap();
    assert_eq!(first.failed, 1);
    assert_eq!(session.state, SessionState::NeedsAttention);

    let stalled = session.deliveries().find(|d| d.request_id == 102).unwrap();
    assert!(matches!(stalled.food, SlotState::Succeeded { .. }), "food went through");
    assert!(matches!(stalled.gift_card, SlotState::Failed { .. }));
    assert_eq!(
        stalled.complete, SlotState::Pending,
        "completion must not run while the books are short"
    );

    // Second pass, fault cleared.
    fake.clear_behaviour();
    fake.calls.lock().unwrap().clear();
    let second = submit(&fake, &mut session, &config, NOW).await.unwrap();

    let calls = fake.calls.lock().unwrap().clone();
    assert!(
        !calls.iter().any(|c| c == "item 102 food"),
        "food was already done and must not be rewritten; calls: {calls:?}"
    );
    assert!(calls.contains(&"item 102 giftcard".to_string()));
    assert!(calls.contains(&"complete 102".to_string()));
    assert!(second.is_clean());
    assert_eq!(session.state, SessionState::Submitted);
    assert_eq!(fake.dollars_logged(), 70 * 3 + 80 + 60 + 100, "still exactly right");
}

/// One bad row must not strand the rows after it -- the old `?`-in-loop bug.
#[tokio::test]
async fn a_failure_does_not_abort_the_batch() {
    let fake = FakeServWare::default();
    let mut session = confirmed_session();
    fake.fail(101, Slot::Food);

    let report = submit(&fake, &mut session, &ConferenceConfig::default(), NOW).await.unwrap();

    assert_eq!(report.failed, 1);
    let later = session.deliveries().find(|d| d.request_id == 103).unwrap();
    assert!(
        matches!(later.complete, SlotState::Succeeded { .. }),
        "later deliveries must still be processed"
    );
    assert_eq!(report.attention.len(), 1);
    assert!(report.attention[0].contains("Alvarez"), "names the family: {:?}", report.attention);
}

/// A conflict is a human decision, never an automatic retry.
#[tokio::test]
async fn conflicts_are_recorded_for_a_human_and_not_retried() {
    let fake = FakeServWare::default();
    let config = ConferenceConfig::default();
    let mut session = confirmed_session();
    fake.conflict(103, Slot::Complete);

    let report = submit(&fake, &mut session, &config, NOW).await.unwrap();
    assert_eq!(report.conflicts, 1);
    assert_eq!(session.state, SessionState::NeedsAttention);

    let d = session.deliveries().find(|d| d.request_id == 103).unwrap();
    assert!(matches!(d.complete, SlotState::Conflict { .. }));
    assert!(report.attention.iter().any(|a| a.contains("Okafor")));
}

/// Submission is gated on human confirmation -- the only safety mechanism,
/// because ServWare exposes no way to delete a mistaken assistance item.
#[tokio::test]
async fn an_unconfirmed_plan_is_refused() {
    let fake = FakeServWare::default();
    let mut session = confirmed_session();
    session.state = SessionState::Draft;

    let err = submit(&fake, &mut session, &ConferenceConfig::default(), NOW).await.unwrap_err();
    assert!(err.contains("not been confirmed"), "{err}");
    assert_eq!(fake.item_count(), 0, "nothing may be written");
}

/// A plan that has already sent money cannot be quietly re-planned.
#[tokio::test]
async fn a_partly_submitted_plan_cannot_be_edited() {
    let fake = FakeServWare::default();
    let mut session = confirmed_session();
    fake.fail(102, Slot::GiftCard);
    submit(&fake, &mut session, &ConferenceConfig::default(), NOW).await.unwrap();

    assert!(session.has_written());
    let err = session.touch_for_edit().unwrap_err();
    assert!(err.contains("already been partly submitted"), "{err}");
}

#[test]
fn pending_slots_are_ordered_assistance_before_completion() {
    let d = delivery(1, "X", 3, 70);
    assert_eq!(d.pending_slots(), vec![Slot::Food, Slot::GiftCard, Slot::Complete]);
}
