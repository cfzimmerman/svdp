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

/// Fields this tool actually depends on. Everything here is required; a missing
/// or mistyped field is an error, not a default.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestSummary {
    pub id: u64,
    /// Hibernate optimistic-lock counter. Recorded at plan time and re-checked
    /// at submit time to detect edits made in ServWare meanwhile.
    pub version: u64,
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
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistanceItemSummary {
    #[serde(default)]
    pub monetary_value: f64,
    #[serde(default)]
    pub date_provided: String,
}

impl RequestSummary {
    /// Total dollars recorded against this request.
    pub fn assistance_total(&self) -> f64 {
        self.assistance_items.iter().map(|i| i.monetary_value).sum()
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

#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(rename = "iTotalDisplayRecords")]
    total_display_records: u32,
    #[serde(rename = "aaData")]
    data: Vec<serde_json::Value>,
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

const PAGE_SIZE: u32 = 100;

/// Refuse to walk more pages than this without an explicit opt-in.
///
/// ServWare is production and every page is a request against it. An unfiltered
/// listing covers thousands of historical requests, which one careless command
/// turns into fifty round trips. Callers that genuinely want everything must say
/// so with `fetch_all_paged`.
const DEFAULT_MAX_PAGES: u32 = 3;

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
    let mut out: Vec<RequestSummary> = Vec::new();
    let mut start = 0u32;
    let mut pages = 0u32;

    loop {
        let envelope = fetch_page(client, filter, start, SortDir::Asc).await?;
        pages += 1;
        let total = envelope.total_display_records;
        let returned = envelope.data.len();

        for raw in envelope.data {
            // Strip nulls so `#[serde(default)]` can apply to the *optional*
            // fields; required fields still fail loudly when absent.
            let mut raw = raw;
            strip_nulls(&mut raw);
            let summary: RequestSummary = serde_json::from_value(raw).map_err(|e| {
                ServWareError::Malformed(format!(
                    "ServWare's request format changed — {e}. \
                     This tool needs an update; nothing was written."
                ))
            })?;
            out.push(summary);
        }

        if returned == 0 || out.len() as u32 >= total {
            if (out.len() as u32) < total {
                return Err(ServWareError::Malformed(format!(
                    "ServWare reported {total} requests but only {} could be read",
                    out.len()
                )));
            }
            return Ok(out);
        }
        if max_pages != 0 && pages >= max_pages {
            return Err(ServWareError::Malformed(format!(
                "ServWare has {total} matching requests, which is more than this tool \
                 will page through ({} of {total} read in {pages} requests). Narrow the \
                 filter rather than fetching everything.",
                out.len()
            )));
        }
        start += PAGE_SIZE;
    }
}

async fn fetch_page(
    client: &ServWareClient,
    filter: StatusFilter,
    start: u32,
    sort: SortDir,
) -> Result<Envelope> {
    let columns = "id,id,status,dateRequested,client.lastName,client.firstName,\
                   requestAssignedToMember,streetAddressLine1,client.homePhone,\
                   client.mobilePhone,pendingItems,id";
    let query = [
        ("sEcho", "1".to_string()),
        ("iColumns", "12".to_string()),
        ("sColumns", columns.to_string()),
        ("iDisplayStart", start.to_string()),
        ("iDisplayLength", PAGE_SIZE.to_string()),
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
        for i in 0..12 {
            qs.append_pair(&format!("mDataProp_{i}"), "id");
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

/// Remove null-valued keys so `#[serde(default)]` applies to optional fields.
/// Required fields are unaffected and still fail when genuinely absent.
pub(crate) fn strip_nulls(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.retain(|_, v| !v.is_null());
            for v in map.values_mut() {
                strip_nulls(v);
            }
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(strip_nulls),
        _ => {}
    }
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

/// Fetch requests whose `dateRequested` falls in an inclusive window.
///
/// Walks newest-first and stops at the far edge of the window, so a three-month
/// pull costs a handful of requests rather than the whole history. With no
/// `from` this is a full-history walk and the caller must have decided that is
/// what it wants -- `max_pages` still bounds it.
pub async fn fetch_window(
    client: &ServWareClient,
    filter: StatusFilter,
    from: Option<chrono::NaiveDate>,
    to: Option<chrono::NaiveDate>,
    max_pages: u32,
) -> Result<Vec<RequestSummary>> {
    let mut out: Vec<RequestSummary> = Vec::new();
    let mut start = 0u32;
    let mut pages = 0u32;

    loop {
        let envelope = fetch_page(client, filter, start, SortDir::Desc).await?;
        pages += 1;
        let total = envelope.total_display_records;
        let returned = envelope.data.len();

        let mut page: Vec<RequestSummary> = Vec::with_capacity(returned);
        for mut raw in envelope.data {
            strip_nulls(&mut raw);
            let summary: RequestSummary = serde_json::from_value(raw).map_err(|e| {
                ServWareError::Malformed(format!(
                    "ServWare's request format changed — {e}. This tool needs an update."
                ))
            })?;
            page.push(summary);
        }

        let past = page_is_past(&page, from);
        out.extend(
            page.into_iter()
                .filter(|r| in_window(&r.date_requested, from, to)),
        );

        if past || returned == 0 || start + PAGE_SIZE >= total {
            return Ok(out);
        }
        if max_pages != 0 && pages >= max_pages {
            return Err(ServWareError::Malformed(format!(
                "ServWare has {total} requests in total and the window is still open after                  {pages} pages ({} rows kept). Narrow the date range.",
                out.len()
            )));
        }
        start += PAGE_SIZE;
    }
}
