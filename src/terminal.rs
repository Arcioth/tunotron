use std::io::{self, stdout, Stdout, Write};
use std::panic::{self, PanicHookInfo};
use std::path::PathBuf;
use crossterm::{
    cursor::{Hide, Show},
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
    },
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

/// RAII Terminal Guard: Guarantees that the terminal is restored to normal mode
/// even if the app crashes, panics, or returns an early error.
pub struct TerminalHarness {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalHarness {
    pub fn init(crash_log_path: PathBuf) -> io::Result<Self> {
        // 1. Install panic hook FIRST before modifying terminal state
        Self::install_panic_hook(crash_log_path);

        // 2. Configure terminal: Raw Mode, Alternate Screen, Mouse, Hide Cursor
        enable_raw_mode()?;
        let mut out = stdout();
        execute!(out, EnterAlternateScreen, EnableMouseCapture, Hide)?;
        let terminal = Terminal::new(CrosstermBackend::new(out))?;

        Ok(Self { terminal })
    }

    pub fn terminal_mut(&mut self) -> &mut Terminal<CrosstermBackend<Stdout>> {
        &mut self.terminal
    }

    /// Restore terminal to original state
    pub fn restore_terminal() -> io::Result<()> {
        disable_raw_mode()?;
        execute!(stdout(), LeaveAlternateScreen, DisableMouseCapture, Show)?;
        stdout().flush()?;
        Ok(())
    }

    fn install_panic_hook(log_path: PathBuf) {
        let original_hook = panic::take_hook();

        panic::set_hook(Box::new(move |panic_info: &PanicHookInfo| {
            // STEP 1: Immediately restore the terminal so the user's shell isn't corrupted
            let _ = Self::restore_terminal();

            // STEP 2: Dump backtrace to a persistent crash log file
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
            {
                let backtrace = std::backtrace::Backtrace::capture();
                let _ = writeln!(
                    file,
                    "--- TUNOTRON CRASH [{}] ---\n{}\nBacktrace:\n{}\n",
                    chrono::Utc::now().to_rfc3339(),
                    panic_info,
                    backtrace
                );
            }

            // STEP 3: Delegate to the standard hook to print cleanly to stderr
            original_hook(panic_info);
        }));
    }
}

impl Drop for TerminalHarness {
    fn drop(&mut self) {
        let _ = Self::restore_terminal();
    }
}
