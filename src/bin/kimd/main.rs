#![windows_subsystem = "windows"]
//! `kimd` — silent background daemon.
//! Phase 3 (T017): channel creation, thread spawning, named-event stop mechanism.
//! Phase 9 (T052): simplelog file logging initialised before any thread is spawned.

use std::fs::OpenOptions;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crossbeam_channel::bounded;
use simplelog::{Config, LevelFilter, WriteLogger};
use windows::Win32::Foundation::{CloseHandle, LPARAM, WPARAM};
use windows::Win32::System::Threading::{
    CreateEventW, GetCurrentThreadId, WaitForSingleObject,
};
use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};
use windows::core::PCWSTR;

use kim::db::writer::run_writer_thread;
use kim::hooks::{run_event_thread, run_hook_thread};
use kim::ime::run_uia_thread;
use kim::state::{delete_pid_file, kim_data_dir, write_pid_file};

// ── T052: file logger ─────────────────────────────────────────────────────────

fn init_logger() {
    let log_path = match kim_data_dir() {
        Ok(dir) => dir.join("kim.log"),
        Err(e) => {
            eprintln!("kimd: warning – could not determine log directory: {e}");
            return;
        }
    };
    let log_file = match OpenOptions::new().create(true).append(true).open(&log_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("kimd: warning – could not open log file {}: {e}", log_path.display());
            return;
        }
    };
    // Ignore error if logger is already initialised (e.g. in tests).
    WriteLogger::init(LevelFilter::Info, Config::default(), log_file).ok();
}

fn main() {
    // ── T052: initialise file logger first so all subsequent messages land in kim.log
    init_logger();

    // ── Autostart: delay 3 s so the desktop has settled before hooking ───────
    if std::env::args().any(|a| a == "--autostart") {
        log::info!("kimd: autostart mode — waiting 3 s before installing hooks");
        std::thread::sleep(std::time::Duration::from_secs(3));
    }

    // ── Write PID file so `kim status` / `kim stop` can find us ─────────────
    if let Err(e) = write_pid_file() {
        // Non-fatal: log but continue; PID file is a convenience, not critical.
        log::warn!("kimd: could not write PID file: {e}");
        eprintln!("kimd: warning – could not write PID file: {e}");
    }

    log::info!("kimd: daemon starting (PID: {})", std::process::id());

    // ── Shared stop flag ─────────────────────────────────────────────────────
    let stop_flag = Arc::new(AtomicBool::new(false));

    // ── Bounded input-event channel (capacity 1024) ──────────────────────────
    let (tx, rx) = bounded(1024);

    // ── Named stop event `Local\kim-stop-event` ──────────────────────────────
    // `kim stop` will open this event and call SetEvent to signal it.
    let stop_event_name: Vec<u16> =
        "Local\\kim-stop-event\0".encode_utf16().collect();
    let stop_event = unsafe {
        CreateEventW(
            None,
            true,  // manual-reset: stays signalled after WaitForSingleObject
            false, // not initially signalled
            PCWSTR(stop_event_name.as_ptr()),
        )
        .expect("kimd: failed to create stop event")
    };

    // ── Hook thread: installs keyboard/mouse/window hooks + Win32 msg loop ───
    // We need the thread ID so we can post WM_QUIT to it on shutdown.
    let (tid_tx, tid_rx) = std::sync::mpsc::channel::<u32>();
    let hook_thread = std::thread::spawn(move || {
        let tid = unsafe { GetCurrentThreadId() };
        let _ = tid_tx.send(tid);
        run_hook_thread(tx);
    });
    let hook_tid = tid_rx.recv().unwrap_or(0);

    // ── Event-processing thread: routes InputEvents for app tracking ──────────
    let event_stop = Arc::clone(&stop_flag);
    let event_thread = std::thread::spawn(move || {
        run_event_thread(rx, event_stop);
    });

    // ── UIA STA thread: TextChanged (IME chars) + FocusChanged (password) ───
    // Capture the thread ID so we can post WM_QUIT to its message loop on shutdown.
    let (uia_tid_tx, uia_tid_rx) = std::sync::mpsc::channel::<u32>();
    let uia_thread = std::thread::spawn(move || {
        let tid = unsafe { GetCurrentThreadId() };
        let _ = uia_tid_tx.send(tid);
        run_uia_thread();
    });
    let uia_tid = uia_tid_rx.recv().unwrap_or(0);

    // ── DB writer thread: 30-second flush loop ───────────────────────────────
    let writer_stop = Arc::clone(&stop_flag);
    let writer_thread = std::thread::spawn(move || {
        run_writer_thread(writer_stop);
    });

    // ── Block until `kim stop` signals the named event ───────────────────────
    unsafe {
        WaitForSingleObject(stop_event, 0xFFFF_FFFF_u32);  // INFINITE
        let _ = CloseHandle(stop_event);
    }

    log::info!("kimd: stop signal received — beginning graceful shutdown");

    // ── Signal all threads to exit ────────────────────────────────────────────
    stop_flag.store(true, Ordering::Relaxed);

    // Tell the hook thread to exit its GetMessageW loop.
    if hook_tid != 0 {
        unsafe {
            let _ = PostThreadMessageW(hook_tid, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }

    // Tell the UIA STA thread to exit its GetMessageW loop.
    if uia_tid != 0 {
        unsafe {
            let _ = PostThreadMessageW(uia_tid, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }

    // Wait for orderly shutdown (generous timeout; daemon is not interactive).
    let _ = hook_thread.join();
    let _ = event_thread.join();
    let _ = uia_thread.join();
    let _ = writer_thread.join();

    // ── Clean up PID file ────────────────────────────────────────────────────
    let _ = delete_pid_file();
    log::info!("kimd: shutdown complete");
}
