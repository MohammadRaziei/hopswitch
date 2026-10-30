//! What can go wrong on the command line.

use std::fmt;

/// A CLI failure, distinguishing "you typed it wrong" from "it did not work".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    /// The command line itself is wrong (exit code 2).
    Usage(String),
    /// The command was understood but failed (exit code 1).
    Failure(String),
}

impl CliError {
    /// The process exit code for this error.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Usage(_) => 2,
            Self::Failure(_) => 1,
        }
    }

    pub(crate) fn usage(message: impl Into<String>) -> Self {
        Self::Usage(message.into())
    }

    pub(crate) fn failure(message: impl fmt::Display) -> Self {
        Self::Failure(message.to_string())
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(message) | Self::Failure(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for CliError {}
