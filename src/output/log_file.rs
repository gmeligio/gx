use std::fs::{File, create_dir_all};
use std::io::{BufWriter, Write};
use std::path::PathBuf;

/// Writes timestamped log entries to a file in the OS temp directory.
pub struct LogFile {
    writer: BufWriter<File>,
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
        let _ = writeln!(self.writer, "[{ts}] {msg}");
        let _ = self.writer.flush();
    }

    /// Return the path of this log file.
    #[must_use]
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

/// Return current time as RFC-3339-compatible filename string (colons replaced with dashes).
fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Format as YYYY-MM-DDTHH-MM-SS (colons → dashes for filename safety)
    let (y, mo, d, h, mi, s) = secs_to_datetime(secs);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}-{mi:02}-{s:02}")
}

/// Return current time as `HH:MM:SS` for log entries.
fn wall_clock_hms() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let (_, _, _, h, mi, s) = secs_to_datetime(secs);
    format!("{h:02}:{mi:02}:{s:02}")
}

/// Convert Unix timestamp to (year, month, day, hour, min, sec).
fn secs_to_datetime(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400;

    // Days since 1970-01-01
    let mut year = 1970u32;
    let mut remaining = days;

    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        year += 1;
    }

    let months = [31u64, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 0u32;
    for (i, &days_in_month) in months.iter().enumerate() {
        let days_in_month = if i == 1 && is_leap(year) {
            29u64
        } else {
            days_in_month
        };
        if remaining < days_in_month {
            month = (i + 1) as u32;
            break;
        }
        remaining -= days_in_month;
    }

    #[allow(clippy::cast_possible_truncation)]
    (
        year,
        month,
        remaining as u32 + 1,
        h as u32,
        m as u32,
        s as u32,
    )
}

fn is_leap(year: u32) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}
