//! Audio output: plays mono signed 16-bit samples through raylib.

use std::collections::VecDeque;

use raylib::core::audio::{AudioStream, RaylibAudio};
use raylib::core::error::{AudioInitError, UpdateAudioStreamError};

/// Samples in each of the stream's device buffers. Raylib double-buffers a
/// stream, so this is also how much audio one refill must supply.
const STREAM_BUFFER_FRAMES: usize = 1024;

/// Most samples held between the producer and the device. Beyond this the
/// oldest are dropped, bounding latency when the producer runs ahead of the
/// device clock.
const QUEUE_CAPACITY: usize = STREAM_BUFFER_FRAMES * 4;

/// Errors raised while opening or feeding the audio device.
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("failed to open the audio device")]
    Init(#[source] AudioInitError),
    #[error("the audio device rejected a {sample_rate} Hz mono 16-bit stream")]
    InvalidStream { sample_rate: u32 },
    #[error("failed to send samples to the audio stream")]
    Update(#[source] UpdateAudioStreamError),
}

/// The open audio device. Streams created from it borrow it, so it must
/// outlive them.
pub struct AudioDevice {
    audio: RaylibAudio,
}

impl AudioDevice {
    /// Opens the platform's default audio device.
    pub fn open() -> Result<Self, AudioError> {
        let audio = RaylibAudio::init_audio_device().map_err(AudioError::Init)?;
        tracing::info!("audio device opened");
        Ok(Self { audio })
    }

    /// Starts a playing mono 16-bit stream at `sample_rate` Hz.
    pub fn output(&self, sample_rate: u32) -> Result<AudioOutput<'_>, AudioError> {
        // Applies to streams created after this call.
        self.audio
            .set_audio_stream_buffer_size_default(STREAM_BUFFER_FRAMES as i32);
        let stream = self.audio.new_audio_stream(sample_rate, 16, 1);
        if !stream.is_audio_stream_valid() {
            return Err(AudioError::InvalidStream { sample_rate });
        }
        stream.play();
        tracing::info!(
            sample_rate,
            buffer_frames = STREAM_BUFFER_FRAMES,
            "audio stream started"
        );
        Ok(AudioOutput {
            stream,
            queue: SampleQueue::new(QUEUE_CAPACITY),
            block: vec![0; STREAM_BUFFER_FRAMES],
        })
    }
}

/// A playing stream fed from a queue of samples.
///
/// The producer [`queue`](Self::queue)s samples at its own pace; the device
/// takes them as it plays when [`pump`](Self::pump) runs. Gaps play as
/// silence and excess is dropped oldest-first.
pub struct AudioOutput<'a> {
    stream: AudioStream<'a>,
    queue: SampleQueue,
    block: Vec<i16>,
}

impl AudioOutput<'_> {
    /// Appends `samples` to the queue for playback.
    pub fn queue(&mut self, samples: &[i16]) {
        let dropped = self.queue.push(samples);
        if dropped > 0 {
            tracing::debug!(dropped, "audio queue full, dropped the oldest samples");
        }
    }

    /// Refills every device buffer that has finished playing.
    ///
    /// Call at least once per host frame; a device buffer lasts
    /// [`STREAM_BUFFER_FRAMES`] samples.
    pub fn pump(&mut self) -> Result<(), AudioError> {
        while self.stream.is_processed() {
            let padded = self.queue.fill(&mut self.block);
            if padded > 0 {
                tracing::trace!(padded, "audio queue ran dry, padded with silence");
            }
            self.stream
                .update(&self.block)
                .map_err(AudioError::Update)?;
        }
        Ok(())
    }
}

/// A bounded FIFO of samples.
#[derive(Debug)]
struct SampleQueue {
    samples: VecDeque<i16>,
    capacity: usize,
}

impl SampleQueue {
    fn new(capacity: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Appends `samples`, dropping the oldest held samples past the capacity.
    /// Returns how many were dropped.
    fn push(&mut self, samples: &[i16]) -> usize {
        self.samples.extend(samples);
        let excess = self.samples.len().saturating_sub(self.capacity);
        self.samples.drain(..excess);
        excess
    }

    /// Moves the oldest samples into `out`, padding with silence when the
    /// queue runs out. Returns how many samples were padded.
    fn fill(&mut self, out: &mut [i16]) -> usize {
        let available = self.samples.len().min(out.len());
        for (slot, sample) in out.iter_mut().zip(self.samples.drain(..available)) {
            *slot = sample;
        }
        out[available..].fill(0);
        out.len() - available
    }
}

#[cfg(test)]
mod tests;
