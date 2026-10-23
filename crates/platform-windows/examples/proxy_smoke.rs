//! Try the Windows proxy adapter for real.
//!
//! ```text
//! cargo run -p hopswitch-platform-windows --example proxy_smoke                 # read-only
//! cargo run -p hopswitch-platform-windows --example proxy_smoke -- apply v2rayn-socks
//! cargo run -p hopswitch-platform-windows --example proxy_smoke -- restore      # after a crash
//! ```
//!
//! `apply` snapshots your current proxy to a session file, applies the preset,
//! waits for Enter, then restores exactly what you had.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::io::BufRead;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use hopswitch_core::{proxy_preset, ProxyService, ProxyState, SystemNetwork};
use hopswitch_platform_windows::WindowsNetwork;
use hopswitch_storage::JsonFileSessionStore;

fn main() -> ExitCode {
    match run(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let network = Arc::new(WindowsNetwork::system());
    let path: PathBuf = std::env::temp_dir().join("hopswitch-smoke-session-proxy.json");
    let store = Arc::new(JsonFileSessionStore::new(path));
    let mut service = ProxyService::new(network.clone(), store.clone());

    let show = |label: &str| -> Result<(), String> {
        let state = network.read_proxy().map_err(|e| e.to_string())?;
        println!("{label}: {state:?}");
        Ok(())
    };

    match args.first().map(String::as_str) {
        None => show("current proxy"),
        Some("restore") => {
            let session = store
                .load::<ProxyState>()
                .map_err(|e| e.to_string())?
                .ok_or("no saved session to restore")?;
            service.resume(session);
            service.deactivate().map_err(|e| e.to_string())?;
            show("restored")
        }
        Some("apply") => {
            let id = args.get(1).ok_or("usage: apply <preset-id>")?;
            let preset = proxy_preset(id).ok_or_else(|| format!("unknown preset `{id}`"))?;
            let profile = preset.to_profile(preset.id()).map_err(|e| e.to_string())?;

            show("before")?;
            service.activate(&profile).map_err(|e| e.to_string())?;
            show("applied")?;
            println!("Press Enter to restore your original settings...");
            std::io::stdin()
                .lock()
                .lines()
                .next()
                .transpose()
                .map_err(|e| e.to_string())?;
            service.deactivate().map_err(|e| e.to_string())?;
            show("restored")
        }
        Some(other) => Err(format!(
            "unknown command `{other}` (use: apply <id> | restore)"
        )),
    }
}
