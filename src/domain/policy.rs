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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistanceType {
    pub id: String,
    /// Expected name, asserted against ServWare's own label at start-up.
    pub name_contains: String,
    /// Fixed dollar value, where the conference gives a standard amount.
    pub value: Option<u32>,
}

impl Default for ConferenceConfig {
    fn default() -> Self {
        Self {
            second_harvest: AssistanceType {
                id: "16542".into(),
                name_contains: "Second Harvest".into(),
                value: Some(70),
            },
            gift_card: AssistanceType {
                id: "16522".into(),
                name_contains: "Gift Card".into(),
                value: None, // scales with household size
            },
            gift_card_ladder: vec![50, 60, 70, 80, 90, 100],
            visit_mileage: "5".into(),
            visit_notes_html: "<p>Delivered food and gift cards</p>".into(),
            tag_assistance_notes: true,
        }
    }
}

impl ConferenceConfig {
    /// Load from `conference.toml` beside the binary or in the working
    /// directory, falling back to the Nativity defaults.
    pub fn load() -> Self {
        for path in ["conference.toml", "../conference.toml"] {
            if let Ok(text) = std::fs::read_to_string(path) {
                match toml::from_str(&text) {
                    Ok(cfg) => {
                        tracing::info!(path, "loaded conference config");
                        return cfg;
                    }
                    Err(e) => tracing::warn!(path, %e, "ignoring malformed conference config"),
                }
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
