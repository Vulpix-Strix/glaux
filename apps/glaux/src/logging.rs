//! Stderr plus one log file per session in Glaux's data
//! directory.

use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use tracing::Subscriber;
use tracing::level_filters::LevelFilter;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{EnvFilter, Layer};

/// Session log files kept in the logs directory, including the current one.
const MAX_SESSIONS: usize = 10;

const SESSION_PREFIX: &str = "glaux-";
const SESSION_SUFFIX: &str = ".log";

/// Keeps the file writer alive; dropping it flushes the queued events.
///
/// The file writer is non-blocking with a bounded queue, so events can be
/// dropped under heavy load.
pub struct Logging {
    _file_guard: Option<WorkerGuard>,
}

/// Installs the global subscriber: INFO and above to stderr, DEBUG and above
/// to a new session file under `<data_dir>/logs`. `RUST_LOG` overrides both
/// levels.
///
/// Failing to set up the file only disables file logging; stderr logging
/// always works.
pub fn init(data_dir: Option<&Path>) -> Logging {
    let file = data_dir
        .context("no data directory is available on this platform")
        .and_then(|dir| prepare_file_logging(&dir.join("logs"), SystemTime::now()));

    let (file_session, file_error) = match file {
        Ok(session) => (Some(session), None),
        Err(err) => (None, Some(err)),
    };
    let (file_layer, guard, pruning_errors) = match file_session {
        Some(SessionFile {
            file,
            pruning_errors,
        }) => {
            let (writer, guard) = tracing_appender::non_blocking(file);
            let layer = tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(writer)
                .with_filter(filter(LevelFilter::DEBUG));
            (Some(layer), Some(guard), pruning_errors)
        }
        None => (None, None, Vec::new()),
    };

    let installed = tracing_subscriber::registry()
        .with(stderr_layer())
        .with(file_layer)
        .try_init();
    if let Err(err) = installed {
        eprintln!("glaux: failed to install the log subscriber: {err}");
    }

    if let Some(err) = file_error {
        tracing::warn!(error = %format!("{err:#}"), "file logging unavailable, logging to stderr only");
    }
    for err in pruning_errors {
        tracing::warn!(error = %format!("{err:#}"), "failed to remove an old session log");
    }

    Logging { _file_guard: guard }
}

/// Installs the global subscriber with only the stderr output of [`init`],
/// for processes that should leave no session log behind.
pub fn init_stderr() -> Logging {
    let installed = tracing_subscriber::registry()
        .with(stderr_layer())
        .try_init();
    if let Err(err) = installed {
        eprintln!("glaux: failed to install the log subscriber: {err}");
    }
    Logging { _file_guard: None }
}

/// INFO and above to stderr, unless `RUST_LOG` says otherwise.
fn stderr_layer<S>() -> impl Layer<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    tracing_subscriber::fmt::layer()
        .with_writer(io::stderr)
        .with_filter(filter(LevelFilter::INFO))
}

/// A filter at `default`, unless `RUST_LOG` says otherwise.
fn filter(default: LevelFilter) -> EnvFilter {
    EnvFilter::builder()
        .with_default_directive(default.into())
        .from_env_lossy()
}

/// A newly created session log file.
#[derive(Debug)]
struct SessionFile {
    file: File,
    /// Old session logs that could not be removed; not fatal.
    pruning_errors: Vec<anyhow::Error>,
}

/// Creates `logs_dir`, removes the oldest session logs so that at most
/// [`MAX_SESSIONS`] remain with the new one, and creates the new session log.
fn prepare_file_logging(logs_dir: &Path, now: SystemTime) -> anyhow::Result<SessionFile> {
    fs::create_dir_all(logs_dir)
        .with_context(|| format!("failed to create {}", logs_dir.display()))?;

    let pruning_errors = prune_sessions(logs_dir, MAX_SESSIONS.saturating_sub(1));
    let file = create_session_file(logs_dir, &session_log_name(now))?;
    Ok(SessionFile {
        file,
        pruning_errors,
    })
}

/// Removes session logs in `logs_dir` beyond the newest `keep`, returning
/// the failures.
fn prune_sessions(logs_dir: &Path, keep: usize) -> Vec<anyhow::Error> {
    let entries = match fs::read_dir(logs_dir) {
        Ok(entries) => entries,
        Err(err) => {
            return vec![
                anyhow::Error::new(err).context(format!("failed to list {}", logs_dir.display())),
            ];
        }
    };
    let names: Vec<String> = entries
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .collect();

    stale_sessions(names.iter().map(String::as_str), keep)
        .into_iter()
        .filter_map(|name| {
            let path = logs_dir.join(name);
            fs::remove_file(&path)
                .with_context(|| format!("failed to remove {}", path.display()))
                .err()
        })
        .collect()
}

/// Opens `name` in `logs_dir` as a new file. If it already exists, tries
/// `name` with a `-2`, `-3`, ... suffix before the extension.
fn create_session_file(logs_dir: &Path, name: &str) -> anyhow::Result<File> {
    let stem = name.strip_suffix(SESSION_SUFFIX).unwrap_or(name);
    let mut path = logs_dir.join(name);
    for attempt in 2.. {
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok(file),
            Err(err) if err.kind() == ErrorKind::AlreadyExists && attempt <= 100 => {
                path = logs_dir.join(format!("{stem}-{attempt}{SESSION_SUFFIX}"));
            }
            Err(err) => {
                return Err(err).with_context(|| format!("failed to create {}", path.display()));
            }
        }
    }
    unreachable!("the attempt counter is unbounded")
}

/// Session log names among `names` that fall outside the newest `keep`,
/// oldest first. Names that are not Glaux session logs are ignored.
fn stale_sessions<'a>(names: impl IntoIterator<Item = &'a str>, keep: usize) -> Vec<&'a str> {
    let mut sessions: Vec<&str> = names
        .into_iter()
        .filter(|name| is_session_log(name))
        .collect();
    // Session names start with a fixed-width UTC timestamp, so name order is
    // creation order.
    sessions.sort_unstable();
    let stale = sessions.len().saturating_sub(keep);
    sessions.truncate(stale);
    sessions
}

/// Whether `name` has the shape produced by [`session_log_name`], including
/// a collision suffix.
fn is_session_log(name: &str) -> bool {
    let Some(stamp) = name
        .strip_prefix(SESSION_PREFIX)
        .and_then(|rest| rest.strip_suffix(SESSION_SUFFIX))
    else {
        return false;
    };
    // `YYYY-MM-DDTHH-MM-SSZ` is 20 characters.
    let (timestamp, suffix) = stamp.split_at_checked(20).unwrap_or((stamp, ""));
    let timestamp_ok = timestamp.len() == 20
        && timestamp.char_indices().all(|(i, c)| match i {
            4 | 7 | 13 | 16 => c == '-',
            10 => c == 'T',
            19 => c == 'Z',
            _ => c.is_ascii_digit(),
        });
    let suffix_ok = suffix.is_empty()
        || suffix
            .strip_prefix('-')
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
    timestamp_ok && suffix_ok
}

/// File name of the session log started at `now`:
/// `glaux-YYYY-MM-DDTHH-MM-SSZ.log`, in UTC.
fn session_log_name(now: SystemTime) -> String {
    // Times before the epoch only happen with a broken clock; clamp them.
    let secs = now
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    let days = i64::try_from(secs / 86_400).unwrap_or(i64::MAX);
    let (year, month, day) = civil_from_days(days);
    let rem = secs % 86_400;
    let (hour, minute, second) = (rem / 3600, rem % 3600 / 60, rem % 60);
    format!(
        "{SESSION_PREFIX}{year:04}-{month:02}-{day:02}T{hour:02}-{minute:02}-{second:02}Z{SESSION_SUFFIX}"
    )
}

/// Converts days since 1970-01-01 into a proleptic Gregorian
/// `(year, month, day)`, using Howard Hinnant's `civil_from_days`.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests;
