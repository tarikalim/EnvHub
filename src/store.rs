//! The `KEY=VALUE` index: reading it out of `.env` files and writing it back.

use std::io;
use std::path::{Path, PathBuf};

use crate::config::{self, Config, SCRATCH_REPO};

/// One `KEY=VALUE` line of one file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Home-relative directory of the file, or [`SCRATCH_REPO`].
    pub repo: String,
    pub file: PathBuf,
    /// 0-based.
    pub line: usize,
    pub key: String,
    pub value: String,
    /// The line reads `# KEY=VALUE` in the file.
    pub commented: bool,
    // Precomputed so that searching allocates nothing.
    haystack: String,
    value_lower: String,
}

impl Entry {
    pub fn new(
        repo: String,
        file: PathBuf,
        line: usize,
        key: String,
        value: String,
        commented: bool,
    ) -> Self {
        let haystack = format!("{repo} {key}").to_lowercase();
        let value_lower = value.to_lowercase();
        Self {
            repo,
            file,
            line,
            key,
            value,
            commented,
            haystack,
            value_lower,
        }
    }

    /// Call after changing [`Entry::value`].
    pub fn refresh_value_index(&mut self) {
        self.value_lower = self.value.to_lowercase();
    }

    /// Every term must match the key, the repo or, optionally, the value.
    /// Terms are expected to be lowercase.
    pub fn matches(&self, terms: &[String], search_values: bool) -> bool {
        terms
            .iter()
            .all(|t| self.haystack.contains(t) || (search_values && self.value_lower.contains(t)))
    }

    fn as_line(&self) -> String {
        if self.commented {
            format!("# {}={}", self.key, self.value)
        } else {
            format!("{}={}", self.key, self.value)
        }
    }
}

/// Parses `KEY=VALUE`, including commented-out lines, into key, value and
/// whether it was commented.
pub fn parse_line(raw: &str) -> Option<(String, String, bool)> {
    let mut line = raw.trim();
    let commented = line.starts_with('#');
    if commented {
        line = line.trim_start_matches('#').trim();
    }
    let (key, value) = line.split_once('=')?;
    let key = key.trim_end();

    let mut chars = key.chars();
    let first = chars.next()?;
    let valid = (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !valid {
        return None;
    }
    Some((key.to_owned(), value.trim().to_owned(), commented))
}

/// `.env`, `.env.local`, … but not the checked-in templates.
pub fn is_env_file(name: &str) -> bool {
    name.starts_with(".env") && !name.ends_with(".example") && !name.ends_with(".sample")
}

fn read_env_file(path: &Path, repo: &str, out: &mut Vec<Entry>) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    for (line, raw) in text.lines().enumerate() {
        if let Some((key, value, commented)) = parse_line(raw) {
            out.push(Entry::new(
                repo.to_owned(),
                path.to_path_buf(),
                line,
                key,
                value,
                commented,
            ));
        }
    }
}

/// Every entry under the configured roots, sorted by repo then key.
pub fn scan(cfg: &Config) -> Vec<Entry> {
    let mut entries = Vec::new();
    let scratch = config::scratch_file();
    if scratch.is_file() {
        read_env_file(&scratch, SCRATCH_REPO, &mut entries);
    }

    for root in &cfg.roots {
        let walk = ignore::WalkBuilder::new(root)
            .hidden(false)
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false)
            .follow_links(false)
            .max_depth(Some(config::MAX_DEPTH))
            .filter_entry({
                let skip = cfg.skip.clone();
                move |e| !config::is_skipped(&e.file_name().to_string_lossy(), &skip)
            })
            .build();

        for dir_entry in walk.flatten() {
            if !dir_entry.file_type().is_some_and(|t| t.is_file()) {
                continue;
            }
            if !is_env_file(&dir_entry.file_name().to_string_lossy()) {
                continue;
            }
            let path = dir_entry.path();
            if path == scratch {
                continue; // read above, and it is not a repo
            }
            let repo = path.parent().map(config::display_path).unwrap_or_default();
            read_env_file(path, &repo, &mut entries);
        }
    }

    entries.sort_by(|a, b| (&a.repo, &a.key).cmp(&(&b.repo, &b.key)));
    entries
}

// Via a temporary file in the same directory: a crash or a full disk must not
// leave a half-written .env behind.
fn write_atomically(path: &Path, text: &str) -> io::Result<()> {
    let tmp = path.with_extension(format!("envhub-{}.tmp", std::process::id()));
    std::fs::write(&tmp, text)?;
    if let Ok(meta) = std::fs::metadata(path) {
        let _ = std::fs::set_permissions(&tmp, meta.permissions());
    }
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = std::fs::remove_file(&tmp);
            Err(err)
        }
    }
}

/// Rewrites one line, leaving the rest of the file untouched. Fails instead of
/// guessing when the target line no longer holds the same key.
pub fn write_back(entry: &Entry) -> io::Result<()> {
    let text = std::fs::read_to_string(&entry.file)?;
    let mut lines: Vec<&str> = text.lines().collect();

    let stale =
        |what: &str| io::Error::new(io::ErrorKind::InvalidData, format!("{what}, rescan first"));
    let current = lines
        .get(entry.line)
        .ok_or_else(|| stale("file got shorter"))?;
    match parse_line(current) {
        Some((key, _, _)) if key == entry.key => {}
        _ => return Err(stale("that line changed on disk")),
    }

    let replacement = entry.as_line();
    lines[entry.line] = &replacement;
    let mut joined = lines.join("\n");
    if text.ends_with('\n') {
        joined.push('\n');
    }
    write_atomically(&entry.file, &joined)
}

/// Appends a key to the scratch file.
pub fn append_scratch(key: &str, value: &str) -> io::Result<Entry> {
    let path = config::scratch_file();
    std::fs::create_dir_all(config::config_dir())?;

    let mut text = std::fs::read_to_string(&path).unwrap_or_default();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    let line = text.lines().count();
    text.push_str(&format!("{key}={value}\n"));
    write_atomically(&path, &text)?;

    Ok(Entry::new(
        SCRATCH_REPO.to_owned(),
        path,
        line,
        key.to_owned(),
        value.to_owned(),
        false,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("envhub-test-{name}.env"))
    }

    fn entry(file: &Path, line: usize, key: &str, value: &str, commented: bool) -> Entry {
        Entry::new(
            "t".into(),
            file.to_path_buf(),
            line,
            key.into(),
            value.into(),
            commented,
        )
    }

    #[test]
    fn parses_only_real_keys() {
        assert_eq!(
            parse_line("FOO=bar"),
            Some(("FOO".into(), "bar".into(), false))
        );
        assert_eq!(
            parse_line("  A_1 = x y "),
            Some(("A_1".into(), "x y".into(), false))
        );
        assert_eq!(
            parse_line("# FOO=bar"),
            Some(("FOO".into(), "bar".into(), true))
        );
        assert_eq!(
            parse_line("#FOO=bar"),
            Some(("FOO".into(), "bar".into(), true))
        );
        assert_eq!(
            parse_line("URL=https://x/y?a=b"),
            Some(("URL".into(), "https://x/y?a=b".into(), false))
        );
        assert_eq!(parse_line("# just a note"), None);
        assert_eq!(parse_line("1BAD=x"), None);
        assert_eq!(parse_line("no equals"), None);
    }

    #[test]
    fn env_file_names() {
        assert!(is_env_file(".env"));
        assert!(is_env_file(".env.local"));
        assert!(!is_env_file(".env.example"));
        assert!(!is_env_file("env"));
    }

    #[test]
    fn write_back_touches_one_line_only() {
        let f = tmp("one-line");
        std::fs::write(&f, "# c\nA=1\nB=2\n").unwrap();
        write_back(&entry(&f, 1, "A", "9", false)).unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "# c\nA=9\nB=2\n");
        std::fs::remove_file(f).unwrap();
    }

    #[test]
    fn commented_line_stays_commented() {
        let f = tmp("commented");
        std::fs::write(&f, "A=1\n# B=2\n").unwrap();
        write_back(&entry(&f, 1, "B", "9", true)).unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "A=1\n# B=9\n");
        std::fs::remove_file(f).unwrap();
    }

    #[test]
    fn refuses_to_write_when_the_line_moved() {
        let f = tmp("moved");
        std::fs::write(&f, "A=1\nB=2\n").unwrap();
        // The file lost a line since the scan: line 1 is no longer B.
        std::fs::write(&f, "B=2\n").unwrap();
        assert!(write_back(&entry(&f, 1, "B", "9", false)).is_err());
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "B=2\n");
        std::fs::remove_file(f).unwrap();
    }

    #[test]
    fn matching_is_case_insensitive_and_anded() {
        let e = entry(Path::new("/x/.env"), 0, "STRIPE_KEY", "sk_live_abc", false);
        assert!(e.matches(&["stripe".into()], false));
        assert!(e.matches(&["stripe".into(), "t".into()], false));
        assert!(!e.matches(&["stripe".into(), "nope".into()], false));
        assert!(!e.matches(&["sk_live".into()], false));
        assert!(e.matches(&["sk_live".into()], true));
    }
}
