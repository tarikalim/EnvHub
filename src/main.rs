use std::path::{Path, PathBuf};

use eframe::egui;

/// Directories never worth walking into.
const SKIP: &[&str] = &[
    "Library", ".Trash", "node_modules", ".git", ".venv", "venv", "target",
    ".cargo", ".rustup", ".npm", ".cache", "Applications",
];

#[derive(Clone)]
struct Entry {
    repo: String, // home-relative dir, or "scratch"
    file: PathBuf,
    line: usize, // 0-based
    key: String,
    value: String,
    hay: String, // lowercased "repo key", for search
    value_lc: String,
    commented: bool,
}

impl Entry {
    fn new(repo: String, file: PathBuf, line: usize, key: String, value: String) -> Self {
        Self::with_comment(repo, file, line, key, value, false)
    }

    fn with_comment(repo: String, file: PathBuf, line: usize, key: String, value: String, commented: bool) -> Self {
        let hay = format!("{repo} {key}").to_lowercase();
        let value_lc = value.to_lowercase();
        Self { repo, file, line, key, value, hay, value_lc, commented }
    }
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").expect("HOME"))
}

fn config_dir() -> PathBuf {
    match std::env::var("XDG_CONFIG_HOME") {
        Ok(d) if !d.is_empty() => PathBuf::from(d).join("envhub"),
        _ => home().join(".config/envhub"),
    }
}

fn config_file() -> PathBuf {
    config_dir().join("config")
}

/// Loose key/values that belong to no repo.
fn scratch_file() -> PathBuf {
    config_dir().join("scratch.env")
}

#[derive(Default)]
struct Config {
    roots: Vec<PathBuf>,
    skip: Vec<String>,
}

fn expand(s: &str) -> PathBuf {
    match s.strip_prefix("~/") {
        Some(rest) => home().join(rest),
        None if s == "~" => home(),
        None => PathBuf::from(s),
    }
}

/// One directive per line: a path to scan, or `!name` for a directory to skip.
fn parse_config(text: &str) -> Config {
    let mut c = Config::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match line.strip_prefix('!') {
            Some(skip) => c.skip.push(skip.trim().to_string()),
            None => c.roots.push(expand(line)),
        }
    }
    c
}

fn load_config() -> Config {
    let mut c = std::fs::read_to_string(config_file()).map(|t| parse_config(&t)).unwrap_or_default();
    if c.roots.is_empty() {
        c.roots.push(home());
    }
    c
}

fn is_env_file(name: &str) -> bool {
    name.starts_with(".env") && !name.ends_with(".example") && !name.ends_with(".sample")
}

fn read_env_file(path: &Path, repo: String, out: &mut Vec<Entry>) {
    let Ok(text) = std::fs::read_to_string(path) else { return };
    for (i, raw) in text.lines().enumerate() {
        let Some((k, v, commented)) = parse_line(raw) else { continue };
        out.push(Entry::with_comment(repo.clone(), path.to_path_buf(), i, k, v, commented));
    }
}

fn scan(cfg: &Config) -> Vec<Entry> {
    let home = home();
    let mut out = Vec::new();
    let scratch = scratch_file();
    if scratch.is_file() {
        read_env_file(&scratch, "scratch".into(), &mut out);
    }

    for root in &cfg.roots {
        let skip = cfg.skip.clone();
        let walk = ignore::WalkBuilder::new(root)
            .hidden(false)
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false)
            .follow_links(false)
            .max_depth(Some(8))
            .filter_entry(move |e| {
                let n = e.file_name().to_string_lossy().to_string();
                !SKIP.contains(&n.as_str()) && !skip.iter().any(|s| *s == n)
            })
            .build();

        for dent in walk.flatten() {
            if !dent.file_type().is_some_and(|t| t.is_file()) {
                continue;
            }
            if !is_env_file(&dent.file_name().to_string_lossy()) {
                continue;
            }
            let path = dent.path();
            if path == scratch {
                continue;
            }
            let repo = path
                .parent()
                .map(|p| p.strip_prefix(&home).unwrap_or(p))
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            read_env_file(path, repo, &mut out);
        }
    }
    out.sort_by(|a, b| (&a.repo, &a.key).cmp(&(&b.repo, &b.key)));
    out
}

/// Parses `KEY=VALUE`, including commented-out ones (`# KEY=VALUE`).
fn parse_line(raw: &str) -> Option<(String, String, bool)> {
    let mut line = raw.trim();
    let commented = line.starts_with('#');
    if commented {
        line = line.trim_start_matches('#').trim();
    }
    let (k, v) = line.split_once('=')?;
    let k = k.trim_end();
    let mut chars = k.chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_') || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some((k.to_string(), v.trim().to_string(), commented))
}

/// Rewrites one KEY=VALUE line in place, leaving the rest of the file untouched.
fn write_back(e: &Entry) -> std::io::Result<()> {
    let text = std::fs::read_to_string(&e.file)?;
    let mut lines: Vec<&str> = text.lines().collect();
    let replaced = if e.commented {
        format!("# {}={}", e.key, e.value)
    } else {
        format!("{}={}", e.key, e.value)
    };
    if e.line >= lines.len() {
        return Ok(());
    }
    lines[e.line] = &replaced;
    let mut joined = lines.join("\n");
    if text.ends_with('\n') {
        joined.push('\n');
    }
    std::fs::write(&e.file, joined)
}

/// Appends a key to the scratch file and returns its line number.
fn append_scratch(key: &str, value: &str) -> std::io::Result<usize> {
    let path = scratch_file();
    std::fs::create_dir_all(config_dir())?;
    let mut text = std::fs::read_to_string(&path).unwrap_or_default();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    let line = text.lines().count();
    text.push_str(&format!("{key}={value}\n"));
    std::fs::write(&path, text)?;
    Ok(line)
}

fn matches(e: &Entry, terms: &[String], search_values: bool) -> bool {
    terms
        .iter()
        .all(|t| e.hay.contains(t) || (search_values && e.value_lc.contains(t)))
}

fn mask(value: &str) -> String {
    let n = value.chars().count();
    let head: String = value.chars().take(2).collect();
    format!("{head}{}", "•".repeat(n.saturating_sub(2).min(22)))
}

struct App {
    cfg: Config,
    entries: Vec<Entry>,
    query: String,
    filter: String, // narrows the list currently on screen
    repo: Option<String>,
    search_values: bool,
    reveal: bool,
    edit_mode: bool,
    show_commented: bool,
    toast: String,
    editing: Option<usize>,
    focus_edit: bool,
    focus_search: bool,
    adding: bool,
    new_key: String,
    new_value: String,
}

impl App {
    fn new() -> Self {
        let cfg = load_config();
        Self {
            entries: scan(&cfg),
            cfg,
            query: String::new(),
            filter: String::new(),
            repo: None,
            search_values: false,
            reveal: false,
            edit_mode: false,
            show_commented: false,
            toast: String::new(),
            editing: None,
            focus_edit: false,
            focus_search: true,
            adding: false,
            new_key: String::new(),
            new_value: String::new(),
        }
    }

    fn copy(&mut self, ctx: &egui::Context, e: &Entry) {
        ctx.copy_text(e.value.clone());
        self.toast = format!("copied: {}", e.key);
    }

    fn add_scratch_entry(&mut self) {
        let key = self.new_key.trim().to_string();
        let value = self.new_value.trim().to_string();
        if key.is_empty() {
            return;
        }
        match append_scratch(&key, &value) {
            Ok(line) => {
                self.entries.push(Entry::new("scratch".into(), scratch_file(), line, key.clone(), value));
                self.entries.sort_by(|a, b| (&a.repo, &a.key).cmp(&(&b.repo, &b.key)));
                self.toast = format!("added: {key}");
                self.new_key.clear();
                self.new_value.clear();
            }
            Err(err) => self.toast = format!("error: {err}"),
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = &ui.ctx().clone();
        let terms: Vec<String> = self
            .query
            .split_whitespace()
            .chain(self.filter.split_whitespace())
            .map(|t| t.to_lowercase())
            .collect();
        let visible: Vec<usize> = (0..self.entries.len())
            .filter(|&i| {
                let e = &self.entries[i];
                (self.show_commented || !e.commented)
                    && self.repo.as_ref().is_none_or(|r| &e.repo == r)
                    && matches(e, &terms, self.search_values)
            })
            .collect();

        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let search = ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text("search key or repo…")
                        .desired_width(320.0),
                );
                if self.focus_search {
                    search.request_focus();
                    self.focus_search = false;
                }
                ui.checkbox(&mut self.search_values, "search values");
                ui.checkbox(&mut self.reveal, "reveal values");
                ui.checkbox(&mut self.show_commented, "commented")
                    .on_hover_text("also list keys that are commented out in the .env files");
                if ui.checkbox(&mut self.edit_mode, "edit mode").changed() && !self.edit_mode {
                    self.editing = None;
                }
                if ui.button("+ new").on_hover_text("add a key to the scratch file").clicked() {
                    self.adding = !self.adding;
                }
                if ui.button("↻ rescan").clicked() {
                    self.cfg = load_config();
                    self.entries = scan(&self.cfg);
                    self.toast = format!("{} keys", self.entries.len());
                }
                ui.label(egui::RichText::new(&self.toast).weak());
            });
            if self.adding {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.new_key).hint_text("KEY").desired_width(260.0));
                    ui.add(egui::TextEdit::singleline(&mut self.new_value).hint_text("value").desired_width(420.0));
                    if ui.button("add").clicked() {
                        self.add_scratch_entry();
                    }
                    ui.label(egui::RichText::new("stored in ~/.config/envhub/scratch.env").weak().small());
                });
            }
            ui.add_space(6.0);
        });

        egui::Panel::left("repos").default_size(280.0).show(ui, |ui| {
            let mut repos: Vec<(String, usize)> = Vec::new();
            for e in &self.entries {
                match repos.last_mut() {
                    Some((r, n)) if *r == e.repo => *n += 1,
                    _ => repos.push((e.repo.clone(), 1)),
                }
            }
            repos.sort_by(|a, b| b.1.cmp(&a.1));

            egui::ScrollArea::vertical().show(ui, |ui| {
                if ui.selectable_label(self.repo.is_none(), format!("all repos  ({})", self.entries.len())).clicked() {
                    self.repo = None;
                    self.filter.clear();
                }
                ui.separator();
                for (r, n) in repos {
                    let label = format!("{}  ({})", r, n);
                    if ui.selectable_label(self.repo.as_deref() == Some(r.as_str()), label).clicked() {
                        self.repo = Some(r.clone());
                        self.filter.clear();
                    }
                }
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                let hint = match &self.repo {
                    Some(r) => format!("filter in {r}…"),
                    None => "filter these results…".to_string(),
                };
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .hint_text(hint)
                        .desired_width(280.0),
                );
                if !self.filter.is_empty() && ui.small_button("clear").clicked() {
                    self.filter.clear();
                }
                ui.label(egui::RichText::new(format!(
                    "{} results — click to copy{}",
                    visible.len(),
                    if self.edit_mode { ", click value to edit" } else { "" }
                )).weak());
            });
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for i in visible {
                    ui.horizontal(|ui| {
                        let e = self.entries[i].clone();
                        let label = if e.commented {
                            egui::RichText::new(format!("# {}", e.key)).italics().weak()
                        } else {
                            egui::RichText::new(&e.key).strong()
                        };
                        let key = ui.add_sized(
                            [260.0, 18.0],
                            egui::Label::new(label).truncate().sense(egui::Sense::click()),
                        );
                        if key.on_hover_text(&e.key).clicked() {
                            self.copy(ctx, &e);
                        }
                        if self.repo.is_none() {
                            ui.add_sized(
                                [200.0, 18.0],
                                egui::Label::new(egui::RichText::new(&e.repo).weak().small()).truncate(),
                            );
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("copy").on_hover_text("copy value to clipboard").clicked() {
                                self.copy(ctx, &e);
                            }
                            if ui.small_button("open").on_hover_text("open the .env file in $EDITOR").clicked() {
                                let _ = std::process::Command::new(std::env::var("EDITOR").unwrap_or_else(|_| "open".into()))
                                    .arg(&e.file)
                                    .spawn();
                            }
                            if self.editing == Some(i) {
                                let r = ui.add(
                                    egui::TextEdit::singleline(&mut self.entries[i].value)
                                        .desired_width(ui.available_width()),
                                );
                                if self.focus_edit {
                                    r.request_focus();
                                    self.focus_edit = false;
                                }
                                if r.lost_focus() {
                                    self.editing = None;
                                    self.entries[i].value_lc = self.entries[i].value.to_lowercase();
                                    match write_back(&self.entries[i]) {
                                        Ok(()) => self.toast = format!("saved: {}", e.key),
                                        Err(err) => self.toast = format!("error: {err}"),
                                    }
                                }
                            } else {
                                let shown = if self.reveal { e.value.clone() } else { mask(&e.value) };
                                let w = ui.available_width();
                                let val = ui.add_sized(
                                    [w, 18.0],
                                    egui::Label::new(egui::RichText::new(shown).monospace().weak())
                                        .truncate()
                                        .sense(egui::Sense::click()),
                                );
                                if val.on_hover_text(&e.value).clicked() {
                                    if self.edit_mode {
                                        self.editing = Some(i);
                                        self.focus_edit = true;
                                    } else {
                                        self.copy(ctx, &e);
                                    }
                                }
                            }
                        });
                    });
                }
            });
        });
    }
}

fn main() -> eframe::Result<()> {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 700.0])
            .with_title("envhub")
            .with_icon(egui::IconData {
                rgba: include_bytes!("../tools/icon.rgba").to_vec(),
                width: 256,
                height: 256,
            }),
        ..Default::default()
    };
    eframe::run_native("envhub", opts, Box::new(|_cc| Ok(Box::new(App::new()))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_real_keys() {
        assert_eq!(parse_line("FOO=bar"), Some(("FOO".into(), "bar".into(), false)));
        assert_eq!(parse_line("  A_1 = x y "), Some(("A_1".into(), "x y".into(), false)));
        assert_eq!(parse_line("# FOO=bar"), Some(("FOO".into(), "bar".into(), true)));
        assert_eq!(parse_line("#FOO=bar"), Some(("FOO".into(), "bar".into(), true)));
        assert_eq!(parse_line("# just a note"), None);
        assert_eq!(parse_line("1BAD=x"), None);
        assert_eq!(parse_line("no equals"), None);
    }

    #[test]
    fn write_back_touches_one_line_only() {
        let f = std::env::temp_dir().join("envhub_test.env");
        std::fs::write(&f, "# c\nA=1\nB=2\n").unwrap();
        let e = Entry::new("t".into(), f.clone(), 1, "A".into(), "9".into());
        write_back(&e).unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "# c\nA=9\nB=2\n");
        std::fs::remove_file(f).unwrap();
    }

    #[test]
    fn commented_line_stays_commented_when_saved() {
        let f = std::env::temp_dir().join("envhub_commented.env");
        std::fs::write(&f, "A=1\n# B=2\n").unwrap();
        let e = Entry::with_comment("t".into(), f.clone(), 1, "B".into(), "9".into(), true);
        write_back(&e).unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "A=1\n# B=9\n");
        std::fs::remove_file(f).unwrap();
    }

    #[test]
    fn config_roots_and_skips() {
        let c = parse_config("# comment\n~/code\n\n/tmp/x\n!dist\n");
        assert_eq!(c.roots, vec![home().join("code"), PathBuf::from("/tmp/x")]);
        assert_eq!(c.skip, vec!["dist".to_string()]);
    }

    #[test]
    fn env_file_names() {
        assert!(is_env_file(".env"));
        assert!(is_env_file(".env.local"));
        assert!(!is_env_file(".env.example"));
        assert!(!is_env_file("env"));
    }
}
