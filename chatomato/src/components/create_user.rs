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

use crate::center;

use super::{Component, State};

pub struct CreateUser {
    inbuffer: String,
    secret: SecretKey,
    public: Box<[u8]>,
}

impl CreateUser {
    pub fn new() -> Self {
        let secret = k256::SecretKey::random(&mut OsRng);
        let public = secret.public_key().to_encoded_point(true).to_bytes();

        Self {
            inbuffer: String::new(),
            secret,
            public,
        }
    }
}

impl Component for CreateUser {
    fn on_press(&mut self, state: &mut State, ev: KeyCode) {
        match ev {
            KeyCode::Esc => {
                state.quit = true;
            }
            KeyCode::Enter => {
                self.inbuffer.clear();
            }
            KeyCode::Char(a) => {
                self.inbuffer.push(a);
            }
            _ => {}
        }
    }
}

impl Widget for &CreateUser {
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

        let blk = Block::bordered().title(" public key ");
        Paragraph::new(hex::encode(&self.public))
            .block(blk)
            .render(layout[1], buf);

        let blk = Block::bordered().title(" name ");
        Paragraph::new(self.inbuffer.as_str()).block(blk).render(layout[2], buf);
    }
}
