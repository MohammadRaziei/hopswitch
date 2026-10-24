//! File-based persistence for Hopswitch: profiles, the active session and the auto settings.

mod atomic;
mod auto_store;
mod repository;
mod session_store;

pub use atomic::write_atomic;
pub use auto_store::JsonFileAutoStore;
pub use repository::JsonFileRepository;
pub use session_store::JsonFileSessionStore;
