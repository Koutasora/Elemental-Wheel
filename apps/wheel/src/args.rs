//! Command line parsing (pure, no Win32).

use wheel_core::{DEFAULT_OUTER_SECTORS, DEFAULT_SCALE_PCT, clamp_outer_sectors, clamp_scale_pct};

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
pub enum Command {
    /// No arguments (or options without `--demo`): print the status line.
    Info,
    Version,
    Demo(DemoOptions),
}

/// Parses the arguments (without the program name). The error is a message for the user.
pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Command, String> {
    let mut args = args.into_iter();
    let mut demo = false;
    let mut opts = DemoOptions {
        theme: Theme::System,
        scale_pct: DEFAULT_SCALE_PCT,
        sectors: DEFAULT_OUTER_SECTORS,
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--version" => return Ok(Command::Version),
            "--demo" => demo = true,
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
    Ok(if demo {
        Command::Demo(opts)
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
        ] {
            assert!(parse_strs(args).is_err(), "{args:?}");
        }
    }
}
