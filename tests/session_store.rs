//! The session store, and the one distinction in it that guards real money.
//!
//! A saved receipt that cannot be read must never look like "no delivery in
//! progress". The idempotency tag embeds the session id, so starting a fresh
//! session over an unreadable one produces tags that match nothing already in
//! ServWare — and a night's writes get made a second time, against a system with
//! no way to delete an assistance item. See DECISIONS.md D39.

use svdp::domain::session::DeliverySession;
use svdp::domain::session::Group;
use svdp::domain::store::SessionStore;

fn session(date: &str) -> DeliverySession {
    DeliverySession::new(date, "2026-09-05T18:00:00")
}

#[test]
fn a_saved_session_round_trips() {
    let tmp = tempfile::tempdir().unwrap();
    let store = SessionStore::open(tmp.path()).unwrap();

    let mut s = session("09/05/2026");
    s.groups = vec![Group {
        volunteer_id: "44270".into(),
        volunteer_name: "Ada Lovelace".into(),
        deliveries: vec![],
    }];
    store.save(&s).unwrap();

    let loaded = store.load(&s.id).unwrap();
    assert_eq!(loaded.id, s.id);
    assert_eq!(loaded.delivery_date, "09/05/2026");
    assert_eq!(store.current().unwrap().map(|c| c.id), Some(s.id));
}

#[test]
fn no_sessions_means_none_not_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let store = SessionStore::open(tmp.path()).unwrap();
    assert!(store.current().unwrap().is_none());
}

/// The load-bearing one.
///
/// `current()` used to map an unparseable file to `None`, and the caller matched
/// `Ok(Some(..))` with a `_ => DeliverySession::new(..)` fallback — so a corrupt
/// or newer-schema receipt silently started a fresh session with a fresh id.
#[test]
fn an_unreadable_record_is_an_error_not_an_absence() {
    let tmp = tempfile::tempdir().unwrap();
    let store = SessionStore::open(tmp.path()).unwrap();
    std::fs::write(tmp.path().join("01JBQ.json"), "{ this is not a session }").unwrap();

    let err = store
        .current()
        .expect_err("an unreadable receipt must stop the tool, not read as 'nothing in progress'");
    let text = err.to_string();
    assert!(text.contains("01JBQ.json"), "should name the file: {text}");
    assert!(
        text.contains("money already sent"),
        "should say why it matters, in words a volunteer can act on: {text}"
    );
}

/// A field added to the session types without `#[serde(default)]` makes every
/// in-flight receipt unreadable. That must be loud rather than silent — which is
/// the same path as the test above, reached the way it would really be reached.
#[test]
fn a_receipt_from_a_different_schema_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let store = SessionStore::open(tmp.path()).unwrap();
    // A plausible-looking record missing a field the current types require.
    std::fs::write(
        tmp.path().join("01JBR.json"),
        r#"{"id":"01JBR","created_at":"2026-09-05T18:00:00"}"#,
    )
    .unwrap();
    assert!(store.current().is_err());
}

/// Listing is deliberately lenient where `current()` is strict: one bad file
/// must not hide every other session from someone trying to look at them.
#[test]
fn listing_skips_a_bad_file_rather_than_hiding_the_good_ones() {
    let tmp = tempfile::tempdir().unwrap();
    let store = SessionStore::open(tmp.path()).unwrap();
    let good = session("09/05/2026");
    store.save(&good).unwrap();
    std::fs::write(tmp.path().join("broken.json"), "not json").unwrap();

    let listed = store.list().unwrap();
    assert_eq!(listed.len(), 1, "the readable session is still listed");
    assert_eq!(listed[0].id, good.id);
    assert!(store.current().is_err(), "but current() still refuses");
}
