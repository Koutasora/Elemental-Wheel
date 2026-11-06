//! Command line parsing (pure, no Win32).

use wheel_core::{
    DEFAULT_OUTER_SECTORS, DEFAULT_SCALE_PCT, TriggerSettings, clamp_hold_ms, clamp_move_px,
    clamp_outer_sectors, clamp_scale_pct,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DemoOptions {
    pub theme: Theme,
    pub scale_pct: u16,
    pub sectors: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunOptions {
    /// Look of the wheel (the same options as `--demo`).
    pub wheel: DemoOptions,
    pub trigger: TriggerSettings,
    /// Hidden test flag: foreign injected input counts as physical.
    pub accept_injected: bool,
    /// Hidden test flag: stall the mouse hook once so the watchdog has to reinstall it.
    pub stall_hook_once: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// No arguments (or options without a command): print the status line.
    Info,
    Version,
    Demo(DemoOptions),
    /// Background mode with the right-button trigger.
    Run(RunOptions),
    /// Stop the running instance.
    Quit,
}

/// Parses the arguments (without the program name). The error is a message for the user.
pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Command, String> {
    let mut args = args.into_iter();
    let (mut demo, mut run, mut quit) = (false, false, false);
    let mut opts = DemoOptions {
        theme: Theme::System,
        scale_pct: DEFAULT_SCALE_PCT,
        sectors: DEFAULT_OUTER_SECTORS,
    };
    let mut trigger = TriggerSettings::default();
    let mut accept_injected = false;
    let mut stall_hook_once = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--version" => return Ok(Command::Version),
            "--demo" => demo = true,
            "--run" => run = true,
            "--quit" => quit = true,
            "--accept-injected" => accept_injected = true,
            "--test-stall-hook" => stall_hook_once = true,
            "--hold-ms" => {
                let n = number(&value(&mut args, &arg)?, &arg)?;
                trigger.hold_ms = clamp_hold_ms(n.clamp(0, u32::MAX.into()) as u32);
            }
            "--move-px" => {
                let n = number(&value(&mut args, &arg)?, &arg)?;
                trigger.move_px = clamp_move_px(n.clamp(i32::MIN.into(), i32::MAX.into()) as i32);
            }
            "--theme" => {
                opts.theme = match value(&mut args, &arg)?.as_str() {
                    "system" => Theme::System,
                    "light" => Theme::Light,
                    "dark" => Theme::Dark,
                    other => return Err(format!("Invalid value for --theme: {other}")),
                }
            }
            "--scale" => {
                let n = number(&value(&mut args, &arg)?, &arg)?;
                opts.scale_pct = clamp_scale_pct(n.clamp(0, u16::MAX.into()) as u16);
            }
            "--sectors" => {
                let n = number(&value(&mut args, &arg)?, &arg)?;
                opts.sectors = clamp_outer_sectors(n.clamp(0, u8::MAX.into()) as u8);
            }
            other => return Err(format!("Unknown argument: {other}")),
        }
    }
    if [demo, run, quit].iter().filter(|&&c| c).count() > 1 {
        return Err("Use only one of --demo, --run, --quit".to_string());
    }
    Ok(if demo {
        Command::Demo(opts)
    } else if run {
        Command::Run(RunOptions {
            wheel: opts,
            trigger,
            accept_injected,
            stall_hook_once,
        })
    } else if quit {
        Command::Quit
    } else {
        Command::Info
    })
}

fn value(args: &mut impl Iterator<Item = String>, option: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("Missing value for {option}"))
}

/// Parses an integer. Any valid integer is accepted, however large: values beyond `i64` are
/// saturated and wheel-core's clamp functions decide the final value. Only non-numeric text
/// is an error.
fn number(text: &str, option: &str) -> Result<i64, String> {
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("Invalid number for {option}: {text}"));
    }
    Ok(text.parse().unwrap_or(if text.starts_with('-') {
        i64::MIN
    } else {
        i64::MAX
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_strs(args: &[&str]) -> Result<Command, String> {
        parse(args.iter().map(|s| s.to_string()))
    }

    fn demo(args: &[&str]) -> DemoOptions {
        match parse_strs(args) {
            Ok(Command::Demo(o)) => o,
            other => panic!("expected demo, got {other:?}"),
        }
    }

    #[test]
    fn defaults() {
        let o = demo(&["--demo"]);
        assert_eq!(o.theme, Theme::System);
        assert_eq!(o.scale_pct, 100);
        assert_eq!(o.sectors, 12);
    }

    #[test]
    fn no_args_and_version() {
        assert_eq!(parse_strs(&[]), Ok(Command::Info));
        assert_eq!(parse_strs(&["--version"]), Ok(Command::Version));
    }

    #[test]
    fn parses_values() {
        let o = demo(&[
            "--demo",
            "--theme",
            "dark",
            "--scale",
            "140",
            "--sectors",
            "6",
        ]);
        assert_eq!(o.theme, Theme::Dark);
        assert_eq!(o.scale_pct, 140);
        assert_eq!(o.sectors, 6);
        assert_eq!(demo(&["--theme", "light", "--demo"]).theme, Theme::Light);
        assert_eq!(demo(&["--demo", "--theme", "system"]).theme, Theme::System);
    }

    #[test]
    fn clamps_out_of_range() {
        assert_eq!(demo(&["--demo", "--scale", "10"]).scale_pct, 60);
        assert_eq!(demo(&["--demo", "--scale", "999"]).scale_pct, 140);
        assert_eq!(demo(&["--demo", "--scale", "70000"]).scale_pct, 140);
        assert_eq!(demo(&["--demo", "--scale", "-5"]).scale_pct, 60);
        assert_eq!(demo(&["--demo", "--sectors", "1"]).sectors, 4);
        assert_eq!(demo(&["--demo", "--sectors", "40"]).sectors, 12);
        assert_eq!(demo(&["--demo", "--sectors", "300"]).sectors, 12);
        let huge = "99999999999999999999999999";
        assert_eq!(demo(&["--demo", "--scale", huge]).scale_pct, 140);
        assert_eq!(demo(&["--demo", "--sectors", huge]).sectors, 12);
        assert_eq!(
            demo(&["--demo", "--scale", "-99999999999999999999999999"]).scale_pct,
            60
        );
        assert_eq!(
            demo(&["--demo", "--sectors", "-99999999999999999999999999"]).sectors,
            4
        );
    }

    fn run(args: &[&str]) -> RunOptions {
        match parse_strs(args) {
            Ok(Command::Run(o)) => o,
            other => panic!("expected run, got {other:?}"),
        }
    }

    #[test]
    fn run_defaults() {
        let o = run(&["--run"]);
        assert_eq!(o.wheel.theme, Theme::System);
        assert_eq!(o.wheel.scale_pct, 100);
        assert_eq!(o.wheel.sectors, 12);
        assert_eq!(o.trigger, TriggerSettings::default());
        assert!(!o.accept_injected);
    }

    #[test]
    fn run_parses_values() {
        let o = run(&[
            "--run",
            "--theme",
            "dark",
            "--scale",
            "80",
            "--sectors",
            "8",
            "--hold-ms",
            "250",
            "--move-px",
            "10",
            "--accept-injected",
            "--test-stall-hook",
        ]);
        assert_eq!(o.wheel.theme, Theme::Dark);
        assert_eq!(o.wheel.scale_pct, 80);
        assert_eq!(o.wheel.sectors, 8);
        assert_eq!(o.trigger.hold_ms, 250);
        assert_eq!(o.trigger.move_px, 10);
        assert!(o.accept_injected);
        assert!(o.stall_hook_once);
    }

    #[test]
    fn run_clamps_trigger_values() {
        assert_eq!(run(&["--run", "--hold-ms", "1"]).trigger.hold_ms, 50);
        assert_eq!(run(&["--run", "--hold-ms", "99999"]).trigger.hold_ms, 2000);
        assert_eq!(run(&["--run", "--hold-ms", "-5"]).trigger.hold_ms, 50);
        assert_eq!(run(&["--run", "--move-px", "0"]).trigger.move_px, 1);
        assert_eq!(run(&["--run", "--move-px", "500"]).trigger.move_px, 50);
        assert_eq!(run(&["--run", "--move-px", "-3"]).trigger.move_px, 1);
        let huge = "99999999999999999999999999";
        assert_eq!(run(&["--run", "--hold-ms", huge]).trigger.hold_ms, 2000);
        assert_eq!(run(&["--run", "--move-px", huge]).trigger.move_px, 50);
    }

    #[test]
    fn quit_and_conflicts() {
        assert_eq!(parse_strs(&["--quit"]), Ok(Command::Quit));
        for args in [
            &["--run", "--demo"][..],
            &["--run", "--quit"],
            &["--demo", "--quit"],
        ] {
            assert!(parse_strs(args).is_err(), "{args:?}");
        }
        // Run options alone do not start anything.
        assert_eq!(parse_strs(&["--hold-ms", "200"]), Ok(Command::Info));
    }

    #[test]
    fn rejects_bad_input() {
        for args in [
            &["--demo", "--scale", "abc"][..],
            &["--demo", "--sectors", "1.5"],
            &["--demo", "--theme", "blue"],
            &["--demo", "--scale"],
            &["--theme"],
            &["--bogus"],
            &["--demo", "--sectors", ""],
            &["--run", "--hold-ms", "abc"],
            &["--run", "--move-px"],
            &["--run", "--hold-ms", "1.5"],
        ] {
            assert!(parse_strs(args).is_err(), "{args:?}");
        }
    }
}
