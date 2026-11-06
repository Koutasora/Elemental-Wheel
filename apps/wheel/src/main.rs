use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        None => {
            println!(
                "Elemental Wheel skeleton (stage 01): {} sectors, nothing to run yet.",
                wheel_core::SECTORS
            );
            ExitCode::SUCCESS
        }
        Some("--version") => {
            println!("Elemental Wheel {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("Unknown argument: {other}");
            ExitCode::from(2)
        }
    }
}
