use ratatui::{buffer::Buffer, crossterm::event::KeyCode, layout::Rect};

use crate::{db::Update, State};

pub mod create_user;
pub mod main;
pub mod popup;

pub trait Component {
    fn on_update(&mut self, update: &Update);

    fn on_press(&mut self, state: &mut State, ev: KeyCode);

    fn render(&self, state: &State, area: Rect, buf: &mut Buffer);
}
