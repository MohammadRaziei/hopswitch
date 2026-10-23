//! The real [`CommandRunner`]: starts actual processes.

use std::io;
use std::process::Command;

use hopswitch_core::{CommandOutput, CommandRunner, SystemError};

/// Runs programs with `std::process::Command`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessRunner;

impl CommandRunner for ProcessRunner {
    fn run(&self, program: &str, args: &[String]) -> Result<CommandOutput, SystemError> {
        let mut command = Command::new(program);
        command.args(args);
        hide_console_window(&mut command);

        match command.output() {
            Ok(output) => Ok(CommandOutput {
                success: output.status.success(),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            }),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Err(SystemError::Unsupported(
                format!("`{program}` was not found"),
            )),
            Err(e) => Err(SystemError::CommandFailed {
                command: program.to_owned(),
                details: e.to_string(),
            }),
        }
    }
}

/// Keeps a console window from flashing when the app has no console of its own.
#[cfg(windows)]
fn hide_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console_window(_command: &mut Command) {}
