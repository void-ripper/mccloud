use std::{collections::HashSet, net::SocketAddr, path::PathBuf, sync::Arc};

use indexmap::IndexMap;
use mcriddle::{Config, Peer};
use ratatui::{
    buffer::Buffer,
    crossterm::event::{self, KeyCode, KeyEvent, KeyEventKind},
    layout::{Constraint, Flex, Layout, Rect},
    style::Stylize,
    text::Line,
    widgets::{Block, Clear, Paragraph, Widget},
};
use tokio::runtime::Runtime;

fn center(area: Rect, horizontal: Constraint, vertical: Constraint) -> Rect {
    let [area] = Layout::horizontal([horizontal]).flex(Flex::Center).areas(area);
    let [area] = Layout::vertical([vertical]).flex(Flex::Center).areas(area);
    area
}

#[derive(PartialEq, Eq, Debug)]
enum Mode {
    Normal,
    Message,
    Connect,
}

struct App {
    rt: Runtime,
    mode: Mode,
    show_popup_help: bool,
    peers: IndexMap<String, Arc<Peer>>,
    port_pool: u16,
    selected: usize,
    select_conn: usize,
    message_input: String,
}

impl App {
    fn spawn_peer(&mut self) {
        let cfg = Config {
            addr: format!("127.0.0.1:{}", self.port_pool),
            folder: PathBuf::from("data").join(self.port_pool.to_string()),
        };
        self.port_pool += 1;

        let p = self.rt.block_on(async { Peer::new(cfg).unwrap() });
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
        let lines: Vec<Line> = match self.mode {
            Mode::Normal => {
                vec![
                    " s: spawn peer".into(),
                    " d: delete peer".into(),
                    " m: send message".into(),
                    " c: connect to".into(),
                    " q: quit app".into(),
                ]
            }
            Mode::Connect => {
                vec![" c: connect two peers".into(), " q | esc: to normal mode".into()]
            }
            _ => {
                vec![]
            }
        };
        Clear.render(loc, buf);
        Paragraph::new(lines).block(block).render(loc, buf);
    }

    fn show_message(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::bordered().title(" Message ");
        Clear.render(area, buf);
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

            let block = Block::bordered().title(" known ");
            let mut lines: Vec<Line> = Vec::new();
            let known = self.rt.block_on(peer.known_pubkeys());
            for k in known {
                lines.push(hex::encode(k).into());
            }
            Paragraph::new(lines).block(block).render(layout[0], buf);

            let block = Block::bordered().title(" Data ");
            let mut lines: Vec<Line> = Vec::new();
            let it = self.rt.block_on(peer.block_it());
            for blk in it {
                lines.push(hex::encode(&blk.hash).into());
                for d in blk.data.iter() {
                    lines.push(format!("+ {}", String::from_utf8_lossy(d)).into());
                }
            }
            Paragraph::new(lines).block(block).render(layout[1], buf);
        }
    }

    fn on_message_mode(&mut self, ev: KeyEvent) {
        match ev.code {
            KeyCode::Char(a) => {
                self.message_input.push(a);
            }
            KeyCode::Enter => {
                let _ = self
                    .rt
                    .block_on(self.peers[self.selected].share(self.message_input.as_bytes().to_vec()));

                self.message_input.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Esc => {
                self.message_input.clear();
            }
            _ => {}
        }
    }

    fn on_normal_mode(&mut self, ev: KeyEvent) -> bool {
        match ev.code {
            KeyCode::Char('q') | KeyCode::Esc => return true,
            KeyCode::Char('j') | KeyCode::Down => {
                let len = self.peers.len();
                if len > 0 && self.selected < len - 1 {
                    self.selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
            }
            KeyCode::Char(' ') => {
                self.show_popup_help = !self.show_popup_help;
            }
            KeyCode::Char('s') => {
                self.spawn_peer();
                self.show_popup_help = false;
            }
            KeyCode::Char('d') => {
                self.delete_peer();
            }
            KeyCode::Char('m') => {
                self.show_popup_help = false;
                self.mode = Mode::Message;
            }
            KeyCode::Char('c') => {
                self.show_popup_help = false;
                self.mode = Mode::Connect;
            }
            _ => {}
        }

        false
    }

    fn on_connect_mode(&mut self, ev: KeyEvent) {
        match ev.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.mode = Mode::Normal;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                let len = self.peers.len();
                if len > 0 && self.selected < len - 1 {
                    self.select_conn += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.select_conn > 0 {
                    self.select_conn -= 1;
                }
            }
            KeyCode::Char('c') => {
                let p0 = self.peers[self.selected].clone();
                let p1 = self.peers[self.select_conn].clone();
                let addr: SocketAddr = p1.cfg.addr.parse().unwrap();
                let _ = self.rt.block_on(p0.connect(addr));
                self.mode = Mode::Normal;
            }
            _ => {}
        }
    }
}

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let status_layout = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);
        let layout = Layout::horizontal([Constraint::Percentage(25), Constraint::Min(1)]).split(status_layout[0]);

        let title = Line::from(" Peers ".bold());
        let block = Block::bordered().title(title.centered()); //.border_type(BorderType::Thick);
        let mut peers: Vec<Line> = Vec::new();

        let clients: HashSet<String> = {
            if let Some((_, peer)) = self.peers.get_index(self.selected) {
                let clients = self.rt.block_on(peer.client_pubkeys());
                clients.into_iter().map(|n| hex::encode(n)).collect()
            } else {
                HashSet::new()
            }
        };
        for (idx, p) in self.peers.keys().enumerate() {
            if idx == self.selected {
                peers.push(p.as_str().bold().on_light_blue().into());
            } else {
                if self.mode == Mode::Connect && self.select_conn == idx {
                    peers.push(p.as_str().gray().on_light_yellow().into());
                } else if clients.contains(p) {
                    peers.push(p.as_str().gray().on_light_green().into());
                } else {
                    peers.push(p.as_str().into());
                }
            }
        }
        Paragraph::new(peers).block(block).render(layout[0], buf);

        self.peer_info(layout[1], buf);

        Paragraph::new(format!(" Mode: {:?}", self.mode)).render(status_layout[1], buf);

        match self.mode {
            Mode::Normal => {
                if self.show_popup_help {
                    self.show_popup(layout[1], buf);
                }
            }
            Mode::Message => {
                let centered = center(layout[1], Constraint::Percentage(50), Constraint::Length(3));
                self.show_message(centered, buf);
            }
            Mode::Connect => {}
        }
    }
}

fn main() {
    let log = tracing_appender::rolling::never("", "test.log");
    let (writer, _guard) = tracing_appender::non_blocking(log);
    tracing_subscriber::fmt().with_writer(writer).init();

    let rt = Runtime::new().unwrap();
    let mut term = ratatui::init();

    let mut app = App {
        rt,
        mode: Mode::Normal,
        show_popup_help: false,
        peers: IndexMap::new(),
        port_pool: 29092,
        selected: 0,
        select_conn: 0,
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
                    match app.mode {
                        Mode::Normal => {
                            if app.on_normal_mode(ev) {
                                break;
                            }
                        }
                        Mode::Message => {
                            app.on_message_mode(ev);
                        }
                        Mode::Connect => {
                            app.on_connect_mode(ev);
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
