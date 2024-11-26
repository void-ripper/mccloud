use std::sync::Arc;

use ratatui::{
    buffer::Buffer,
    crossterm::event::KeyCode,
    layout::{Constraint, Layout, Rect},
    style::Stylize,
    widgets::{Block, Clear, Paragraph, Widget},
};

use crate::{center, db::Database};

use super::{Component, State};

#[derive(PartialEq, Eq)]
enum Focus {
    Rooms,
    Messages,
    Chat,
}

pub struct MainView {
    db: Arc<Database>,
    focus: Focus,
    input_buffer: String,
    show_create_room: bool,
}

impl MainView {
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            db,
            focus: Focus::Rooms,
            input_buffer: String::new(),
            show_create_room: false,
        }
    }

    fn is_focused<'a>(&self, blk: Block<'a>, f: Focus) -> Block<'a> {
        if self.focus == f {
            blk.light_blue()
        } else {
            blk
        }
    }

    fn show_rooms(&self, area: Rect, buf: &mut Buffer) {
        let blk = self.is_focused(Block::bordered().title(" Rooms "), Focus::Rooms);

        Paragraph::new("").block(blk).render(area, buf);
    }

    fn show_messages(&self, area: Rect, buf: &mut Buffer) {
        let blk = self.is_focused(Block::bordered().title(" Messages "), Focus::Messages);
        Paragraph::new("").block(blk).render(area, buf);
    }

    fn show_chat(&self, area: Rect, buf: &mut Buffer) {
        let layout = Layout::vertical([Constraint::Min(3), Constraint::Length(3)]).split(area);

        let blk = self.is_focused(Block::bordered().title(" Chat "), Focus::Chat);
        Paragraph::new("").block(blk).render(layout[0], buf);

        let blk = self.is_focused(Block::bordered(), Focus::Chat);
        Paragraph::new(self.input_buffer.as_str())
            .block(blk)
            .render(layout[1], buf);
    }

    fn show_create_room_popup(&self, area: Rect, buf: &mut Buffer) {
        let area = center(area, Constraint::Percentage(50), Constraint::Percentage(50));

        Clear.render(area, buf);
        let blk = Block::bordered().title(" New Room ");
        Paragraph::new(self.input_buffer.as_str()).block(blk).render(area, buf);
    }
}

impl Component for MainView {
    fn on_press(&mut self, state: &mut State, ev: KeyCode) {
        if self.show_create_room {
            match ev {
                KeyCode::Esc => {
                    self.show_create_room = false;
                    self.input_buffer.clear();
                }
                KeyCode::Enter => {
                    self.show_create_room = false;

                    if let Some(user) = &state.user {
                        self.db.create_room(self.input_buffer.clone(), user.user.pubkey);
                        state.update_rooms();
                    }

                    self.input_buffer.clear();
                }
                KeyCode::Char(a) => {
                    self.input_buffer.push(a);
                }
                _ => {}
            }
        } else {
            match ev {
                KeyCode::Esc | KeyCode::Char('q') => {
                    state.quit = true;
                }
                KeyCode::Char('1') => {
                    self.focus = Focus::Rooms;
                }
                KeyCode::Char('2') => {
                    self.focus = Focus::Messages;
                }
                KeyCode::Char('3') => {
                    self.focus = Focus::Chat;
                }
                KeyCode::Char('c') => {
                    self.show_create_room = true;
                }
                _ => {}
            }
        }
    }
}

impl Widget for &MainView {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let layout = Layout::horizontal([Constraint::Percentage(25), Constraint::Percentage(75)]).split(area);
        let left = Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).split(layout[0]);

        self.show_rooms(left[0], buf);
        self.show_messages(left[1], buf);
        self.show_chat(layout[1], buf);

        if self.show_create_room {
            self.show_create_room_popup(area, buf);
        }
    }
}
