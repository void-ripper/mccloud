use ratatui::{
    buffer::Buffer,
    crossterm::event::KeyCode,
    layout::{Constraint, Layout, Rect},
    style::Stylize,
    text::Line,
    widgets::{Block, Clear, Paragraph, Widget},
};

use crate::center;

use super::{Component, State};

#[derive(PartialEq, Eq)]
enum Focus {
    Rooms,
    Messages,
    Chat,
}

pub struct MainView {
    focus: Focus,
    selected_room: usize,
    input_buffer: String,
    room_name: String,
    show_create_room: bool,
}

impl MainView {
    pub fn new() -> Self {
        Self {
            focus: Focus::Rooms,
            selected_room: 0,
            input_buffer: String::new(),
            room_name: String::new(),
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

    fn show_rooms(&self, state: &State, area: Rect, buf: &mut Buffer) {
        let blk = self.is_focused(Block::bordered().title(" Rooms "), Focus::Rooms);

        let mut lines: Vec<Line> = Vec::new();
        for (idx, room) in state.rooms.iter().enumerate() {
            let line: Line = room.name.as_str().into();
            lines.push(if idx == self.selected_room {
                line.white().on_light_blue()
            } else {
                line
            });
        }
        Paragraph::new(lines).block(blk).render(area, buf);
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
        let area = center(area, Constraint::Percentage(15), Constraint::Length(3));

        Clear.render(area, buf);
        let blk = Block::bordered().title(" New Room ");
        Paragraph::new(self.room_name.as_str()).block(blk).render(area, buf);
    }
}

impl Component for MainView {
    fn on_press(&mut self, state: &mut State, ev: KeyCode) {
        if self.show_create_room {
            match ev {
                KeyCode::Esc => {
                    self.show_create_room = false;
                    self.room_name.clear();
                }
                KeyCode::Enter => {
                    self.show_create_room = false;

                    if let Some(user) = &state.user {
                        match state.db.create_room(self.room_name.clone(), user.user.pubkey) {
                            Ok(_) => {
                                state.update_rooms();
                            }
                            Err(e) => {
                                tracing::error!("{e}");
                            }
                        }
                    }

                    self.room_name.clear();
                }
                KeyCode::Char(a) => {
                    self.room_name.push(a);
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

    fn render(&self, state: &State, area: Rect, buf: &mut Buffer) {
        let layout = Layout::horizontal([Constraint::Percentage(25), Constraint::Percentage(75)]).split(area);
        let left = Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).split(layout[0]);

        self.show_rooms(state, left[0], buf);
        self.show_messages(left[1], buf);
        self.show_chat(layout[1], buf);

        if self.show_create_room {
            self.show_create_room_popup(area, buf);
        }
    }
}
