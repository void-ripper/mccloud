use ratatui::crossterm::event::KeyCode;

use crate::State;

pub mod create_user;
pub mod main;
pub mod popup;

pub trait Component {
    fn on_press(&mut self, state: &mut State, ev: KeyCode);
}
