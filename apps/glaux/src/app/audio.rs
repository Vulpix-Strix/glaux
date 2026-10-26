//! The audio device and the stream the system's sound goes through.
//!
//! Audio is optional. When the device or the stream is unavailable, or fails
//! later, the rest of the session runs silent.

use glaux_host_raylib::{AudioDevice, AudioOutput};
use glaux_machine::psg::SAMPLE_RATE;

use crate::logging;

/// Opens the default audio device, or `None` when there is none.
pub(super) fn open_device() -> Option<AudioDevice> {
    AudioDevice::open()
        .inspect_err(|err| {
            tracing::warn!(target: logging::DIAGNOSTIC_TARGET, error = %err, "audio unavailable, running silent");
        })
        .ok()
}

/// The stream samples are queued on, if one could be started.
pub(super) struct AudioSink<'a>(Option<AudioOutput<'a>>);

impl<'a> AudioSink<'a> {
    /// Starts a stream on `device`, if there is one.
    pub(super) fn open(device: Option<&'a AudioDevice>) -> Self {
        Self(device.and_then(|device| {
            device
                .output(SAMPLE_RATE)
                .inspect_err(|err| {
                    tracing::warn!(target: logging::DIAGNOSTIC_TARGET, error = %err, "audio unavailable, running silent");
                })
                .ok()
        }))
    }

    /// Queues `samples` for the device.
    pub(super) fn queue(&mut self, samples: &[i16]) {
        if let Some(output) = &mut self.0 {
            output.queue(samples);
        }
    }

    /// Feeds the device the samples queued so far. A failed stream is closed
    /// and the rest of the session runs silent.
    pub(super) fn pump(&mut self) {
        if let Some(output) = &mut self.0
            && let Err(err) = output.pump()
        {
            tracing::warn!(
                target: logging::DIAGNOSTIC_TARGET,
                error = %err,
                "audio stream failed, running silent"
            );
            self.0 = None;
        }
    }
}
