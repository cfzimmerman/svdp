//! Household-member parsing and the CSV export layer.
//!
//! The load-bearing assertions here are about *what does not come out*: the
//! household-members table on the live page carries SSN last-four and driver's
//! licence columns beside the ages, so "we only read four columns" has to be a
//! checked property rather than a comment.

use svdp::domain::export;
use svdp::domain::export::ExportDir;
use svdp::domain::export::HouseholdRoster;
use svdp::domain::pull;
use svdp::servware::clients::NeighborSummary;
use svdp::servware::detail;
use svdp::servware::list;

const OPEN: &str = include_str!("fixtures/detail_open.html");
const SHIFTED: &str = include_str!("fixtures/detail_members_shifted.html");
const SPARSE: &str = include_str!("fixtures/detail_members_sparse.html");
const ABSENT: &str = include_str!("fixtures/detail_members_absent.html");
const LARGE: &str = include_str!("fixtures/detail_members_large.html");

/// Values planted in the fixture's SSN and driver's-licence cells. Nothing this
/// tool produces may contain them.
const SENTINELS: &[&str] = &["9999", "SENTINELDLNEVERREAD", "SENTINELNOTE"];

fn members(html: &str) -> Vec<detail::HouseholdMember> {
    detail::parse(1, html).expect("fixture parses").household_members
}

#[test]
fn reads_the_household_from_the_family_members_tab() {
    let m = members(OPEN);
    assert_eq!(m.len(), 4, "four people in the fixture household");
    assert_eq!(m[0].first_name, "Maria");
    assert!(m[0].is_self());
    assert_eq!(
        m.iter().map(|p| p.age).collect::<Vec<_>>(),
        vec![Some(41), Some(17), Some(12), Some(8)]
    );
    assert_eq!(m[3].relationship, "Daughter");
}

/// Columns are found by header text. If they were found by position, an upstream
/// column insertion would quietly start reporting the wrong number as an age --
/// and an age is what decides whether a family lands on a Christmas list.
#[test]
fn an_inserted_column_does_not_shift_the_age() {
    assert!(SHIFTED.contains("<th>Nickname</th>"), "fixture has the extra column");
    assert_eq!(
        members(SHIFTED).iter().map(|p| p.age).collect::<Vec<_>>(),
        vec![Some(41), Some(17), Some(12), Some(8)]
    );
}

#[test]
fn an_unrecorded_age_is_absent_rather_than_zero() {
    let m = members(SPARSE);
    assert_eq!(m.len(), 2);
    assert_eq!(m[1].first_name, "Wren");
    assert_eq!(m[1].age, None, "a blank cell must not become an age of 0");
}

#[test]
fn a_page_without_the_tab_is_empty_rather_than_an_error() {
    let d = detail::parse(1, ABSENT).expect("the rest of the page still parses");
    assert!(d.household_members.is_empty());
    // The form and assistance items are unaffected.
    assert_eq!(d.status(), "Open");
}

/// The one that matters. The identity columns sit next to the ages in the same
/// table; only four columns are ever addressed, so their contents cannot reach a
/// parsed field, a debug line, or a CSV.
#[test]
fn identity_documents_never_leave_the_page() {
    for s in SENTINELS {
        assert!(OPEN.contains(s), "fixture should plant {s}");
    }

    let m = members(OPEN);
    let parsed = format!("{m:#?}");
    let table = export::members_table(&[HouseholdRoster {
        client_id: 7,
        household_last_name: "Okonkwo",
        members: &m,
    }]);
    let rendered = format!("{:?}{:?}", table.header, table.rows);

    for s in SENTINELS {
        assert!(!parsed.contains(s), "{s} reached a parsed household member");
        assert!(!rendered.contains(s), "{s} reached the members CSV");
    }
}

/// Extraction must not saturate where a human layout did.
///
/// The spreadsheet this replaces had columns `C1`-`C4`, so a household was
/// capped at four children and larger families silently lost some. Real data has
/// households of ten, three of them with more than four children. Nothing in the
/// parse or the CSV may impose a limit.
#[test]
fn a_large_household_is_not_truncated() {
    let m = members(LARGE);
    assert_eq!(m.len(), 10, "every member must survive parsing");

    let table = export::members_table(&[HouseholdRoster {
        client_id: 7,
        household_last_name: "Bergstrom",
        members: &m,
    }]);
    assert_eq!(table.len(), 10, "every member must survive the CSV");

    let children = m.iter().filter(|p| p.age.is_some_and(|a| a <= 17)).count();
    assert_eq!(children, 8, "eight under-18s, twice what C1-C4 could hold");

    // One row per person and no cap, so the ages come out in full.
    let mut ages: Vec<u32> = m.iter().filter_map(|p| p.age).collect();
    ages.sort_unstable();
    assert_eq!(ages, vec![2, 4, 6, 9, 11, 13, 15, 17, 38, 71]);
}

/// Raw data carries what ServWare recorded, never what this tool thinks a family
/// should get.
///
/// The gift-card ladder is a *weekly delivery* policy. A Christmas program sets
/// its own scale, and that decision belongs to the volunteer running it -- so no
/// export may carry a suggested, computed, or policy-derived amount. The only
/// money in an export is money ServWare says was actually given.
#[test]
fn exports_carry_recorded_money_only_never_a_suggested_amount() {
    let banned = [
        "suggested", "gift_card", "giftcard", "ladder", "budget",
        "recommend", "allocation", "per_family", "should",
    ];
    for header in [
        export::NEIGHBORS_HEADER,
        export::REQUESTS_HEADER,
        export::MEMBERS_HEADER,
        export::ASSISTANCE_HEADER,
    ] {
        for column in header {
            for bad in banned {
                assert!(!column.contains(bad), "column {column:?} looks policy-derived ({bad:?})");
            }
        }
    }

    // And structurally: the export and pull layers must not reach for conference
    // policy at all. This is the canary for someone later wiring the delivery
    // ladder into a project export, which is the actual mistake to prevent.
    for (name, src) in [
        ("export.rs", include_str!("../src/domain/export.rs")),
        ("pull.rs", include_str!("../src/domain/pull.rs")),
    ] {
        assert!(
            !src.contains("domain::policy") && !src.contains("ConferenceConfig"),
            "{name} reaches for conference policy; exports must stay raw"
        );
    }
}

/// Column sets are pinned so a new ServWare field cannot add a column by
/// accident. Changing an export's shape should mean editing this list.
#[test]
fn the_column_allowlist_is_pinned() {
    assert_eq!(
        export::MEMBERS_HEADER,
        &["client_id", "household_last_name", "first_name", "relationship", "age"]
    );
    assert_eq!(export::NEIGHBORS_HEADER.len(), 22);
    assert_eq!(export::REQUESTS_HEADER.len(), 15);
    assert_eq!(export::NEIGHBORS_HEADER[0], "client_id");
    assert_eq!(export::REQUESTS_HEADER[1], "client_id", "the join key");

    // Nothing identity-shaped, and no free text written by a caseworker.
    let forbidden = [
        "ssn", "drivers_license", "driverslicense", "identification", "birth", "dob",
        "notes", "alert",
    ];
    for header in [export::NEIGHBORS_HEADER, export::REQUESTS_HEADER, export::MEMBERS_HEADER] {
        for column in header {
            for bad in forbidden {
                assert!(!column.contains(bad), "column {column:?} looks like {bad:?}");
            }
        }
    }
}

/// Dates of birth are read but never emitted: the age column is derived, and
/// the birth date itself is not a public field at all. See DECISIONS.md D21.
#[test]
fn a_birth_date_becomes_an_age_and_never_a_column() {
    let today = list::parse_date("09/06/2026").unwrap();
    let n: NeighborSummary = serde_json::from_value(serde_json::json!({
        "id": 1, "firstName": "Ada", "lastName": "Nakamura", "birthDate": "09/07/1980",
    }))
    .unwrap();
    assert_eq!(n.age_on(today), Some(45), "birthday is tomorrow, so still 45");

    let n2: NeighborSummary = serde_json::from_value(serde_json::json!({
        "id": 2, "firstName": "Ada", "lastName": "Nakamura", "birthDate": "09/06/1980",
    }))
    .unwrap();
    assert_eq!(n2.age_on(today), Some(46), "birthday is today");

    let n3: NeighborSummary = serde_json::from_value(serde_json::json!({
        "id": 3, "firstName": "Ada", "lastName": "Nakamura",
    }))
    .unwrap();
    assert_eq!(n3.age_on(today), None, "no birth date, no age");

    let table = export::neighbors_table(&[n, n2, n3], today);
    assert!(!export::NEIGHBORS_HEADER.contains(&"birth_date"));
    let rendered = format!("{:?}", table.rows);
    assert!(!rendered.contains("1980"), "a birth year reached the CSV");
    assert_eq!(table.rows[0].last().unwrap(), "45");
    assert_eq!(table.rows[2].last().unwrap(), "", "unknown age is blank, not 0");
}

#[test]
fn a_neighbour_missing_an_identity_field_is_an_error_not_a_blank() {
    let complete = serde_json::json!({
        "id": 501, "firstName": "Ada", "lastName": "Nakamura", "city": "Menlo Park"
    });
    let n: NeighborSummary = serde_json::from_value(complete).expect("required fields present");
    assert_eq!(n.display_name(), "Ada Nakamura");
    assert_eq!(n.state_code, "", "presentational fields still default");
    assert_eq!(n.household_adult_count, None);

    let renamed = serde_json::json!({ "id": 501, "firstName": "Ada", "surname": "Nakamura" });
    assert!(
        serde_json::from_value::<NeighborSummary>(renamed).is_err(),
        "a renamed identity field must fail loudly, not produce an empty column"
    );
}

fn request(id: u64, client_id: u64, date: &str) -> list::RequestSummary {
    serde_json::from_value(serde_json::json!({
        "id": id,
        "version": 1,
        "status": "Completed",
        "dateRequested": date,
        "calculatedHouseholdCount": 4,
        "client": { "id": client_id, "firstName": "Ada", "lastName": "Nakamura" },
    }))
    .expect("minimal request")
}

#[test]
fn the_date_window_is_inclusive_at_both_ends() {
    let from = list::parse_date("06/01/2026");
    let to = list::parse_date("08/31/2026");
    assert!(list::in_window("06/01/2026", from, to));
    assert!(list::in_window("08/31/2026", from, to));
    assert!(!list::in_window("05/31/2026", from, to));
    assert!(!list::in_window("09/01/2026", from, to));
    // An unreadable date is kept: dropping it would silently shrink an export.
    assert!(list::in_window("", from, to));
}

/// Requests arrive newest-first, so the first sighting of a household is its
/// most recent request -- and that is the one detail page we fetch.
#[test]
fn one_detail_page_per_household() {
    let requests = vec![
        request(90, 7, "08/20/2026"),
        request(89, 8, "08/18/2026"),
        request(88, 7, "07/02/2026"),
        request(87, 7, "06/03/2026"),
    ];
    let latest = pull::latest_per_household(&requests);
    assert_eq!(latest.len(), 2, "two households, not four requests");
    assert_eq!(latest[0].id, 90, "the newest request for household 7");
    assert_eq!(latest[1].id, 89);
}

#[test]
fn assistance_dollars_are_totalled_for_the_export() {
    let r: list::RequestSummary = serde_json::from_value(serde_json::json!({
        "id": 1, "version": 1, "status": "Completed", "dateRequested": "08/01/2026",
        "calculatedHouseholdCount": 4,
        "client": { "id": 7, "firstName": "Ada", "lastName": "Nakamura" },
        "assistanceItems": [ { "monetaryValue": 70.0 }, { "monetaryValue": 60.0 } ],
    }))
    .unwrap();
    let table = export::requests_table(&[r]);
    assert_eq!(table.rows[0].last().unwrap(), "130.00");
    assert_eq!(table.rows[0][export::REQUESTS_HEADER.len() - 2], "2");
}

#[test]
fn a_second_export_on_the_same_day_does_not_overwrite_the_first() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = ExportDir::at(tmp.path());
    let table = export::members_table(&[HouseholdRoster {
        client_id: 7,
        household_last_name: "Okonkwo",
        members: &members(OPEN),
    }]);
    let day = list::parse_date("09/06/2026").unwrap();

    let first = dir.write(&table, day).unwrap();
    let second = dir.write(&table, day).unwrap();
    assert_ne!(first, second, "a volunteer may already be editing the first");
    assert!(first.ends_with("svdp-household-members-2026-09-06.csv"));
    assert!(second.ends_with("svdp-household-members-2026-09-06-2.csv"));

    let body = std::fs::read_to_string(&first).unwrap();
    assert!(body.starts_with("client_id,household_last_name,first_name,relationship,age\n"));
    assert_eq!(body.lines().count(), 5, "header plus four people");
}

#[cfg(unix)]
#[test]
fn exports_are_readable_only_by_their_owner() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let dir = ExportDir::at(tmp.path());
    let table = export::neighbors_table(&[], list::parse_date("09/06/2026").unwrap());
    let path = dir.write(&table, list::parse_date("09/06/2026").unwrap()).unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn read_export_only_ever_reads_its_own_files() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("secrets.csv"), "nope").unwrap();
    let dir = ExportDir::at(tmp.path());
    let day = list::parse_date("09/06/2026").unwrap();
    dir.write(&export::neighbors_table(&[], list::parse_date("09/06/2026").unwrap()), day).unwrap();

    assert!(dir.read("svdp-neighbors-2026-09-06.csv", 1 << 20).is_ok());

    for bad in [
        "../../../etc/passwd",
        "/etc/passwd",
        "secrets.csv",
        "svdp-../escape.csv",
        "svdp-neighbors-2026-09-06.txt",
        "",
    ] {
        assert!(dir.read(bad, 1 << 20).is_err(), "{bad:?} should be refused");
    }

    // A file too big to be useful in a conversation is refused, not truncated.
    assert!(dir.read("svdp-neighbors-2026-09-06.csv", 1).is_err());
}

/// A refusal that says "narrow the date range" must reach the volunteer saying
/// that, not "something went wrong" -- nothing is broken and there is a clear
/// next step. This was reported as a system fault until `TooBroad` existed.
#[test]
fn asking_for_too_much_gives_guidance_not_a_fault() {
    let e = svdp::servware::error::ServWareError::TooBroad(
        "That date range covers 300 households, which is more than this will look up \
         in one go (200)."
            .to_string(),
    );
    let msg = e.user_message();
    assert!(msg.contains("300 households"), "the count must survive: {msg}");
    assert!(msg.contains("more than this will look up"));
    assert!(
        !msg.contains("did not understand"),
        "must not be flattened into a generic ServWare failure"
    );

    let generic = svdp::servware::error::ServWareError::Malformed("serde error at line 4".into());
    assert!(
        !generic.user_message().contains("serde"),
        "genuine faults still hide their detail"
    );
}

#[test]
fn listing_shows_only_exports() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("holiday-photo.jpg"), "x").unwrap();
    let dir = ExportDir::at(tmp.path());
    dir.write(&export::neighbors_table(&[], list::parse_date("09/06/2026").unwrap()), list::parse_date("09/06/2026").unwrap())
        .unwrap();
    let listing = dir.list();
    assert_eq!(listing.len(), 1);
    assert_eq!(listing[0].0, "svdp-neighbors-2026-09-06.csv");
}
