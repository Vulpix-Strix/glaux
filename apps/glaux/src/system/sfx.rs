//! System ROM sound effects, played a logical tick at a time.

use glaux_audio::AudioState;
use glaux_audio::sfx::SfxPlayer;
use glaux_machine::psg::Psg;
use glaux_machine::system_rom::{SystemSfx, SystemSounds};

use super::audio;

/// The System ROM's sound. It plays one effect at a time, writing into audio
/// registers of its own that the shell and cartridges never share.
#[derive(Debug, Clone)]
pub struct SystemSound {
    sounds: SystemSounds,
    player: SfxPlayer,
    audio: AudioState,
    /// Whether the PSG must be reset before the next apply, because the
    /// registers were replaced.
    restart: bool,
}

impl SystemSound {
    pub fn new(sounds: SystemSounds) -> Self {
        Self {
            sounds,
            player: SfxPlayer::new(),
            audio: AudioState::new(),
            restart: false,
        }
    }

    /// Starts `sfx` from its first step. Whatever was playing is cut off:
    /// the registers start over silent and the PSG is reset on the next
    /// [`apply`](Self::apply), so nothing of the previous sound carries over.
    pub fn play(&mut self, sfx: SystemSfx) {
        self.reset();
        self.player.play(self.sounds.get(sfx).clone());
    }

    /// Cuts off whatever is playing, as [`play`](Self::play) does, and
    /// leaves the registers silent.
    pub fn stop(&mut self) {
        self.reset();
        self.player.stop();
    }

    fn reset(&mut self) {
        self.audio = AudioState::new();
        self.restart = true;
    }

    /// Whether steps of the current effect remain. The last one may have
    /// released a gate whose envelope is still fading out in the PSG.
    #[cfg(test)]
    pub fn is_active(&self) -> bool {
        self.player.is_active()
    }

    /// Applies the steps scheduled for this logical tick. A step the audio
    /// registers reject stops the effect.
    pub fn tick(&mut self) {
        if let Err(err) = self.player.tick(&mut self.audio) {
            tracing::warn!(error = %err, "system sound effect stopped");
        }
    }

    /// Writes the registers into `psg`. `fresh` means `psg` was just reset
    /// for the System ROM. It is also reset here if the registers were
    /// replaced since the last apply.
    pub fn apply(&mut self, psg: &mut Psg, fresh: bool) {
        let restart = std::mem::take(&mut self.restart);
        if restart && !fresh {
            *psg = Psg::new();
        }
        audio::apply(&mut self.audio, psg, fresh || restart);
    }
}
