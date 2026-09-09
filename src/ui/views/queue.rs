use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState},
    Frame,
};
use crate::action::{Action, ViewId};
use crate::library::Track;
use crate::ui::theme::Theme;

pub struct QueueView {
    pub state: ListState,
    pub queue: Vec<Track>,
}

impl QueueView {
    pub fn new() -> Self {
        let mut state = ListState::default();
        state.select(Some(0));
        Self {
            state,
            queue: Vec::new(),
        }
    }

    pub fn set_queue(&mut self, queue: Vec<Track>) {
        self.queue = queue;
        if self.state.selected().is_none() && !self.queue.is_empty() {
            self.state.select(Some(0));
        }
    }

    pub fn push(&mut self, track: Track) {
        if self.queue.is_empty() {
            self.state.select(Some(0));
        }
        self.queue.push(track);
    }

    pub fn remove_selected(&mut self) -> Option<Track> {
        if let Some(idx) = self.state.selected() {
            if idx < self.queue.len() {
                let removed = self.queue.remove(idx);
                if self.queue.is_empty() {
                    self.state.select(None);
                } else if idx >= self.queue.len() {
                    self.state.select(Some(self.queue.len() - 1));
                }
                return Some(removed);
            }
        }
        None
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.state.selected()
    }
}

impl super::View for QueueView {
    fn id(&self) -> ViewId {
        ViewId::Queue
    }

    fn title(&self) -> &'static str {
        "3: Queue"
    }

    fn handle_action(&mut self, action: &Action) -> bool {
        let total = self.queue.len();
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

        let title = format!(" Play Queue ({} tracks) ", self.queue.len());
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style)
            .title(title);

        if self.queue.is_empty() {
            let item = ListItem::new("Queue is empty. Press 'a' in the Library to enqueue tracks.");
            let list = List::new(vec![item]).block(block);
            frame.render_widget(list, area);
            return;
        }

        let items: Vec<ListItem> = self
            .queue
            .iter()
            .enumerate()
            .map(|(i, track)| {
                let text = format!("{}. {} - {} ({})", i + 1, track.artist, track.title, track.formatted_duration());
                ListItem::new(text).style(Style::default().fg(theme.fg))
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
