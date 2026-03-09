//! `kim` -- Key Input Monitor CLI.
//! Phase 4 (T020-T023): subcommand dispatch, daemon lifecycle management.

use std::process;

use clap::{Parser, Subcommand};

use kim::cli::{apps, autostart, history, langs, today};
use kim::db::{open_connection, schema::initialize_db};
use kim::state::{delete_pid_file, read_pid_file};

// ---------- CLI definition --------------------------------------------------

#[derive(Parser)]
#[command(name = "kim", version, about = "Key Input Monitor -- CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the background monitoring daemon
    Start,
    /// Stop the daemon gracefully (5-second timeout, then force-kill)
    Stop,
    /// Show daemon running status and uptime
    Status,
    /// Display today's input statistics
    Today {
        /// Output as JSON (Phase 9)
        #[arg(long)]
        json: bool,
    },
    /// Display statistics for a date or the last N days
    History {
        /// Target date (YYYY-MM-DD | yesterday | last-week); default: yesterday
        date: Option<String>,
        /// Show last N days (1-30)
        #[arg(long, default_value = "7")]
        days: u32,
        /// Output as JSON (Phase 9)
        #[arg(long)]
        json: bool,
    },
    /// Show per-application input statistics (Phase 7)
    Apps {
        #[arg(long)]
        date: Option<String>,
        #[arg(long, default_value = "10")]
        top: u32,
        #[arg(long)]
        json: bool,
    },
    /// Show programming-language input statistics (Phase 8)
    Langs {
        #[arg(long)]
        date: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Manage autostart on Windows login
    Autostart {
        #[command(subcommand)]
        sub: AutostartSub,
    },
}

#[derive(Subcommand)]
enum AutostartSub {
    /// Enable autostart (write HKCU Run registry key)
    Enable,
    /// Disable autostart (remove HKCU Run registry key)
    Disable,
    /// Show current autostart status
    Status,
}

// ---------- main ------------------------------------------------------------

fn main() {
    let cli = Cli::parse();
    let code = match cli.command {
        Commands::Start => cmd_start(),
        Commands::Stop => cmd_stop(),
        Commands::Status => cmd_status(),
        Commands::Today { json: _ } => with_db(today::cmd_today),
        Commands::History { date, days, json: _ } => {
            with_db(|c| history::cmd_history(c, date.as_deref(), days))
        }
        Commands::Apps { date, top, json: _ } => {
            with_db(|c| apps::cmd_apps(c, date.as_deref(), top))
        }
        Commands::Langs { date, json: _ } => {
            with_db(|c| langs::cmd_langs(c, date.as_deref()))
        }
        Commands::Autostart { sub } => match sub {
            AutostartSub::Enable => cmd_autostart_enable(),
            AutostartSub::Disable => cmd_autostart_disable(),
            AutostartSub::Status => cmd_autostart_status(),
        },
    };
    process::exit(code);
}

/// Open + init the DB, then run `f`.  Returns an exit code.
fn with_db<F: FnOnce(&rusqlite::Connection) -> i32>(f: F) -> i32 {
    let conn = match open_connection() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Database error: {}", e);
            return 2;
        }
    };
    if let Err(e) = initialize_db(&conn) {
        eprintln!("Database init error: {}", e);
        return 2;
    }
    f(&conn)
}

// ---------- Daemon lifecycle (T021-T023) ------------------------------------

/// T021 -- `kim start`
fn cmd_start() -> i32 {
    // Check if already running.
    if let Ok(Some(pid)) = read_pid_file() {
        if is_process_alive(pid) {
            println!("kim is already running (PID: {})", pid);
            return 1;
        }
        // Stale PID file -- clean it up.
        delete_pid_file().ok();
    }

    // Locate kimd.exe in the same directory as kim.exe.
    let kimd_path = match std::env::current_exe() {
        Ok(p) => p.with_file_name("kimd.exe"),
        Err(e) => {
            eprintln!("Cannot determine kimd.exe path: {}", e);
            return 2;
        }
    };
    if !kimd_path.exists() {
        eprintln!("kimd.exe not found at: {}", kimd_path.display());
        return 2;
    }

    // Spawn kimd.exe as a detached, console-less background process.
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    if let Err(e) = std::process::Command::new(&kimd_path)
        .creation_flags(DETACHED_PROCESS)
        .spawn()
    {
        eprintln!("Failed to start kimd.exe: {}", e);
        return 2;
    }

    // Wait up to 2 s for the PID file to appear.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        if let Ok(Some(_)) = read_pid_file() {
            break;
        }
        if std::time::Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    match read_pid_file() {
        Ok(Some(pid)) => {
            println!("kim started (PID: {})", pid);
            0
        }
        _ => {
            eprintln!("kimd started but PID file not created within 2 seconds.");
            2
        }
    }
}

/// T022 -- `kim stop`
fn cmd_stop() -> i32 {
    let pid = match read_pid_file() {
        Ok(Some(pid)) => pid,
        Ok(None) => {
            println!("kim is not running");
            return 1;
        }
        Err(e) => {
            eprintln!("Cannot read PID file: {}", e);
            return 1;
        }
    };

    if !is_process_alive(pid) {
        delete_pid_file().ok();
        println!("kim is not running");
        return 1;
    }

    // Signal the daemon to stop.
    signal_stop_event();

    // Wait up to 5 s for the process to exit.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let force_stopped = loop {
        if !is_process_alive(pid) {
            break false;
        }
        if std::time::Instant::now() >= deadline {
            force_terminate(pid);
            break true;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    };

    delete_pid_file().ok();

    if force_stopped {
        println!("kim force-stopped");
    } else {
        println!("kim stopped");
    }
    0
}

/// T023 -- `kim status`
fn cmd_status() -> i32 {
    let pid = match read_pid_file() {
        Ok(Some(pid)) => pid,
        _ => {
            println!("stopped");
            return 1;
        }
    };

    if !is_process_alive(pid) {
        println!("stopped");
        return 1;
    }

    let uptime = get_process_creation_secs(pid)
        .and_then(|creation| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_secs();
            Some(format_uptime(now.saturating_sub(creation)))
        })
        .unwrap_or_else(|| "unknown".to_string());

    println!("running  PID: {}  uptime: {}", pid, uptime);
    0
}

fn format_uptime(secs: u64) -> String {
    format!("{:02}:{:02}:{:02}", secs / 3600, (secs % 3600) / 60, secs % 60)
}

// ---------- Windows process helpers -----------------------------------------

fn is_process_alive(pid: u32) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    unsafe {
        match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) => {
                let mut code = 0u32;
                // STILL_ACTIVE = 259
                let alive = GetExitCodeProcess(h, &mut code).is_ok() && code == 259;
                CloseHandle(h).ok();
                alive
            }
            Err(_) => false,
        }
    }
}

/// Returns the Unix-epoch second at which `pid` was created, or `None`.
fn get_process_creation_secs(pid: u32) -> Option<u64> {
    use windows::Win32::Foundation::{CloseHandle, FILETIME};
    use windows::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let (mut ct, mut et, mut kt, mut ut) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        GetProcessTimes(h, &mut ct, &mut et, &mut kt, &mut ut).ok()?;
        CloseHandle(h).ok();

        // FILETIME = 100-ns intervals since 1601-01-01.
        let ft = ((ct.dwHighDateTime as u64) << 32) | (ct.dwLowDateTime as u64);
        // Windows-to-Unix epoch offset in 100-ns units.
        const EPOCH_DIFF: u64 = 116_444_736_000_000_000;
        if ft < EPOCH_DIFF {
            return None;
        }
        Some((ft - EPOCH_DIFF) / 10_000_000)
    }
}

/// Signal the daemon to stop via the named event `Local\kim-stop-event`.
fn signal_stop_event() {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenEventW, SetEvent, EVENT_MODIFY_STATE};
    use windows::core::PCWSTR;

    let name: Vec<u16> = "Local\\kim-stop-event\0".encode_utf16().collect();
    unsafe {
        if let Ok(h) = OpenEventW(EVENT_MODIFY_STATE, false, PCWSTR(name.as_ptr())) {
            SetEvent(h).ok();
            CloseHandle(h).ok();
        }
    }
}

/// Force-terminate a process by PID.
fn force_terminate(pid: u32) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

    unsafe {
        if let Ok(h) = OpenProcess(PROCESS_TERMINATE, false, pid) {
            TerminateProcess(h, 1).ok();
            CloseHandle(h).ok();
        }
    }
}

// ---------- Autostart subcommands -------------------------------------------

fn cmd_autostart_enable() -> i32 {
    match autostart::enable() {
        Ok(()) => {
            println!("Autostart enabled. kim will start automatically on next login.");
            0
        }
        Err(e) => {
            eprintln!("Failed to enable autostart: {}", e);
            1
        }
    }
}

fn cmd_autostart_disable() -> i32 {
    match autostart::disable() {
        Ok(()) => {
            println!("Autostart disabled.");
            0
        }
        Err(e) => {
            eprintln!("Failed to disable autostart: {}", e);
            1
        }
    }
}

fn cmd_autostart_status() -> i32 {
    match autostart::status() {
        Ok(Some(path)) => {
            println!("Autostart: enabled");
            println!("Path: {}", path);
            0
        }
        Ok(None) => {
            println!("Autostart: disabled");
            1
        }
        Err(e) => {
            eprintln!("Failed to query autostart: {}", e);
            1
        }
    }
}
