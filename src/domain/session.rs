//! The delivery session: what was delivered, to whom, credited to which
//! volunteer, and how much of it has reached ServWare.
//!
//! The session-level state is a rollup. **The real state is per-slot**: each
//! delivery has three independent writes (food, gift card, completion), so a run
//! that dies after logging food resumes by writing only the gift card and the
//! completion. See DECISIONS.md D6.

use serde::Deserialize;
use serde::Serialize;

use crate::domain::policy::Slot;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliverySession {
    pub id: String,
    pub created_at: String,
    /// Delivery date as ServWare formats it, `MM/DD/YYYY`.
    pub delivery_date: String,
    pub state: SessionState,
    /// Bumped on every plan edit; callers pass it back to catch stale writes.
    pub revision: u32,
    pub groups: Vec<Group>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// Freely editable. No money has been written.
    Draft,
    /// A human approved the plan; contents are frozen.
    Confirmed,
    /// At least one write has been attempted.
    Submitting,
    /// Every slot reached a good terminal state.
    Submitted,
    /// A pass ended with at least one failure or conflict. Resumable.
    NeedsAttention,
    Abandoned,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub volunteer_id: String,
    pub volunteer_name: String,
    pub deliveries: Vec<Delivery>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delivery {
    pub request_id: u64,
    pub client_id: u64,
    pub name: String,
    pub household_size: u32,
    pub gift_card_dollars: u32,
    pub food_dollars: u32,
    /// ServWare's optimistic-lock counter at plan time, re-checked at submit.
    pub version: Option<u64>,
    /// Present from the start even though the first workflow only ever sets
    /// `Delivered`; the call-sheet workflow needs the others and a schema
    /// migration later would be worse.
    pub outcome: DeliveryOutcome,
    pub food: SlotState,
    pub gift_card: SlotState,
    pub complete: SlotState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryOutcome {
    Planned,
    Delivered,
    NotReached,
    Declined,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum SlotState {
    Pending,
    /// Written and confirmed by read-back.
    Succeeded { at: String },
    /// Already present in ServWare; nothing was sent.
    Skipped,
    Failed { error: String },
    /// Needs a human decision; never retried automatically.
    Conflict { reason: String },
}

impl SlotState {
    /// Whether this slot is finished and needs no further write.
    pub fn is_done(&self) -> bool {
        matches!(self, Self::Succeeded { .. } | Self::Skipped)
    }

    pub fn needs_attention(&self) -> bool {
        matches!(self, Self::Failed { .. } | Self::Conflict { .. })
    }
}

impl Delivery {
    pub fn slot(&self, slot: Slot) -> &SlotState {
        match slot {
            Slot::Food => &self.food,
            Slot::GiftCard => &self.gift_card,
            Slot::Complete => &self.complete,
        }
    }

    pub fn set_slot(&mut self, slot: Slot, state: SlotState) {
        match slot {
            Slot::Food => self.food = state,
            Slot::GiftCard => self.gift_card = state,
            Slot::Complete => self.complete = state,
        }
    }

    /// Total dollars this delivery records against the county's books.
    pub fn total_dollars(&self) -> u32 {
        self.food_dollars + self.gift_card_dollars
    }

    /// Slots still to write, in the order they must be written.
    ///
    /// Assistance first, completion last: a completed request leaves the Open
    /// list, so completion is the commit marker. See DECISIONS.md D7.
    pub fn pending_slots(&self) -> Vec<Slot> {
        [Slot::Food, Slot::GiftCard, Slot::Complete]
            .into_iter()
            .filter(|s| !self.slot(*s).is_done())
            .collect()
    }
}

/// A short, sortable, human-inspectable session id.
///
/// Deliberately not a ULID or UUID: this id is written into ServWare's notes
/// field where a caseworker may read it, one conference creates two of these a
/// week, and a millisecond timestamp plus nanosecond entropy is unique well
/// past any plausible collision. One less dependency to age.
fn new_session_id() -> String {
    fn base36(mut n: u128) -> String {
        const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
        if n == 0 {
            return "0".into();
        }
        let mut out = Vec::new();
        while n > 0 {
            out.push(ALPHABET[(n % 36) as usize]);
            n /= 36;
        }
        out.reverse();
        String::from_utf8(out).expect("base36 alphabet is ascii")
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}{}", base36(now.as_millis()), base36(now.subsec_nanos() as u128 % 1296))
}

impl DeliverySession {
    pub fn new(delivery_date: impl Into<String>, now: impl Into<String>) -> Self {
        Self {
            id: new_session_id(),
            created_at: now.into(),
            delivery_date: delivery_date.into(),
            state: SessionState::Draft,
            revision: 1,
            groups: Vec::new(),
        }
    }

    pub fn deliveries(&self) -> impl Iterator<Item = &Delivery> {
        self.groups.iter().flat_map(|g| g.deliveries.iter())
    }

    pub fn deliveries_mut(&mut self) -> impl Iterator<Item = &mut Delivery> {
        self.groups.iter_mut().flat_map(|g| g.deliveries.iter_mut())
    }

    /// A session still holding money that has not reached ServWare.
    pub fn is_open(&self) -> bool {
        !matches!(self.state, SessionState::Submitted | SessionState::Abandoned)
    }

    /// Whether any write has been attempted. Once true, the session must not be
    /// silently abandoned -- the audit trail is the only record of what was sent.
    pub fn has_written(&self) -> bool {
        self.deliveries()
            .any(|d| [Slot::Food, Slot::GiftCard, Slot::Complete]
                .iter()
                .any(|s| !matches!(d.slot(*s), SlotState::Pending)))
    }

    pub fn total_dollars(&self) -> u32 {
        self.deliveries().map(Delivery::total_dollars).sum()
    }

    /// Recompute the rollup from the per-slot states.
    pub fn recompute_state(&mut self) {
        if matches!(self.state, SessionState::Abandoned) {
            return;
        }
        let mut any_attention = false;
        let mut all_done = true;
        for d in self.deliveries() {
            for slot in [Slot::Food, Slot::GiftCard, Slot::Complete] {
                let s = d.slot(slot);
                any_attention |= s.needs_attention();
                all_done &= s.is_done();
            }
        }
        self.state = if all_done && self.has_written() {
            SessionState::Submitted
        } else if any_attention {
            SessionState::NeedsAttention
        } else if self.has_written() {
            SessionState::Submitting
        } else {
            self.state
        };
    }

    /// Editing a confirmed-but-unwritten plan reopens it, so approval always
    /// refers to the bytes that will actually be submitted.
    pub fn touch_for_edit(&mut self) -> Result<(), String> {
        if self.has_written() {
            return Err("this delivery has already been partly submitted to ServWare; \
                        it cannot be re-planned"
                .into());
        }
        self.state = SessionState::Draft;
        self.revision += 1;
        Ok(())
    }
}
