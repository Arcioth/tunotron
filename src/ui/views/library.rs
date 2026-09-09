use ratatui::{
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    widgets::{Block, BorderType, Borders, Cell, Row, Table, TableState},
    Frame,
};
use crate::action::{Action, ViewId};
use crate::library::Track;
use crate::ui::theme::Theme;

pub struct LibraryView {
    pub state: TableState,
    pub tracks: Vec<Track>,
}

impl LibraryView {
    pub fn new() -> Self {
        let mut state = TableState::default();
        state.select(Some(0));
        Self {
            state,
            tracks: Vec::new(),
        }
    }

    #[allow(dead_code)]
    pub fn set_tracks(&mut self, tracks: Vec<Track>) {
        self.tracks = tracks;
        if self.state.selected().is_none() && !self.tracks.is_empty() {
            self.state.select(Some(0));
        }
    }

    pub fn append_tracks(&mut self, new_tracks: Vec<Track>) {
        if self.tracks.is_empty() && !new_tracks.is_empty() {
            self.state.select(Some(0));
        }
        self.tracks.extend(new_tracks);
    }

    pub fn selected_track(&self) -> Option<&Track> {
        self.state.selected().and_then(|idx| self.tracks.get(idx))
    }

    #[allow(dead_code)]
    pub fn selected_index(&self) -> Option<usize> {
        self.state.selected()
    }
}

impl super::View for LibraryView {
    fn id(&self) -> ViewId {
        ViewId::Library
    }

    fn title(&self) -> &'static str {
        "1: Library"
    }

    fn handle_action(&mut self, action: &Action) -> bool {
        let total = self.tracks.len();
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
            Action::HalfPageDown => {
                let next = (current + 15).min(total.saturating_sub(1));
                self.state.select(Some(next));
                true
            }
            Action::HalfPageUp => {
                let prev = current.saturating_sub(15);
                self.state.select(Some(prev));
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

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style)
            .title(format!(" Library ({} tracks) ", self.tracks.len()));

        if self.tracks.is_empty() {
            let empty_row = Row::new(vec![Cell::from("No audio tracks found. Use :scan <dir> or launch with a directory.")]);
            let table = Table::new(vec![empty_row], [Constraint::Percentage(100)]).block(block);
            frame.render_widget(table, area);
            return;
        }

        let header_style = Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD);

        let header = Row::new(vec![
            Cell::from("#"),
            Cell::from("Title"),
            Cell::from("Artist"),
            Cell::from("Album"),
            Cell::from("Time"),
        ])
        .style(header_style)
        .bottom_margin(1);

        let rows: Vec<Row> = self
            .tracks
            .iter()
            .enumerate()
            .map(|(idx, track)| {
                let num_str = track
                    .track_number
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| (idx + 1).to_string());

                Row::new(vec![
                    Cell::from(num_str),
                    Cell::from(track.title.clone()),
                    Cell::from(track.artist.clone()),
                    Cell::from(track.album.clone()),
                    Cell::from(track.formatted_duration()),
                ])
                .style(Style::default().fg(theme.fg))
            })
            .collect();

        let widths = [
            Constraint::Length(4),
            Constraint::Percentage(35),
            Constraint::Percentage(30),
            Constraint::Percentage(23),
            Constraint::Length(8),
        ];

        let highlight_style = Style::default()
            .bg(theme.selection_bg)
            .fg(theme.selection_fg)
            .add_modifier(Modifier::BOLD);

        let table = Table::new(rows, widths)
            .header(header)
            .block(block)
            .row_highlight_style(highlight_style)
            .highlight_symbol("▶ ");

        frame.render_stateful_widget(table, area, &mut self.state);
    }
}
