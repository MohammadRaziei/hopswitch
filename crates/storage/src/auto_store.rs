//! An [`AutoConfigStore`] backed by one JSON file.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use hopswitch_core::{AutoConfig, AutoConfigError, AutoConfigStore};
use serde::Serialize;
use serde_json::Value;

use crate::write_atomic;

/// Highest schema version this build reads and the one it writes.
const SCHEMA_VERSION: u32 = 1;

/// Stores the auto settings in a single versioned JSON file, replaced atomically on save.
///
/// File shape: `{ "version": 1, "proxy": {...}, "dns": {...}, "connectivity": {...} }`.
/// A damaged file is reported, never overwritten or deleted.
#[derive(Debug, Clone)]
pub struct JsonFileAutoStore {
    path: PathBuf,
}

#[derive(Serialize)]
struct FileOut<'a> {
    version: u32,
    #[serde(flatten)]
    config: &'a AutoConfig,
}

fn io_error(path: &Path, error: &io::Error) -> AutoConfigError {
    AutoConfigError::Io(format!("{}: {error}", path.display()))
}

impl JsonFileAutoStore {
    /// A store at `path` (created on first save).
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl AutoConfigStore for JsonFileAutoStore {
    fn load(&self) -> Result<AutoConfig, AutoConfigError> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(AutoConfig::default()),
            Err(e) => return Err(io_error(&self.path, &e)),
        };
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|e| AutoConfigError::Corrupt(e.to_string()))?;
        let version = value
            .get("version")
            .and_then(Value::as_u64)
            .filter(|v| *v >= 1)
            .ok_or_else(|| {
                AutoConfigError::Corrupt("missing or invalid `version` field".to_owned())
            })?;
        if version > u64::from(SCHEMA_VERSION) {
            return Err(AutoConfigError::UnsupportedVersion {
                found: u32::try_from(version).unwrap_or(u32::MAX),
                supported: SCHEMA_VERSION,
            });
        }
        serde_json::from_value(value).map_err(|e| AutoConfigError::Corrupt(e.to_string()))
    }

    fn save(&self, config: &AutoConfig) -> Result<(), AutoConfigError> {
        let bytes = serde_json::to_vec_pretty(&FileOut {
            version: SCHEMA_VERSION,
            config,
        })
        .map_err(|e| AutoConfigError::Io(e.to_string()))?;
        write_atomic(&self.path, &bytes).map_err(|e| io_error(&self.path, &e))
    }
}
