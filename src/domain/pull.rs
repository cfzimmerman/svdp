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

/// Below this many households, "all of them are empty" is a coincidence worth
/// tolerating; at or above it, it is a broken parser.
const EMPTY_ROSTER_CANARY: usize = 5;

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
    /// Requests whose page could not be read. Named, never silently dropped:
    /// a household missing from an Adopt-a-Family list is a family missing from
    /// the programme.
    pub unreadable: Vec<u64>,
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

    let requests = list::fetch_window(client, StatusFilter::Any, from, to).await?;
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
    let mut unreadable: Vec<u64> = Vec::new();

    for (i, r) in latest.iter().enumerate() {
        if i > 0 {
            tokio::time::sleep(PACE).await;
        }
        // Deliberately not `?`. This loop walks up to 200 households at 150 ms
        // each, so a transient 502 at household 180 used to unwind and throw
        // away all 180 rosters already fetched -- three minutes of somebody
        // else's production traffic wasted, and a retry that pays for all 200
        // again. This is the same `?`-in-a-batch-loop defect CLAUDE.md lists
        // for the legacy path. See DECISIONS.md D45.
        let members = match detail::fetch_household_members(client, r.id).await {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(request = r.id, error = %e, "household roster unreadable");
                unreadable.push(r.id);
                fetched += 1;
                continue;
            }
        };
        fetched += 1;
        if members.is_empty() {
            without_members += 1;
        }
        out.push(Household {
            client_id: r.client.id,
            last_name: r.client.last_name.clone(),
            members,
        });
    }

    // Every household coming back empty is not an answer, it is a symptom.
    // ServWare accepts a head count *or* individual people, so some empties are
    // real (19 of 158 in one window) -- but all of them, in a pull of any size,
    // means the tab or a column has been renamed and the honest answer is that
    // we do not know. Reporting it as "nobody lives in any of these houses"
    // reads like a correct answer and is the worst possible failure here.
    if !out.is_empty() && without_members == out.len() && out.len() >= EMPTY_ROSTER_CANARY {
        return Err(ServWareError::Malformed(format!(
            "None of the {} households looked up has anybody listed in it. That is not a \
             plausible answer, so ServWare's household page has most likely changed and \
             this tool needs an update. Nothing has been written and no list is being \
             shown, because a wrong list here is worse than none.",
            out.len()
        )));
    }

    let stats = PullStats {
        households: out.len(),
        servware_requests: list_pages + fetched,
        without_members,
        unreadable,
    };
    Ok((out, stats))
}
