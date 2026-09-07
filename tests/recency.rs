//! Which households count as recently delivered to.

use svdp::domain::policy::ConferenceConfig;
use svdp::domain::recency::DeliveryRecency;
use svdp::servware::list;
use svdp::servware::list::RequestSummary;

/// A request with assistance items given on the listed dates.
fn request(id: u64, client_id: u64, requested: &str, provided: &[&str]) -> RequestSummary {
    let items: Vec<_> = provided
        .iter()
        .map(|d| serde_json::json!({ "monetaryValue": 70.0, "dateProvided": d }))
        .collect();
    serde_json::from_value(serde_json::json!({
        "id": id,
        "version": 1,
        "status": "Completed",
        "dateRequested": requested,
        "calculatedHouseholdCount": 4,
        "client": { "id": client_id, "firstName": "Ada", "lastName": "Nakamura" },
        "assistanceItems": items,
    }))
    .expect("minimal request")
}

fn date(s: &str) -> chrono::NaiveDate {
    list::parse_date(s).expect("test date")
}

#[test]
fn the_last_delivery_is_the_newest_date_help_actually_arrived() {
    let history = vec![
        request(1, 7, "05/02/2026", &["05/06/2026"]),
        request(2, 7, "07/01/2026", &["07/04/2026", "07/11/2026"]),
        request(3, 8, "08/01/2026", &["08/03/2026"]),
    ];
    let r = DeliveryRecency::from_history(&history);

    // Newest across all of the household's requests, not the newest request.
    assert_eq!(r.last_delivery(7, 0), Some(date("07/11/2026")));
    assert_eq!(r.last_delivery(8, 0), Some(date("08/03/2026")));
    assert_eq!(r.last_delivery(999, 0), None, "a household never served");
}

/// A request date is not a delivery date. A family can ask in June and be
/// served in July, which is why recency reads `date_provided`. See D23.
#[test]
fn recency_ignores_when_the_family_asked() {
    let history = vec![request(1, 7, "06/01/2026", &["07/20/2026"])];
    let r = DeliveryRecency::from_history(&history);
    assert_eq!(r.last_delivery(7, 0), Some(date("07/20/2026")));
}

/// The case that would otherwise be a trap: a delivery whose recording was
/// interrupted leaves items on a still-open request. If those counted, the
/// request would hide itself from the list of work left to do.
#[test]
fn a_partly_recorded_delivery_does_not_hide_its_own_request() {
    let today = date("09/07/2026");
    let config = ConferenceConfig::default();

    // Request 42 is open, and its food item was already written before the
    // recording was interrupted today.
    let history = vec![
        request(42, 7, "09/05/2026", &["09/07/2026"]),
        request(9, 7, "04/01/2026", &["04/04/2026"]),
    ];
    let r = DeliveryRecency::from_history(&history);

    // Seen from request 42, the household's last delivery is April, so it is due
    // and stays visible.
    let from_own = r.last_delivery(7, 42);
    assert_eq!(from_own, Some(date("04/04/2026")));
    assert!(!config.served_recently(from_own.unwrap(), today));

    // Seen from any other request, today's item does count.
    assert_eq!(r.last_delivery(7, 9), Some(date("09/07/2026")));
}

#[test]
fn items_without_a_readable_date_are_skipped_rather_than_guessed() {
    let history = vec![request(1, 7, "07/01/2026", &["", "not a date", "07/04/2026"])];
    let r = DeliveryRecency::from_history(&history);
    assert_eq!(r.last_delivery(7, 0), Some(date("07/04/2026")));
}

#[test]
fn no_history_means_nothing_is_held_back() {
    let r = DeliveryRecency::from_history(&[]);
    assert!(r.is_empty());
    assert_eq!(r.last_delivery(7, 0), None);
}

/// The lookback has to be much wider than the interval, because help arrives
/// well after a family asks and the list endpoint can only filter on the
/// request date.
#[test]
fn the_history_lookback_covers_the_interval_plus_real_delivery_lag() {
    let interval = i64::from(ConferenceConfig::default().delivery_interval_days);
    let lookback = svdp::domain::recency::HISTORY_LOOKBACK_DAYS;
    assert!(
        lookback >= interval + 60,
        "lookback {lookback} leaves too little slack over a {interval}-day interval; \
         measured request-to-delivery lag reaches 39 days at p99.9"
    );
}
