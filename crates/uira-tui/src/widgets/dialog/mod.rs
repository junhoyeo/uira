pub mod alert;
pub mod confirm;
pub mod help;
pub mod overlay;
pub mod prompt;
pub mod select;
pub mod stack;

use crossterm::event::{KeyCode, MouseEvent};
use ratatui::{layout::Rect, Frame};

use crate::Theme;

pub use alert::DialogAlert;
pub use confirm::DialogConfirm;
pub use help::DialogHelp;
pub use prompt::DialogPrompt;
pub use select::{DialogSelect, DialogSelectItem};
pub use stack::DialogStack;

pub enum DialogResult {
    None,
    Close,
    Replace(Box<dyn DialogContent>),
}

pub trait DialogContent {
    fn desired_size(&self, viewport: Rect) -> (u16, u16);

    fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme);

    fn handle_key(&mut self, _key: KeyCode) -> DialogResult {
        DialogResult::None
    }

    fn handle_mouse(&mut self, _event: MouseEvent, _area: Rect) -> DialogResult {
        DialogResult::None
    }
}
