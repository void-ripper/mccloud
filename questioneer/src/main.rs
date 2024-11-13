use std::{path::PathBuf, sync::Arc};

use indexmap::IndexMap;
use mcriddle::{Config, Peer};
use ratatui::{
    buffer::Buffer,
    crossterm::event::{self, KeyCode, KeyEventKind},
    layout::{Constraint, Flex, Layout, Rect},
    style::Stylize,
    text::{Line, Text},
    widgets::{Block, BorderType, Paragraph, Widget},
};

fn center(area: Rect, horizontal: Constraint, vertical: Constraint) -> Rect {
    let [area] = Layout::horizontal([horizontal]).flex(Flex::Center).areas(area);
    let [area] = Layout::vertical([vertical]).flex(Flex::Center).areas(area);
    area
}

struct App {
    show_popup: bool,
    show_message: bool,
    peers: IndexMap<String, Arc<Peer>>,
    port_pool: u16,
    selected: usize,
    message_input: String,
}

impl App {
    fn spawn_peer(&mut self) {
        let cfg = Config {
            addr: format!("127.0.0.1:{}", self.port_pool),
            folder: PathBuf::from("data").join(self.port_pool.to_string()),
        };
        self.port_pool += 1;

        let p = Peer::new(cfg).unwrap();
        self.peers.insert(p.pubhex(), p);
    }

    fn delete_peer(&mut self) {
        if let Some((_k, v)) = self.peers.shift_remove_index(self.selected) {
            v.shutdown();
        }

        if self.peers.len() == 0 {
            self.selected = 0;
        } else if self.selected >= self.peers.len() {
            self.selected = self.peers.len() - 1;
        }
    }

    fn show_popup(&self, area: Rect, buf: &mut Buffer) {
        let w = 30;
        let h = 10;
        let loc = Rect::new(area.x + area.width - w - 1, area.y + area.height - h - 1, w, h);

        let block = Block::bordered().title(" Popup ");
        let lines: Vec<Line> = vec![
            " c: spawn peer".into(),
            " d: delete peer".into(),
            " m: send message".into(),
            " q: quit app".into(),
        ];
        Paragraph::new(lines).block(block).render(loc, buf);
    }

    fn show_message(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::bordered().title(" Message ");
        Paragraph::new(self.message_input.as_str())
            .block(block)
            .render(area, buf);
    }

    fn peer_info(&self, area: Rect, buf: &mut Buffer) {
        if self.peers.len() > 0 {
            let layout = Layout::vertical([Constraint::Length(3), Constraint::Min(3)]).split(area);
            let peer = &self.peers[self.selected];

            let block = Block::bordered().title(" Public Key ");
            Paragraph::new(peer.pubhex()).block(block).render(layout[0], buf);

            let layout = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(layout[1]);

            let block = Block::bordered().title(" Connections ");
            Paragraph::new("").block(block).render(layout[0], buf);

            let block = Block::bordered().title(" Data ");
            Paragraph::new("").block(block).render(layout[1], buf);
        }
    }
}

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let layout = Layout::horizontal([Constraint::Percentage(20), Constraint::Min(0)]).split(area);

        let title = Line::from(" Peers ".bold());
        let block = Block::bordered().title(title.centered()); //.border_type(BorderType::Thick);
        let mut peers: Vec<Line> = Vec::new();

        for (idx, p) in self.peers.keys().enumerate() {
            if idx == self.selected {
                peers.push(p.as_str().bold().on_light_blue().into());
            } else {
                peers.push(p.as_str().into());
            }
        }
        Paragraph::new(peers).block(block).render(layout[0], buf);

        self.peer_info(layout[1], buf);

        if self.show_popup {
            self.show_popup(layout[1], buf);
        } else if self.show_message {
            let centered = center(layout[1], Constraint::Percentage(50), Constraint::Length(3));
            self.show_message(centered, buf);
        }
    }
}

#[tokio::main]
async fn main() {
    let mut term = ratatui::init();

    let mut app = App {
        show_popup: false,
        show_message: false,
        peers: IndexMap::new(),
        port_pool: 29092,
        selected: 0,
        message_input: String::new(),
    };
    let mut err = None;

    loop {
        if let Err(e) = term.draw(|frame| frame.render_widget(&app, frame.area())) {
            err = Some(e);
            break;
        }

        match event::read() {
            Ok(event::Event::Key(ev)) => {
                if ev.kind == KeyEventKind::Press {
                    if app.show_message {
                        match ev.code {
                            KeyCode::Char(a) => {
                                app.message_input.push(a);
                            }
                            KeyCode::Enter => {
                                app.peers[app.selected]
                                    .share(app.message_input.as_bytes().to_vec())
                                    .await;
                                app.show_message = false;
                                app.message_input.clear();
                            }
                            KeyCode::Esc => {
                                app.show_message = false;
                                app.message_input.clear();
                            }
                            _ => {}
                        }
                    } else {
                        match ev.code {
                            KeyCode::Char('q') | KeyCode::Esc => break,
                            KeyCode::Char('j') | KeyCode::Down => {
                                let len = app.peers.len();
                                if len > 0 && app.selected < len - 1 {
                                    app.selected += 1;
                                }
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                if app.selected > 0 {
                                    app.selected -= 1;
                                }
                            }
                            KeyCode::Char(' ') => {
                                app.show_popup = !app.show_popup;
                            }
                            KeyCode::Char('c') => {
                                if app.show_popup {
                                    app.spawn_peer();
                                    app.show_popup = false;
                                }
                            }
                            KeyCode::Char('d') => {
                                if app.show_popup {
                                    app.delete_peer();
                                }
                            }
                            KeyCode::Char('m') => {
                                if app.show_popup {
                                    app.show_message = true;
                                    app.show_popup = false;
                                }
                            }
                            _ => {}
                        }
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
