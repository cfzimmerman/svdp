//! Conference policy: what SVdP at Nativity gives, and how much.
//!
//! These were `const`s in `nativity.rs`. `api.md` notes that assistance type IDs
//! are **conference-specific**, so they are configuration, not constants -- and
//! they are validated against ServWare at start-up rather than trusted, because
//! a renumbered type would otherwise log $70 of "Second Harvest Food" against
//! whatever category that ID now means.

use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConferenceConfig {
    pub second_harvest: AssistanceType,
    pub gift_card: AssistanceType,
    /// Gift card dollars by household size, index 0 == household of 0 or 1.
    /// The last entry applies to every larger household.
    pub gift_card_ladder: Vec<u32>,
    pub visit_mileage: String,
    pub visit_notes_html: String,
    /// Write a machine tag into each assistance item's notes so a retry can
    /// recognise its own work. See DECISIONS.md D7.
    pub tag_assistance_notes: bool,
    /// A household delivered to within this many days is held back from the
    /// working list by default. See DECISIONS.md D29.
    #[serde(default = "default_delivery_interval_days")]
    pub delivery_interval_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistanceType {
    pub id: String,
    /// Expected name, asserted against ServWare's own label at start-up.
    pub name_contains: String,
    /// Fixed dollar value, where the conference gives a standard amount.
    pub value: Option<u32>,
}

/// The conference config, compiled in.
///
/// Embedding it means a shipped binary always has a valid, reviewed policy even
/// though an MCP server's working directory is undefined -- reading a file
/// relative to the process would silently find nothing.
const EMBEDDED: &str = include_str!("../../conference.toml");

impl Default for ConferenceConfig {
    fn default() -> Self {
        toml::from_str(EMBEDDED).expect("embedded conference.toml must parse; covered by a test")
    }
}

impl ConferenceConfig {
    /// The embedded policy, unless an external file overrides it.
    ///
    /// Override order: `SVDP_CONFERENCE_CONFIG`, then `conference.toml` in the
    /// working directory. A malformed override is ignored with a warning rather
    /// than taken as policy -- a half-parsed config could mean wrong money.
    pub fn load() -> Self {
        let candidates = std::env::var("SVDP_CONFERENCE_CONFIG")
            .into_iter()
            .chain(["conference.toml".to_string()]);
        for path in candidates {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            match toml::from_str::<Self>(&text) {
                Ok(cfg) => {
                    tracing::info!(path, "using external conference config");
                    return cfg;
                }
                Err(e) => tracing::warn!(path, %e, "ignoring malformed conference config"),
            }
        }
        Self::default()
    }

    /// Gift card dollars for a household of `size`.
    ///
    /// Monotonic and clamped: the ladder's first entry is the floor ($50) and
    /// its last is the ceiling ($100).
    pub fn gift_card_dollars(&self, size: u32) -> u32 {
        debug_assert!(!self.gift_card_ladder.is_empty(), "ladder must not be empty");
        let index = size.saturating_sub(1) as usize;
        *self
            .gift_card_ladder
            .get(index)
            .or_else(|| self.gift_card_ladder.last())
            .unwrap_or(&0)
    }

    /// The machine tag for one write slot of one session.
    pub fn tag(&self, session: &str, slot: Slot) -> String {
        format!("svdp:s={session};i={}", slot.as_str())
    }

    /// Notes text for an assistance item: machine tag first, then something a
    /// caseworker reading ServWare can understand.
    pub fn item_notes(&self, session: &str, slot: Slot, date: &str) -> String {
        if !self.tag_assistance_notes {
            return String::new();
        }
        format!("{} — SVdP delivery {date}", self.tag(session, slot))
    }
}

/// Kept as a function rather than a literal in the struct so an older external
/// `conference.toml` without the field still loads.
fn default_delivery_interval_days() -> u32 {
    28
}

/// Whether a household delivered to on `last` is still inside the once-a-month
/// interval, and so should be held back from the working list.
impl ConferenceConfig {
    pub fn served_recently(&self, last: chrono::NaiveDate, today: chrono::NaiveDate) -> bool {
        (today - last).num_days() < i64::from(self.delivery_interval_days)
    }
}

/// The three independent writes made per delivery. Each is tracked separately so
/// a resumed submission redoes only what is missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Slot {
    Food,
    GiftCard,
    Complete,
}

impl Slot {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Food => "food",
            Self::GiftCard => "giftcard",
            Self::Complete => "complete",
        }
    }
}
