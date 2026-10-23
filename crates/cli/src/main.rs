//! The `hopswitch` executable: wires the real system into the library.

use std::process::ExitCode;
use std::sync::Arc;

use hopswitch_cli::{default_dir, run, Context, Platform, UnsupportedNetwork};
use hopswitch_core::SystemNetwork;
use hopswitch_platform_windows::WindowsNetwork;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let Some(dir) = default_dir(Platform::current(), &|key| std::env::var(key).ok()) else {
        eprintln!("error: cannot locate a settings folder; set HOPSWITCH_DIR");
        return ExitCode::FAILURE;
    };
    let network: Arc<dyn SystemNetwork> = if cfg!(windows) {
        Arc::new(WindowsNetwork::system())
    } else {
        Arc::new(UnsupportedNetwork)
    };

    match run(
        &args,
        &Context { dir, network },
        &mut std::io::stdout().lock(),
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            match &error {
                hopswitch_cli::CliError::Usage(message) => eprintln!("{message}"),
                hopswitch_cli::CliError::Failure(message) => eprintln!("error: {message}"),
            }
            ExitCode::from(error.exit_code())
        }
    }
}
