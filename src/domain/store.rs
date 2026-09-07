//! Session persistence.
//!
//! Writes are atomic (temp file, fsync, rename): Claude Desktop kills MCP
//! servers, and a half-written session file after money has reached ServWare
//! would destroy the only local record of what was sent.
//!
//! Sessions hold neighbour names and addresses, so they live in the OS
//! application-data directory at mode 0600 -- never in the repository.

use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use crate::domain::session::DeliverySession;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("could not find a place to store delivery records")]
    NoHome,
    #[error("could not read the delivery record: {0}")]
    Read(String),
    #[error("could not save the delivery record: {0}")]
    Write(String),
    /// A saved session exists but cannot be read.
    ///
    /// Never treated as "there is no session". That is the path to a second
    /// charge: an unreadable receipt used to be skipped, `current()` returned
    /// `None`, the caller started a fresh session with a fresh id, and every
    /// assistance item was written again under a tag that matched nothing.
    /// See DECISIONS.md D39.
    #[error(
        "there is a saved delivery record that cannot be read ({0}). It may list money \
         already sent to ServWare, so nothing new can be started until somebody looks at it."
    )]
    Unreadable(String),
}

pub struct SessionStore {
    dir: PathBuf,
}

impl SessionStore {
    /// The per-user application data directory, e.g.
    /// `~/Library/Application Support/svdp-servware/sessions` on macOS.
    pub fn open_default() -> Result<Self, StoreError> {
        let base = directories::ProjectDirs::from("org", "svdp", "svdp-servware")
            .ok_or(StoreError::NoHome)?
            .data_dir()
            .join("sessions");
        Self::open(base)
    }

    pub fn open(dir: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir).map_err(|e| StoreError::Write(e.to_string()))?;
        restrict(&dir, 0o700);
        Ok(Self { dir })
    }

    fn path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    pub fn save(&self, session: &DeliverySession) -> Result<(), StoreError> {
        let json = serde_json::to_vec_pretty(session)
            .map_err(|e| StoreError::Write(e.to_string()))?;
        let final_path = self.path(&session.id);
        let temp_path = final_path.with_extension("json.tmp");

        {
            let mut file = std::fs::File::create(&temp_path)
                .map_err(|e| StoreError::Write(e.to_string()))?;
            file.write_all(&json).map_err(|e| StoreError::Write(e.to_string()))?;
            // Durable before the rename, or a crash can leave an empty file.
            file.sync_all().map_err(|e| StoreError::Write(e.to_string()))?;
        }
        restrict(&temp_path, 0o600);
        std::fs::rename(&temp_path, &final_path).map_err(|e| StoreError::Write(e.to_string()))?;
        Ok(())
    }

    pub fn load(&self, id: &str) -> Result<DeliverySession, StoreError> {
        let text = std::fs::read_to_string(self.path(id))
            .map_err(|e| StoreError::Read(e.to_string()))?;
        serde_json::from_str(&text).map_err(|e| StoreError::Read(e.to_string()))
    }

    /// Every readable session, newest first, plus the names of any that could
    /// not be read.
    ///
    /// The two are returned together rather than one silently swallowing the
    /// other: a listing wants to show what it can, while `current()` must treat
    /// an unreadable record as a hard stop.
    fn scan(&self) -> Result<(Vec<DeliverySession>, Vec<String>), StoreError> {
        let mut out = Vec::new();
        let mut unreadable = Vec::new();
        let entries = std::fs::read_dir(&self.dir).map_err(|e| StoreError::Read(e.to_string()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            match std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()) {
                Some(session) => out.push(session),
                None => {
                    tracing::warn!(?path, "unreadable session record");
                    unreadable.push(
                        path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                    );
                }
            }
        }
        out.sort_by(|a: &DeliverySession, b: &DeliverySession| b.created_at.cmp(&a.created_at));
        Ok((out, unreadable))
    }

    /// Every readable session, newest first. A malformed file is skipped rather
    /// than hiding all the others.
    pub fn list(&self) -> Result<Vec<DeliverySession>, StoreError> {
        Ok(self.scan()?.0)
    }

    /// The session still in flight, if any.
    ///
    /// A second session is never created while one is open: models re-call tools
    /// when confused, and if starting fresh were easy a delivery night would
    /// eventually be submitted twice.
    ///
    /// **An unreadable record is an error, not an absence.** Returning `None`
    /// there is indistinguishable from "no delivery in progress", which is how a
    /// night's writes get replayed under a new session id against a tag that
    /// matches nothing already in ServWare. See DECISIONS.md D39.
    pub fn current(&self) -> Result<Option<DeliverySession>, StoreError> {
        let (sessions, unreadable) = self.scan()?;
        if !unreadable.is_empty() {
            return Err(StoreError::Unreadable(unreadable.join(", ")));
        }
        Ok(sessions.into_iter().find(DeliverySession::is_open))
    }
}

#[cfg(unix)]
fn restrict(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
}

#[cfg(not(unix))]
fn restrict(_path: &Path, _mode: u32) {}
