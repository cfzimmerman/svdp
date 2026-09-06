//! Gift-card policy. Real money, so the ladder is pinned exhaustively.

use svdp::domain::policy::ConferenceConfig;
use svdp::domain::policy::Slot;

#[test]
fn gift_card_ladder_matches_conference_practice() {
    let c = ConferenceConfig::default();
    // Household size -> dollars. Floor $50, ceiling $100, confirmed against a
    // real capture (a household of 5 was given $90).
    for (size, expected) in [
        (0, 50), (1, 50), (2, 60), (3, 70), (4, 80), (5, 90),
        (6, 100), (7, 100), (12, 100), (100, 100),
    ] {
        assert_eq!(c.gift_card_dollars(size), expected, "household of {size}");
    }
}

#[test]
fn ladder_is_monotonic_and_bounded() {
    let c = ConferenceConfig::default();
    let mut previous = 0;
    for size in 0..=40 {
        let d = c.gift_card_dollars(size);
        assert!(d >= previous, "ladder must never decrease (size {size})");
        assert!((50..=100).contains(&d), "size {size} gave ${d}, outside $50..=$100");
        previous = d;
    }
}

/// A short ladder must still clamp rather than panic or return zero.
#[test]
#[allow(clippy::field_reassign_with_default)] // start from real policy, vary one field
fn oversized_household_clamps_to_the_last_rung() {
    let mut c = ConferenceConfig::default();
    c.gift_card_ladder = vec![25, 35];
    assert_eq!(c.gift_card_dollars(1), 25);
    assert_eq!(c.gift_card_dollars(2), 35);
    assert_eq!(c.gift_card_dollars(99), 35);
}

#[test]
fn tags_are_stable_and_slot_specific() {
    let c = ConferenceConfig::default();
    assert_eq!(c.tag("01JBQ", Slot::Food), "svdp:s=01JBQ;i=food");
    assert_eq!(c.tag("01JBQ", Slot::GiftCard), "svdp:s=01JBQ;i=giftcard");
    assert_ne!(c.tag("01JBQ", Slot::Food), c.tag("01JBQ", Slot::GiftCard));
    assert_ne!(c.tag("A", Slot::Food), c.tag("B", Slot::Food));
}

/// The tag must survive the round trip through ServWare's notes field.
#[test]
fn item_notes_round_trip_through_tag_extraction() {
    let c = ConferenceConfig::default();
    let notes = c.item_notes("01JBQ", Slot::Food, "09/05/2026");
    assert!(notes.contains("SVdP delivery"), "must stay readable to a caseworker");
    assert_eq!(
        svdp::servware::detail::extract_tag(&notes),
        Some(c.tag("01JBQ", Slot::Food))
    );
}

#[test]
#[allow(clippy::field_reassign_with_default)] // start from real policy, vary one field
fn tagging_can_be_disabled() {
    let mut c = ConferenceConfig::default();
    c.tag_assistance_notes = false;
    assert_eq!(c.item_notes("01JBQ", Slot::Food, "09/05/2026"), "");
}

/// The config is compiled in with `include_str!`, so a malformed edit would
/// panic every binary at first use. Fail here instead.
#[test]
fn embedded_conference_config_parses_and_matches_practice() {
    let c = ConferenceConfig::default(); // parses EMBEDDED or panics
    assert_eq!(c.second_harvest.id, "16542");
    assert_eq!(c.second_harvest.value, Some(70));
    assert_eq!(c.gift_card.id, "16522");
    assert_eq!(c.gift_card.value, None, "gift cards scale with household size");
    assert_eq!(c.gift_card_ladder, vec![50, 60, 70, 80, 90, 100]);
    assert!(c.tag_assistance_notes, "idempotency depends on the tag");
}

/// An external file overrides the embedded policy; a malformed one must not.
#[test]
fn external_config_overrides_but_malformed_is_ignored() {
    let dir = tempfile::tempdir().unwrap();

    let good = dir.path().join("good.toml");
    std::fs::write(&good, r#"
gift_card_ladder = [10, 20]
visit_mileage = "9"
visit_notes_html = "<p>x</p>"
tag_assistance_notes = false
[second_harvest]
id = "1"
name_contains = "Food"
value = 5
[gift_card]
id = "2"
name_contains = "Card"
"#).unwrap();
    unsafe { std::env::set_var("SVDP_CONFERENCE_CONFIG", &good) };
    let c = ConferenceConfig::load();
    assert_eq!(c.second_harvest.id, "1", "external config must win");
    assert_eq!(c.gift_card_dollars(9), 20);

    let bad = dir.path().join("bad.toml");
    std::fs::write(&bad, "this is not = valid [toml").unwrap();
    unsafe { std::env::set_var("SVDP_CONFERENCE_CONFIG", &bad) };
    let c = ConferenceConfig::load();
    assert_eq!(c.second_harvest.id, "16542", "malformed override must fall back, not half-apply");

    unsafe { std::env::remove_var("SVDP_CONFERENCE_CONFIG") };
}
