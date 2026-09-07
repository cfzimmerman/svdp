//! Form extraction behaviour, pinned against synthetic fixtures.
//!
//! These fixtures mirror the control names and types of the live ServWare
//! request edit page. Every value in them is invented.

use svdp::servware::form::Form;
use svdp::servware::form::FormError;

const OPEN: &str = include_str!("fixtures/detail_open.html");
const COMPLETED: &str = include_str!("fixtures/detail_completed.html");
const NESTED: &str = include_str!("fixtures/detail_nested_modal.html");

/// Located exactly as production locates it. Tests used to call `Form::extract`
/// with a CSS selector, which production never used -- so the form-scoping tests
/// were exercising a code path that did not ship, and could not have caught the
/// nested-form defect below.
fn open_form() -> Form {
    Form::extract_containing(OPEN, svdp::servware::write::COMPLETION_FIELDS)
        .expect("fixture has the request form")
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
    let modal = Form::extract_containing(OPEN, &["sendToRoleId"]).unwrap();
    assert_eq!(modal.get("sendToRoleId"), Some("3"));
    assert!(!modal.contains("status"), "the modal is not the edit form");
}

/// The scoping test above is only meaningful while the modals are *siblings* of
/// the edit form. Nested, the HTML5 parser destroys the distinction before any
/// of our code runs: the inner `<form>` start tag is dropped, the first
/// `</form>` closes the OUTER form, and the result both absorbs the modal's
/// controls and loses every real control after it.
///
/// Reproduced against this repo's own `scraper` build. Extraction must refuse.
#[test]
fn a_nested_form_is_refused_rather_than_silently_merged() {
    let err = Form::extract_containing(NESTED, &["status", "visitNotes", "visitCompleted"])
        .expect_err("a nested form must not parse into a trustworthy result");
    assert!(
        matches!(err, FormError::NestedForms { .. }),
        "expected NestedForms, got {err:?}"
    );

    // Demonstrating what the refusal prevents: without the canary the parser
    // hands back a form that has swallowed the modal and dropped a real field.
    let doc = scraper::Html::parse_document(NESTED);
    let sel = scraper::Selector::parse("form").unwrap();
    assert_eq!(doc.select(&sel).count(), 1, "the inner <form> start tag is dropped");
    let merged: Vec<String> = doc
        .select(&sel)
        .next()
        .unwrap()
        .select(&scraper::Selector::parse("input, select, textarea").unwrap())
        .filter_map(|e| e.value().attr("name").map(str::to_string))
        .collect();
    assert!(merged.contains(&"sendToRoleId".to_string()), "modal control absorbed");
    assert!(
        !merged.contains(&"controlAfterTheModal".to_string()),
        "a real control after the modal is lost"
    );
}

/// `count_form_start_tags` must not be fooled by markup-shaped text that the
/// parser would never turn into an element, or the canary fires on a good page
/// and the tool stops working for a reason nobody can find.
#[test]
fn the_nesting_canary_ignores_form_tags_in_comments_and_scripts() {
    let page = r#"<html><body>
      <!-- <form id="commented-out"><input name="ghost"/></form> -->
      <script>var s = "<form>"; document.write("<form id=x>");</script>
      <style>/* <form> */</style>
      <form id="real"><input name="status" value="Open"/>
        <textarea name="visitNotes"></textarea>
        <input type="checkbox" name="visitCompleted" value="true"/>
      </form>
    </body></html>"#;
    let f = Form::extract_containing(page, &["status", "visitNotes", "visitCompleted"])
        .expect("comments and scripts are not forms");
    assert_eq!(f.get("status"), Some("Open"));
    assert!(!f.contains("ghost"));
}

#[test]
fn select_uses_selected_option_then_falls_back_to_first() {
    let f = open_form();
    assert_eq!(f.get("status"), Some("Open"), "explicitly selected");
    assert_eq!(f.get("requestAssignedToMemberId"), Some(""), "first option when none selected");
    let c = Form::extract_containing(COMPLETED, svdp::servware::write::COMPLETION_FIELDS).unwrap();
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

/// The county's intake assignment must survive a completion.
///
/// `requestAssignedToMemberId` records who at the county took the request. The
/// delivery volunteer belongs in the *visit* assignment. Overwriting the intake
/// field destroys the county's record, so it is claimed only when empty.
#[test]
fn intake_assignment_is_claimed_only_when_empty() {
    let unassigned = open_form();
    assert_eq!(unassigned.get("requestAssignedToMemberId"), Some(""));

    let assigned = Form::extract_containing(
        include_str!("fixtures/detail_open_assigned.html"),
        svdp::servware::write::COMPLETION_FIELDS,
    )
    .unwrap();
    assert_eq!(
        assigned.get("requestAssignedToMemberId"),
        Some("44271"),
        "fixture must model a request the county already assigned"
    );

    // Completion overlays the visit assignment but leaves intake untouched.
    let after = assigned
        .overlay([
            ("status", "Completed".to_string()),
            ("visitAssignedToMemberId", "44270".to_string()),
        ])
        .unwrap();
    assert_eq!(
        after.get("requestAssignedToMemberId"),
        Some("44271"),
        "the county's intake assignment must be preserved"
    );
    assert_eq!(after.get("visitAssignedToMemberId"), Some("44270"));
}


/// A radio group is selected BY the requested value. The previous overlay kept
/// whatever value the *first* button in the group declared, so asking for `B`
/// submitted `A` -- a wrong write with no error anywhere.
#[test]
fn overlaying_a_radio_group_selects_the_button_that_declares_the_value() {
    let page = r#"<html><body><form>
        <input type="radio" name="mode" value="A"/>
        <input type="radio" name="mode" value="B" checked/>
        <input type="radio" name="mode" value="C"/>
        <input type="hidden" name="marker" value="1"/>
      </form></body></html>"#;
    let f = Form::extract_containing(page, &["mode", "marker"]).unwrap();
    assert_eq!(f.get("mode"), Some("B"), "the checked button is the one submitted");

    for want in ["A", "B", "C"] {
        let after = f.overlay([("mode", want.to_string())]).unwrap();
        assert_eq!(after.get("mode"), Some(want), "overlay must select {want}");
        assert_eq!(
            after.pairs().iter().filter(|(n, _)| n == "mode").count(),
            1,
            "exactly one button in a group submits"
        );
    }

    let err = f.overlay([("mode", "Z".to_string())]).unwrap_err();
    assert!(
        matches!(err, FormError::UnknownValue { ref name, ref value } if name == "mode" && value == "Z"),
        "a value no button declares must be an error, not a silent fallback: {err:?}"
    );
}

/// The falsey set was `"" | "false" | "off"`, so `"0"` and `"no"` turned a
/// checkbox ON -- the opposite of what the caller asked for.
#[test]
fn falsey_overlay_values_uncheck_a_box() {
    let before = open_form();
    assert_eq!(before.get("homeVisitRequired"), Some("true"), "starts checked");
    for falsey in ["false", "off", "0", "no", "", "FALSE", " off "] {
        let after = before.overlay([("homeVisitRequired", falsey.to_string())]).unwrap();
        assert_eq!(after.get("homeVisitRequired"), None, "{falsey:?} must uncheck");
    }
    for truthy in ["true", "on", "1", "yes"] {
        let after = before.overlay([("homeVisitRequired", truthy.to_string())]).unwrap();
        assert_eq!(after.get("homeVisitRequired"), Some("true"), "{truthy:?} must check");
    }
}

/// `diff` compares the whole multiset of submitted pairs. Resolving by first
/// occurrence invented differences for repeated names and hid real ones -- on
/// the screen a volunteer approves before the only irreversible step.
#[test]
fn diff_sees_repeated_names_rather_than_only_the_first() {
    let page = r#"<html><body><form>
        <select name="tag" multiple>
          <option value="x" selected>x</option>
          <option value="y" selected>y</option>
        </select>
        <input type="hidden" name="marker" value="1"/>
      </form></body></html>"#;
    let f = Form::extract_containing(page, &["tag", "marker"]).unwrap();
    assert_eq!(f.pairs().iter().filter(|(n, _)| n == "tag").count(), 2);

    // Collapsing two submitted values to one IS a change, and must show up.
    let after = f.overlay([("tag", "x".to_string())]).unwrap();
    let diff = f.diff(&after);
    assert_eq!(diff.len(), 1, "the lost second value must appear: {diff:?}");
    assert_eq!(diff[0].0, "tag");
    assert_eq!(diff[0].1, "x, y");
    assert_eq!(diff[0].2, "x");

    // And identity still diffs to nothing.
    assert!(f.diff(&f.overlay(std::iter::empty()).unwrap()).is_empty());
}
