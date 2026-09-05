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
    let form = Form::extract(&html, "form#requestForm")
        .or_else(|_| Form::extract(&html, "form"))
        .expect("a form is present");

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
    let found = dropped.iter().filter(|f| form.contains(*f)).count();
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
