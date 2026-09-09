pub mod browser;
pub mod library;
pub mod queue;

pub use browser::FileBrowserView;
pub use library::LibraryView;
pub use queue::QueueView;

use ratatui::{layout::Rect, Frame};
use crate::action::{Action, ViewId};
use crate::ui::theme::Theme;

#[allow(dead_code)]
pub trait View: Send {
    fn id(&self) -> ViewId;
    fn title(&self) -> &'static str;
    fn handle_action(&mut self, action: &Action) -> bool;
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme, is_active: bool);
}
