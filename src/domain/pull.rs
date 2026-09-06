//! Pulling household rosters, which is the one read that fans out.
//!
//! Every other export is a handful of JSON pages. This one costs a 200 KB HTML
//! page per household, against a live county system, under a named account —
//! so it is bounded, paced, and honest about what it did. See DECISIONS.md D22.

use crate::servware::client::ServWareClient;
use crate::servware::detail;
use crate::servware::detail::HouseholdMember;
use crate::servware::error::Result;
use crate::servware::error::ServWareError;
use crate::servware::list;
use crate::servware::list::RequestSummary;
use crate::servware::list::StatusFilter;

/// Default ceiling on households per pull.
pub const DEFAULT_MAX_HOUSEHOLDS: u32 = 200;

/// Ceiling no caller may raise past. The conference has a few hundred
/// neighbours in total; anything approaching this is a mistake, not a request.
pub const HARD_MAX_HOUSEHOLDS: u32 = 400;

/// Pause between detail-page fetches. Sequential and unhurried on purpose: this
/// is somebody else's production server.
const PACE: std::time::Duration = std::time::Duration::from_millis(150);

pub struct Household {
    pub client_id: u64,
    pub last_name: String,
    pub members: Vec<HouseholdMember>,
}

pub struct PullStats {
    /// Households the window turned up.
    pub households: usize,
    /// Requests made against ServWare, list pages included.
    pub servware_requests: usize,
    /// Households whose detail page listed no members at all.
    pub without_members: usize,
}

/// One request per household — the most recent — so each household's detail
/// page is fetched exactly once.
///
/// Requests arrive newest-first from `fetch_window`, so the first sighting of a
/// client id is their latest request.
pub fn latest_per_household(requests: &[RequestSummary]) -> Vec<&RequestSummary> {
    let mut seen = std::collections::HashSet::new();
    requests
        .iter()
        .filter(|r| seen.insert(r.client.id))
        .collect()
}

/// Fetch the household roster for every household with a request in the window.
///
/// Refuses rather than truncating when the window turns up more households than
/// the caller budgeted for: silently returning half a list is how a family gets
/// left off a Christmas program.
pub async fn household_members(
    client: &ServWareClient,
    from: Option<chrono::NaiveDate>,
    to: Option<chrono::NaiveDate>,
    max_households: u32,
) -> Result<(Vec<Household>, PullStats)> {
    let max = max_households.min(HARD_MAX_HOUSEHOLDS);

    let requests =
        list::fetch_window(client, StatusFilter::Any, from, to, list::WINDOW_MAX_PAGES).await?;
    let list_pages = requests.len().div_ceil(100).max(1);

    let latest = latest_per_household(&requests);
    if latest.len() as u32 > max {
        return Err(ServWareError::TooBroad(format!(
            "That date range covers {} households, which is more than this will look up in \
             one go ({max}). Ask for a shorter date range, or say explicitly how many \
             households to allow — but every one is a separate request to ServWare.",
            latest.len()
        )));
    }

    let mut out = Vec::with_capacity(latest.len());
    let mut without_members = 0usize;
    let mut fetched = 0usize;

    for (i, r) in latest.iter().enumerate() {
        if i > 0 {
            tokio::time::sleep(PACE).await;
        }
        let d = detail::fetch(client, r.id).await?;
        fetched += 1;
        if d.household_members.is_empty() {
            without_members += 1;
        }
        out.push(Household {
            client_id: r.client.id,
            last_name: r.client.last_name.clone(),
            members: d.household_members,
        });
    }

    let stats = PullStats {
        households: out.len(),
        servware_requests: list_pages + fetched,
        without_members,
    };
    Ok((out, stats))
}
