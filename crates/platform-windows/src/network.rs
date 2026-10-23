//! [`SystemNetwork`] for Windows.

use std::sync::Arc;

use base64::Engine;
use hopswitch_core::{CommandRunner, DnsState, ProxyState, SystemError, SystemNetwork};

use crate::registry::{self, RegistryValues};
use crate::ProcessRunner;

const POWERSHELL: &str = "powershell";

const DNS_UNSUPPORTED: &str = "changing DNS on Windows needs administrator rights and \
     arrives together with the elevated helper";

/// Reads and writes the Windows user proxy (registry + `WinINet` refresh).
///
/// DNS is not implemented yet: it needs administrator rights, which the
/// elevated helper provides.
pub struct WindowsNetwork {
    runner: Arc<dyn CommandRunner>,
}

impl WindowsNetwork {
    /// An adapter that runs `PowerShell` through `runner`.
    pub fn new(runner: Arc<dyn CommandRunner>) -> Self {
        Self { runner }
    }

    /// An adapter that runs the real `PowerShell`.
    pub fn system() -> Self {
        Self::new(Arc::new(ProcessRunner))
    }

    fn powershell(&self, script: &str) -> Result<String, SystemError> {
        // UTF-16LE + base64 avoids every command-line quoting problem.
        let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        let args = [
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
            &encoded,
        ]
        .map(str::to_owned);

        let output = self.runner.run(POWERSHELL, &args)?;
        if output.success {
            return Ok(output.stdout);
        }

        let details = output.stderr.trim().to_owned();
        let lowered = details.to_lowercase();
        if lowered.contains("access is denied") || lowered.contains("permissiondenied") {
            Err(SystemError::PermissionDenied(details))
        } else {
            Err(SystemError::CommandFailed {
                command: POWERSHELL.to_owned(),
                details,
            })
        }
    }
}

impl SystemNetwork for WindowsNetwork {
    fn read_proxy(&self) -> Result<ProxyState, SystemError> {
        let stdout = self.powershell(&registry::read_script())?;
        let values: RegistryValues =
            serde_json::from_str(stdout.trim().trim_start_matches('\u{feff}')).map_err(|e| {
                SystemError::CommandFailed {
                    command: POWERSHELL.to_owned(),
                    details: format!("unexpected output: {e}"),
                }
            })?;
        Ok(registry::parse_state(&values))
    }

    fn write_proxy(&self, state: &ProxyState) -> Result<(), SystemError> {
        let ops = registry::plan_write(state)?;
        self.powershell(&registry::render_write(&ops)).map(|_| ())
    }

    fn read_dns(&self) -> Result<DnsState, SystemError> {
        Err(SystemError::Unsupported(DNS_UNSUPPORTED.to_owned()))
    }

    fn write_dns(&self, _state: &DnsState) -> Result<(), SystemError> {
        Err(SystemError::Unsupported(DNS_UNSUPPORTED.to_owned()))
    }
}
