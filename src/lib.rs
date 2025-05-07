//! A simple logger that writes to stdout and optionally also to a log file.
//!
//! The logger is initialized by calling [`init`], and after that one can use log's macros. Each log will be in the format [ A B C ]: D, where A is the UTC time and date in the RFC 3339 format, B is log's target, C is the log level, and D is the actual message.
//!
//! # Panics
//!
//! The logger can panic during logging if writing to stdout of the log file returns an error, the time fails to format, or if internal synchonization becomes poisoned.
//!
//! # Examples
//!
//! ```
//! hydrolox_log::init(log::LevelFilter::Info, false).unwrap();
//! log::info!("Logging works!");
//! ```

use std::{
    fmt::Display,
    fs::File,
    io::{stdout, BufWriter, Write},
    sync::{Mutex, OnceLock},
};

use log::Log;
use time::{format_description::BorrowedFormatItem, macros::format_description, OffsetDateTime};

const FILENAME_FORMAT: &[BorrowedFormatItem<'_>] =
    format_description!("[year]-[month]-[day]T[hour repr:24]_[minute]_[second]");

const ENTRY_FORMAT: &[BorrowedFormatItem<'_>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:4]");

fn now_formatted(format: &[BorrowedFormatItem<'_>]) -> Result<String, time::error::Format> {
    OffsetDateTime::now_utc()
        .format(format)
        .map(|mut s| {
            unsafe {
                s.as_bytes_mut().iter_mut().for_each(|b| {
                    if *b == b':' {
                        *b = b'_'
                    }
                })
            };
            s
        })
}
fn now_filename() -> Result<String, time::error::Format> {
    now_formatted(FILENAME_FORMAT)
}
fn now_entry() -> Result<String, time::error::Format> {
    now_formatted(ENTRY_FORMAT)
}

#[derive(Debug)]
pub enum LoggerInitError {
    ExePathGetErr(std::io::Error),
    NonUTF8Path,
    TimeFormatErr(time::error::Format),
    CreateFileErr(std::io::Error),
    SetLoggerErr(log::SetLoggerError),
}
impl Display for LoggerInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExePathGetErr(err) => write!(f, "Failed to get the current exe path: {err}"),
            Self::NonUTF8Path => write!(f, "Exe path contained non-UTF8 characters"),
            Self::TimeFormatErr(err) => write!(f, "Failed to format current time: {err}"),
            Self::CreateFileErr(err) => write!(f, "Failed to create logfile: {err}"),
            Self::SetLoggerErr(err) => write!(f, "Failed to set logger: {err}"),
        }
    }
}
impl std::error::Error for LoggerInitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ExePathGetErr(err) => Some(err),
            Self::NonUTF8Path => None,
            Self::TimeFormatErr(err) => Some(err),
            Self::CreateFileErr(err) => Some(err),
            Self::SetLoggerErr(err) => Some(err),
        }
    }
}
impl From<time::error::Format> for LoggerInitError {
    fn from(value: time::error::Format) -> Self {
        Self::TimeFormatErr(value)
    }
}
impl From<log::SetLoggerError> for LoggerInitError {
    fn from(value: log::SetLoggerError) -> Self {
        Self::SetLoggerErr(value)
    }
}

struct Logger {
    logfile: Mutex<Option<BufWriter<File>>>,
}
impl Logger {
    fn new(use_logfile: bool) -> Result<Self, LoggerInitError> {
        let logfile = Mutex::new(if use_logfile {
            let mut prefix =
                std::env::current_exe().map_err(|e| LoggerInitError::ExePathGetErr(e))?;
            prefix.pop();
            Some(BufWriter::new(
                File::create(format!(
                    "{}/log_{}.txt",
                    prefix.to_str().ok_or(LoggerInitError::NonUTF8Path)?,
                    now_filename()?
                ))
                .map_err(|e| LoggerInitError::CreateFileErr(e))?,
            ))
        } else {
            None
        });
        Ok(Self { logfile })
    }
}
impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::max_level()
    }
    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            let output = format!(
                "[ {} {} {} ]: {}\n",
                now_entry().expect("Failed to format current time"),
                record.target(),
                record.level(),
                record.args()
            );
            stdout()
                .write_all(output.as_bytes())
                .expect("Failed to log to stdout");
            if let Some(file) = self.logfile.lock().unwrap().as_mut() {
                file.write_all(output.as_bytes())
                    .expect("Failed to log to file");
            }
        }
    }
    fn flush(&self) {
        if let Some(file) = self.logfile.lock().unwrap().as_mut() {
            file.flush().expect("Failed to flush logfile");
        }
    }
}

static LOGGER: OnceLock<Logger> = OnceLock::new();

pub struct LogState {}
impl Drop for LogState {
    fn drop(&mut self) {
        if let Some(logger) = LOGGER.get() {
            logger.flush();
        }
    }
}

/// Initializes the logger.
///
/// If `use_logfile` is true, then the logger will also output log messages to a logfile located in the same path as the current executable. The file will be called log_X.txt, where X is the UTC time and date the logger was initialized in the RFC 3339 format. If writing to the logfile is enabled, then the function will return Some(LogState). This state should be dropped after all logging is complete to flush the logile.
///
/// # Errors
///
/// The function will return an error if the logger is already set. Additionally, if `use_logfile` is true, the function will return an error if:
///  - Getting the executable's current path returns an error
///  - Said path contains non-UTF8 characters
///  - Attempting to create the logfile returns an error
#[must_use]
pub fn init(
    max_level: log::LevelFilter,
    use_logfile: bool,
) -> Result<Option<LogState>, LoggerInitError> {
    _ = LOGGER.set(Logger::new(use_logfile)?);
    log::set_logger(LOGGER.get().unwrap())?;
    log::set_max_level(max_level);
    if use_logfile {
        Ok(Some(LogState {}))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_logfile() {
        let _log_state = init(log::LevelFilter::Debug, true).unwrap();
        log::debug!("Testing")
    }
}
