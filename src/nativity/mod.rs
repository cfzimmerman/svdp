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

use anyhow::Context;
use serde::Deserialize;
use serde::Serialize;

use crate::api::ServWare;
use crate::api::fetch_requests::FetchRequestsParams;
use crate::api::update_assistance::UpdateAssistanceInput;
use crate::api::update_request::UpdateRequestInput;

// ---------------------------------------------------------------------------
// Nativity constants
// ---------------------------------------------------------------------------

const VISIT_MILEAGE: &str = "5";
const VISIT_NOTES: &str = "<p>Delivered food and gift cards</p>";

/// ServWare assistance type ID for Second Harvest food.
const SECOND_HARVEST_TYPE_ID: &str = "16542";
/// Monetary value recorded for a Second Harvest food delivery.
const SECOND_HARVEST_VALUE: &str = "70";

/// ServWare assistance type ID for gift cards.
const GIFT_CARD_TYPE_ID: &str = "16522";

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
    pub gift_card_dollars: u32,

    // Fields from `Client`
    pub neighbor_id: u64,
    pub neighbor_first_name: String,
    pub neighbor_last_name: String,
    pub neighbor_last_request_date: String,
}

fn gift_card_dollars(family_size: u32) -> u32 {
    match family_size {
        0 | 1 => 50,
        2 => 60,
        3 => 70,
        4 => 80,
        5 => 90,
        _ => 100,
    }
}

// ---------------------------------------------------------------------------
// Public functions
// ---------------------------------------------------------------------------

pub async fn members_to_csv(client: &ServWare, csv: &Path) -> anyhow::Result<()> {
    let mut writer = csv::Writer::from_path(csv)?;

    // Find an arbitrary request from which to scrape volunteer names.
    let reqs = client
        .fetch_requests(&FetchRequestsParams::new_open_asc())
        .await
        .context("failed to fetch open requests")?;

    let first = reqs
        .aa_data
        .first()
        .context("no open requests found to scrape member list from")?;
    let request_id = first.id;

    for mber in client.fetch_members(request_id).await? {
        println!("{mber:?}");
        writer.serialize(&mber)?;
    }

    Ok(())
}

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
            gift_card_dollars: self::gift_card_dollars(req.calculated_household_count),

            neighbor_id: req.client.id,
            neighbor_first_name: req.client.first_name,
            neighbor_last_name: req.client.last_name,
            neighbor_last_request_date: req.client.last_request_date,
        };

        writer.serialize(open)?;
        tracing::info!("wrote request id: {}", req.id);
    }

    Ok(())
}

/// Updates ServWare to mark every request in the CSV as complete,
/// assigning the given volunteer and visit date.
pub async fn update_complete(client: &ServWare, csv: &Path, member_id: &str) -> anyhow::Result<()> {
    let visit_date = chrono::Local::now().format("%m/%d/%Y").to_string();
    tracing::info!("using visit date: {visit_date}");

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

        tracing::info!("updating request: {row:?}");
        client.update_request(row.req_id, &update).await?;
        tracing::info!("marked request {} complete", row.req_id);
    }

    Ok(())
}

/// Adds two assistance items (Second Harvest food + gift cards) to every
/// request in the CSV.
pub async fn add_assistance(client: &ServWare, csv: &Path) -> anyhow::Result<()> {
    let date_provided = chrono::Local::now().format("%m/%d/%Y").to_string();
    println!("using date provided: {date_provided}");

    let mut reader = csv::Reader::from_path(csv)?;
    for row in reader.deserialize() {
        let row: OpenRequest = row?;
        let client_id = row.neighbor_id.to_string();

        // 1. Second Harvest food
        let second_harvest = UpdateAssistanceInput::new(
            SECOND_HARVEST_TYPE_ID,
            &client_id,
            SECOND_HARVEST_VALUE,
            "1",
            &date_provided,
        );
        client
            .update_assistance(row.req_id, &second_harvest)
            .await?;
        tracing::info!(
            "  request {}: added Second Harvest (${SECOND_HARVEST_VALUE})",
            row.req_id
        );

        // 2. Gift cards
        let gift_cards = UpdateAssistanceInput::new(
            GIFT_CARD_TYPE_ID,
            &client_id,
            row.gift_card_dollars.to_string(),
            "1",
            &date_provided,
        );
        client.update_assistance(row.req_id, &gift_cards).await?;
        tracing::info!(
            "  request {}: added gift cards (${})",
            row.req_id,
            row.gift_card_dollars
        );
    }

    Ok(())
}
