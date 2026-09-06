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

impl RequestSummary {
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
        let envelope = fetch_page(client, filter, start).await?;
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
        ("sSortDir_0", "asc".to_string()),
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
fn strip_nulls(value: &mut serde_json::Value) {
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
