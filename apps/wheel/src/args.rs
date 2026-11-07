//! Command line parsing (pure, no Win32).

use std::path::PathBuf;

use wheel_core::{ThemeChoice, clamp_hold_ms, clamp_move_px, clamp_outer_sectors, clamp_scale_pct};

/// Values given on the command line; they override the config file only when present.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Overrides {
    pub theme: Option<ThemeChoice>,
    pub scale_pct: Option<u16>,
    pub sectors: Option<u8>,
    pub hold_ms: Option<u32>,
    pub move_px: Option<i32>,
}

/// Hidden test flag `--test-panic`: where the first panic is raised on purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanicTest {
    /// In the hook state code, on the first right-button press the trigger handles.
    Hook,
    /// In the wheel window procedure, on the first open (`--demo`: the first poll).
    Window,
    /// In the first action thread, before the action runs.
    Action,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DemoOptions {
    pub overrides: Overrides,
    /// `--config`; the default is `%APPDATA%\Elemental-Wheel\config.json`.
    pub config: Option<PathBuf>,
    /// `--log`; the default is `%LOCALAPPDATA%\Elemental-Wheel\wheel.log`.
    pub log: Option<PathBuf>,
    /// Hidden test flag: one deliberate panic to check that it is caught.
    pub test_panic: Option<PanicTest>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOptions {
    /// Look of the wheel and trigger values (the same options as `--demo`, plus the trigger).
    pub wheel: DemoOptions,
    /// Hidden test flag: foreign injected input counts as physical.
    pub accept_injected: bool,
    /// Hidden test flag: stall the mouse hook once so the watchdog has to reinstall it.
    pub stall_hook_once: bool,
    /// Hidden test flag: log open timings and process memory (see `stats`).
    pub stats: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
    let mut o = Overrides::default();
    let mut config = None;
    let mut log = None;
    let mut accept_injected = false;
    let mut stall_hook_once = false;
    let mut stats = false;
    let mut test_panic = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--version" => return Ok(Command::Version),
            "--demo" => demo = true,
            "--run" => run = true,
            "--quit" => quit = true,
            "--accept-injected" => accept_injected = true,
            "--test-stall-hook" => stall_hook_once = true,
            "--stats" => stats = true,
            "--test-panic" => {
                test_panic = Some(match value(&mut args, &arg)?.as_str() {
                    "hook" => PanicTest::Hook,
                    "window" => PanicTest::Window,
                    "action" => PanicTest::Action,
                    other => return Err(format!("Invalid value for --test-panic: {other}")),
                })
            }
            "--config" => {
                let path = value(&mut args, &arg)?;
                if path.is_empty() {
                    return Err("Invalid value for --config: empty path".to_string());
                }
                config = Some(PathBuf::from(path));
            }
            "--log" => {
                let path = value(&mut args, &arg)?;
                if path.is_empty() {
                    return Err("Invalid value for --log: empty path".to_string());
                }
                log = Some(PathBuf::from(path));
            }
            "--hold-ms" => {
                let n = number(&value(&mut args, &arg)?, &arg)?;
                o.hold_ms = Some(clamp_hold_ms(n.clamp(0, u32::MAX.into()) as u32));
            }
            "--move-px" => {
                let n = number(&value(&mut args, &arg)?, &arg)?;
                o.move_px = Some(clamp_move_px(
                    n.clamp(i32::MIN.into(), i32::MAX.into()) as i32
                ));
            }
            "--theme" => {
                o.theme = Some(match value(&mut args, &arg)?.as_str() {
                    "system" => ThemeChoice::System,
                    "light" => ThemeChoice::Light,
                    "dark" => ThemeChoice::Dark,
                    other => return Err(format!("Invalid value for --theme: {other}")),
                })
            }
            "--scale" => {
                let n = number(&value(&mut args, &arg)?, &arg)?;
                o.scale_pct = Some(clamp_scale_pct(n.clamp(0, u16::MAX.into()) as u16));
            }
            "--sectors" => {
                let n = number(&value(&mut args, &arg)?, &arg)?;
                o.sectors = Some(clamp_outer_sectors(n.clamp(0, u8::MAX.into()) as u8));
            }
            other => return Err(format!("Unknown argument: {other}")),
        }
    }
    if [demo, run, quit].iter().filter(|&&c| c).count() > 1 {
        return Err("Use only one of --demo, --run, --quit".to_string());
    }
    let wheel = DemoOptions {
        overrides: o,
        config,
        log,
        test_panic,
    };
    if demo && matches!(test_panic, Some(PanicTest::Hook | PanicTest::Action)) {
        return Err("--test-panic hook and action need --run".to_string());
    }
    if demo && stats {
        return Err("--stats needs --run".to_string());
    }
    Ok(if demo {
        Command::Demo(wheel)
    } else if run {
        Command::Run(RunOptions {
            wheel,
            accept_injected,
            stall_hook_once,
            stats,
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

    fn run(args: &[&str]) -> RunOptions {
        match parse_strs(args) {
            Ok(Command::Run(o)) => o,
            other => panic!("expected run, got {other:?}"),
        }
    }

    #[test]
    fn nothing_overrides_by_default() {
        let o = demo(&["--demo"]);
        assert_eq!(o.overrides, Overrides::default());
        assert_eq!(o.config, None);
        assert_eq!(o.log, None);
        let o = run(&["--run"]);
        assert_eq!(o.wheel.overrides, Overrides::default());
        assert!(!o.accept_injected);
        assert!(!o.stats);
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
        ])
        .overrides;
        assert_eq!(o.theme, Some(ThemeChoice::Dark));
        assert_eq!(o.scale_pct, Some(140));
        assert_eq!(o.sectors, Some(6));
        let theme = |args: &[&str]| demo(args).overrides.theme;
        assert_eq!(
            theme(&["--theme", "light", "--demo"]),
            Some(ThemeChoice::Light)
        );
        assert_eq!(
            theme(&["--demo", "--theme", "system"]),
            Some(ThemeChoice::System)
        );
    }

    #[test]
    fn config_path() {
        assert_eq!(
            demo(&["--demo", "--config", "C:\\t\\c.json"]).config,
            Some(PathBuf::from("C:\\t\\c.json"))
        );
        assert_eq!(
            run(&["--config", "x.json", "--run"]).wheel.config,
            Some(PathBuf::from("x.json"))
        );
        assert!(parse_strs(&["--run", "--config"]).is_err());
        assert!(parse_strs(&["--run", "--config", ""]).is_err());
    }

    #[test]
    fn log_path() {
        assert_eq!(
            demo(&["--demo", "--log", "dir/w.log"]).log,
            Some(PathBuf::from("dir/w.log"))
        );
        assert_eq!(
            run(&["--log", "w.log", "--run"]).wheel.log,
            Some(PathBuf::from("w.log"))
        );
        assert!(parse_strs(&["--run", "--log"]).is_err());
        assert!(parse_strs(&["--run", "--log", ""]).is_err());
    }

    #[test]
    fn clamps_out_of_range() {
        let scale = |v: &str| demo(&["--demo", "--scale", v]).overrides.scale_pct;
        let sectors = |v: &str| demo(&["--demo", "--sectors", v]).overrides.sectors;
        assert_eq!(scale("10"), Some(60));
        assert_eq!(scale("999"), Some(140));
        assert_eq!(scale("70000"), Some(140));
        assert_eq!(scale("-5"), Some(60));
        assert_eq!(sectors("1"), Some(4));
        assert_eq!(sectors("40"), Some(12));
        assert_eq!(sectors("300"), Some(12));
        let huge = "99999999999999999999999999";
        assert_eq!(scale(huge), Some(140));
        assert_eq!(sectors(huge), Some(12));
        assert_eq!(scale("-99999999999999999999999999"), Some(60));
        assert_eq!(sectors("-99999999999999999999999999"), Some(4));
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
        let v = o.wheel.overrides;
        assert_eq!(v.theme, Some(ThemeChoice::Dark));
        assert_eq!(v.scale_pct, Some(80));
        assert_eq!(v.sectors, Some(8));
        assert_eq!(v.hold_ms, Some(250));
        assert_eq!(v.move_px, Some(10));
        assert!(o.accept_injected);
        assert!(o.stall_hook_once);
    }

    #[test]
    fn test_panic_flag() {
        assert_eq!(demo(&["--demo"]).test_panic, None);
        assert_eq!(run(&["--run"]).wheel.test_panic, None);
        for (text, kind) in [
            ("hook", PanicTest::Hook),
            ("window", PanicTest::Window),
            ("action", PanicTest::Action),
        ] {
            assert_eq!(
                run(&["--run", "--test-panic", text]).wheel.test_panic,
                Some(kind)
            );
        }
        assert_eq!(
            demo(&["--test-panic", "window", "--demo"]).test_panic,
            Some(PanicTest::Window)
        );
        assert!(parse_strs(&["--demo", "--test-panic", "hook"]).is_err());
        assert!(parse_strs(&["--demo", "--test-panic", "action"]).is_err());
        assert!(parse_strs(&["--run", "--test-panic"]).is_err());
        assert!(parse_strs(&["--run", "--test-panic", "bogus"]).is_err());
    }

    #[test]
    fn stats_flag() {
        assert!(run(&["--run", "--stats"]).stats);
        assert!(run(&["--stats", "--run"]).stats);
        assert!(parse_strs(&["--demo", "--stats"]).is_err());
    }

    #[test]
    fn run_clamps_trigger_values() {
        let hold = |v: &str| run(&["--run", "--hold-ms", v]).wheel.overrides.hold_ms;
        let moved = |v: &str| run(&["--run", "--move-px", v]).wheel.overrides.move_px;
        assert_eq!(hold("1"), Some(50));
        assert_eq!(hold("99999"), Some(2000));
        assert_eq!(hold("-5"), Some(50));
        assert_eq!(moved("0"), Some(1));
        assert_eq!(moved("500"), Some(50));
        assert_eq!(moved("-3"), Some(1));
        let huge = "99999999999999999999999999";
        assert_eq!(hold(huge), Some(2000));
        assert_eq!(moved(huge), Some(50));
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
