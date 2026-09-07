//! The assistance-request list endpoint (DataTables server-side processing).
//!
//! Two deliberate departures from the previous implementation:
//!
//! * **Strict deserialization of the fields we depend on.** Blanket
//!   `#[serde(default)]` plus null-stripping turned a renamed
//!   `calculatedHouseholdCount` into `0`, which the gift-card ladder maps to $50
//!   for *every* family. Required fields turn that silent money bug into a loud
//!   parse failure. See DECISIONS.md D10.
//! * **Real pagination.** The old call pinned `iDisplayLength=100` and never
//!   compared against `iTotalDisplayRecords`, so requests past the first hundred
//!   were dropped silently.

use serde::Deserialize;

use crate::servware::client::ServWareClient;
use crate::servware::error::Result;
use crate::servware::error::ServWareError;
use crate::servware::paging;
use crate::servware::paging::Envelope;
use crate::servware::paging::PAGE_SIZE;

/// Fields this tool actually depends on. Everything here is required; a missing
/// or mistyped field is an error, not a default.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestSummary {
    pub id: u64,
    pub status: String,
    pub date_requested: String,
    /// Drives the gift-card amount. Must never silently default.
    pub calculated_household_count: u32,
    pub client: ClientSummary,

    // Export-only breakdowns. These default rather than being required because
    // nothing computes money from them -- a blank column in a spreadsheet is a
    // visible absence, unlike a silent `0` feeding the gift-card ladder.
    #[serde(default)]
    pub calculated_adult_count: u32,
    #[serde(default)]
    pub calculated_child_count: u32,
    #[serde(default)]
    pub assistance_items: Vec<AssistanceItemSummary>,

    // Presentational; safe to default because nothing numeric depends on them.
    #[serde(default)]
    pub street_address_line1: String,
    #[serde(default)]
    pub street_address_line2: String,
    #[serde(default)]
    pub city: String,
    #[serde(default)]
    pub state_code: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientSummary {
    pub id: u64,
    pub first_name: String,
    pub last_name: String,
    #[serde(default)]
    pub home_phone: String,
    #[serde(default)]
    pub mobile_phone: String,
}

/// An assistance item as the list endpoint nests it. Export-only.
///
/// `date_provided` is when help actually reached the family, which is **not**
/// the same as when they asked. ServWare's own Neighbor Assistance Summary
/// report is built on this date, so any export claiming to answer "how much did
/// this family receive between two dates" has to carry it. See DECISIONS.md D23.
///
/// `monetary_value` and `date_provided` are `Option`, not defaulted scalars.
/// They used to default to `0.0` and `""`, so a renamed `monetaryValue` wrote
/// `0.00` into the assistance CSV — indistinguishable from an item that really
/// was worth nothing — and an empty `date_provided` silently emptied the
/// delivery-recency check while `fetch_window` still returned `Ok`. An absent
/// value now reads as absent everywhere: a blank cell in the spreadsheet, and a
/// row the recency check knows it does not have. See DECISIONS.md D43.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistanceItemSummary {
    #[serde(default)]
    pub monetary_value: Option<f64>,
    #[serde(default)]
    pub date_provided: Option<String>,
    #[serde(default)]
    pub quantity: f64,
    #[serde(default)]
    pub pending: bool,
    #[serde(default)]
    pub assistance_type: Option<AssistanceTypeSummary>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistanceTypeSummary {
    #[serde(default)]
    pub name: String,
}

impl AssistanceItemSummary {
    pub fn type_name(&self) -> &str {
        self.assistance_type.as_ref().map_or("", |t| t.name.as_str())
    }
}

impl RequestSummary {
    /// Total dollars recorded against this request, ignoring items that carry no
    /// amount at all.
    pub fn assistance_total(&self) -> f64 {
        self.assistance_items
            .iter()
            .filter_map(|i| i.monetary_value)
            .sum()
    }

    /// Items that carry no monetary value.
    ///
    /// Reported rather than silently treated as zero: if it is *every* item, the
    /// field has probably been renamed and no total from this pull is safe to
    /// rely on.
    pub fn items_without_value(&self) -> usize {
        self.assistance_items
            .iter()
            .filter(|i| i.monetary_value.is_none())
            .count()
    }

    pub fn display_name(&self) -> String {
        format!("{} {}", self.client.first_name, self.client.last_name)
            .trim()
            .to_string()
    }

    pub fn address(&self) -> String {
        let line2 = self.street_address_line2.trim();
        let street = if line2.is_empty() {
            self.street_address_line1.trim().to_string()
        } else {
            format!("{} {}", self.street_address_line1.trim(), line2)
        };
        [street.as_str(), self.city.trim(), self.state_code.trim()]
            .iter()
            .filter(|p| !p.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Which requests to list. `Open` is the working set; `Any` is needed to read a
/// request back after it has been completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusFilter {
    Open,
    Any,
}

impl StatusFilter {
    fn as_param(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Any => "",
        }
    }
}

/// Which end of the `dateRequested` ordering to start from.
///
/// `Desc` exists so a date-windowed pull can stop as soon as it walks past the
/// start of the window, instead of paging the whole history to find it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    Asc,
    Desc,
}

impl SortDir {
    fn as_param(self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }
}

/// Refuse to walk more pages than this without an explicit opt-in.
///
/// ServWare is production and every page is a request against it. An unfiltered
/// listing covers thousands of historical requests, which one careless command
/// turns into fifty round trips. Callers that genuinely want everything must say
/// so with `fetch_all_paged`.
const DEFAULT_MAX_PAGES: u32 = 3;

/// Page budget for a date-windowed pull.
///
/// Sixty pages is six thousand requests -- comfortably past the whole history
/// (4,907 at the time of writing) so that "pull everything" works, while still
/// refusing to run away if the conference grows an order of magnitude. The seek
/// means a narrow window costs only the pages inside it regardless.
pub const WINDOW_MAX_PAGES: u32 = 60;

/// Fetch every matching request, following pagination to completion.
pub async fn fetch_all(
    client: &ServWareClient,
    filter: StatusFilter,
) -> Result<Vec<RequestSummary>> {
    fetch_all_paged(client, filter, DEFAULT_MAX_PAGES).await
}

/// Fetch with an explicit page budget. `max_pages` of 0 means no limit, and
/// should be used only when the caller has thought about the request count.
pub async fn fetch_all_paged(
    client: &ServWareClient,
    filter: StatusFilter,
    max_pages: u32,
) -> Result<Vec<RequestSummary>> {
    paging::paginate("requests", max_pages, |start| {
        fetch_page(client, filter, start, SortDir::Asc)
    })
    .await
}

async fn fetch_page(
    client: &ServWareClient,
    filter: StatusFilter,
    start: u32,
    sort: SortDir,
) -> Result<Envelope> {
    fetch_page_sized(client, filter, start, sort, PAGE_SIZE).await
}

async fn fetch_page_sized(
    client: &ServWareClient,
    filter: StatusFilter,
    start: u32,
    sort: SortDir,
    length: u32,
) -> Result<Envelope> {
    let columns = "id,id,status,dateRequested,client.lastName,client.firstName,\
                   requestAssignedToMember,streetAddressLine1,client.homePhone,\
                   client.mobilePhone,pendingItems,id";
    let query = [
        ("sEcho", "1".to_string()),
        ("iColumns", "12".to_string()),
        ("sColumns", columns.to_string()),
        ("iDisplayStart", start.to_string()),
        ("iDisplayLength", length.to_string()),
        ("iSortCol_0", "3".to_string()),
        ("sSortDir_0", sort.as_param().to_string()),
        ("iSortingCols", "1".to_string()),
        ("sSearch", String::new()),
        ("bRegex", "false".to_string()),
        ("filterByStatus", filter.as_param().to_string()),
        ("filterByPartnerConf", String::new()),
        ("filterByReqAssigned", String::new()),
        ("filterByVisitAssigned", String::new()),
    ];
    // Scoped so the serializer is dropped before the await: its encoding-override
    // field is not `Sync`, and holding it across a suspension point would make the
    // whole future non-Sync.
    let query_string = {
        let mut qs = form_urlencoded::Serializer::new(String::new());
        for (k, v) in &query {
            qs.append_pair(k, v);
        }
        // Per column, exactly as the captured browser request sends them
        // (api.md:163, api.md:182). These collapsed into a loop that wrote
        // "id" for all twelve while `iSortCol_0=3` still said "sort by column
        // 3" -- and column 3 is `dateRequested`. DataTables resolves the sort
        // property as `mDataProp_{iSortCol_0}`, so the request contradicted
        // itself in the one parameter that decides the ordering, and every
        // caller below assumes rows arrive newest-first by date.
        // See DECISIONS.md D44.
        for (i, col) in columns.split(',').enumerate() {
            qs.append_pair(&format!("mDataProp_{i}"), col);
            qs.append_pair(&format!("bSortable_{i}"), "true");
        }
        qs.finish()
    };

    let raw = client
        .get_json(&format!("/app/assistancerequests/list?{query_string}"))
        .await?;
    serde_json::from_value(raw)
        .map_err(|e| ServWareError::Malformed(format!("unexpected list response: {e}")))
}

/// Parse a ServWare date. Every date on the wire is `MM/DD/YYYY`.
pub fn parse_date(s: &str) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(s.trim(), "%m/%d/%Y").ok()
}

/// Whether a request's date falls inside an inclusive window. An unparseable or
/// missing date is kept: dropping a row because we could not read its date would
/// silently shrink an export.
pub fn in_window(
    date_requested: &str,
    from: Option<chrono::NaiveDate>,
    to: Option<chrono::NaiveDate>,
) -> bool {
    let Some(d) = parse_date(date_requested) else {
        return true;
    };
    from.is_none_or(|f| d >= f) && to.is_none_or(|t| d <= t)
}

/// Whether a page fetched newest-first has walked past the start of the window,
/// so paging can stop. Rows with unreadable dates never trigger a stop.
fn page_is_past(rows: &[RequestSummary], from: Option<chrono::NaiveDate>) -> bool {
    let Some(from) = from else {
        return false;
    };
    rows.iter()
        .filter_map(|r| parse_date(&r.date_requested))
        .next_back()
        .is_some_and(|oldest| oldest < from)
}

/// The row at one offset, fetched as cheaply as the endpoint allows.
///
/// Used to find where a date window begins without dragging back the pages in
/// front of it. Each probe returns a single record instead of a hundred.
async fn probe_date(
    client: &ServWareClient,
    filter: StatusFilter,
    offset: u32,
) -> Result<Option<chrono::NaiveDate>> {
    let envelope = fetch_page_sized(client, filter, offset, SortDir::Desc, 1).await?;
    let Some(raw) = envelope.data.first() else {
        return Ok(None);
    };
    Ok(raw
        .get("dateRequested")
        .and_then(|v| v.as_str())
        .and_then(parse_date))
}

/// Binary-search for the first offset inside the window.
///
/// Rows come back newest-first, so `dateRequested` decreases monotonically with
/// offset and "is this row at or before the end of the window?" flips exactly
/// once. Reaching a window a year back would otherwise mean pulling every newer
/// page in full -- around a dozen hundred-record responses thrown away. Returns
/// the offset to start paging from, and how many probes it cost.
async fn seek_window_start(
    client: &ServWareClient,
    filter: StatusFilter,
    to: Option<chrono::NaiveDate>,
    total: u32,
) -> Result<(u32, u32)> {
    let Some(to) = to else {
        return Ok((0, 0));
    };
    if total <= PAGE_SIZE {
        return Ok((0, 0));
    }

    // The common case is a recent window, where the newest request is already
    // inside it and no search is needed.
    let mut probes = 1;
    match probe_date(client, filter, 0).await? {
        Some(newest) if newest <= to => return Ok((0, probes)),
        None => return Ok((0, probes)), // unreadable date; do not skip anything
        _ => {}
    }

    let (mut lo, mut hi) = (0u32, total - 1);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        probes += 1;
        match probe_date(client, filter, mid).await? {
            Some(d) if d <= to => hi = mid,
            Some(_) => lo = mid + 1,
            // A row we cannot read a date for makes the search unsound, so give
            // up on skipping and walk from the top.
            None => return Ok((0, probes)),
        }
    }

    // Start a whole page early: rows sharing the boundary date may sit just
    // before the offset we found, and re-reading one page is cheap insurance.
    Ok((
        (lo / PAGE_SIZE).saturating_sub(1) * PAGE_SIZE,
        probes,
    ))
}

/// Fetch requests whose `dateRequested` falls in an inclusive window.
///
/// Walks newest-first, skips ahead to where the window begins, and stops at its
/// far edge. With no `from` this is a full-history walk and the caller must have
/// decided that is what it wants -- [`WINDOW_MAX_PAGES`] still bounds it.
///
/// The budget is not a parameter: all four call sites passed the same constant,
/// so it was a knob nothing turned.
pub async fn fetch_window(
    client: &ServWareClient,
    filter: StatusFilter,
    from: Option<chrono::NaiveDate>,
    to: Option<chrono::NaiveDate>,
) -> Result<Vec<RequestSummary>> {
    let total = fetch_page_sized(client, filter, 0, SortDir::Desc, 1)
        .await?
        .total_display_records;
    let (mut start, probes) = seek_window_start(client, filter, to, total).await?;
    tracing::info!(total, start, probes, "seeking date window");

    let mut out: Vec<RequestSummary> = Vec::new();
    let mut pages = 0u32;

    loop {
        if pages > 0 {
            // Unhurried on purpose. With no date range this walks the whole
            // history, which is the one place a single command becomes fifty
            // hundred-record requests against somebody's production server.
            tokio::time::sleep(std::time::Duration::from_millis(120)).await;
        }
        let envelope = fetch_page(client, filter, start, SortDir::Desc).await?;
        pages += 1;
        let total = envelope.total_display_records;
        let returned = envelope.data.len();

        let page: Vec<RequestSummary> = paging::decode_rows(envelope.data, "requests")?;

        let past = page_is_past(&page, from);
        out.extend(
            page.into_iter()
                .filter(|r| in_window(&r.date_requested, from, to)),
        );

        if past || returned == 0 || start + PAGE_SIZE >= total {
            return Ok(out);
        }
        if pages >= WINDOW_MAX_PAGES {
            // Guidance, not a fault: `Malformed` would replace this sentence
            // with "ServWare sent back something this tool did not understand".
            return Err(ServWareError::TooBroad(format!(
                "ServWare has {total} requests in total and the date range is still open \
                 after {pages} pages ({} kept so far). Narrow the date range.",
                out.len()
            )));
        }
        start += PAGE_SIZE;
    }
}
