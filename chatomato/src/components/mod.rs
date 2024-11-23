use ratatui::crossterm::event::KeyCode;

pub mod create_user;
pub mod popup;

pub trait Component {
    fn on_press(&mut self, ev: KeyCode);
}
