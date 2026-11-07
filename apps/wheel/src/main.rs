mod actions;
mod args;
mod backdrop;
mod fit;
mod guard;
mod hook;
mod icons;
mod instance;
mod render;
mod settings;
mod stats;
mod theme;
mod watch;
mod watchdog;
mod window;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use args::{Command, RunOptions};

fn main() -> ExitCode {
    match args::parse(std::env::args().skip(1)) {
        Ok(Command::Info) => {
            println!(
                "Elemental Wheel (stage 05). Options for --demo and --run: \
                 [--theme system|light|dark] [--scale 60..140] [--sectors 4..12] \
                 [--config <path>] [--log <path>].\n  \
                 They override config.json (default %APPDATA%\\Elemental-Wheel\\config.json).\n  \
                 --demo   show the wheel under the cursor\n  \
                 --run    run in the background; holding the right mouse button opens the wheel \
                 [--hold-ms 50..2000] [--move-px 1..50]\n  \
                 --quit   stop the running instance"
            );
            ExitCode::SUCCESS
        }
        Ok(Command::Version) => {
            println!("Elemental Wheel {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Ok(Command::Demo(options)) => {
            start_log(options.log.as_deref());
            guard::arm(options.test_panic);
            let Some(path) = config_path(options.config.as_deref()) else {
                return ExitCode::from(1);
            };
            match window::run_demo(&options, path) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    wheel_core::log_line!("wheel: {e}");
                    ExitCode::from(1)
                }
            }
        }
        Ok(Command::Run(options)) => run(&options),
        Ok(Command::Quit) => {
            if instance::quit_running() {
                println!("stopped");
                ExitCode::SUCCESS
            } else {
                println!("not running");
                ExitCode::from(1)
            }
        }
        Err(message) => {
            wheel_core::log_line!("{message}");
            ExitCode::from(2)
        }
    }
}

fn config_path(arg: Option<&Path>) -> Option<PathBuf> {
    match settings::resolve_path(arg) {
        Ok(path) => Some(path),
        Err(e) => {
            wheel_core::log_line!("wheel: {e}");
            None
        }
    }
}

/// Opens the log file and routes panics to it. A log that cannot be opened is not fatal.
fn start_log(arg: Option<&Path>) {
    wheel_core::log::install_panic_hook();
    let Some(path) = arg
        .map(Path::to_path_buf)
        .or_else(|| wheel_core::log::default_path("wheel.log"))
    else {
        wheel_core::log_line!("wheel: LOCALAPPDATA is not set; no log file");
        return;
    };
    if let Err(e) = wheel_core::log::init(&path) {
        wheel_core::log_line!("wheel: cannot open the log {}: {e}", path.display());
    }
}

fn run(options: &RunOptions) -> ExitCode {
    start_log(options.wheel.log.as_deref());
    guard::arm(options.wheel.test_panic);
    let Some(path) = config_path(options.wheel.config.as_deref()) else {
        return ExitCode::from(1);
    };
    match instance::acquire() {
        Ok(true) => {}
        Ok(false) => {
            println!("already running");
            return ExitCode::from(1);
        }
        Err(e) => {
            wheel_core::log_line!("wheel: {e}");
            return ExitCode::from(1);
        }
    }
    wheel_core::log_line!(
        "wheel {} started (pid {}, config {})",
        env!("CARGO_PKG_VERSION"),
        std::process::id(),
        path.display()
    );
    match window::run_background(options, path) {
        Ok(()) => {
            wheel_core::log_line!("wheel stopped");
            ExitCode::SUCCESS
        }
        Err(e) => {
            wheel_core::log_line!("wheel: {e}");
            ExitCode::from(1)
        }
    }
}
