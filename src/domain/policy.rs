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
    ///
    /// Rejected at parse time if empty. It used to be guarded by a
    /// `debug_assert!`, which is compiled out of the `--release` binary that
    /// ships -- so an external `conference.toml` with `gift_card_ladder = []`
    /// parsed happily and `gift_card_dollars` fell through to **$0 for every
    /// family**, silently. See DECISIONS.md D34.
    #[serde(deserialize_with = "non_empty_ladder")]
    pub gift_card_ladder: Vec<u32>,
    pub visit_mileage: String,
    pub visit_notes_html: String,
    /// The largest amount that may be recorded in a single assistance item.
    ///
    /// Not a policy figure -- the volunteer's number always wins within it. It
    /// is a bound on what can reach ServWare at all, because nothing else stood
    /// between a mistyped or hallucinated figure and money in the county's books
    /// that cannot be taken back out. See DECISIONS.md D36.
    #[serde(default = "default_max_item_dollars")]
    pub max_item_dollars: u32,
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
        let index = size.saturating_sub(1) as usize;
        let ladder = &self.gift_card_ladder;
        *ladder
            .get(index)
            // Non-empty is a parse-time invariant, so `last()` is always Some
            // and there is no "no amount" case left to invent a number for.
            .unwrap_or_else(|| ladder.last().expect("ladder is non-empty by construction"))
    }

    /// The machine tag for one write slot of one session.
    pub fn tag(&self, session: &str, slot: Slot) -> String {
        format!("svdp:s={session};i={}", slot.as_str())
    }

    /// Notes text for an assistance item: machine tag first, then something a
    /// caseworker reading ServWare can understand.
    ///
    /// Always tagged. This used to be switchable with `tag_assistance_notes`,
    /// whose `false` arm was a second, weaker idempotency scheme keyed on
    /// `(kind, date)` -- the key DECISIONS.md D7 rejects as unsound -- that no
    /// shipped config ever selected. See DECISIONS.md D35.
    pub fn item_notes(&self, session: &str, slot: Slot, date: &str) -> String {
        format!("{} — SVdP delivery {date}", self.tag(session, slot))
    }
}

/// Kept as a function rather than a literal in the struct so an older external
/// `conference.toml` without the field still loads.
fn default_delivery_interval_days() -> u32 {
    28
}

fn default_max_item_dollars() -> u32 {
    500
}

/// Reject an empty gift-card ladder while it is still just text on disk.
///
/// Validating here rather than at the call site means an invalid override is
/// caught by the existing "ignoring malformed conference config" path and the
/// reviewed, embedded policy is used instead -- so no code downstream has to
/// have an opinion about what to pay a family when the ladder is missing.
fn non_empty_ladder<'de, D>(d: D) -> std::result::Result<Vec<u32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let ladder = Vec::<u32>::deserialize(d)?;
    if ladder.is_empty() {
        return Err(D::Error::custom(
            "gift_card_ladder must have at least one amount",
        ));
    }
    Ok(ladder)
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
