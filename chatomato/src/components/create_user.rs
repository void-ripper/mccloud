use k256::{
    elliptic_curve::{rand_core::OsRng, sec1::ToEncodedPoint},
    SecretKey,
};
use ratatui::{
    buffer::Buffer,
    crossterm::event::KeyCode,
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::{Block, Clear, Paragraph, Widget},
};

use crate::{center, config::Config};

use super::{Component, State};

trait OnEnter: FnMut(String) {}

pub struct CreateUser<F: OnEnter> {
    cfg: Config,
    inbuffer: String,
    on_enter: F,
}

impl<F: OnEnter> CreateUser<F> {
    pub fn new(cfg: Config, on_enter: F) -> Self {
        Self {
            cfg,
            inbuffer: String::new(),
            on_enter,
        }
    }
}

impl<F: OnEnter> Component for CreateUser<F> {
    fn on_press(&mut self, state: &mut State, ev: KeyCode) {
        match ev {
            KeyCode::Esc => {
                state.quit = true;
            }
            KeyCode::Enter => {
                if self.inbuffer.len() > 3 {
                    (self.on_enter)(self.inbuffer.clone());
                }
                self.inbuffer.clear();
            }
            KeyCode::Char(a) => {
                self.inbuffer.push(a);
            }
            _ => {}
        }
    }
}

impl<F: OnEnter> Widget for &CreateUser<F> {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let center = center(area, Constraint::Percentage(50), Constraint::Percentage(50));

        Clear.render(center, buf);
        Block::bordered().render(center, buf);
        let inner = Rect::new(center.x + 1, center.y + 1, center.width - 2, center.height - 2);

        let layout =
            Layout::vertical([Constraint::Length(3), Constraint::Length(3), Constraint::Length(3)]).split(inner);

        let lines: Vec<Line> = vec![" Please enter you display name".into()];
        Paragraph::new(lines).render(layout[0], buf);

        let blk = Block::bordered().title(" name ");
        Paragraph::new(self.inbuffer.as_str()).block(blk).render(layout[1], buf);

        Paragraph::new("Hint: Name has to belonger then 3 chars.").render(layout[2], buf);
    }
}
