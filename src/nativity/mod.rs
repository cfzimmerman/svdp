//! # Nativity
//!
//! The api module makes at least some attempt at being
//! generic. This module hosts specific tools to simplify
//! the SVdP workflow at Nativity.

/*

Plan:
- Fetch all requests and write their relevant info into a CSV
    - Get number in family. Attach dollar value to donation.
- Not now, eventually: plan a route
- Read the CSV. Mark complete.
- Add assistance to completed requests.

*/

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::api::{
    ServWare, fetch_requests::FetchRequestsParams, update_request::UpdateRequestInput,
};

// ---------------------------------------------------------------------------
// Nativity constants
// ---------------------------------------------------------------------------

const VISIT_MILEAGE: &str = "5";
const VISIT_NOTES: &str = "<p>Delivered food and gift cards</p>";

// ---------------------------------------------------------------------------
// CSV row type
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenRequest {
    // Fields from `AssistanceRequest`.
    pub req_id: u64,
    pub req_date_created: String,
    pub req_status: String,
    pub req_calculated_household_count: u32,

    // Field(s) that don't map to any specific ServWare entry.
    pub merged_address: String,

    // Fields from `Client`
    pub neighbor_id: u64,
    pub neighbor_first_name: String,
    pub neighbor_last_name: String,
    pub neighbor_last_request_date: String,
}

// ---------------------------------------------------------------------------
// Public functions
// ---------------------------------------------------------------------------

/// Fetches all open requests and writes them to a (truncated)
/// csv at the given path.
pub async fn requests_to_csv(client: &ServWare, csv: &Path) -> anyhow::Result<()> {
    let mut writer = csv::Writer::from_path(csv)?;

    let reqs = client
        .fetch_requests(&FetchRequestsParams::new_open_asc())
        .await?;
    for req in reqs.aa_data {
        let open = OpenRequest {
            req_id: req.id,
            req_date_created: req.date_created,
            req_status: req.status,
            req_calculated_household_count: req.calculated_household_count,

            merged_address: format!(
                "{} {}, {}, {}",
                req.street_address_line1, req.street_address_line2, req.city, req.state_code
            ),

            neighbor_id: req.client.id,
            neighbor_first_name: req.client.first_name,
            neighbor_last_name: req.client.last_name,
            neighbor_last_request_date: req.client.last_request_date,
        };

        writer.serialize(open)?;
        println!("wrote request id: {}", req.id);
    }

    Ok(())
}

/// Updates ServWare to mark every request in the CSV as complete,
/// assigning the given volunteer and visit date.
pub async fn mark_csv_complete(
    client: &ServWare,
    csv: &Path,
    member_id: &str,
) -> anyhow::Result<()> {
    let visit_date = chrono::Local::now().format("%m/%d/%Y").to_string();
    println!("using visit date: {visit_date}");

    let mut reader = csv::Reader::from_path(csv)?;
    for row in reader.deserialize() {
        let row: OpenRequest = row?;
        let update = UpdateRequestInput {
            status: Some("Completed".to_string()),
            request_assigned_to_member_id: Some(member_id.to_string()),
            home_visit_required: Some(true),
            home_visit_cnt: Some("1".to_string()),
            visit_completed: Some(true),
            visit_assigned_to_member_id: Some(member_id.to_string()),
            visit_mileage_in_service: Some(VISIT_MILEAGE.to_string()),
            visit_scheduled_date: Some(visit_date.to_string()),
            visit_notes: Some(VISIT_NOTES.to_string()),
            ..Default::default()
        };
        client.update_request(row.req_id, &update).await?;
        println!("marked request {} complete", row.req_id);
    }
    Ok(())
}
