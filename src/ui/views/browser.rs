use std::fs;
use std::path::PathBuf;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState},
    Frame,
};
use crate::action::{Action, ViewId};
use crate::library::Track;
use crate::ui::theme::Theme;

#[derive(Debug, Clone)]
pub struct BrowserItem {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

pub struct FileBrowserView {
    pub current_dir: PathBuf,
    pub items: Vec<BrowserItem>,
    pub state: ListState,
}

impl FileBrowserView {
    pub fn new(start_dir: PathBuf) -> Self {
        let mut view = Self {
            current_dir: start_dir.clone(),
            items: Vec::new(),
            state: ListState::default(),
        };
        view.refresh_directory();
        view.state.select(Some(0));
        view
    }

    pub fn refresh_directory(&mut self) {
        self.items.clear();

        // Add parent directory ".." if available
        if let Some(parent) = self.current_dir.parent() {
            self.items.push(BrowserItem {
                name: ".. (Parent Directory)".to_string(),
                path: parent.to_path_buf(),
                is_dir: true,
            });
        }

        if let Ok(entries) = fs::read_dir(&self.current_dir) {
            let mut dirs = Vec::new();
            let mut files = Vec::new();

            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                let file_name = entry.file_name().to_string_lossy().to_string();

                if path.is_dir() {
                    dirs.push(BrowserItem {
                        name: format!("📁 {}/", file_name),
                        path,
                        is_dir: true,
                    });
                } else if Track::is_audio_file(&path) {
                    files.push(BrowserItem {
                        name: format!("🎵 {}", file_name),
                        path,
                        is_dir: false,
                    });
                }
            }

            dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

            self.items.extend(dirs);
            self.items.extend(files);
        }

        if self.state.selected().is_none() && !self.items.is_empty() {
            self.state.select(Some(0));
        }
    }

    pub fn selected_item(&self) -> Option<&BrowserItem> {
        self.state.selected().and_then(|idx| self.items.get(idx))
    }

    pub fn enter_selected(&mut self) -> Option<PathBuf> {
        if let Some(item) = self.selected_item().cloned() {
            if item.is_dir {
                self.current_dir = item.path;
                self.refresh_directory();
                self.state.select(Some(0));
                None
            } else {
                Some(item.path)
            }
        } else {
            None
        }
    }
}

impl super::View for FileBrowserView {
    fn id(&self) -> ViewId {
        ViewId::FileBrowser
    }

    fn title(&self) -> &'static str {
        "2: File Browser"
    }

    fn handle_action(&mut self, action: &Action) -> bool {
        let total = self.items.len();
        if total == 0 {
            return false;
        }

        let current = self.state.selected().unwrap_or(0);
        match action {
            Action::MoveDown(count) => {
                let next = (current + count).min(total.saturating_sub(1));
                self.state.select(Some(next));
                true
            }
            Action::MoveUp(count) => {
                let prev = current.saturating_sub(*count);
                self.state.select(Some(prev));
                true
            }
            Action::MoveToTop => {
                self.state.select(Some(0));
                true
            }
            Action::MoveToBottom => {
                self.state.select(Some(total.saturating_sub(1)));
                true
            }
            _ => false,
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme, is_active: bool) {
        let border_style = if is_active {
            Style::default().fg(theme.accent)
        } else {
            Style::default().fg(theme.border)
        };

        let title = format!(" Filesystem: {} ", self.current_dir.display());
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style)
            .title(title);

        let items: Vec<ListItem> = self
            .items
            .iter()
            .map(|item| {
                let style = if item.is_dir {
                    Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.fg)
                };
                ListItem::new(item.name.clone()).style(style)
            })
            .collect();

        let highlight_style = Style::default()
            .bg(theme.selection_bg)
            .fg(theme.selection_fg)
            .add_modifier(Modifier::BOLD);

        let list = List::new(items)
            .block(block)
            .highlight_style(highlight_style)
            .highlight_symbol("▶ ");

        frame.render_stateful_widget(list, area, &mut self.state);
    }
}
