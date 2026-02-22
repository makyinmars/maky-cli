pub mod controller;
pub mod event;
pub mod markdown;
pub mod state;
pub mod ui;

use std::{
    io,
    panic::{self, PanicHookInfo},
    sync::Arc,
};

use anyhow::{Context, Result};
use crossterm::{
    cursor::{Hide, Show},
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use self::controller::AppController;

type SharedPanicHook = Arc<dyn for<'a> Fn(&PanicHookInfo<'a>) + Send + Sync + 'static>;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StartupOptions {
    pub resume_session_id: Option<String>,
    pub force_new_session: bool,
}

pub fn run(startup: StartupOptions) -> Result<()> {
    let _panic_hook_guard = PanicHookGuard::install();
    let _terminal_cleanup_guard = TerminalCleanupGuard::activate()?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend).context("failed to create terminal backend")?;
    terminal.clear().context("failed to clear terminal")?;

    let mut controller =
        AppController::new(startup).context("failed to initialize app controller")?;
    let run_result = controller.run(&mut terminal);

    let _ = terminal.show_cursor();
    run_result
}

struct TerminalCleanupGuard;

impl TerminalCleanupGuard {
    fn activate() -> Result<Self> {
        enable_raw_mode().context("failed to enable raw mode")?;
        if let Err(err) = execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture, Hide) {
            let _ = disable_raw_mode();
            return Err(err).context("failed to enter alternate screen");
        }
        Ok(Self)
    }
}

impl Drop for TerminalCleanupGuard {
    fn drop(&mut self) {
        let _ = restore_terminal();
    }
}

struct PanicHookGuard {
    previous_hook: SharedPanicHook,
}

impl PanicHookGuard {
    fn install() -> Self {
        let previous_hook: SharedPanicHook = Arc::from(panic::take_hook());
        let hook_for_panic = Arc::clone(&previous_hook);

        panic::set_hook(Box::new(move |panic_info| {
            let _ = restore_terminal();
            hook_for_panic(panic_info);
        }));

        Self { previous_hook }
    }
}

impl Drop for PanicHookGuard {
    fn drop(&mut self) {
        // Rust does not allow panic hook mutation from a panicking thread.
        // During unwinding, we leave the current hook in place and prioritize
        // finishing teardown without triggering a secondary panic.
        if std::thread::panicking() {
            return;
        }

        let _ = panic::take_hook();
        let hook_to_restore = Arc::clone(&self.previous_hook);
        panic::set_hook(Box::new(move |panic_info| {
            hook_to_restore(panic_info);
        }));
    }
}

fn restore_terminal() -> io::Result<()> {
    let raw_mode_result = disable_raw_mode();
    let screen_result = execute!(
        io::stdout(),
        Show,
        DisableMouseCapture,
        LeaveAlternateScreen
    );

    match (raw_mode_result, screen_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(raw_mode_error), Ok(())) => Err(raw_mode_error),
        (Ok(()), Err(screen_error)) => Err(screen_error),
        (Err(_), Err(screen_error)) => Err(screen_error),
    }
}
