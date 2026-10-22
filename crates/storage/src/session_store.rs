//! A durable [`SessionStore`] backed by one JSON file.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use hopswitch_core::{Session, SessionStore, StoreError};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use crate::write_atomic;

/// Highest schema version this build reads and the one it writes.
const SCHEMA_VERSION: u32 = 1;

/// Persists a [`Session`] as `{ "version": 1, "session": { ... } }`.
///
/// `save` is atomic and fsynced, so the snapshot is on disk before the caller
/// changes any system setting. Use one instance (and path) per kind of setting.
#[derive(Debug, Clone)]
pub struct JsonFileSessionStore {
    path: PathBuf,
}

#[derive(Serialize)]
struct FileOut<'a, S> {
    version: u32,
    session: &'a Session<S>,
}

fn store_error(path: &Path, error: impl std::fmt::Display) -> StoreError {
    StoreError(format!("{}: {error}", path.display()))
}

impl JsonFileSessionStore {
    /// A store at `path` (created on first save).
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Reads the persisted session, `None` if there is none.
    ///
    /// A corrupt or newer-version file is an error and is never deleted: the
    /// snapshot inside may be the only copy of the user's original settings.
    pub fn load<S: DeserializeOwned>(&self) -> Result<Option<Session<S>>, StoreError> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(store_error(&self.path, e)),
        };

        let mut value: Value =
            serde_json::from_slice(&bytes).map_err(|e| store_error(&self.path, e))?;
        let version = value
            .get("version")
            .and_then(Value::as_u64)
            .filter(|v| *v >= 1)
            .ok_or_else(|| store_error(&self.path, "missing or invalid `version` field"))?;
        if version > u64::from(SCHEMA_VERSION) {
            return Err(store_error(
                &self.path,
                format!("saved by a newer version (file v{version}, supported v{SCHEMA_VERSION})"),
            ));
        }

        let session = value
            .get_mut("session")
            .map(Value::take)
            .ok_or_else(|| store_error(&self.path, "missing `session` field"))?;
        serde_json::from_value(session)
            .map(Some)
            .map_err(|e| store_error(&self.path, e))
    }
}

impl<S: Serialize> SessionStore<S> for JsonFileSessionStore {
    fn save(&self, session: &Session<S>) -> Result<(), StoreError> {
        let bytes = serde_json::to_vec_pretty(&FileOut {
            version: SCHEMA_VERSION,
            session,
        })
        .map_err(|e| store_error(&self.path, e))?;
        write_atomic(&self.path, &bytes).map_err(|e| store_error(&self.path, e))
    }

    fn clear(&self) -> Result<(), StoreError> {
        match fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(store_error(&self.path, e)),
        }
        sync_parent(&self.path).map_err(|e| store_error(&self.path, e))
    }
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> io::Result<()> {
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => fs::File::open(dir)?.sync_all(),
        _ => Ok(()),
    }
}

#[cfg(not(unix))]
fn sync_parent(_path: &Path) -> io::Result<()> {
    Ok(())
}
