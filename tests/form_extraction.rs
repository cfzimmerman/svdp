//! Form extraction behaviour, pinned against synthetic fixtures.
//!
//! These fixtures mirror the control names and types of the live ServWare
//! request edit page. Every value in them is invented.

use svdp::servware::form::Form;
use svdp::servware::form::FormError;

const OPEN: &str = include_str!("fixtures/detail_open.html");
const COMPLETED: &str = include_str!("fixtures/detail_completed.html");

fn open_form() -> Form {
    Form::extract(OPEN, "form#editForm").expect("fixture has the request form")
}

/// The single highest-value test: an empty overlay must reproduce the form
/// byte-for-byte. If this holds, we provably preserve every field we don't
/// intend to touch -- which is the bug that silently cleared 11 live fields.
#[test]
fn empty_overlay_is_identity() {
    let f = open_form();
    let same = f.overlay(std::iter::empty()).unwrap();
    assert_eq!(f.pairs(), same.pairs());
    assert!(f.diff(&same).is_empty(), "identity overlay must not diff");
}

#[test]
fn extracts_every_named_control_in_document_order() {
    let f = open_form();
    let pairs = f.pairs();
    assert_eq!(pairs.first().map(|(n, _)| n.as_str()), Some("status"));
    // Spring pairs: companion always present, control only when checked.
    assert_eq!(f.get("homeVisitRequired"), Some("true"));
    assert_eq!(f.get("_homeVisitRequired"), Some("on"));
    assert!(f.contains("otherVisit"), "unchecked checkbox is still rendered");
    assert_eq!(f.get("otherVisit"), None, "but is not submitted");
    assert_eq!(f.get("_otherVisit"), Some("on"), "companion is always submitted");
}

/// The fields the previous hand-written builder dropped entirely.
#[test]
fn preserves_fields_the_old_builder_silently_cleared() {
    let f = open_form();
    for (name, expected) in [
        ("otherVisitCnt", "3"),
        ("eldercareVisitCnt", "4"),
        ("hospitalVisitCnt", "5"),
        ("prisonVisitCnt", "6"),
        ("phoneVisitCnt", "7"),
        ("churchPantryVisitCnt", "8"),
        ("referralOrganizationId", "900"),
        ("referralConference", "17"),
    ] {
        assert_eq!(f.get(name), Some(expected), "{name} must survive extraction");
    }
}

#[test]
fn skips_disabled_controls_and_submit_buttons() {
    let f = open_form();
    assert!(!f.contains("ignoredBecauseDisabled"));
    assert!(f.pairs().iter().all(|(n, _)| n != "action"));
}

/// Extraction is scoped to one form; a modal elsewhere on the page is not ours.
#[test]
fn ignores_other_forms_on_the_page() {
    let f = open_form();
    assert!(!f.contains("sendToRoleId"));
    assert!(!f.contains("clientMapBoundaryScope"));
    let modal = Form::extract(OPEN, "form#sendEmailForm").unwrap();
    assert_eq!(modal.get("sendToRoleId"), Some("3"));
}

#[test]
fn select_uses_selected_option_then_falls_back_to_first() {
    let f = open_form();
    assert_eq!(f.get("status"), Some("Open"), "explicitly selected");
    assert_eq!(f.get("requestAssignedToMemberId"), Some(""), "first option when none selected");
    let c = Form::extract(COMPLETED, "form#editForm").unwrap();
    assert_eq!(c.get("status"), Some("Completed"));
    assert_eq!(c.get("visitAssignedToMemberId"), Some("44270"));
}

/// The canary: overlaying a field ServWare no longer renders must fail loudly,
/// not append a parameter while the real field keeps its old value.
#[test]
fn overlay_rejects_unknown_field() {
    let err = open_form()
        .overlay([("fieldServwareRenamed", "x".to_string())])
        .unwrap_err();
    assert!(matches!(err, FormError::UnknownField(f) if f == "fieldServwareRenamed"));
}

/// Marking complete must change exactly the intended fields and nothing else.
#[test]
fn overlay_changes_only_intended_fields() {
    let before = open_form();
    let after = before
        .overlay([
            ("status", "Completed".to_string()),
            ("visitAssignedToMemberId", "44270".to_string()),
            ("visitScheduledDate", "09/05/2026".to_string()),
        ])
        .unwrap();

    let mut diff = before.diff(&after);
    diff.sort();
    let changed: Vec<&str> = diff.iter().map(|(n, _, _)| n.as_str()).collect();
    assert_eq!(changed, ["status", "visitAssignedToMemberId", "visitScheduledDate"]);
    assert_eq!(before.pairs().len(), after.pairs().len(), "no field added or lost");
    assert_eq!(after.get("otherVisitCnt"), Some("3"), "untouched field intact");
}

/// An unchecked checkbox is present in the DOM but not submitted. Marking a
/// visit complete must be able to turn it on -- and that must not be confused
/// with ServWare having removed the field.
#[test]
fn overlay_can_check_an_unchecked_box() {
    let before = open_form();
    assert!(before.contains("visitCompleted"), "control is rendered");
    assert_eq!(before.get("visitCompleted"), None, "but not submitted while unchecked");

    let after = before.overlay([("visitCompleted", "true".to_string())]).unwrap();
    assert_eq!(after.get("visitCompleted"), Some("true"));
    assert_eq!(after.get("_visitCompleted"), Some("on"), "companion still submitted");
    assert_eq!(
        before.diff(&after),
        vec![("visitCompleted".to_string(), String::new(), "true".to_string())]
    );
}

/// And it must be able to turn one off.
#[test]
fn overlay_can_uncheck_a_checked_box() {
    let before = open_form();
    assert_eq!(before.get("homeVisitRequired"), Some("true"));
    let after = before.overlay([("homeVisitRequired", "false".to_string())]).unwrap();
    assert_eq!(after.get("homeVisitRequired"), None, "unchecked means not submitted");
}
