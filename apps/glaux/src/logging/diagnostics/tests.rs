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
