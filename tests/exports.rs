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
const EMPTY: &str = include_str!("fixtures/detail_members_empty.html");
const WITH_SELF: &str = include_str!("fixtures/detail_members_with_self.html");
const LARGE: &str = include_str!("fixtures/detail_members_large.html");

/// Values planted in the fixture's SSN and driver's-licence cells. Nothing this
/// tool produces may contain them.
const SENTINELS: &[&str] = &["9999", "SENTINELDLNEVERREAD", "SENTINELNOTE"];

/// Every table this tool can write. Listed once so a new export cannot be added
/// without the PII assertions below covering it -- which is exactly how
/// `ASSISTANCE_HEADER` came to be unchecked.
const ALL_HEADERS: &[&[&str]] = &[
    export::NEIGHBORS_HEADER,
    export::REQUESTS_HEADER,
    export::ASSISTANCE_HEADER,
    export::MEMBERS_HEADER,
];

fn members(html: &str) -> Vec<detail::HouseholdMember> {
    detail::parse_household_members(&scraper::Html::parse_document(html))
        .expect("fixture parses")
}

#[test]
fn reads_the_household_from_the_family_members_tab() {
    let m = members(OPEN);
    assert_eq!(m.len(), 3, "three dependants; the neighbour is not a row");
    assert_eq!(m[0].first_name, "Peter");
    assert!(!m.iter().any(|p| p.is_self()), "D19: ServWare never lists a Self row");
    assert_eq!(
        m.iter().map(|p| p.age).collect::<Vec<_>>(),
        vec![Some(17), Some(12), Some(8)]
    );
    assert_eq!(m[2].relationship, "Daughter");
}

/// Columns are found by header text. If they were found by position, an upstream
/// column insertion would quietly start reporting the wrong number as an age --
/// and an age is what decides whether a family lands on a Christmas list.
#[test]
fn an_inserted_column_does_not_shift_the_age() {
    assert!(SHIFTED.contains("<th>Nickname</th>"), "fixture has the extra column");
    assert_eq!(
        members(SHIFTED).iter().map(|p| p.age).collect::<Vec<_>>(),
        vec![Some(17), Some(12), Some(8)]
    );
}

#[test]
fn an_unrecorded_age_is_absent_rather_than_zero() {
    let m = members(SPARSE);
    assert_eq!(m.len(), 2);
    assert_eq!(m[1].first_name, "Wren");
    assert_eq!(m[1].age, None, "a blank cell must not become an age of 0");
}

/// A household really can have nobody listed: ServWare accepts a head count OR
/// individual people, and 19 of 158 households in one window had only the count.
/// That is an answer, and must parse as one.
#[test]
fn a_household_recorded_only_as_a_head_count_is_an_empty_roster() {
    assert!(EMPTY.contains("tabs-familymembers"), "fixture has the tab");
    assert!(members(EMPTY).is_empty());
}

/// A page with no tab at all is ServWare having changed shape, which is a
/// completely different thing -- and it used to be indistinguishable from the
/// case above.
///
/// That mattered: `pull::household_members` derives "this family has nobody
/// listed" purely from an empty roster, so a renamed tab id would have had the
/// tool announce, with total confidence, that every household in the conference
/// has nobody living in it -- a sentence that reads exactly like a correct
/// answer. See DECISIONS.md D32.
#[test]
fn a_page_without_the_tab_is_an_error_not_an_empty_household() {
    assert!(!ABSENT.contains("tabs-familymembers"));
    let err = detail::parse_household_members(&scraper::Html::parse_document(ABSENT))
        .expect_err("a missing tab is a change in ServWare, not an empty family");
    assert!(format!("{err}").contains("form has changed"), "{err}");

    // The rest of the page is unaffected: this must not break the write path.
    let d = detail::parse(1, ABSENT).expect("the rest of the page still parses");
    assert_eq!(d.status(), "Open");
}

/// Household size is documented as "rows plus one", so the neighbour must never
/// also be a row. D19 says ServWare does not produce one here, but another
/// conference might -- and counting that family twice can move them up a
/// gift-card rung.
#[test]
fn a_self_row_is_excluded_so_rows_plus_one_stays_right() {
    assert!(WITH_SELF.contains("<td>Self</td>"), "fixture plants the row");
    let m = members(WITH_SELF);
    assert_eq!(m.len(), 1, "the Self row is dropped");
    assert_eq!(m[0].first_name, "Ruth");
    assert!(!m.iter().any(|p| p.is_self()));
}

/// The columns just past Age are Phone, SSN (Last 4) and Drivers License. If the
/// header row and the body cells ever stop lining up, reading "age" by index
/// reads one of those instead -- and `"9999".parse::<u32>()` succeeds, so the
/// value lands in the CSV and then in the conversation. See DECISIONS.md D40.
#[test]
fn a_row_that_does_not_match_the_header_is_refused() {
    let skewed = OPEN.replacen("<td>Peter</td>", "<td>Peter</td><td>extra</td>", 1);
    let err = detail::parse_household_members(&scraper::Html::parse_document(&skewed))
        .expect_err("a row wider than the header must not be read by index");
    assert!(format!("{err}").contains("form has changed"), "{err}");
}

/// An age that is not an age means the columns have shifted, and the cell being
/// read is probably the SSN.
#[test]
fn an_implausible_age_is_refused_rather_than_exported() {
    let ssn_in_the_age_column = OPEN.replacen("<td>17</td>", "<td>9999</td>", 1);
    let err = detail::parse_household_members(&scraper::Html::parse_document(
        &ssn_in_the_age_column,
    ))
    .expect_err("9999 is not an age");
    assert!(format!("{err}").contains("form has changed"), "{err}");
    // And the refusal must not quote the value it refused.
    assert!(!format!("{err}").contains("9999"), "the error leaked the cell: {err}");
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
    for header in ALL_HEADERS.iter().copied() {
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
///
/// **Every header is asserted exactly**, as DECISIONS.md D21 specifies. Only
/// `MEMBERS_HEADER` used to be; the others were pinned by length plus one
/// element, which cannot catch a *replaced* column (swap `marital_status` for
/// `income_level` and the length is still 22). And `ASSISTANCE_HEADER` was left
/// out of the forbidden-substring loop entirely -- so SSN, date of birth and
/// case-note columns could be added to the one table that already carries a
/// first and last name on every row, and the whole suite stayed green. Proven
/// by doing exactly that. See DECISIONS.md D47.
#[test]
fn the_column_allowlist_is_pinned() {
    assert_eq!(
        export::NEIGHBORS_HEADER,
        &[
            "client_id", "first_name", "last_name", "street_address_line1",
            "street_address_line2", "city", "state_code", "postal_code", "home_phone",
            "mobile_phone", "work_phone", "email_address", "primary_language",
            "marital_status", "parishioner", "homeless", "disabled_client", "veteran",
            "last_request_date", "household_adult_count", "household_child_count", "age",
        ]
    );
    assert_eq!(
        export::REQUESTS_HEADER,
        &[
            "request_id", "client_id", "first_name", "last_name", "date_requested", "status",
            "street_address_line1", "city", "home_phone", "mobile_phone",
            "calculated_adult_count", "calculated_child_count", "calculated_household_count",
            "assistance_item_count", "assistance_total_dollars",
        ]
    );
    assert_eq!(
        export::ASSISTANCE_HEADER,
        &[
            "request_id", "client_id", "first_name", "last_name", "date_requested",
            "date_provided", "assistance_type", "monetary_value", "quantity", "pending",
        ]
    );
    assert_eq!(
        export::MEMBERS_HEADER,
        &["client_id", "household_last_name", "first_name", "relationship", "age"]
    );

    // Nothing identity-shaped, and no free text written by a caseworker.
    // EVERY table, including assistance.
    let forbidden = [
        "ssn", "social", "drivers_license", "driverslicense", "license", "identification",
        "identity", "birth", "dob", "notes", "note", "alert", "income", "case_number",
    ];
    for header in ALL_HEADERS.iter().copied() {
        for column in header {
            for bad in forbidden {
                assert!(!column.contains(bad), "column {column:?} looks like {bad:?}");
            }
        }
    }
}

/// The tables get opened in Excel by design, and ServWare's free-text fields are
/// typed by caseworkers. A cell beginning `=` is a formula there, not a string.
#[test]
fn a_cell_that_looks_like_a_formula_is_neutralised() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = ExportDir::at(tmp.path());
    let table = export::Table {
        name: "household-members",
        header: export::MEMBERS_HEADER,
        rows: vec![
            vec![
                "7".into(),
                "=HYPERLINK(\"http://x\",\"click\")".into(),
                "@SUM(A1:A9)".into(),
                "+SUM(A1:A9)".into(),
                "8".into(),
            ],
            vec!["8".into(), "Okonkwo".into(), "Ruth".into(), "Daughter".into(), "-3".into()],
        ],
    };
    let path = dir.write(&table, list::parse_date("09/06/2026").unwrap()).unwrap();
    let body = std::fs::read_to_string(&path).unwrap();

    for live in ["=HYPERLINK", "@SUM"] {
        assert!(
            !body.contains(&format!(",{live}")) && !body.lines().any(|l| l.starts_with(live)),
            "{live} reached the file as a live formula:\n{body}"
        );
    }
    assert!(body.contains("'=HYPERLINK"), "should be quoted as text:\n{body}");
    assert!(body.contains("'@SUM"), "should be quoted as text:\n{body}");
    assert!(body.contains("'+SUM"), "a non-numeric leading + is still defused:\n{body}");
    // Numbers are left alone, so amounts still sum in a spreadsheet. `-3` and
    // `+1` are values, not formulas: a spreadsheet renders them as numbers, and
    // quoting them would turn a column of money into a column of text.
    assert!(body.contains(",-3"), "a negative number must stay a number:\n{body}");
    assert_eq!(
        export::defuse_for_test("+1"),
        "+1",
        "a signed number is a number, not a formula"
    );
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
    assert_eq!(body.lines().count(), 4, "header plus the household's three dependants");
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

/// An extension installed but not yet given a username and password must say so
/// in words a volunteer can act on. Previously the server exited at startup and
/// the explanation went to a log file, so the extension simply appeared dead.
#[test]
fn an_unconfigured_extension_explains_itself() {
    let msg = svdp::servware::error::ServWareError::NotConfigured.user_message();
    for expected in ["Settings", "Extensions", "SVdP ServWare", "servware.org"] {
        assert!(msg.contains(expected), "setup text must mention {expected}: {msg}");
    }
    assert!(
        !msg.contains("env") && !msg.contains("SERVWARE_USER") && !msg.contains("log"),
        "must not mention environment variables or log files"
    );
    // Not to be confused with a rejected sign-in: nothing was tried.
    assert_ne!(
        msg,
        svdp::servware::error::ServWareError::LoginFailed.user_message()
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
