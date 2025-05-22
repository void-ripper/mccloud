use std::{
    collections::HashMap,
    fs::File,
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, LazyLock, Mutex, Once, RwLock,
    },
    thread::ThreadId,
};

use mccloud::Peer;
use tracing::{field::Visit, span};

pub mod cluster;
pub mod configs;

static LOGFILE: LazyLock<Mutex<File>> = LazyLock::new(|| {
    // let date = time::OffsetDateTime::now_utc();
    // let h =  date.to_hms();
    // let n = date.nanosecond();
    // let name = format!("data/test_{}{:02}{:02}_{:02}{:02}{:02}_{}.log", date.year(), date.month(), date.day(), h.0, h.1, h.2, n);
    let name = "data/test.log";
    Mutex::new(File::create(name).unwrap())
});

static INIT_ONCE: Once = Once::new();

pub fn init_log(filename: &str) -> tracing::span::Span {
    INIT_ONCE.call_once(|| {
        tracing::subscriber::set_global_default(MockSubscriber::new()).unwrap();
    });

    if let Some(parent) = PathBuf::from(filename).parent() {
        std::fs::create_dir_all(parent).unwrap();
    }

    tracing::debug_span!("TEST", file = filename)
}

struct MockSubscriber {
    span_ids: AtomicU64,
    file_to_id: Mutex<HashMap<String, span::Id>>,
    id_to_file: RwLock<HashMap<span::Id, Mutex<File>>>,
    current_id: RwLock<HashMap<ThreadId, span::Id>>,
}

impl MockSubscriber {
    fn new() -> Self {
        Self {
            span_ids: AtomicU64::new(2),
            file_to_id: Mutex::new(HashMap::new()),
            id_to_file: RwLock::new(HashMap::new()),
            current_id: RwLock::new(HashMap::new()),
        }
    }
}

impl tracing::Subscriber for MockSubscriber {
    fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, span: &span::Attributes<'_>) -> span::Id {
        let mut visitor = FileVisitor::new();
        span.record(&mut visitor);

        if let Some(ref file) = visitor.file {
            let mut f2id = self.file_to_id.lock().unwrap();
            let id = if let Some(id) = f2id.get(file) {
                id.clone()
            } else {
                let id = self.span_ids.fetch_add(1, Ordering::SeqCst);
                let id = span::Id::from_u64(id);
                f2id.insert(file.clone(), id.clone());
                let mut id2file = self.id_to_file.write().unwrap();
                id2file.insert(id.clone(), Mutex::new(File::create(file).unwrap()));
                id
            };

            {
                let mut log = LOGFILE.lock().unwrap();
                writeln!(log, "log to {:?}{} {:?}", id, file, span.metadata().name()).unwrap();
            }
            id
        } else {
            span::Id::from_u64(1)
        }
    }

    fn record(&self, _span: &span::Id, _values: &span::Record<'_>) {
        todo!()
    }

    fn record_follows_from(&self, _span: &span::Id, _follows: &span::Id) {
        todo!()
    }

    fn event(&self, event: &tracing::Event<'_>) {
        let fields: Vec<String> = event.fields().map(|f| f.to_string()).collect();
        let meta = event.metadata();
        let mut visitor = FileVisitor::new();
        event.record(&mut visitor);

        if event.is_contextual() {
            let tid = std::thread::current().id();
            let id = self.current_id.read().unwrap().get(&tid).cloned();
            let now = time::OffsetDateTime::now_utc();
            let msg = format!(
                "{} {:15}\t{}\t{} {} {:?}\n",
                now.date(),
                now.time(),
                meta.level(),
                meta.target(),
                visitor.message,
                visitor.unknown
            );

            if let Some(id) = id {
                let id2file = self.id_to_file.read().unwrap();
                let id2file = id2file.get(&id);

                if let Some(mfile) = id2file {
                    let mut log = mfile.lock().unwrap();
                    log.write(msg.as_bytes()).unwrap();
                } else {
                    let mut log = LOGFILE.lock().unwrap();
                    log.write(msg.as_bytes()).unwrap();
                }
            } else {
                let mut log = LOGFILE.lock().unwrap();
                log.write(msg.as_bytes()).unwrap();
            }
        } else {
            let mut log = LOGFILE.lock().unwrap();
            writeln!(
                log,
                "{:?} target({}) file({:?}) msg({}) {:?} {}",
                fields,
                meta.target(),
                visitor.file,
                visitor.message,
                event.parent(),
                event.is_contextual(),
            )
            .unwrap();
        }
    }

    fn enter(&self, span: &span::Id) {
        let mut log = LOGFILE.lock().unwrap();
        let tid = std::thread::current().id();
        writeln!(log, "enter {:?} {:?}", span, tid).unwrap();
        self.current_id.write().unwrap().insert(tid, span.clone());
    }

    fn exit(&self, span: &span::Id) {
        let mut log = LOGFILE.lock().unwrap();
        let tid = std::thread::current().id();
        writeln!(log, "exit {:?} {:?}", span, tid).unwrap();
        self.current_id.write().unwrap().remove(&tid);
    }
}

struct FileVisitor {
    file: Option<String>,
    message: String,
    unknown: HashMap<String, String>,
}

impl FileVisitor {
    fn new() -> Self {
        Self {
            file: None,
            message: String::new(),
            unknown: HashMap::new(),
        }
    }
}

impl Visit for FileVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        match field.name() {
            "message" => {
                self.message = format!("{:?}", value);
            }
            _ => {
                self.unknown.insert(field.name().to_string(), format!("{:?}", value));
            }
        }
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.unknown.insert(field.name().to_string(), format!("{}", value));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        match field.name() {
            "file" => {
                self.file = Some(value.to_string());
            }
            "message" => {
                self.message = value.to_string();
            }
            _ => {
                self.unknown.insert(field.name().to_string(), value.to_string());
            }
        }
    }
}

#[allow(dead_code)]
pub async fn assert_all_known(peers: &Vec<Arc<Peer>>, cnt: usize) {
    for (i, p) in peers.iter().enumerate() {
        if !p.is_shutdown() {
            let all_kn_cnt = p.known_pubkeys().await.len();
            assert_eq!(all_kn_cnt, cnt, "peer({})", i);
        }
    }
}
