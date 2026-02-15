use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use anyhow::Context;
use reqwest::Url;
use serde::Deserialize;

use super::ServWare;

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// DataTables server-side processing response envelope.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchRequestsResponse {
    #[serde(rename = "sEcho")]
    pub s_echo: u32,
    #[serde(rename = "iTotalRecords")]
    pub i_total_records: u32,
    #[serde(rename = "iTotalDisplayRecords")]
    pub i_total_display_records: u32,
    #[serde(rename = "aaData")]
    pub aa_data: Vec<AssistanceRequest>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AssistanceRequest {
    // --- Metadata ---
    pub id: u64,
    pub version: u64,
    pub marked_for_deletion: bool,
    pub date_created: String,
    pub date_modified: String,
    pub created_by: String,
    pub modified_by: String,

    // --- Core Request Fields ---
    pub status: String,
    pub date_requested: String,
    pub request_note: String,
    pub denial_reason: Option<String>,
    pub intake_person: Option<String>,
    pub case_number: Option<String>,

    // --- Address ---
    pub street_address_line1: String,
    pub street_address_line2: String,
    pub city: String,
    pub state_code: String,
    pub postal_code: String,

    // --- Assignment ---
    pub request_assigned_to_member: Option<String>,
    pub visit_assigned_to: Option<String>,
    pub visit_assigned_to_member: Option<String>,
    pub visit_assigned_to_member_secondary: Option<String>,

    // --- Household ---
    pub household_adult_count: Option<u32>,
    pub household_child_count: Option<u32>,
    pub calculated_adult_count: u32,
    pub calculated_child_count: u32,
    pub calculated_household_count: u32,
    pub people_helped_override: Option<u32>,
    pub household_income_level: Option<String>,
    pub household_income_level_desc: Option<String>,

    // --- Client Characteristics ---
    pub parishioner: bool,
    pub homeless: bool,
    pub disabled_client: bool,

    // --- Visit Info ---
    pub home_visit_required: bool,
    pub home_visit_scheduled: Option<String>,
    pub visit_completed: bool,
    pub visit_scheduled_date: Option<String>,
    pub visit_scheduled_duration_minutes: Option<u32>,
    pub visit_notes: String,
    pub visit_mileage_hrs_in_svc: Option<f64>,
    pub visit_type: Option<String>,
    pub home_visit_cnt: Option<u32>,

    // --- Visit Type Flags ---
    pub other_visit: bool,
    pub prison_visit: bool,
    pub hospital_visit: bool,
    pub elder_care_visit: bool,
    pub telephone_visit: bool,
    pub church_pantry_visit: bool,

    // --- Visit Counts ---
    pub other_visit_cnt: Option<u32>,
    pub prison_visit_cnt: Option<u32>,
    pub hospital_visit_cnt: Option<u32>,
    pub eldercare_visit_cnt: Option<u32>,
    pub phone_visit_cnt: Option<u32>,
    pub church_pantry_visit_cnt: Option<u32>,

    // --- Referrals ---
    pub referred_to_conference: bool,
    pub referral_conference: Option<serde_json::Value>,
    pub referred_to_agency: bool,
    pub referred_from_org: Option<String>,
    pub referral_note: String,
    pub referral_organization: Option<serde_json::Value>,

    // --- Conference/Partner ---
    pub partner_conference: Option<serde_json::Value>,
    pub client_county: Option<String>,
    pub conference_view_required: bool,
    pub initiated_by_district: bool,
    pub initiated_by_council: Option<bool>,

    // --- Assistance ---
    pub includes_other_payments: bool,
    pub requested_items: Vec<serde_json::Value>,
    pub assistance_items: Vec<AssistanceItem>,
    pub pantry_id: Option<String>,

    // --- Nested ---
    pub client: Client,

    // --- Pending items (display field) ---
    pub pending_items: Option<serde_json::Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Client {
    pub id: u64,
    pub first_name: String,
    pub last_name: String,
    pub middle_initial: String,
    pub maiden_name: String,
    pub birth_date: String,
    pub gender: String,
    pub ethnicity: String,
    pub primary_language: String,
    pub marital_status: String,

    // --- Contact ---
    pub home_phone: String,
    pub work_phone: String,
    pub mobile_phone: String,
    pub email_address: String,
    pub text_communication_preferred: bool,

    // --- Address ---
    pub street_address_line1: String,
    pub street_address_line2: String,
    pub city: String,
    pub state_code: String,
    pub postal_code: String,

    // --- Status ---
    pub parishioner: bool,
    pub homeless: bool,
    pub disabled_client: bool,
    pub veteran: bool,
    pub private_client: bool,

    // --- Notes ---
    pub notes: String,
    pub alert_note: String,

    // --- Other ---
    pub last_request_date: String,
    pub assigned_member: Option<String>,
    pub open_follow_up: bool,
    pub follow_ups: Vec<serde_json::Value>,

    // --- Nested ---
    pub conference: serde_json::Value,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AssistanceItem {
    pub id: u64,
    pub monetary_value: f64,
    pub total_assistance_item_value: f64,
    pub quantity: u32,
    pub pending: bool,
    pub date_provided: String,
    pub promised_date: Option<String>,
    pub date_paid: Option<String>,
    pub notes: String,
    pub sub_type: Option<String>,

    // --- Check/Payment ---
    pub check_requested: bool,
    pub check_number: String,
    pub payee_name: String,

    // --- Nested ---
    pub assistance_type: AssistanceType,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AssistanceType {
    pub id: u64,
    pub name: String,
    pub abbr_name: String,
    pub description: String,
    pub active: bool,
    pub monetary_value: Option<f64>,
    pub allow_quantity_to_be_specified: bool,
    pub track_quantity: bool,
}

// ---------------------------------------------------------------------------
// Query parameters
// ---------------------------------------------------------------------------

/// Parameters for fetching assistance requests.
pub struct FetchRequestsParams {
    pub display_start: u32,
    pub display_length: u32,
    pub sort_col: u32,
    pub sort_dir: String,
    pub search: String,
    pub filter_by_status: String,
    pub filter_by_partner_conf: String,
    pub filter_by_req_assigned: String,
    pub filter_by_visit_assigned: String,
}

impl FetchRequestsParams {
    /// Config for fetching "Open" status requests in
    /// ascending order by date.
    pub fn new_open_asc() -> Self {
        Self {
            display_start: 0,
            display_length: 100,
            sort_col: 3,
            sort_dir: "asc".into(),
            search: String::new(),
            filter_by_status: "Open".into(),
            filter_by_partner_conf: String::new(),
            filter_by_req_assigned: String::new(),
            filter_by_visit_assigned: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

impl ServWare {
    /// Fetch assistance requests using the DataTables server-side processing API.
    pub async fn fetch_requests(
        &self,
        params: &FetchRequestsParams,
    ) -> anyhow::Result<FetchRequestsResponse> {
        let url = Self::list_url();

        let cache_buster = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .to_string();

        let s_columns = "id,id,status,dateRequested,client.lastName,client.firstName,\
                         requestAssignedToMember,streetAddressLine1,client.homePhone,\
                         client.mobilePhone,pendingItems,id";

        let display_start = params.display_start.to_string();
        let display_length = params.display_length.to_string();
        let sort_col = params.sort_col.to_string();

        let query_pairs: &[(&str, &str)] = &[
            ("sEcho", "1"),
            ("iColumns", "12"),
            ("sColumns", s_columns),
            ("iDisplayStart", &display_start),
            ("iDisplayLength", &display_length),
            ("mDataProp_0", "id"),
            ("mDataProp_1", "id"),
            ("mDataProp_2", "status"),
            ("mDataProp_3", "dateRequested"),
            ("mDataProp_4", "client.lastName"),
            ("mDataProp_5", "client.firstName"),
            ("mDataProp_6", "requestAssignedToMember"),
            ("mDataProp_7", "streetAddressLine1"),
            ("mDataProp_8", "client.homePhone"),
            ("mDataProp_9", "client.mobilePhone"),
            ("mDataProp_10", "id"),
            ("mDataProp_11", "id"),
            ("iSortCol_0", &sort_col),
            ("sSortDir_0", &params.sort_dir),
            ("iSortingCols", "1"),
            ("bSortable_0", "false"),
            ("bSortable_1", "false"),
            ("bSortable_2", "true"),
            ("bSortable_3", "true"),
            ("bSortable_4", "true"),
            ("bSortable_5", "true"),
            ("bSortable_6", "false"),
            ("bSortable_7", "false"),
            ("bSortable_8", "false"),
            ("bSortable_9", "false"),
            ("bSortable_10", "false"),
            ("bSortable_11", "false"),
            ("sSearch", &params.search),
            ("bRegex", "false"),
            ("filterByStatus", &params.filter_by_status),
            ("filterByPartnerConf", &params.filter_by_partner_conf),
            ("filterByReqAssigned", &params.filter_by_req_assigned),
            ("filterByVisitAssigned", &params.filter_by_visit_assigned),
            ("_", &cache_buster),
        ];

        let mut full_url = Url::parse(&url).context("failed to parse list URL")?;
        {
            let mut qs = full_url.query_pairs_mut();
            for &(k, v) in query_pairs {
                qs.append_pair(k, v);
            }
        }

        tracing::debug!(%full_url, "fetching assistance requests");

        let response = self
            .client
            .get(full_url)
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Accept", "application/json, text/javascript, */*; q=0.01")
            .send()
            .await
            .context("fetch requests HTTP request failed")?;

        let status = response.status();
        tracing::debug!(%status, "fetch requests response");

        if !status.is_success() {
            anyhow::bail!("fetch requests failed with status {status}");
        }

        let mut raw: serde_json::Value = response
            .json()
            .await
            .context("failed to parse fetch requests response JSON")?;

        self::strip_json_nulls(&mut raw);

        let body: FetchRequestsResponse = serde_json::from_value(raw)
            .context("failed to deserialize fetch requests response")?;

        tracing::debug!(
            total = body.i_total_display_records,
            returned = body.aa_data.len(),
            "fetched assistance requests"
        );

        Ok(body)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Recursively remove null-valued entries from JSON objects so that
/// `#[serde(default)]` can provide Rust defaults for those fields.
fn strip_json_nulls(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.retain(|_, v| !v.is_null());
            for v in map.values_mut() {
                strip_json_nulls(v);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                strip_json_nulls(v);
            }
        }
        _ => {}
    }
}
