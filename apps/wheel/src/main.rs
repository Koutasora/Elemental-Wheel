mod args;
mod fit;
mod hook;
mod instance;
mod render;
mod theme;
mod watchdog;
mod window;

use std::process::ExitCode;

use args::{Command, RunOptions};

fn main() -> ExitCode {
    match args::parse(std::env::args().skip(1)) {
        Ok(Command::Info) => {
            println!(
                "Elemental Wheel (stage 03). Options for --demo and --run: \
                 [--theme system|light|dark] [--scale 60..140] [--sectors 4..12].\n  \
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
        Ok(Command::Demo(options)) => match window::run_demo(&options) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("wheel: {e}");
                ExitCode::from(1)
            }
        },
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

fn run(options: &RunOptions) -> ExitCode {
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
    match window::run_background(options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("wheel: {e}");
            ExitCode::from(1)
        }
    }
}
