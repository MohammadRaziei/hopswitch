//! The persisted record that makes "turn it off" restore the *original* state.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// What was there before Hopswitch changed anything, and which profile is active.
///
/// Serialisable so it can be written to disk *before* settings are applied and
/// still be honoured after a crash or restart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session<S> {
    original: S,
    active_id: String,
}

impl<S> Session<S> {
    /// A session whose snapshot is `original` and whose active profile is `active_id`.
    pub fn new(original: S, active_id: impl Into<String>) -> Self {
        Self {
            original,
            active_id: active_id.into(),
        }
    }

    /// The system state captured before the first change.
    pub fn original(&self) -> &S {
        &self.original
    }

    /// Id of the currently active profile.
    pub fn active_id(&self) -> &str {
        &self.active_id
    }

    pub(crate) fn set_active_id(&mut self, id: &str) {
        id.clone_into(&mut self.active_id);
    }
}

/// A failure persisting or clearing a [`Session`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("session storage failed: {0}")]
pub struct StoreError(pub String);

/// Where sessions are persisted (a JSON file in Step 3).
pub trait SessionStore<S>: Send + Sync {
    /// Durably records `session`, replacing any previous one.
    fn save(&self, session: &Session<S>) -> Result<(), StoreError>;
    /// Removes the stored session.
    fn clear(&self) -> Result<(), StoreError>;
}
