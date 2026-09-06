//! Detail-page parsing: volunteers, assistance items, and the idempotency tag.

use svdp::servware::detail;

const OPEN: &str = include_str!("fixtures/detail_open.html");
const COMPLETED: &str = include_str!("fixtures/detail_completed.html");

#[test]
fn reads_status_from_the_form() {
    assert_eq!(detail::parse(1, OPEN).unwrap().status(), "Open");
    let done = detail::parse(1, COMPLETED).unwrap();
    assert_eq!(done.status(), "Completed");
    assert!(done.is_completed());
}

/// The member list comes off the detail page, so it works even when there are
/// no open requests -- the old scrape needed an arbitrary open request to exist.
#[test]
fn reads_volunteers_from_the_assignment_dropdown() {
    let d = detail::parse(1, OPEN).unwrap();
    let names: Vec<&str> = d.members.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["Ada Lovelace", "Grace Hopper"]);
    assert_eq!(d.members[0].id, "44270");
    assert!(
        !d.members.iter().any(|m| m.id.is_empty()),
        "the '-- Select --' placeholder must be skipped"
    );
}

/// Located by header text: the real table has no id, only Bootstrap classes.
#[test]
fn reads_server_rendered_assistance_items() {
    assert!(
        detail::parse(1, OPEN).unwrap().assistance_items.is_empty(),
        "an open request has nothing logged yet"
    );

    let d = detail::parse(1, COMPLETED).unwrap();
    assert_eq!(d.assistance_items.len(), 3);
    let food = &d.assistance_items[0];
    assert_eq!(food.kind, "Second Harvest Food");
    assert_eq!(food.value, "70.00", "the $ prefix is stripped");
    assert_eq!(food.date_provided, "09/05/2026");
}

/// The tag is the only exact idempotency key; see DECISIONS.md D7.
#[test]
fn recognises_its_own_items_by_tag() {
    let d = detail::parse(1, COMPLETED).unwrap();
    assert!(d.has_item_tagged("svdp:s=01JBQTESTSESSION;i=food"));
    assert!(d.has_item_tagged("svdp:s=01JBQTESTSESSION;i=giftcard"));
    assert!(
        !d.has_item_tagged("svdp:s=01JBQOTHERSESSION;i=food"),
        "a different session's tag must not count as already done"
    );
    // An untagged historical item carries no tag at all.
    assert_eq!(d.assistance_items[2].tag, None);
}

/// A legitimate re-delivery in a later month must not look like a duplicate.
#[test]
fn same_kind_on_a_different_date_is_not_a_duplicate() {
    let d = detail::parse(1, COMPLETED).unwrap();
    assert_eq!(d.items_like("Gift Cards", "09/05/2026").len(), 1);
    assert_eq!(d.items_like("Gift Cards", "01/03/2026").len(), 1);
    assert_eq!(d.items_like("Gift Cards", "12/25/2026").len(), 0);
}

#[test]
fn tag_extraction_tolerates_trailing_human_text() {
    assert_eq!(
        detail::extract_tag("svdp:s=abc;i=food — SVdP delivery 2026-09-05"),
        Some("svdp:s=abc;i=food".to_string())
    );
    assert_eq!(detail::extract_tag("svdp:s=abc;i=food"), Some("svdp:s=abc;i=food".to_string()));
    assert_eq!(detail::extract_tag("groceries for the family"), None);
    assert_eq!(detail::extract_tag(""), None);
}
