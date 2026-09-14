use eframe::egui::{self, Context, RichText, Ui};

use crate::config::{self, Config};
use crate::store::{self, Entry};

const KEY_COLUMN: f32 = 260.0;
const REPO_COLUMN: f32 = 200.0;
const ROW_HEIGHT: f32 = 18.0;
const MASK_MAX: usize = 22;

pub struct App {
    config: Config,
    entries: Vec<Entry>,
    search: String,
    filter: String,
    repo: Option<String>,
    search_values: bool,
    reveal: bool,
    show_commented: bool,
    edit_mode: bool,
    editing: Option<usize>,
    focus_editor: bool,
    focus_search: bool,
    adding: bool,
    new_key: String,
    new_value: String,
    status: String,
}

impl App {
    pub fn new() -> Self {
        let config = Config::load();
        let entries = store::scan(&config);
        Self {
            config,
            entries,
            search: String::new(),
            filter: String::new(),
            repo: None,
            search_values: false,
            reveal: false,
            show_commented: false,
            edit_mode: false,
            editing: None,
            focus_editor: false,
            focus_search: true,
            adding: false,
            new_key: String::new(),
            new_value: String::new(),
            status: String::new(),
        }
    }

    fn visible(&self) -> Vec<usize> {
        let terms: Vec<String> = self
            .search
            .split_whitespace()
            .chain(self.filter.split_whitespace())
            .map(str::to_lowercase)
            .collect();

        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| self.show_commented || !e.commented)
            .filter(|(_, e)| self.repo.as_ref().is_none_or(|r| &e.repo == r))
            .filter(|(_, e)| e.matches(&terms, self.search_values))
            .map(|(i, _)| i)
            .collect()
    }

    fn repo_counts(&self) -> Vec<(&str, usize)> {
        let mut repos: Vec<(&str, usize)> = Vec::new();
        for entry in &self.entries {
            match repos.last_mut() {
                Some((repo, count)) if *repo == entry.repo => *count += 1,
                _ => repos.push((&entry.repo, 1)),
            }
        }
        repos.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        repos
    }

    fn rescan(&mut self) {
        self.config = Config::load();
        self.entries = store::scan(&self.config);
        self.editing = None;
        self.status = format!("{} keys", self.entries.len());
    }

    fn copy(&mut self, ctx: &Context, index: usize) {
        let entry = &self.entries[index];
        ctx.copy_text(entry.value.clone());
        self.status = format!("copied: {}", entry.key);
    }

    fn save_edit(&mut self, index: usize) {
        self.editing = None;
        self.entries[index].refresh_value_index();
        self.status = match store::write_back(&self.entries[index]) {
            Ok(()) => format!("saved: {}", self.entries[index].key),
            Err(err) => format!("{}: {err}", self.entries[index].key),
        };
    }

    fn add_to_scratch(&mut self) {
        let key = self.new_key.trim();
        let value = self.new_value.trim();
        if key.is_empty() {
            return;
        }
        match store::append_scratch(key, value) {
            Ok(entry) => {
                self.status = format!("added: {}", entry.key);
                self.entries.push(entry);
                self.entries
                    .sort_by(|a, b| (&a.repo, &a.key).cmp(&(&b.repo, &b.key)));
                self.new_key.clear();
                self.new_value.clear();
            }
            Err(err) => self.status = format!("could not write the scratch file: {err}"),
        }
    }

    fn open_config(&mut self) {
        match config::write_template_if_missing() {
            Ok(path) => self.open_path(&path),
            Err(err) => self.status = format!("could not create the config file: {err}"),
        }
    }

    fn open_in_editor(&mut self, index: usize) {
        let path = self.entries[index].file.clone();
        self.open_path(&path);
    }

    fn open_path(&mut self, path: &std::path::Path) {
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "open".to_owned());
        if let Err(err) = std::process::Command::new(&editor).arg(path).spawn() {
            self.status = format!("could not run {editor}: {err}");
        }
    }

    fn toolbar(&mut self, ui: &mut Ui) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let search = ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("search key or repo…")
                    .desired_width(320.0),
            );
            if std::mem::take(&mut self.focus_search) {
                search.request_focus();
            }
            ui.checkbox(&mut self.search_values, "search values");
            ui.checkbox(&mut self.reveal, "reveal values");
            ui.checkbox(&mut self.show_commented, "commented")
                .on_hover_text("also list keys commented out in the files");
            if ui.checkbox(&mut self.edit_mode, "edit mode").changed() && !self.edit_mode {
                self.editing = None;
            }
            if ui
                .button("+ new")
                .on_hover_text("add a key that belongs to no repo")
                .clicked()
            {
                self.adding = !self.adding;
            }
            if ui.button("↻ rescan").clicked() {
                self.rescan();
            }
            if ui
                .button("config")
                .on_hover_text(config::config_file().display().to_string())
                .clicked()
            {
                self.open_config();
            }
            ui.label(RichText::new(&self.status).weak());
        });

        if self.adding {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_key)
                        .hint_text("KEY")
                        .desired_width(KEY_COLUMN),
                );
                let value = ui.add(
                    egui::TextEdit::singleline(&mut self.new_value)
                        .hint_text("value")
                        .desired_width(420.0),
                );
                let submitted = value.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.button("add").clicked() || submitted {
                    self.add_to_scratch();
                }
                ui.label(
                    RichText::new(config::scratch_file().display().to_string())
                        .weak()
                        .small(),
                );
            });
        }
        ui.add_space(6.0);
    }

    fn repo_list(&mut self, ui: &mut Ui) {
        let total = self.entries.len();
        let repos: Vec<(String, usize)> = self
            .repo_counts()
            .into_iter()
            .map(|(repo, count)| (repo.to_owned(), count))
            .collect();

        egui::ScrollArea::vertical().show(ui, |ui| {
            if ui
                .selectable_label(self.repo.is_none(), format!("all repos  ({total})"))
                .clicked()
            {
                self.repo = None;
                self.filter.clear();
            }
            ui.separator();
            for (repo, count) in repos {
                let selected = self.repo.as_deref() == Some(repo.as_str());
                if ui
                    .selectable_label(selected, format!("{repo}  ({count})"))
                    .clicked()
                {
                    self.repo = Some(repo);
                    self.filter.clear();
                }
            }
        });
    }

    fn results(&mut self, ctx: &Context, ui: &mut Ui, visible: &[usize]) {
        ui.horizontal(|ui| {
            let hint = match &self.repo {
                Some(repo) => format!("filter in {repo}…"),
                None => "filter these results…".to_owned(),
            };
            ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .hint_text(hint)
                    .desired_width(280.0),
            );
            if !self.filter.is_empty() && ui.small_button("clear").clicked() {
                self.filter.clear();
            }
            let hint = if self.edit_mode {
                ", click a value to edit"
            } else {
                ""
            };
            ui.label(
                RichText::new(format!("{} results — click to copy{hint}", visible.len())).weak(),
            );
        });
        ui.separator();

        egui::ScrollArea::vertical().show(ui, |ui| {
            for &index in visible {
                ui.horizontal(|ui| self.row(ctx, ui, index));
            }
        });
    }

    fn row(&mut self, ctx: &Context, ui: &mut Ui, index: usize) {
        let entry = &self.entries[index];
        let key_text = if entry.commented {
            RichText::new(format!("# {}", entry.key)).italics().weak()
        } else {
            RichText::new(&entry.key).strong()
        };
        let key = ui.add_sized(
            [KEY_COLUMN, ROW_HEIGHT],
            egui::Label::new(key_text)
                .truncate()
                .sense(egui::Sense::click()),
        );
        if key.on_hover_text(&self.entries[index].key).clicked() {
            self.copy(ctx, index);
        }

        if self.repo.is_none() {
            let repo = RichText::new(&self.entries[index].repo).weak().small();
            ui.add_sized([REPO_COLUMN, ROW_HEIGHT], egui::Label::new(repo).truncate());
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .small_button("copy")
                .on_hover_text("copy the value")
                .clicked()
            {
                self.copy(ctx, index);
            }
            if ui
                .small_button("open")
                .on_hover_text("open the file in $EDITOR")
                .clicked()
            {
                self.open_in_editor(index);
            }
            if self.editing == Some(index) {
                self.value_editor(ui, index);
            } else {
                self.value_label(ctx, ui, index);
            }
        });
    }

    fn value_editor(&mut self, ui: &mut Ui, index: usize) {
        let editor = ui.add(
            egui::TextEdit::singleline(&mut self.entries[index].value)
                .desired_width(ui.available_width()),
        );
        // Only on the first frame, or the field would steal focus back every frame.
        if std::mem::take(&mut self.focus_editor) {
            editor.request_focus();
        }
        if editor.lost_focus() {
            self.save_edit(index);
        }
    }

    fn value_label(&mut self, ctx: &Context, ui: &mut Ui, index: usize) {
        let entry = &self.entries[index];
        let shown = if self.reveal {
            entry.value.clone()
        } else {
            mask(&entry.value)
        };
        let width = ui.available_width();
        let label = ui.add_sized(
            [width, ROW_HEIGHT],
            egui::Label::new(RichText::new(shown).monospace().weak())
                .truncate()
                .sense(egui::Sense::click()),
        );
        if label.on_hover_text(&self.entries[index].value).clicked() {
            if self.edit_mode {
                self.editing = Some(index);
                self.focus_editor = true;
            } else {
                self.copy(ctx, index);
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let visible = self.visible();

        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        egui::Panel::left("repos")
            .default_size(280.0)
            .show(ui, |ui| self.repo_list(ui));
        egui::CentralPanel::default().show(ui, |ui| self.results(&ctx, ui, &visible));
    }
}

fn mask(value: &str) -> String {
    let hidden = value.chars().count().saturating_sub(2).min(MASK_MAX);
    let head: String = value.chars().take(2).collect();
    format!("{head}{}", "•".repeat(hidden))
}

#[cfg(test)]
mod tests {
    use super::mask;

    #[test]
    fn masking_keeps_a_hint_and_caps_the_length() {
        assert_eq!(mask("sk_live_1234"), "sk••••••••••");
        assert_eq!(mask("ab"), "ab");
        assert_eq!(mask(""), "");
        assert_eq!(mask(&"x".repeat(500)).chars().count(), 24);
    }
}
