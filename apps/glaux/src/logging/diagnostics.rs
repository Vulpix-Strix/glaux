//! Carries `tracing` events meant for the log panel to the system.
//!
//! Only events logged to [`TARGET`] at INFO or above qualify. They cross a
//! bounded channel, so a burst can never block the thread that logged.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};

use tracing::field::{Field as TracingField, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::filter::filter_fn;
use tracing_subscriber::layer::Context;

pub const TARGET: &str = "glaux::diagnostic";

/// Events held between drains. Beyond it they are dropped and counted.
const CAPACITY: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    Str(Box<str>),
    I64(i64),
    U64(u64),
    Bool(bool),
}

/// An event with its fields still typed, to be turned into a record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub level: Level,
    pub message: Box<str>,
    pub fields: Vec<(&'static str, Field)>,
}

#[derive(Debug)]
pub struct Diagnostics {
    receiver: Receiver<Diagnostic>,
    dropped: Arc<AtomicU64>,
}

impl Diagnostics {
    /// Takes at most `limit` events and leaves the rest queued.
    pub fn drain(&self, limit: usize) -> Vec<Diagnostic> {
        self.receiver.try_iter().take(limit).collect()
    }

    /// Events dropped for lack of room since the last call.
    pub fn take_dropped(&self) -> u64 {
        self.dropped.swap(0, Ordering::Relaxed)
    }
}

pub struct DiagnosticLayer {
    sender: SyncSender<Diagnostic>,
    dropped: Arc<AtomicU64>,
}

/// The layer carries its own filter, so the stderr and file layers keep
/// theirs and other targets never reach it.
pub fn layer<S>() -> (impl Layer<S>, Diagnostics)
where
    S: Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    let (sender, receiver) = sync_channel(CAPACITY);
    let dropped = Arc::new(AtomicU64::new(0));
    let layer = DiagnosticLayer {
        sender,
        dropped: Arc::clone(&dropped),
    }
    .with_filter(filter_fn(|metadata| {
        metadata.target() == TARGET && *metadata.level() <= Level::INFO
    }));
    (layer, Diagnostics { receiver, dropped })
}

impl<S: Subscriber> Layer<S> for DiagnosticLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut collected = Collected::default();
        event.record(&mut collected);
        let diagnostic = Diagnostic {
            level: *event.metadata().level(),
            message: collected.message.into_boxed_str(),
            fields: collected.fields,
        };
        match self.sender.try_send(diagnostic) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
            // The system is gone: nothing is left to show it to.
            Err(TrySendError::Disconnected(_)) => {}
        }
    }
}

#[derive(Default)]
struct Collected {
    message: String,
    fields: Vec<(&'static str, Field)>,
}

impl Visit for Collected {
    fn record_i64(&mut self, field: &TracingField, value: i64) {
        self.fields.push((field.name(), Field::I64(value)));
    }

    fn record_u64(&mut self, field: &TracingField, value: u64) {
        self.fields.push((field.name(), Field::U64(value)));
    }

    fn record_bool(&mut self, field: &TracingField, value: bool) {
        self.fields.push((field.name(), Field::Bool(value)));
    }

    fn record_str(&mut self, field: &TracingField, value: &str) {
        if field.name() == "message" {
            self.message = value.to_owned();
        } else {
            self.fields.push((field.name(), Field::Str(value.into())));
        }
    }

    fn record_debug(&mut self, field: &TracingField, value: &dyn fmt::Debug) {
        let text = format!("{value:?}");
        if field.name() == "message" {
            self.message = text;
        } else {
            self.fields.push((field.name(), Field::Str(text.into())));
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use tracing_subscriber::prelude::*;

    use super::*;

    /// Runs `emit` under a subscriber with only the diagnostic layer.
    fn collect(emit: impl FnOnce()) -> Diagnostics {
        let (layer, diagnostics) = layer();
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, emit);
        diagnostics
    }

    #[test]
    fn only_events_for_the_diagnostic_target_at_info_or_above_arrive() {
        let diagnostics = collect(|| {
            tracing::info!("an ordinary event");
            tracing::warn!(target: TARGET, path = "game.luau", line = 7_u64, "audio unavailable");
            tracing::debug!(target: TARGET, "too detailed");
            tracing::info!(target: "somewhere::else", "another target");
        });
        assert_eq!(
            diagnostics.drain(10),
            [Diagnostic {
                level: Level::WARN,
                message: "audio unavailable".into(),
                fields: vec![
                    ("path", Field::Str("game.luau".into())),
                    ("line", Field::U64(7)),
                ],
            }]
        );
    }

    #[test]
    fn a_full_channel_drops_events_without_blocking_and_counts_them() {
        let diagnostics = collect(|| {
            for n in 0..CAPACITY + 5 {
                tracing::info!(target: TARGET, n, "event");
            }
        });
        assert_eq!(diagnostics.drain(usize::MAX).len(), CAPACITY);
        assert_eq!(diagnostics.take_dropped(), 5);
        assert_eq!(diagnostics.take_dropped(), 0);
    }
}
