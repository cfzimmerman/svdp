//! When each household was last actually delivered to.
//!
//! Deliveries run once a month per family, so the working list should not offer
//! a household that was served three weeks ago. Answering that needs
//! `date_provided` — when help *reached* the family — not `date_requested`
//! (D23).

use crate::servware::list::RequestSummary;

/// How far back to read request history.
///
/// The list endpoint filters on `date_requested`, but recency is about
/// `date_provided`, and help arrives days to weeks after a family asks: median
/// 4 days, p99.9 39 days, observed maximum 134 across a year of real items. 120
/// days covers a 28-day interval plus 92 days of slack, in about five pages.
pub const HISTORY_LOOKBACK_DAYS: i64 = 120;

/// The days on which each household received something.
pub struct DeliveryRecency {
    /// (client id, request id, date delivered)
    entries: Vec<(u64, u64, chrono::NaiveDate)>,
}

impl DeliveryRecency {
    pub fn from_history(history: &[RequestSummary]) -> Self {
        let mut entries = Vec::new();
        for r in history {
            for item in &r.assistance_items {
                if let Some(date) = crate::servware::list::parse_date(&item.date_provided) {
                    entries.push((r.client.id, r.id, date));
                }
            }
        }
        Self { entries }
    }

    /// The most recent day this household received anything, ignoring items
    /// recorded against `excluding_request`.
    ///
    /// The exclusion matters: a delivery whose recording was interrupted leaves
    /// items on a still-open request, and that must not make the request hide
    /// itself from the list of work left to do.
    pub fn last_delivery(
        &self,
        client_id: u64,
        excluding_request: u64,
    ) -> Option<chrono::NaiveDate> {
        self.entries
            .iter()
            .filter(|(c, r, _)| *c == client_id && *r != excluding_request)
            .map(|(_, _, d)| *d)
            .max()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
