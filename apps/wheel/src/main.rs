mod args;
mod fit;
mod render;
mod theme;
mod window;

use std::process::ExitCode;

use args::Command;

fn main() -> ExitCode {
    match args::parse(std::env::args().skip(1)) {
        Ok(Command::Info) => {
            println!(
                "Elemental Wheel (stage 02): run with --demo [--theme system|light|dark] \
                 [--scale 60..140] [--sectors 4..12] to show the wheel under the cursor."
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
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}
