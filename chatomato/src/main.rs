use components::{create_user::CreateUser, popup::Popup, Component};
use config::Config;
use db::Database;
use indexmap::{IndexMap, IndexSet};
use ratatui::{
    buffer::Buffer,
    crossterm::event::{self, KeyCode, KeyEventKind},
    layout::{Constraint, Flex, Layout, Rect},
    style::Stylize,
    widgets::{Block, Clear, Paragraph, Widget},
};

mod components;
mod config;
mod db;
mod error;

pub fn center(area: Rect, horizontal: Constraint, vertical: Constraint) -> Rect {
    let [area] = Layout::horizontal([horizontal]).flex(Flex::Center).areas(area);
    let [area] = Layout::vertical([vertical]).flex(Flex::Center).areas(area);
    area
}

pub fn bottom_right(area: Rect, horizontal: Constraint, vertical: Constraint) -> Rect {
    let [area] = Layout::horizontal([horizontal]).margin(1).flex(Flex::End).areas(area);
    let [area] = Layout::vertical([vertical]).margin(1).flex(Flex::End).areas(area);
    area
}

#[derive(PartialEq, Eq)]
enum Focus {
    Rooms,
    Messages,
    Chat,
}

#[derive(PartialEq, Eq)]
enum Mode {
    Normal,
    Input,
}

struct App {
    db: Database,
    focus: Focus,
    mode: Mode,
    input_buffer: String,
    rooms: IndexSet<String>,
    popup: Popup,
    create_user: CreateUser,
}

impl App {
    fn on_press(&mut self, ev: KeyCode) -> bool {
        match self.mode {
            Mode::Normal => {
                self.popup.on_press(ev);
                self.create_user.on_press(ev);

                match ev {
                    KeyCode::Esc | KeyCode::Char('q') => return true,
                    KeyCode::Char('1') => {
                        self.focus = Focus::Rooms;
                    }
                    KeyCode::Char('2') => {
                        self.focus = Focus::Messages;
                    }
                    KeyCode::Char('3') => {
                        self.focus = Focus::Chat;
                        self.mode = Mode::Input;
                    }
                    _ => {}
                }
            }
            Mode::Input => match ev {
                KeyCode::Esc => {
                    self.mode = Mode::Normal;
                }
                KeyCode::Enter => {
                    self.mode = Mode::Normal;
                    self.input_buffer.clear();
                }
                KeyCode::Char(a) => {
                    self.input_buffer.push(a);
                }
                _ => {}
            },
        }

        false
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

    fn show_create_user(&self, area: Rect, buf: &mut Buffer) {}
}

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let layout = Layout::horizontal([Constraint::Percentage(25), Constraint::Percentage(75)]).split(area);
        let left = Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).split(layout[0]);

        self.show_rooms(left[0], buf);
        self.show_messages(left[1], buf);
        self.show_chat(layout[1], buf);

        self.create_user.render(area, buf);
        self.popup.render(layout[1], buf);
    }
}

fn main() {
    let cfg = Config {
        addr: "0.0.0.0:29092".into(),
        data: "data".into(),
    };
    let prikey = cfg.data.join("private.key");
    let exists = prikey.exists();
    let db = Database::new(cfg).unwrap();
    let mut app = App {
        db,
        focus: Focus::Rooms,
        mode: if exists { Mode::Normal } else { Mode::Input },
        input_buffer: String::new(),
        popup: Popup::new(exists),
        create_user: CreateUser::new(!exists),
        rooms: IndexSet::new(),
    };
    let mut term = ratatui::init();
    let mut err = None;

    let rooms = app.db.list_rooms().unwrap();
    app.rooms.extend(rooms.iter().map(|r| r.name.clone()));

    loop {
        if let Err(e) = term.draw(|frame| frame.render_widget(&app, frame.area())) {
            err = Some(e);
            break;
        }

        match event::read() {
            Ok(event::Event::Key(ev)) => {
                if ev.kind == KeyEventKind::Press {
                    if app.on_press(ev.code) {
                        break;
                    }
                }
            }
            Err(e) => {
                err = Some(e);
                break;
            }
            _ => {}
        }
    }

    ratatui::restore();

    if let Some(e) = err {
        println!("{e}");
    }
}
