//! A loaded cartridge together with the Luau VM running it.

use glaux_luau::{Budget, Runtime, RuntimeError, StartFailure};

use crate::cartridge::{CartridgeError, LoadedCartridge};

/// Why a cartridge could not start.
#[derive(Debug)]
pub enum StartError {
    /// The entry script could not be read.
    Cartridge(CartridgeError),
    /// The entry chunk or `boot()` failed.
    Script(StartFailure),
}

/// A cartridge that could not start, handed back still loaded.
#[derive(Debug)]
pub struct FailedStart {
    pub cartridge: LoadedCartridge,
    pub error: StartError,
}

/// A cartridge whose VM is alive. Dropping it disposes of the VM.
#[derive(Debug)]
pub struct RunningCartridge {
    cartridge: LoadedCartridge,
    runtime: Runtime,
}

impl RunningCartridge {
    /// Creates a fresh VM for `cartridge` and runs its entry script and
    /// `boot()`. On failure the cartridge is handed back, still loaded.
    pub fn start(cartridge: LoadedCartridge) -> Result<Self, Box<FailedStart>> {
        let source = match cartridge.entry_source() {
            Ok(source) => source,
            Err(err) => return Err(failed(cartridge, StartError::Cartridge(err))),
        };
        let entry = &cartridge.manifest().entry;
        match Runtime::start(entry, &source, Budget::default()) {
            Ok(runtime) => Ok(Self { cartridge, runtime }),
            Err(failure) => Err(failed(cartridge, StartError::Script(failure))),
        }
    }

    pub fn cartridge(&self) -> &LoadedCartridge {
        &self.cartridge
    }

    /// Advances the cartridge by one logical tick.
    pub fn tick(&mut self) -> Result<(), RuntimeError> {
        self.runtime.tick()
    }

    /// Text the cartridge printed since the last call.
    pub fn take_output(&mut self) -> Vec<String> {
        self.runtime.take_output()
    }

    /// Disposes of the VM and gives back the cartridge.
    pub fn stop(self) -> LoadedCartridge {
        self.cartridge
    }
}

fn failed(cartridge: LoadedCartridge, error: StartError) -> Box<FailedStart> {
    Box::new(FailedStart { cartridge, error })
}
