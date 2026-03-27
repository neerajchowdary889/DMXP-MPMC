mod app;
mod data;
mod ui;

use std::io;
use std::sync::Arc;
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use app::App;
use data::Poller;
use dmxp_mpmc::Core::alloc::SharedMemoryAllocator;

/// Must match the size used by the DMXP-MPMC process exactly.
const SHM_SIZE: usize = 64 * 1024 * 1024;

/// UI refresh interval.
const TICK_MS: u64 = 100;

fn main() -> io::Result<()> {
    // The TUI is read-only — it only attaches, never creates.
    // Retry every second until a DMXP process has created the shared memory.
    let allocator: Arc<SharedMemoryAllocator> = loop {
        match SharedMemoryAllocator::attach(SHM_SIZE) {
            Ok(a) => break Arc::new(a),
            Err(_) => {
                eprint!("\rWaiting for DMXP-MPMC to start...");
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    };

    // Start background poller (100 ms interval).
    let poller = Poller::new(allocator, Duration::from_millis(TICK_MS));

    // Set up terminal.
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Build initial app state.
    let mut app = App::new(poller.latest());

    // Event loop.
    let tick = Duration::from_millis(TICK_MS);
    loop {
        // Draw.
        terminal.draw(|f| ui::render(f, &app))?;

        // Poll for keyboard input, non-blocking.
        if event::poll(tick)? {
            if let Event::Key(key) = event::read()? {
                // Only react to key-press events (ignore repeat/release on Windows).
                if key.kind == KeyEventKind::Press {
                    app.handle_key(key.code);
                }
            }
        }

        if app.should_quit {
            break;
        }

        // Pull latest snapshot from poller.
        app.update(poller.latest());
    }

    // Restore terminal.
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
