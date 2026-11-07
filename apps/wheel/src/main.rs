mod actions;
mod args;
mod fit;
mod hook;
mod icons;
mod instance;
mod render;
mod settings;
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
                "Elemental Wheel (stage 04). Options for --demo and --run: \
                 [--theme system|light|dark] [--scale 60..140] [--sectors 4..12] \
                 [--config <path>].\n  \
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
            let Some(path) = config_path(options.config.as_deref()) else {
                return ExitCode::from(1);
            };
            match window::run_demo(&options, path) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("wheel: {e}");
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
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

fn config_path(arg: Option<&Path>) -> Option<PathBuf> {
    match settings::resolve_path(arg) {
        Ok(path) => Some(path),
        Err(e) => {
            eprintln!("wheel: {e}");
            None
        }
    }
}

fn run(options: &RunOptions) -> ExitCode {
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
            eprintln!("wheel: {e}");
            return ExitCode::from(1);
        }
    }
    match window::run_background(options, path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("wheel: {e}");
            ExitCode::from(1)
        }
    }
}
