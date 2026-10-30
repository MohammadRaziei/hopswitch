//! The `hopswitch` executable: wires the real system into the library.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::Arc;

use hopswitch::cli::{
    decide_launch, default_dir, no_gui_hint, run, usage, Context, Launch, Platform,
    UnsupportedNetwork,
};
use hopswitch::core::SystemNetwork;
use hopswitch::platform_windows::WindowsNetwork;
use hopswitch::probe::NetProber;

/// The bare command (ADR-0012): open the GUI when there is a screen and the app is installed,
/// otherwise show the help and say why no window was opened.
fn bare() -> ExitCode {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(PathBuf::from))
        .unwrap_or_default();
    let launch = decide_launch(
        Platform::current(),
        &|key| std::env::var(key).ok(),
        &exe_dir,
        &Path::is_file,
    );
    match launch {
        Launch::Gui(program) => match Command::new(&program).spawn() {
            Ok(_) => ExitCode::SUCCESS,
            Err(error) => {
                println!("{}\n", usage());
                eprintln!("error: could not start {}: {error}", program.display());
                ExitCode::FAILURE
            }
        },
        Launch::NoGui(reason) => {
            println!("{}\n\n{}", usage(), no_gui_hint(reason));
            ExitCode::SUCCESS
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        return bare();
    }

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
        &Context {
            dir,
            network,
            prober: Arc::new(NetProber::new()),
        },
        &mut std::io::stdout().lock(),
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            match &error {
                hopswitch::cli::CliError::Usage(message) => eprintln!("{message}"),
                hopswitch::cli::CliError::Failure(message) => eprintln!("error: {message}"),
            }
            ExitCode::from(error.exit_code())
        }
    }
}
