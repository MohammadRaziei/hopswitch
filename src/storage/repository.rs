//! A [`ProfileRepository`] backed by one JSON file.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::core::{ProfileRepository, Profiles, RepositoryError};
use serde::Serialize;
use serde_json::Value;

use crate::storage::write_atomic;

/// Highest schema version this build reads and the one it writes.
const SCHEMA_VERSION: u32 = 3;

/// Stores all profiles in a single versioned JSON file, replaced atomically on save.
///
/// File shape: `{ "version": 3, "proxies": [...], "groups": [...], "rules": [...], "dns": [...] }`.
/// Versions 1 (no `groups`) and 2 (no `rules`) are still read; an older Hopswitch refuses a newer
/// file rather than silently dropping the groups or rules the next time it saves.
#[derive(Debug, Clone)]
pub struct JsonFileRepository {
    path: PathBuf,
}

#[derive(Serialize)]
struct FileOut<'a> {
    version: u32,
    #[serde(flatten)]
    profiles: &'a Profiles,
}

fn io_error(path: &Path, error: &io::Error) -> RepositoryError {
    RepositoryError::Io(format!("{}: {error}", path.display()))
}

impl JsonFileRepository {
    /// A repository stored at `path` (created on first save).
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Moves a corrupt file aside (`<name>.corrupt`, then `.corrupt-1`, ...) so a
    /// fresh start is possible without deleting the user's data. Returns the new path.
    ///
    /// Never called automatically: `load` only reports corruption.
    pub fn quarantine_corrupt(&self) -> Result<PathBuf, RepositoryError> {
        let name = self.path.file_name().ok_or_else(|| {
            RepositoryError::Io(format!("{}: path has no file name", self.path.display()))
        })?;
        let mut target = self.path.with_file_name(suffixed(name, ".corrupt"));
        let mut n = 1;
        while target.exists() {
            target = self
                .path
                .with_file_name(suffixed(name, &format!(".corrupt-{n}")));
            n += 1;
        }
        fs::rename(&self.path, &target).map_err(|e| io_error(&self.path, &e))?;
        Ok(target)
    }
}

fn suffixed(name: &std::ffi::OsStr, suffix: &str) -> OsString {
    let mut out = name.to_owned();
    out.push(suffix);
    out
}

impl ProfileRepository for JsonFileRepository {
    fn load(&self) -> Result<Profiles, RepositoryError> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Profiles::new()),
            Err(e) => return Err(io_error(&self.path, &e)),
        };

        let value: Value =
            serde_json::from_slice(&bytes).map_err(|e| RepositoryError::Corrupt(e.to_string()))?;
        let version = value
            .get("version")
            .and_then(Value::as_u64)
            .filter(|v| *v >= 1)
            .ok_or_else(|| {
                RepositoryError::Corrupt("missing or invalid `version` field".to_owned())
            })?;
        if version > u64::from(SCHEMA_VERSION) {
            return Err(RepositoryError::UnsupportedVersion {
                found: u32::try_from(version).unwrap_or(u32::MAX),
                supported: SCHEMA_VERSION,
            });
        }

        serde_json::from_value(value).map_err(|e| RepositoryError::Corrupt(e.to_string()))
    }

    fn save(&self, profiles: &Profiles) -> Result<(), RepositoryError> {
        let bytes = serde_json::to_vec_pretty(&FileOut {
            version: SCHEMA_VERSION,
            profiles,
        })
        .map_err(|e| RepositoryError::Io(e.to_string()))?;
        write_atomic(&self.path, &bytes).map_err(|e| io_error(&self.path, &e))
    }
}
