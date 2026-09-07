//! The assistance-item POST, pinned against a captured browser request.
//!
//! Field *names* and their order are protocol facts, not personal data, so the
//! expected sequence is committed. `api.md` lists only 13 of these fields; the
//! real browser sends 30. Where the doc and a capture disagree, the capture
//! wins. See DECISIONS.md D9.

use svdp::domain::policy::ConferenceConfig;
use svdp::domain::policy::Slot;
use svdp::servware::write::assistance_form;

/// Exactly what the browser sends, in order.
const CAPTURED_FIELD_ORDER: &[&str] = &[
    "assistanceTypeId", "clientId", "housingProviderId", "vendorId", "utilityId",
    "clientAccountId", "clientAccountName", "clientAccountNumber", "clientAccountHolder",
    "specialProgramId", "inKindSubType", "monetaryValue", "accountId", "quantity",
    "dateProvided", "voucherAsstId", "_pending", "promisedDate", "_checkRequested",
    "datePaid", "checkNumber", "payeeName", "notes", "councilPaymentValue",
    "councilCheckConfNumber", "districtPaymentValue", "districtCheckConfNumber",
    "otherPaymentValue", "otherCheckConfNumber", "action",
];

#[test]
fn matches_the_captured_browser_field_order() {
    let form = assistance_form("16542", 580815, 70, "09/05/2026", "");
    let names: Vec<&str> = form.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, CAPTURED_FIELD_ORDER);
    assert_eq!(form.len(), 30, "api.md's 13-field example is abridged");
}

#[test]
fn carries_the_values_that_matter() {
    let form = assistance_form("16542", 580815, 70, "09/05/2026", "note text");
    let get = |k: &str| form.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
    assert_eq!(get("assistanceTypeId"), Some("16542"));
    assert_eq!(get("clientId"), Some("580815"));
    assert_eq!(get("monetaryValue"), Some("70"));
    assert_eq!(get("quantity"), Some("1"));
    assert_eq!(get("dateProvided"), Some("09/05/2026"));
    assert_eq!(get("notes"), Some("note text"));
    assert_eq!(get("action"), Some("save"));
}

/// Spring's convention: a companion without its control means *unchecked*.
/// Dropping these would submit a different form than the browser does.
#[test]
fn checkbox_companions_are_present_without_their_controls() {
    let form = assistance_form("16542", 1, 70, "09/05/2026", "");
    let names: Vec<&str> = form.iter().map(|(n, _)| n.as_str()).collect();
    for companion in ["_pending", "_checkRequested"] {
        assert!(names.contains(&companion), "{companion} must be sent");
        assert!(
            !names.contains(&companion.trim_start_matches('_')),
            "{companion}'s control must NOT be sent, or the box reads as checked"
        );
    }
}

/// The idempotency tag has to survive into the field the server stores.
#[test]
fn tag_travels_in_the_notes_field() {
    let config = ConferenceConfig::default();
    let notes = config.item_notes("01JBQ", Slot::GiftCard, "09/05/2026");
    let form = assistance_form("16522", 1, 90, "09/05/2026", &notes);
    let sent = form.iter().find(|(n, _)| n == "notes").unwrap().1.clone();
    assert_eq!(
        svdp::servware::detail::extract_tag(&sent),
        Some("svdp:s=01JBQ;i=giftcard".to_string())
    );
}

/// Money must never be formatted with a currency symbol or decimals.
#[test]
fn monetary_value_is_a_bare_integer() {
    for dollars in [50, 70, 90, 100] {
        let form = assistance_form("16522", 1, dollars, "09/05/2026", "");
        let v = form.iter().find(|(n, _)| n == "monetaryValue").unwrap().1.clone();
        assert_eq!(v, dollars.to_string());
        assert!(v.parse::<u32>().is_ok());
    }
}
