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
    /// Shown on the approval screen, because it is the reason the gift card is
    /// the amount it is. Serde ignores fields it does not know, so removing one
    /// (as `version` and `outcome` were removed) still loads an older receipt.
    pub household_size: u32,
    pub gift_card_dollars: u32,
    pub food_dollars: u32,
    pub food: SlotState,
    pub gift_card: SlotState,
    pub complete: SlotState,
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
    /// Needs a human decision. A later `submit_session` will re-check ServWare
    /// and either resolve it or report it again; nothing is written blindly.
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
    ///
    /// Saturating, not `+`. Release builds have overflow checks off, so plain
    /// addition could show a volunteer an approval total that is not the sum of
    /// the lines above it. A saturated total is visibly absurd; a wrapped one
    /// looks plausible.
    pub fn total_dollars(&self) -> u32 {
        self.food_dollars.saturating_add(self.gift_card_dollars)
    }

    /// Slots still to write, in the order they must be written.
    ///
    /// Assistance first, completion last: a completed request leaves the Open
    /// list, so completion is the commit marker. See DECISIONS.md D7.
    ///
    /// A `Conflict` slot is included: it is not "done", and re-running after a
    /// human has looked at ServWare is exactly how a conflict gets resolved.
    /// Each write re-reads before acting, so a retry that is still conflicted
    /// simply reports the conflict again.
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

    /// A session still holding money that has not reached ServWare.
    pub fn is_open(&self) -> bool {
        !matches!(self.state, SessionState::Submitted | SessionState::Abandoned)
    }

    /// Whether anything is **known to have reached ServWare**. Once true the
    /// session must not be re-planned or discarded: the audit trail is the only
    /// record of what was sent.
    ///
    /// Counts only slots that are done. It used to count `Failed` too, which
    /// meant a session where every single write failed -- Wi-Fi dropped before
    /// the first POST, so nothing reached ServWare at all -- could be neither
    /// re-planned ("finish or abandon it first") nor abandoned ("part of this is
    /// already saved in ServWare"), each refusal pointing at the other. The only
    /// exit was a successful submit, so a persistent failure wedged the
    /// extension for every future delivery night, for an audience with no
    /// command line. See DECISIONS.md D38.
    pub fn has_written(&self) -> bool {
        self.deliveries().any(|d| {
            [Slot::Food, Slot::GiftCard, Slot::Complete]
                .iter()
                .any(|s| d.slot(*s).is_done())
        })
    }

    /// Whether any write was *attempted*, successfully or not. Drives the state
    /// rollup and what a volunteer is told, never whether a session is locked.
    pub fn has_attempted(&self) -> bool {
        self.deliveries().any(|d| {
            [Slot::Food, Slot::GiftCard, Slot::Complete]
                .iter()
                .any(|s| !matches!(d.slot(*s), SlotState::Pending))
        })
    }

    /// Total dollars planned, saturating rather than wrapping. See
    /// [`Delivery::total_dollars`].
    pub fn total_dollars(&self) -> u32 {
        self.deliveries()
            .map(Delivery::total_dollars)
            .fold(0u32, u32::saturating_add)
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
        } else if self.has_attempted() {
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
