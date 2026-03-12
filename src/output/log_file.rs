use std::fs::{File, create_dir_all};
use std::io::{BufWriter, Write as _};
use std::path::PathBuf;
use time::OffsetDateTime;

/// Writes timestamped log entries to a file in the OS temp directory.
pub struct LogFile {
    /// Buffered writer for the log file.
    writer: BufWriter<File>,
    /// Path to the log file.
    path: PathBuf,
}

impl LogFile {
    /// Create a new log file at `{tmp}/gx/{command}/{RFC-date}.log`.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be created or the file cannot be opened.
    pub fn new(command: &str) -> Result<Self, std::io::Error> {
        let dir = std::env::temp_dir().join("gx").join(command);
        create_dir_all(&dir)?;

        let now = chrono_now();
        let filename = format!("{now}.log");
        let path = dir.join(&filename);

        let file = File::create(&path)?;
        let writer = BufWriter::new(file);

        Ok(Self { writer, path })
    }

    /// Write a timestamped message to the log file.
    pub fn write(&mut self, msg: &str) {
        let ts = wall_clock_hms();
        drop(writeln!(self.writer, "[{ts}] {msg}"));
        drop(self.writer.flush());
    }

    /// Return the path of this log file.
    #[must_use]
    pub const fn path(&self) -> &PathBuf {
        &self.path
    }
}

/// Return current time as RFC-3339-compatible filename string (colons replaced with dashes).
fn chrono_now() -> String {
    let dt = OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}-{:02}-{:02}",
        dt.year(),
        u8::from(dt.month()),
        dt.day(),
        dt.hour(),
        dt.minute(),
        dt.second()
    )
}

/// Return current time as `HH:MM:SS` for log entries.
fn wall_clock_hms() -> String {
    let dt = OffsetDateTime::now_utc();
    format!("{:02}:{:02}:{:02}", dt.hour(), dt.minute(), dt.second())
}
