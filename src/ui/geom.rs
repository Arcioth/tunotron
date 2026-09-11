use ratatui::layout::Rect;
use ratatui::widgets::TableState;
use crate::action::WindowId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModalContent {
    pub title: String,
    pub content: String,
}

#[derive(Debug, Default)]
pub struct UiGeom {
    pub table_state: TableState,
    pub browser_rect: Rect,
    pub browser_rows_rect: Rect,
    pub progress_rect: Rect,
    pub header_rect: Rect,
    pub tab_rects: Vec<(Rect, usize)>,
    pub window_stack: Vec<WindowId>,
    pub modal_rect: Rect,
    pub modal_content: Option<ModalContent>,
}

#[allow(dead_code)]
impl UiGeom {
    pub fn new() -> Self {
        let mut s = Self::default();
        s.table_state.select(Some(0));
        s
    }

    // --- Table Selection & Viewport Motions ---

    pub fn selected(&self) -> Option<usize> {
        self.table_state.selected()
    }

    pub fn select(&mut self, index: Option<usize>) {
        self.table_state.select(index);
    }

    pub fn scroll_offset(&self) -> usize {
        self.table_state.offset()
    }

    pub fn clamp_selection(&mut self, total_items: usize) {
        if total_items == 0 {
            self.table_state.select(None);
        } else {
            let current = self.table_state.selected().unwrap_or(0);
            self.table_state.select(Some(current.min(total_items - 1)));
        }
    }

    pub fn move_down(&mut self, count: usize, total_items: usize) {
        if total_items > 0 {
            let current = self.table_state.selected().unwrap_or(0);
            let next = (current + count).min(total_items - 1);
            self.table_state.select(Some(next));
        }
    }

    pub fn move_up(&mut self, count: usize) {
        let current = self.table_state.selected().unwrap_or(0);
        let prev = current.saturating_sub(count);
        self.table_state.select(Some(prev));
    }

    pub fn move_to_top(&mut self) {
        self.table_state.select(Some(0));
    }

    pub fn move_to_bottom(&mut self, total_items: usize) {
        if total_items > 0 {
            self.table_state.select(Some(total_items - 1));
        }
    }

    pub fn select_index(&mut self, idx: usize, total_items: usize) {
        if idx < total_items {
            self.table_state.select(Some(idx));
        }
    }

    // --- Window Stack Compositor ---

    pub fn push_window(&mut self, id: WindowId) {
        self.window_stack.retain(|&w| w != id);
        self.window_stack.push(id);
    }

    pub fn open_modal(&mut self, title: String, content: String) {
        self.modal_content = Some(ModalContent { title, content });
        self.push_window(WindowId::PluginModal);
    }

    pub fn pop_window(&mut self) -> Option<WindowId> {
        let popped = self.window_stack.pop();
        if !self.window_stack.contains(&WindowId::PluginModal) {
            self.modal_content = None;
        }
        popped
    }

    pub fn toggle_window(&mut self, id: WindowId) {
        if self.top_window() == Some(id) {
            self.pop_window();
        } else {
            self.push_window(id);
        }
    }

    pub fn top_window(&self) -> Option<WindowId> {
        self.window_stack.last().copied()
    }

    pub fn has_window(&self) -> bool {
        !self.window_stack.is_empty()
    }

    pub fn close_top_window(&mut self) -> bool {
        self.window_stack.pop().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selection_motions_and_bounds() {
        let mut geom = UiGeom::new();
        assert_eq!(geom.selected(), Some(0));

        geom.move_down(2, 5);
        assert_eq!(geom.selected(), Some(2));

        geom.move_down(10, 5);
        assert_eq!(geom.selected(), Some(4)); // clamped to 5 - 1

        geom.move_up(1);
        assert_eq!(geom.selected(), Some(3));

        geom.move_up(10);
        assert_eq!(geom.selected(), Some(0)); // saturating_sub

        geom.move_to_bottom(10);
        assert_eq!(geom.selected(), Some(9));

        geom.move_to_top();
        assert_eq!(geom.selected(), Some(0));

        geom.clamp_selection(0);
        assert_eq!(geom.selected(), None);
    }

    #[test]
    fn test_window_stack_push_pop_toggle() {
        let mut geom = UiGeom::new();
        assert!(!geom.has_window());
        assert_eq!(geom.top_window(), None);

        geom.push_window(WindowId::Help);
        assert!(geom.has_window());
        assert_eq!(geom.top_window(), Some(WindowId::Help));

        // Push same window deduplicates and keeps it on top
        geom.push_window(WindowId::Help);
        assert_eq!(geom.window_stack.len(), 1);

        // Toggle Help closes it
        geom.toggle_window(WindowId::Help);
        assert!(!geom.has_window());
        assert_eq!(geom.top_window(), None);

        // Toggle Help again opens it
        geom.toggle_window(WindowId::Help);
        assert!(geom.has_window());
        assert_eq!(geom.top_window(), Some(WindowId::Help));

        // Close top window
        assert!(geom.close_top_window());
        assert!(!geom.has_window());
        assert!(!geom.close_top_window());
    }
}
