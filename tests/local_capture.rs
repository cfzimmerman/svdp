//! Validation against a real ServWare capture.
//!
//! Real pages contain neighbour PII and never enter the repo, so this test is
//! opt-in and reads its input from the environment:
//!
//!   SVDP_LOCAL_DETAIL_HTML=/path/to/detail.html cargo test --test local_capture -- --ignored --nocapture
//!
//! It asserts structural properties only and prints no field values.

use svdp::servware::form::Form;

#[test]
#[ignore = "requires a local ServWare capture; see module docs"]
fn real_detail_page_round_trips() {
    let path = match std::env::var("SVDP_LOCAL_DETAIL_HTML") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SVDP_LOCAL_DETAIL_HTML not set; nothing to validate");
            return;
        }
    };
    let html = std::fs::read_to_string(&path).expect("capture readable");
    // Located by contained controls, exactly as the real code does -- no
    // fallback, so a locator regression fails here rather than hiding.
    let form = Form::extract_containing(&html, &["status", "visitNotes", "visitCompleted"])
        .expect("the request edit form is present");

    println!("  controls extracted: {}", form.pairs().len());

    // Identity: the property that makes updates safe.
    let same = form.overlay(std::iter::empty()).unwrap();
    assert_eq!(form.pairs(), same.pairs(), "empty overlay must be identity");

    // Every field the real update path overlays must exist, or we would be
    // silently writing into a form that no longer has it.
    for field in [
        "status",
        "requestAssignedToMemberId",
        "visitAssignedToMemberId",
        "visitCompleted",
        "homeVisitRequired",
        "homeVisitCnt",
        "visitMileageInService",
        "visitScheduledDate",
        "visitNotes",
    ] {
        assert!(form.contains(field), "live form is missing overlay target `{field}`");
    }

    // The fields the old builder dropped must be present and preserved.
    let dropped = [
        "otherVisitCnt", "eldercareVisitCnt", "hospitalVisitCnt",
        "prisonVisitCnt", "phoneVisitCnt", "churchPantryVisitCnt",
        "referralOrganizationId", "referralConference",
    ];
    let found = dropped.iter().filter(|f| form.contains(f)).count();
    println!("  previously-dropped fields now preserved: {found}/{}", dropped.len());
    assert!(found >= 6, "expected the live form to carry the visit-count fields");

    // A realistic mark-complete must touch only what it names.
    let after = form
        .overlay([
            ("status", "Completed".to_string()),
            ("visitCompleted", "true".to_string()),
            ("visitAssignedToMemberId", "44270".to_string()),
        ])
        .unwrap();
    let changed: Vec<String> = form.diff(&after).into_iter().map(|(n, _, _)| n).collect();
    println!("  fields changed by mark-complete: {changed:?}");
    assert!(changed.len() <= 3, "overlay must not disturb other fields");
}

/// Detail parsing against a real page: volunteers and assistance items.
/// Structure and counts only -- no names, amounts, or dates are printed.
#[test]
#[ignore = "requires a local ServWare capture; see module docs"]
fn real_detail_page_parses() {
    let Ok(path) = std::env::var("SVDP_LOCAL_DETAIL_HTML") else {
        eprintln!("SVDP_LOCAL_DETAIL_HTML not set; nothing to validate");
        return;
    };
    let html = std::fs::read_to_string(&path).expect("capture readable");
    let d = svdp::servware::detail::parse(0, &html).expect("detail page parses");

    println!("  status parsed         : {}", d.status());
    println!("  volunteers found      : {}", d.members.len());
    println!("  assistance items found: {}", d.assistance_items.len());

    assert!(!d.status().is_empty(), "status must come off the form");
    assert!(d.members.len() > 1, "the volunteer dropdown should have real entries");
    assert!(
        d.members.iter().all(|m| !m.id.is_empty() && !m.name.is_empty()),
        "no blank member entries"
    );
    assert!(
        d.assistance_items.iter().all(|i| !i.kind.is_empty()),
        "every parsed item must have a kind"
    );
    // Amounts must parse as money once the $ is stripped.
    for item in &d.assistance_items {
        assert!(
            item.value.replace(',', "").parse::<f64>().is_ok(),
            "assistance value did not parse as a number"
        );
        assert!(
            !item.date_provided.is_empty(),
            "assistance item is missing its date"
        );
    }
}

/// Cross-check the assistance-item golden against a real captured POST.
///
///   SVDP_LOCAL_HAR=/path/to/servware.har cargo test --test local_capture -- --ignored --nocapture
///
/// Compares field names and order only; no captured values are read or printed.
#[test]
#[ignore = "requires a local ServWare HAR capture"]
fn golden_assistance_form_matches_a_real_capture() {
    let Ok(path) = std::env::var("SVDP_LOCAL_HAR") else {
        eprintln!("SVDP_LOCAL_HAR not set; nothing to cross-check");
        return;
    };
    let har: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("har readable"))
            .expect("har parses");

    let captured: Vec<String> = har["log"]["entries"]
        .as_array()
        .expect("har entries")
        .iter()
        .find(|e| {
            e["request"]["url"]
                .as_str()
                .is_some_and(|u| u.contains("assistanceitems/new"))
                && e["request"]["method"] == "POST"
        })
        .and_then(|e| e["request"]["postData"]["params"].as_array())
        .map(|params| {
            params
                .iter()
                .filter_map(|p| p["name"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    if captured.is_empty() {
        eprintln!("  no assistanceitems/new POST in this capture; skipping");
        return;
    }

    let ours: Vec<String> = svdp::servware::write::assistance_form("16542", 1, 70, "09/05/2026", "")
        .into_iter()
        .map(|(n, _)| n)
        .collect();

    println!("  captured fields: {}", captured.len());
    println!("  our fields     : {}", ours.len());
    assert_eq!(ours, captured, "our POST must match the browser field-for-field");
}
