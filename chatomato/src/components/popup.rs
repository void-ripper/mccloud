use ratatui::{
    buffer::Buffer,
    crossterm::event::KeyCode,
    layout::{Constraint, Rect},
    text::Line,
    widgets::{Block, Clear, Paragraph, Widget},
};

use crate::bottom_right;

use super::{Component, State};

pub struct Popup {
    show: bool,
}

impl Popup {
    pub fn new(show: bool) -> Self {
        Self { show }
    }
}

impl Component for Popup {
    fn on_update(&mut self, _update: &crate::db::Update) {}

    fn on_press(&mut self, _state: &mut State, ev: KeyCode) {
        match ev {
            KeyCode::Char(' ') => {
                self.show = !self.show;
            }
            _ => {}
        }
    }

    fn render(&self, _: &State, area: Rect, buf: &mut Buffer) {
        if self.show {
            let blk = Block::bordered().title(" Actions ");
            let lines: Vec<Line> = vec![
                " c       : create new room".into(),
                " j       : join room".into(),
                " h       : higher".into(),
                " l       : lower".into(),
                " 1       : focus rooms".into(),
                " 2       : focus messages".into(),
                " 3       : focus chat".into(),
                " Space   : toggle popup".into(),
                " q | Esc : quit".into(),
            ];
            let area = bottom_right(area, Constraint::Max(40), Constraint::Max(20));

            Clear.render(area, buf);
            Paragraph::new(lines).block(blk).render(area, buf);
        }
    }
}
