//! The configuration of the running program: file path, command line overrides, and reloading
//! when the file changes.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use wheel_core::{Config, Loaded, Source, config};

use crate::args::Overrides;

/// The `--config` path, or `%APPDATA%\Elemental-Wheel\config.json`.
pub fn resolve_path(arg: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(path) = arg {
        return Ok(path.to_path_buf());
    }
    std::env::var_os("APPDATA")
        .map(|appdata| config::default_path(Path::new(&appdata)))
        .ok_or_else(|| "The APPDATA variable is not set; use --config <path>".to_string())
}

/// Command line values win over the file.
pub fn apply(config: &mut Config, overrides: &Overrides) {
    let o = &mut config.options;
    if let Some(v) = overrides.theme {
        o.theme = v;
    }
    if let Some(v) = overrides.scale_pct {
        o.scale_pct = v;
    }
    if let Some(v) = overrides.sectors {
        o.outer_sectors = v;
    }
    if let Some(v) = overrides.hold_ms {
        o.hold_ms = v;
    }
    if let Some(v) = overrides.move_px {
        o.move_px = v;
    }
}

fn source_name(source: Source) -> &'static str {
    match source {
        Source::File => "file",
        Source::Backup => "backup",
        Source::Defaults => "defaults",
        Source::Created => "created",
    }
}

fn report(path: &Path, loaded: &Loaded) {
    eprintln!(
        "config: {} ({})",
        path.display(),
        source_name(loaded.source)
    );
    for warning in &loaded.warnings {
        eprintln!("config warning: {warning}");
    }
    if let Some(error) = &loaded.error {
        eprintln!("config error: {error}");
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Remembers which version of the file is loaded.
pub struct Settings {
    path: PathBuf,
    overrides: Overrides,
    modified: Option<SystemTime>,
}

impl Settings {
    /// Loads the file (creating it with the defaults if missing) and reports the result.
    pub fn load(path: PathBuf, overrides: Overrides) -> (Settings, Arc<Config>) {
        let loaded = config::load(&path);
        report(&path, &loaded);
        let mut config = loaded.config;
        apply(&mut config, &overrides);
        let settings = Settings {
            modified: modified(&path),
            path,
            overrides,
        };
        (settings, Arc::new(config))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reloads the file if its modification time changed. A file that vanished is never
    /// recreated: the current configuration stays.
    pub fn refresh(&mut self) -> Option<Arc<Config>> {
        let now = modified(&self.path)?;
        if self.modified == Some(now) {
            return None;
        }
        self.modified = Some(now);
        let loaded = config::load_existing(&self.path)?;
        report(&self.path, &loaded);
        let mut config = loaded.config;
        apply(&mut config, &self.overrides);
        Some(Arc::new(config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use wheel_core::ThemeChoice;

    fn dir(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("target")
            .join("test-tmp")
            .join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn explicit_path_is_used() {
        assert_eq!(
            resolve_path(Some(Path::new("a.json"))),
            Ok(PathBuf::from("a.json"))
        );
    }

    #[test]
    fn overrides_win_only_when_given() {
        let mut c = Config::default();
        c.options.theme = ThemeChoice::Light;
        c.options.scale_pct = 80;
        apply(
            &mut c,
            &Overrides {
                scale_pct: Some(120),
                hold_ms: Some(300),
                ..Overrides::default()
            },
        );
        assert_eq!(c.options.theme, ThemeChoice::Light);
        assert_eq!(c.options.scale_pct, 120);
        assert_eq!(c.options.hold_ms, 300);
        assert_eq!(c.options.outer_sectors, 12);
    }

    #[test]
    fn refresh_follows_the_modification_time() {
        let path = dir("settings_refresh").join("config.json");
        let overrides = Overrides {
            sectors: Some(6),
            ..Overrides::default()
        };
        let (mut settings, config) = Settings::load(path.clone(), overrides);
        assert_eq!(config.options.outer_sectors, 6);
        assert!(settings.refresh().is_none());

        fs::write(&path, r#"{"options":{"scale_pct":90}}"#).unwrap();
        let file = fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(SystemTime::now() + Duration::from_secs(30))
            .unwrap();
        drop(file);
        let config = settings.refresh().expect("changed file is reloaded");
        assert_eq!(config.options.scale_pct, 90);
        assert_eq!(config.options.outer_sectors, 6);
        assert!(settings.refresh().is_none());

        fs::remove_file(&path).unwrap();
        assert!(settings.refresh().is_none());
        assert!(!path.exists(), "a vanished file is not recreated");
    }
}
