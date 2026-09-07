//! The neighbors (clients) list endpoint.
//!
//! Same DataTables server-side protocol as `list.rs`, against
//! `/app/clients/list`. This is the conference's whole roster — 416 records at
//! the time of writing — which makes it five pages rather than the thousands of
//! rows a full request history would be.
//!
//! The projection is deliberately narrow at the *export* layer, not here: the
//! server returns the entire Client object regardless of which columns are
//! requested, so trimming `sColumns` would buy nothing and would risk changing
//! the sort and search semantics. Field selection is enforced where it can be
//! tested — the column allowlist in `domain::export`.

use chrono::Datelike;
use serde::Deserialize;

use crate::servware::client::ServWareClient;
use crate::servware::error::Result;
use crate::servware::error::ServWareError;
use crate::servware::paging;
use crate::servware::paging::Envelope;
use crate::servware::paging::PAGE_SIZE;

/// A neighbour (household) as the roster endpoint returns them.
///
/// Identity fields are required, so a rename becomes a loud parse failure rather
/// than an empty column. Everything else defaults, because it is presentational
/// and nothing numeric depends on it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NeighborSummary {
    pub id: u64,
    pub first_name: String,
    pub last_name: String,

    #[serde(default)]
    pub street_address_line1: String,
    #[serde(default)]
    pub street_address_line2: String,
    #[serde(default)]
    pub city: String,
    #[serde(default)]
    pub state_code: String,
    #[serde(default)]
    pub postal_code: String,

    #[serde(default)]
    pub home_phone: String,
    #[serde(default)]
    pub mobile_phone: String,
    #[serde(default)]
    pub work_phone: String,
    #[serde(default)]
    pub email_address: String,

    #[serde(default)]
    pub primary_language: String,
    #[serde(default)]
    pub marital_status: String,

    #[serde(default)]
    pub parishioner: bool,
    #[serde(default)]
    pub homeless: bool,
    #[serde(default)]
    pub disabled_client: bool,
    #[serde(default)]
    pub veteran: bool,

    /// "MM/DD/YYYY", absent for a neighbour who has never made a request.
    #[serde(default)]
    pub last_request_date: String,

    /// Read only so an age can be derived. **Never exported** -- see
    /// DECISIONS.md D21. Use `age_on` rather than touching this.
    #[serde(default)]
    birth_date: String,

    /// Hand-entered on the neighbour record; often null, in which case the
    /// request list's `calculated*` counts are the better source.
    #[serde(default)]
    pub household_adult_count: Option<u32>,
    #[serde(default)]
    pub household_child_count: Option<u32>,
}

impl NeighborSummary {
    /// The neighbour's age, derived from a birth date that is never itself
    /// emitted. The household members table lists everyone *but* the neighbour
    /// (D19), so without this the head of household has no age anywhere -- which
    /// the report volunteers use today does provide.
    pub fn age_on(&self, today: chrono::NaiveDate) -> Option<u32> {
        let dob = chrono::NaiveDate::parse_from_str(self.birth_date.trim(), "%m/%d/%Y").ok()?;
        let had_birthday = (today.month(), today.day()) >= (dob.month(), dob.day());
        let years = today.year() - dob.year() - i32::from(!had_birthday);
        u32::try_from(years).ok().filter(|y| *y < 130)
    }

    pub fn display_name(&self) -> String {
        format!("{} {}", self.first_name, self.last_name)
            .trim()
            .to_string()
    }
}

/// Ten pages is a thousand neighbours — comfortably above any single
/// conference, and still a bounded number of requests against production.
const DEFAULT_MAX_PAGES: u32 = 10;

/// Fetch the whole conference roster.
pub async fn fetch_all(client: &ServWareClient) -> Result<Vec<NeighborSummary>> {
    fetch_all_paged(client, DEFAULT_MAX_PAGES).await
}

/// Fetch with an explicit page budget. `max_pages` of 0 means no limit.
pub async fn fetch_all_paged(
    client: &ServWareClient,
    max_pages: u32,
) -> Result<Vec<NeighborSummary>> {
    paging::paginate("neighbours", max_pages, |start| fetch_page(client, start)).await
}

/// Query parameters reproduce a captured browser request. The per-column
/// `mDataProp`/`bSortable`/`bSearchable` triplets are what the DataTables client
/// sends; the server is sensitive to the column count agreeing with `iColumns`.
async fn fetch_page(client: &ServWareClient, start: u32) -> Result<Envelope> {
    const COLUMNS: &[&str] = &[
        "id",
        "id",
        "alertNote",
        "lastName",
        "firstName",
        "assignedMember",
        "streetAddressLine1",
        "city",
        "homePhone",
        "mobilePhone",
        "ssnLastFour",
        "birthDate",
        "id",
        "id",
    ];
    /// Columns DataTables marks sortable, by index.
    const SORTABLE: &[usize] = &[3, 4, 7, 8, 9, 10, 11];

    let query_string = {
        let mut qs = form_urlencoded::Serializer::new(String::new());
        qs.append_pair("sEcho", "1");
        qs.append_pair("iColumns", &COLUMNS.len().to_string());
        qs.append_pair("sColumns", &COLUMNS.join(","));
        qs.append_pair("iDisplayStart", &start.to_string());
        qs.append_pair("iDisplayLength", &PAGE_SIZE.to_string());
        for (i, col) in COLUMNS.iter().enumerate() {
            qs.append_pair(&format!("mDataProp_{i}"), col);
            qs.append_pair(&format!("sSearch_{i}"), "");
            qs.append_pair(&format!("bRegex_{i}"), "false");
            qs.append_pair(&format!("bSearchable_{i}"), "true");
            qs.append_pair(
                &format!("bSortable_{i}"),
                if SORTABLE.contains(&i) { "true" } else { "false" },
            );
        }
        qs.append_pair("sSearch", "");
        qs.append_pair("bRegex", "false");
        // Sort by last name then first name, as the Neighbors page does.
        qs.append_pair("iSortCol_0", "3");
        qs.append_pair("sSortDir_0", "asc");
        qs.append_pair("iSortCol_1", "4");
        qs.append_pair("sSortDir_1", "asc");
        qs.append_pair("iSortingCols", "2");
        qs.append_pair("filterByBirthDate", "");
        qs.finish()
    };

    let raw = client
        .get_json(&format!("/app/clients/list?{query_string}"))
        .await?;
    serde_json::from_value(raw)
        .map_err(|e| ServWareError::Malformed(format!("unexpected neighbour list response: {e}")))
}
