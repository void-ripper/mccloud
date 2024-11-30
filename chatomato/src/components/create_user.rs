use ratatui::{
    buffer::Buffer,
    crossterm::event::KeyCode,
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::{Block, Clear, Paragraph, Widget},
};

use crate::{center, Active};

use super::{main::MainView, Component, State};

pub struct CreateUser {
    inbuffer: String,
}

impl CreateUser {
    pub fn new() -> Self {
        Self {
            inbuffer: String::new(),
        }
    }
}

impl Component for CreateUser {
    fn on_update(&mut self, _state: &mut State, _update: &crate::db::Update) {}

    fn on_press(&mut self, state: &mut State, ev: KeyCode) {
        match ev {
            KeyCode::Esc => {
                state.quit = true;
            }
            KeyCode::Enter => {
                if self.inbuffer.len() > 3 {
                    match state.db.create_user(self.inbuffer.clone()) {
                        Ok(user) => {
                            state.user = Some(user);
                            state.next = Some(Active::Main(MainView::new()));
                        }
                        Err(e) => {
                            tracing::error!("{e}");
                        }
                    }
                }
                self.inbuffer.clear();
            }
            KeyCode::Char(a) => {
                self.inbuffer.push(a);
            }
            _ => {}
        }
    }

    fn render(&self, _state: &State, area: Rect, buf: &mut Buffer) {
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
