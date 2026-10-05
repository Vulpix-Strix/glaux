//! Instance ownership through a named mutex, with launches delivered to a
//! message-only window as `WM_COPYDATA`.
//!
//! Windows releases the mutex when the owning process exits for any reason.

use std::cell::RefCell;
use std::ptr;
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::Context;
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND, LPARAM, LRESULT, WPARAM,
};
use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    ASFW_ANY, AllowSetForegroundWindow, CreateWindowExW, DefWindowProcW, DispatchMessageW,
    FindWindowExW, GetMessageW, HWND_MESSAGE, MSG, PostMessageW, PostQuitMessage, RegisterClassW,
    SMTO_ABORTIFHUNG, SMTO_BLOCK, SendMessageTimeoutW, TranslateMessage, UnregisterClassW,
    WM_CLOSE, WM_COPYDATA, WM_DESTROY, WNDCLASSW,
};

use super::{Claim, IDENTIFIER, SecondaryLaunch};

/// Tags Glaux launch payloads among `WM_COPYDATA` messages ("GLXL").
const PAYLOAD_TAG: usize = 0x474C_584C;

/// Largest payload accepted from a later launch.
const MAX_PAYLOAD: u32 = 64 * 1024;

/// How long a later launch looks for the owner's window, which the owner
/// creates shortly after taking the mutex.
const FIND_WINDOW_TIMEOUT: Duration = Duration::from_secs(1);
const FIND_WINDOW_RETRY: Duration = Duration::from_millis(50);

/// How long a later launch waits for the owner to accept the payload.
const SEND_TIMEOUT_MS: u32 = 5_000;

thread_local! {
    /// Where the window procedure delivers launches. Set only on the
    /// listener thread, which owns the window.
    static LAUNCHES: RefCell<Option<Sender<SecondaryLaunch>>> = const { RefCell::new(None) };
}

/// A NUL-terminated UTF-16 copy of `text`.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn mutex_name() -> Vec<u16> {
    // `Local\` scopes the mutex to the current login session.
    wide(&format!("Local\\{IDENTIFIER}.SingleInstance"))
}

fn window_name() -> Vec<u16> {
    wide(&format!("{IDENTIFIER}.SingleInstance"))
}

/// Owns the mutex and the listener window.
///
/// Dropping it closes the window, joins its thread and releases the mutex.
pub(super) struct Owner {
    mutex: HANDLE,
    /// The listener window, as an integer so it can cross threads. Only
    /// meaningful while `listener` is `Some`.
    window: isize,
    listener: Option<JoinHandle<()>>,
}

impl Drop for Owner {
    fn drop(&mut self) {
        if let Some(listener) = self.listener.take() {
            // SAFETY: `window` was created by the listener thread and stays
            // valid until that thread handles WM_CLOSE.
            let posted = unsafe { PostMessageW(self.window as HWND, WM_CLOSE, 0, 0) };
            if posted == 0 {
                // Joining would wait forever on a loop nobody stops.
                tracing::warn!("failed to stop the single-instance listener window");
            } else if listener.join().is_err() {
                tracing::warn!("single-instance listener panicked");
            }
        }
        // SAFETY: `mutex` is a handle returned by CreateMutexW that only
        // this owner closes.
        unsafe { CloseHandle(self.mutex) };
    }
}

/// Takes the mutex, or forwards `launch` to the process holding it.
pub(super) fn claim(launch: &SecondaryLaunch, launches: Sender<SecondaryLaunch>) -> Claim<Owner> {
    let name = mutex_name();
    // SAFETY: `name` is a NUL-terminated UTF-16 string that outlives the
    // call; null security attributes request the defaults.
    let mutex = unsafe { CreateMutexW(ptr::null(), 0, name.as_ptr()) };
    // SAFETY: GetLastError has no preconditions; it is read right after
    // CreateMutexW on the same thread.
    let error = unsafe { GetLastError() };
    if mutex.is_null() {
        return Claim::Unavailable(anyhow::anyhow!(
            "failed to create the single-instance mutex (error {error})"
        ));
    }
    if error == ERROR_ALREADY_EXISTS {
        // SAFETY: `mutex` is a valid handle this function owns.
        unsafe { CloseHandle(mutex) };
        return match forward(launch) {
            Ok(()) => Claim::Forwarded,
            Err(err) => Claim::ForwardFailed(err),
        };
    }

    match spawn_listener(launches) {
        Ok((window, listener)) => Claim::Owner(Owner {
            mutex,
            window,
            listener: Some(listener),
        }),
        Err(err) => {
            // Keep the mutex: another launch must still see this process
            // as the owner rather than starting a second instance.
            tracing::warn!(error = %format!("{err:#}"), "single-instance listener unavailable");
            Claim::Owner(Owner {
                mutex,
                window: 0,
                listener: None,
            })
        }
    }
}

/// Sends `launch` to the owner's listener window.
fn forward(launch: &SecondaryLaunch) -> anyhow::Result<()> {
    // Lets the owner bring its window to the foreground in response.
    // SAFETY: AllowSetForegroundWindow has no pointer arguments.
    unsafe { AllowSetForegroundWindow(ASFW_ANY) };

    let payload = launch.to_json().context("failed to encode the launch")?;
    let len = u32::try_from(payload.len())
        .ok()
        .filter(|&len| len <= MAX_PAYLOAD)
        .context("launch payload is too large")?;
    let window =
        find_listener().context("Glaux is running but its listener window was not found")?;

    let data = COPYDATASTRUCT {
        dwData: PAYLOAD_TAG,
        cbData: len,
        lpData: payload.as_ptr().cast_mut().cast(),
    };
    let mut result = 0;
    // SAFETY: `data` and `payload` outlive this synchronous call, and the
    // system copies the payload into the receiving process.
    let sent = unsafe {
        SendMessageTimeoutW(
            window,
            WM_COPYDATA,
            0,
            ptr::from_ref(&data) as LPARAM,
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            SEND_TIMEOUT_MS,
            &mut result,
        )
    };
    anyhow::ensure!(sent != 0, "the running Glaux did not respond");
    anyhow::ensure!(result == 1, "the running Glaux rejected the launch");
    Ok(())
}

/// The owner's listener window, waiting briefly for it to appear.
fn find_listener() -> Option<HWND> {
    let class = window_name();
    let started = Instant::now();
    loop {
        // SAFETY: `class` is a NUL-terminated UTF-16 string that outlives
        // the call.
        let window = unsafe {
            FindWindowExW(
                HWND_MESSAGE,
                ptr::null_mut(),
                class.as_ptr(),
                class.as_ptr(),
            )
        };
        if !window.is_null() {
            return Some(window);
        }
        if started.elapsed() >= FIND_WINDOW_TIMEOUT {
            return None;
        }
        thread::sleep(FIND_WINDOW_RETRY);
    }
}

/// Starts the listener thread and waits until its window exists.
fn spawn_listener(launches: Sender<SecondaryLaunch>) -> anyhow::Result<(isize, JoinHandle<()>)> {
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let thread = thread::Builder::new()
        .name("glaux-single-instance".into())
        .spawn(move || {
            LAUNCHES.with(|slot| *slot.borrow_mut() = Some(launches));
            run_listener(&ready_tx);
        })
        .context("failed to start the single-instance listener")?;
    match ready_rx.recv() {
        Ok(Ok(window)) => Ok((window, thread)),
        Ok(Err(err)) => {
            let _ = thread.join();
            Err(err)
        }
        Err(_) => {
            let _ = thread.join();
            anyhow::bail!("the single-instance listener stopped during startup")
        }
    }
}

/// Creates the listener window, reports it through `ready` and pumps its
/// messages until it is closed.
fn run_listener(ready: &mpsc::SyncSender<anyhow::Result<isize>>) {
    let class = window_name();
    // SAFETY: a null module name returns this executable's handle.
    let instance = unsafe { GetModuleHandleW(ptr::null()) };
    let window_class = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        lpszClassName: class.as_ptr(),
        ..Default::default()
    };
    // SAFETY: `window_class` and the class name it points to outlive the
    // class, which is unregistered before `class` is dropped.
    if unsafe { RegisterClassW(&window_class) } == 0 {
        // SAFETY: read right after the failing call on the same thread.
        let error = unsafe { GetLastError() };
        let _ = ready.send(Err(anyhow::anyhow!(
            "failed to register the single-instance window class (error {error})"
        )));
        return;
    }

    // SAFETY: the class is registered above; a message-only window takes no
    // menu or creation parameter.
    let window = unsafe {
        CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            ptr::null_mut(),
            instance,
            ptr::null(),
        )
    };
    if window.is_null() {
        // SAFETY: read right after the failing call on the same thread.
        let error = unsafe { GetLastError() };
        let _ = ready.send(Err(anyhow::anyhow!(
            "failed to create the single-instance window (error {error})"
        )));
    } else {
        let _ = ready.send(Ok(window as isize));
        let mut message = MSG::default();
        // SAFETY: `message` is a valid MSG to fill; GetMessageW returns 0
        // after WM_QUIT and -1 on error, both of which end the loop.
        while unsafe { GetMessageW(&mut message, ptr::null_mut(), 0, 0) } > 0 {
            // SAFETY: `message` was filled by GetMessageW.
            unsafe {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
    // SAFETY: no window of this class remains on this thread.
    unsafe { UnregisterClassW(class.as_ptr(), instance) };
}

/// Window procedure of the listener window.
unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_COPYDATA => LRESULT::from(receive(lparam)),
        WM_DESTROY => {
            // SAFETY: called on the thread whose message loop should end.
            unsafe { PostQuitMessage(0) };
            0
        }
        // SAFETY: forwards the unchanged arguments of this call.
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

/// Handles one `WM_COPYDATA`, returning whether it was a Glaux launch.
fn receive(lparam: LPARAM) -> bool {
    // SAFETY: for WM_COPYDATA the system passes a COPYDATASTRUCT pointer in
    // `lparam` that stays valid while the message is handled.
    let data = unsafe { &*(lparam as *const COPYDATASTRUCT) };
    if data.dwData != PAYLOAD_TAG || data.cbData > MAX_PAYLOAD {
        return false;
    }
    let bytes = if data.cbData == 0 || data.lpData.is_null() {
        &[][..]
    } else {
        // SAFETY: the system guarantees `cbData` readable bytes at `lpData`
        // for the duration of the message.
        unsafe { std::slice::from_raw_parts(data.lpData.cast::<u8>(), data.cbData as usize) }
    };
    match SecondaryLaunch::from_json(bytes) {
        Ok(launch) => {
            LAUNCHES.with(|slot| {
                if let Some(launches) = slot.borrow().as_ref() {
                    // The receiver is gone only while Glaux is shutting down.
                    let _ = launches.send(launch);
                }
            });
            true
        }
        Err(err) => {
            tracing::warn!(error = %err, "ignoring an unreadable launch");
            false
        }
    }
}
