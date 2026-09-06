//! CSV exports.
//!
//! The extension's job here is extraction, not analysis: it produces honest,
//! wide, joinable tables and stops. Filtering, grouping and arithmetic happen in
//! the conversation, where they can be adjusted per project without a rebuild.
//! See DECISIONS.md D20.
//!
//! Three grains, all joinable on `client_id`:
//!
//! | table | one row per | source |
//! |---|---|---|
//! | `neighbors` | household | `/app/clients/list` |
//! | `requests` | assistance request | `/app/assistancerequests/list` |
//! | `household-members` | person | request detail pages |
//!
//! **Every column is on an explicit allowlist**, pinned by a test. A field
//! appearing in a ServWare response can therefore never add a column by
//! accident; adding one is a deliberate edit to a visible list. Never emitted,
//! from any table: SSN, driver's licence, other identity documents, case notes,
//! alert notes, and dates of birth. Ages answer every question these projects
//! ask, and a spreadsheet that gets emailed between volunteers should not carry
//! the name + address + date-of-birth triple. See DECISIONS.md D21.

use std::path::Path;
use std::path::PathBuf;

use crate::servware::clients::NeighborSummary;
use crate::servware::detail::HouseholdMember;
use crate::servware::list::RequestSummary;

pub const NEIGHBORS_HEADER: &[&str] = &[
    "client_id",
    "first_name",
    "last_name",
    "street_address_line1",
    "street_address_line2",
    "city",
    "state_code",
    "postal_code",
    "home_phone",
    "mobile_phone",
    "work_phone",
    "email_address",
    "primary_language",
    "marital_status",
    "parishioner",
    "homeless",
    "disabled_client",
    "veteran",
    "last_request_date",
    "household_adult_count",
    "household_child_count",
];

pub const REQUESTS_HEADER: &[&str] = &[
    "request_id",
    "client_id",
    "first_name",
    "last_name",
    "date_requested",
    "status",
    "street_address_line1",
    "city",
    "home_phone",
    "mobile_phone",
    "calculated_adult_count",
    "calculated_child_count",
    "calculated_household_count",
    "assistance_item_count",
    "assistance_total_dollars",
];

pub const MEMBERS_HEADER: &[&str] = &[
    "client_id",
    "household_last_name",
    "first_name",
    "relationship",
    "age",
];

/// A table ready to be written. Built without touching the filesystem so the
/// column allowlist can be asserted in a unit test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    /// Becomes part of the filename: `svdp-<name>-<date>.csv`.
    pub name: &'static str,
    pub header: &'static [&'static str],
    pub rows: Vec<Vec<String>>,
}

impl Table {
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// One household's members, as read from that household's request detail page.
pub struct HouseholdRoster<'a> {
    pub client_id: u64,
    pub household_last_name: &'a str,
    pub members: &'a [HouseholdMember],
}

fn yes_no(b: bool) -> String {
    if b { "yes" } else { "no" }.to_string()
}

fn opt_count(n: Option<u32>) -> String {
    n.map(|v| v.to_string()).unwrap_or_default()
}

pub fn neighbors_table(rows: &[NeighborSummary]) -> Table {
    Table {
        name: "neighbors",
        header: NEIGHBORS_HEADER,
        rows: rows
            .iter()
            .map(|n| {
                vec![
                    n.id.to_string(),
                    n.first_name.clone(),
                    n.last_name.clone(),
                    n.street_address_line1.clone(),
                    n.street_address_line2.clone(),
                    n.city.clone(),
                    n.state_code.clone(),
                    n.postal_code.clone(),
                    n.home_phone.clone(),
                    n.mobile_phone.clone(),
                    n.work_phone.clone(),
                    n.email_address.clone(),
                    n.primary_language.clone(),
                    n.marital_status.clone(),
                    yes_no(n.parishioner),
                    yes_no(n.homeless),
                    yes_no(n.disabled_client),
                    yes_no(n.veteran),
                    n.last_request_date.clone(),
                    opt_count(n.household_adult_count),
                    opt_count(n.household_child_count),
                ]
            })
            .collect(),
    }
}

pub fn requests_table(rows: &[RequestSummary]) -> Table {
    Table {
        name: "requests",
        header: REQUESTS_HEADER,
        rows: rows
            .iter()
            .map(|r| {
                vec![
                    r.id.to_string(),
                    r.client.id.to_string(),
                    r.client.first_name.clone(),
                    r.client.last_name.clone(),
                    r.date_requested.clone(),
                    r.status.clone(),
                    r.street_address_line1.clone(),
                    r.city.clone(),
                    r.client.home_phone.clone(),
                    r.client.mobile_phone.clone(),
                    r.calculated_adult_count.to_string(),
                    r.calculated_child_count.to_string(),
                    r.calculated_household_count.to_string(),
                    r.assistance_items.len().to_string(),
                    format!("{:.2}", r.assistance_total()),
                ]
            })
            .collect(),
    }
}

pub fn members_table(households: &[HouseholdRoster<'_>]) -> Table {
    Table {
        name: "household-members",
        header: MEMBERS_HEADER,
        rows: households
            .iter()
            .flat_map(|h| {
                h.members.iter().map(move |m| {
                    vec![
                        h.client_id.to_string(),
                        h.household_last_name.to_string(),
                        m.first_name.clone(),
                        m.relationship.clone(),
                        m.age.map(|a| a.to_string()).unwrap_or_default(),
                    ]
                })
            })
            .collect(),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("could not work out where to save files on this computer")]
    NoDestination,
    #[error("could not save the file: {0}")]
    Io(String),
    #[error("{0}")]
    BadName(String),
    #[error("that file is {0} bytes, too big to read into the conversation — open it in a spreadsheet instead")]
    TooLarge(u64),
}

/// Where exports land, and the only directory `read` will look in.
///
/// The Desktop is chosen so an elderly volunteer cannot lose the file: it is
/// where they already look, and double-clicking opens their spreadsheet. It is
/// discovered rather than configured, so the extension's install flow — two
/// boxes, username and password — does not grow a third.
pub struct ExportDir {
    dir: PathBuf,
}

impl ExportDir {
    /// The Desktop, falling back to the home directory and then to the app's own
    /// data directory.
    pub fn desktop() -> Result<Self, ExportError> {
        if let Some(dirs) = directories::UserDirs::new() {
            if let Some(desktop) = dirs.desktop_dir() {
                return Ok(Self::at(desktop));
            }
            return Ok(Self::at(dirs.home_dir()));
        }
        let fallback = directories::ProjectDirs::from("org", "svdp", "svdp-servware")
            .ok_or(ExportError::NoDestination)?
            .data_dir()
            .join("exports");
        Ok(Self::at(fallback))
    }

    pub fn at(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn path(&self) -> &Path {
        &self.dir
    }

    /// Write a table, returning the path. A same-day re-run writes `-2`, `-3`
    /// rather than overwriting: the volunteer may already have started editing
    /// this morning's copy.
    pub fn write(&self, table: &Table, today: chrono::NaiveDate) -> Result<PathBuf, ExportError> {
        std::fs::create_dir_all(&self.dir).map_err(|e| ExportError::Io(e.to_string()))?;
        let stem = format!("svdp-{}-{}", table.name, today.format("%Y-%m-%d"));
        let path = self.free_path(&stem);

        let mut w = csv::Writer::from_path(&path).map_err(|e| ExportError::Io(e.to_string()))?;
        w.write_record(table.header)
            .map_err(|e| ExportError::Io(e.to_string()))?;
        for row in &table.rows {
            w.write_record(row)
                .map_err(|e| ExportError::Io(e.to_string()))?;
        }
        w.flush().map_err(|e| ExportError::Io(e.to_string()))?;
        restrict(&path);
        Ok(path)
    }

    fn free_path(&self, stem: &str) -> PathBuf {
        let first = self.dir.join(format!("{stem}.csv"));
        if !first.exists() {
            return first;
        }
        for n in 2..1000 {
            let candidate = self.dir.join(format!("{stem}-{n}.csv"));
            if !candidate.exists() {
                return candidate;
            }
        }
        first
    }

    /// Exports currently in the directory, newest first.
    pub fn list(&self) -> Vec<(String, u64)> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut out: Vec<(String, u64, std::time::SystemTime)> = entries
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                if !name.starts_with("svdp-") || !name.ends_with(".csv") {
                    return None;
                }
                let meta = e.metadata().ok()?;
                Some((name, meta.len(), meta.modified().ok()?))
            })
            .collect();
        out.sort_by(|a, b| b.2.cmp(&a.2));
        out.into_iter().map(|(n, s, _)| (n, s)).collect()
    }

    /// Read one export back as text.
    ///
    /// Takes a bare filename and nothing else. A path separator, a `..`, or an
    /// absolute path is refused rather than normalised, so this can only ever
    /// read files this tool wrote.
    pub fn read(&self, file: &str, max_bytes: u64) -> Result<String, ExportError> {
        let name = file.trim();
        if name.is_empty()
            || name.contains('/')
            || name.contains('\\')
            || name.contains("..")
            || !name.starts_with("svdp-")
            || !name.ends_with(".csv")
        {
            return Err(ExportError::BadName(format!(
                "{name:?} is not the name of a file this tool created"
            )));
        }
        let path = self.dir.join(name);
        let meta = std::fs::metadata(&path)
            .map_err(|_| ExportError::BadName(format!("there is no file called {name}")))?;
        if meta.len() > max_bytes {
            return Err(ExportError::TooLarge(meta.len()));
        }
        std::fs::read_to_string(&path).map_err(|e| ExportError::Io(e.to_string()))
    }
}

/// Exports hold real neighbour information, so they are readable only by the
/// person who made them — the same rule the session store follows.
#[cfg(unix)]
fn restrict(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict(_path: &Path) {}
