//! Search roots and skip lists, read from `~/.config/envhub/config`.

use std::path::{Path, PathBuf};

// Noise that is never worth walking into.
const BUILTIN_SKIP: &[&str] = &[
    "Library",
    ".Trash",
    "node_modules",
    ".git",
    ".venv",
    "venv",
    "target",
    ".cargo",
    ".rustup",
    ".npm",
    ".cache",
    "Applications",
];

pub const MAX_DEPTH: usize = 8;

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

pub fn config_dir() -> PathBuf {
    match std::env::var_os("XDG_CONFIG_HOME") {
        Some(d) if !d.is_empty() => PathBuf::from(d).join("envhub"),
        _ => home().join(".config/envhub"),
    }
}

pub fn config_file() -> PathBuf {
    config_dir().join("config")
}

/// Holds keys that belong to no repo.
pub fn scratch_file() -> PathBuf {
    config_dir().join("scratch.env")
}

pub const SCRATCH_REPO: &str = "scratch";

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Config {
    /// Directories to scan; empty means the home directory.
    pub roots: Vec<PathBuf>,
    /// Extra directory names to skip.
    pub skip: Vec<String>,
}

impl Config {
    /// Falls back to the defaults when the file is missing or unreadable.
    pub fn load() -> Self {
        let mut cfg = std::fs::read_to_string(config_file())
            .map(|text| Self::parse(&text))
            .unwrap_or_default();
        if cfg.roots.is_empty() {
            cfg.roots.push(home());
        }
        cfg
    }

    /// One directive per line: a path to scan, or `!name` for a directory to skip.
    pub fn parse(text: &str) -> Self {
        let mut cfg = Self::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            match line.strip_prefix('!') {
                Some(skip) => cfg.skip.push(skip.trim().to_owned()),
                None => cfg.roots.push(expand(line)),
            }
        }
        cfg
    }
}

pub fn is_skipped(dir_name: &str, extra: &[String]) -> bool {
    BUILTIN_SKIP.contains(&dir_name) || extra.iter().any(|s| s == dir_name)
}

pub fn expand(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => home().join(rest),
        None if path == "~" => home(),
        None => PathBuf::from(path),
    }
}

/// `/Users/me/code/api` -> `code/api`.
pub fn display_path(path: &Path) -> String {
    path.strip_prefix(home())
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_roots_and_skips() {
        let cfg = Config::parse("# comment\n~/code\n\n/tmp/x\n!dist\n  !  spaced  \n");
        assert_eq!(
            cfg.roots,
            vec![home().join("code"), PathBuf::from("/tmp/x")]
        );
        assert_eq!(cfg.skip, vec!["dist".to_owned(), "spaced".to_owned()]);
    }

    #[test]
    fn empty_config_is_default() {
        assert_eq!(Config::parse("\n# nothing here\n"), Config::default());
    }

    #[test]
    fn skips_builtin_and_configured_names() {
        let cfg = Config::parse("!dist");
        assert!(is_skipped("node_modules", &cfg.skip));
        assert!(is_skipped("dist", &cfg.skip));
        assert!(!is_skipped("src", &cfg.skip));
    }

    #[test]
    fn expands_tilde_only_at_the_start() {
        assert_eq!(expand("~/code"), home().join("code"));
        assert_eq!(expand("~"), home());
        assert_eq!(expand("/etc/~"), PathBuf::from("/etc/~"));
    }
}
