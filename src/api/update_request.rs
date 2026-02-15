use anyhow::Context;

use crate::api::fetch_requests::FetchRequestsParams;

use super::ServWare;
use super::fetch_requests::AssistanceRequest;

// ---------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------

/// Fields to update on an assistance request.
///
/// All fields are optional — `None` means "keep the current value from the
/// server." Internally, `update_request` fetches the current state and merges
/// these overrides before POSTing the full form.
///
/// **Member ID limitation:** The list API returns display names (e.g.
/// `"John Smith"`) but the form requires numeric IDs. If you don't provide
/// member IDs here, the corresponding form fields will be sent empty, which
/// may clear the assignment on the server.
#[derive(Debug, Default)]
pub struct UpdateRequestInput {
    // Status
    pub status: Option<String>,
    pub denial_reason_id: Option<String>,
    pub denial_reason_str: Option<String>,

    // Client info
    pub client_first_name: Option<String>,
    pub client_last_name: Option<String>,
    pub date_requested: Option<String>,

    // Assignment
    pub request_assigned_to_member_id: Option<String>,
    pub request_note: Option<String>,

    // Visit type checkboxes
    pub home_visit_required: Option<bool>,
    pub other_visit: Option<bool>,
    pub elder_care_visit: Option<bool>,
    pub hospital_visit: Option<bool>,
    pub prison_visit: Option<bool>,
    pub telephone_visit: Option<bool>,
    pub church_pantry_visit: Option<bool>,

    // Visit details
    pub home_visit_cnt: Option<String>,
    pub visit_completed: Option<bool>,
    pub visit_assigned_to_member_id: Option<String>,
    pub visit_assigned_to_member_id_secondary: Option<String>,
    pub visit_mileage_in_service: Option<String>,
    pub visit_hours_in_service: Option<String>,
    pub visit_scheduled_date: Option<String>,
    pub visit_scheduled_time: Option<String>,
    pub people_helped_override: Option<String>,
    pub visit_notes: Option<String>,

    // Referral
    pub referred_to_agency: Option<bool>,
    pub referred_to_conference: Option<bool>,
    pub referred_from_organization_id: Option<String>,
    pub referral_note: Option<String>,
}

// ---------------------------------------------------------------------------
// Form building
// ---------------------------------------------------------------------------

/// Add a Spring MVC checkbox pair to the form.
///
/// - Checked: `name=true` + `_name=on`
/// - Unchecked: `_name=on` only
fn push_checkbox(form: &mut Vec<(String, String)>, name: &str, checked: bool) {
    if checked {
        form.push((name.to_string(), "true".into()));
    }
    form.push((format!("_{name}"), "on".into()));
}

/// Build the full 30-field form by merging user input over the current server
/// state.
fn build_update_form(
    current: &AssistanceRequest,
    input: &UpdateRequestInput,
) -> Vec<(String, String)> {
    let mut f: Vec<(String, String)> = Vec::with_capacity(40);

    // --- Status ---
    f.push((
        "status".into(),
        input
            .status
            .clone()
            .unwrap_or_else(|| current.status.clone()),
    ));
    f.push((
        "denialReasonId".into(),
        input.denial_reason_id.clone().unwrap_or_default(),
    ));
    f.push((
        "denialReasonStr".into(),
        input.denial_reason_str.clone().unwrap_or_default(),
    ));

    // --- Client info ---
    f.push((
        "clientFirstName".into(),
        input
            .client_first_name
            .clone()
            .unwrap_or_else(|| current.client.first_name.clone()),
    ));
    f.push((
        "clientLastName".into(),
        input
            .client_last_name
            .clone()
            .unwrap_or_else(|| current.client.last_name.clone()),
    ));
    f.push((
        "dateRequested".into(),
        input
            .date_requested
            .clone()
            .unwrap_or_else(|| current.date_requested.clone()),
    ));

    // --- Assignment ---
    // Note: list API doesn't expose member IDs, only display names. If the
    // user doesn't provide an ID, we send empty string.
    f.push((
        "requestAssignedToMemberId".into(),
        input
            .request_assigned_to_member_id
            .clone()
            .unwrap_or_default(),
    ));
    f.push((
        "requestNote".into(),
        input
            .request_note
            .clone()
            .unwrap_or_else(|| current.request_note.clone()),
    ));
    f.push(("files".into(), String::new()));

    // --- Visit type checkboxes ---
    push_checkbox(
        &mut f,
        "homeVisitRequired",
        input
            .home_visit_required
            .unwrap_or(current.home_visit_required),
    );
    push_checkbox(
        &mut f,
        "otherVisit",
        input.other_visit.unwrap_or(current.other_visit),
    );
    push_checkbox(
        &mut f,
        "elderCareVisit",
        input.elder_care_visit.unwrap_or(current.elder_care_visit),
    );
    push_checkbox(
        &mut f,
        "hospitalVisit",
        input.hospital_visit.unwrap_or(current.hospital_visit),
    );
    push_checkbox(
        &mut f,
        "prisonVisit",
        input.prison_visit.unwrap_or(current.prison_visit),
    );
    push_checkbox(
        &mut f,
        "telephoneVisit",
        input.telephone_visit.unwrap_or(current.telephone_visit),
    );
    push_checkbox(
        &mut f,
        "churchPantryVisit",
        input
            .church_pantry_visit
            .unwrap_or(current.church_pantry_visit),
    );

    // --- Visit details ---
    f.push((
        "homeVisitCnt".into(),
        input.home_visit_cnt.clone().unwrap_or_else(|| {
            current
                .home_visit_cnt
                .map_or(String::new(), |v| v.to_string())
        }),
    ));

    push_checkbox(
        &mut f,
        "visitCompleted",
        input.visit_completed.unwrap_or(current.visit_completed),
    );

    f.push((
        "visitAssignedToMemberId".into(),
        input
            .visit_assigned_to_member_id
            .clone()
            .unwrap_or_default(),
    ));
    f.push((
        "visitAssignedToMemberIdSecondary".into(),
        input
            .visit_assigned_to_member_id_secondary
            .clone()
            .unwrap_or_default(),
    ));
    f.push((
        "visitMileageInService".into(),
        input.visit_mileage_in_service.clone().unwrap_or_else(|| {
            current
                .visit_mileage_hrs_in_svc
                .map_or(String::new(), |v| v.to_string())
        }),
    ));
    f.push((
        "visitHoursInService".into(),
        input.visit_hours_in_service.clone().unwrap_or_default(),
    ));
    f.push((
        "visitScheduledDate".into(),
        input
            .visit_scheduled_date
            .clone()
            .unwrap_or_else(|| current.visit_scheduled_date.clone().unwrap_or_default()),
    ));
    f.push((
        "visitScheduledTime".into(),
        input.visit_scheduled_time.clone().unwrap_or_default(),
    ));
    f.push((
        "peopleHelpedOverride".into(),
        input.people_helped_override.clone().unwrap_or_else(|| {
            current
                .people_helped_override
                .map_or(String::new(), |v| v.to_string())
        }),
    ));
    f.push((
        "visitNotes".into(),
        input
            .visit_notes
            .clone()
            .unwrap_or_else(|| current.visit_notes.clone()),
    ));
    f.push(("files".into(), String::new()));

    // --- Referral ---
    push_checkbox(
        &mut f,
        "referredToAgency",
        input
            .referred_to_agency
            .unwrap_or(current.referred_to_agency),
    );
    push_checkbox(
        &mut f,
        "referredToConference",
        input
            .referred_to_conference
            .unwrap_or(current.referred_to_conference),
    );
    f.push((
        "referredFromOrganizationId".into(),
        input
            .referred_from_organization_id
            .clone()
            .unwrap_or_default(),
    ));
    f.push((
        "referralNote".into(),
        input
            .referral_note
            .clone()
            .unwrap_or_else(|| current.referral_note.clone()),
    ));

    f
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

impl ServWare {
    /// Update an assistance request using read-modify-write.
    ///
    /// 1. Fetches the current state of the request from the server
    /// 2. Merges `input` fields over the current values
    /// 3. POSTs the complete form
    pub async fn update_request(
        &self,
        request_id: u64,
        input: &UpdateRequestInput,
    ) -> anyhow::Result<()> {
        tracing::debug!(request_id, "fetching current request state for update");
        let current = self
            .get_request_by_id(request_id)
            .await
            .context("failed to fetch current request state")?;

        let form = build_update_form(&current, input);
        let url = Self::request_url(request_id);

        tracing::debug!(url, fields = form.len(), "posting request update");

        let response = self
            .client
            .post(&url)
            .form(&form)
            .send()
            .await
            .context("update request POST failed")?;

        let status = response.status();
        tracing::debug!(%status, "update request response");

        if !status.is_success() && !status.is_redirection() {
            anyhow::bail!("update request failed with status {status}");
        }

        tracing::info!(request_id, "request updated successfully");
        Ok(())
    }

    /// Fetch a single open assistance request by ID.
    ///
    /// Internally fetches open requests (large page size) and finds the matching one.
    async fn get_request_by_id(&self, id: u64) -> anyhow::Result<AssistanceRequest> {
        let params = FetchRequestsParams {
            display_length: 1000,
            ..FetchRequestsParams::new_open_asc()
        };

        let response = self
            .fetch_requests(&params)
            .await
            .context("failed to fetch requests for get_request_by_id")?;

        response
            .aa_data
            .into_iter()
            .find(|r| r.id == id)
            .with_context(|| format!("request {id} not found in fetched results"))
    }
}
