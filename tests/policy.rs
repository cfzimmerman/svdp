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

/// The gift-card ladder is rejected while it is still text on disk.
///
/// It used to be guarded only by a `debug_assert!`, which is compiled out of
/// the `--release` binary that ships -- so an external config with an empty
/// ladder parsed happily and `gift_card_dollars` fell through to $0 for every
/// household, silently. See DECISIONS.md D34.
#[test]
fn an_empty_gift_card_ladder_is_refused_at_parse_time() {
    let err = toml::from_str::<ConferenceConfig>(
        r#"
gift_card_ladder = []
visit_mileage = "5"
visit_notes_html = "<p>x</p>"
[second_harvest]
id = "1"
name_contains = "Food"
[gift_card]
id = "2"
name_contains = "Card"
"#,
    )
    .expect_err("an empty ladder must not parse");
    assert!(
        err.to_string().contains("at least one amount"),
        "the error should say what is wrong: {err}"
    );
}

/// Every household size maps to a real amount, and never to zero.
#[test]
fn the_ladder_is_clamped_at_both_ends_and_never_zero() {
    let c = ConferenceConfig::default();
    assert_eq!(c.gift_card_dollars(0), 50, "a household of zero gets the floor");
    assert_eq!(c.gift_card_dollars(1), 50);
    assert_eq!(c.gift_card_dollars(6), 100);
    assert_eq!(c.gift_card_dollars(99), 100, "past the top rung is the ceiling");
    for size in 0..200 {
        assert!(c.gift_card_dollars(size) >= 50, "size {size} produced a nonsense amount");
    }
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
    // The tag is the idempotency key (D7) and is written unconditionally; there
    // is no longer a config branch that can turn it off.
    assert_eq!(
        svdp::servware::detail::extract_tag(&c.item_notes("S", Slot::Food, "09/05/2026")),
        Some("svdp:s=S;i=food".to_string())
    );
    assert!(c.max_item_dollars >= 100, "the ceiling must clear the top of the ladder");
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

/// Deliveries run once a month per family, so a household served three weeks ago
/// is held back from the working list.
///
/// 28 days, not 30, and the difference is not cosmetic: deliveries run on fixed
/// weekdays, so a monthly cadence lands on exactly four weeks very often. Across
/// 1,114 real repeat deliveries a 28-day threshold held back 55 of them; 30 days
/// would have held back 280. See DECISIONS.md D29.
#[test]
fn the_monthly_interval_holds_back_four_weeks_not_a_calendar_month() {
    let config = ConferenceConfig::default();
    assert_eq!(config.delivery_interval_days, 28);

    let today = chrono::NaiveDate::from_ymd_opt(2026, 9, 7).unwrap();
    let days_ago = |n: i64| today - chrono::Duration::days(n);

    assert!(config.served_recently(days_ago(0), today), "delivered today");
    assert!(config.served_recently(days_ago(21), today), "three weeks ago");
    assert!(config.served_recently(days_ago(27), today), "one day short");
    assert!(
        !config.served_recently(days_ago(28), today),
        "exactly four weeks is due again -- this is the case 30 days would break"
    );
    assert!(!config.served_recently(days_ago(35), today), "five weeks ago");
}
