use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use tracing_subscriber::fmt::MakeWriter;

struct LogFile(Arc<Mutex<Option<File>>>);

struct LogWriter<'a>(MutexGuard<'a, Option<File>>);

impl io::Write for LogWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.0.as_mut() {
            Some(file) => file.write(buf),
            None => Ok(buf.len()),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.0.as_mut() {
            Some(file) => file.flush(),
            None => Ok(()),
        }
    }
}

impl<'a> MakeWriter<'a> for LogFile {
    type Writer = LogWriter<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        let guard = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        LogWriter(guard)
    }
}

pub fn init(store_dir: &Path) -> io::Result<()> {
    let log_path = store_dir
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("danie.log");
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .ok();
    let writer = LogFile(Arc::new(Mutex::new(file)));
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_writer(writer)
        .try_init()
        .ok();
    Ok(())
}
