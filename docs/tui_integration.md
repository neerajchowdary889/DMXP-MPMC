# Adding TUI to DMXP-MPMC

This guide explains how to add an interactive real-time TUI (Terminal User Interface) to DMXP-MPMC for monitoring channel statistics, memory usage, and performance metrics.

## Overview

The TUI module provides real-time visualization of:
- Channel statistics and availability
- Memory consumption and usage rates
- SFU (Stable Fragmented Buffer) space allocation
- Producer/consumer throughput graphs
- Real-time data flow monitoring

## Architecture

### Module Structure
```
src/
├── main.rs              # Entry point (can remain unchanged)
├── lib.rs               # Core library (no modifications needed)
├── tui/                 # New TUI module
│   ├── mod.rs           # TUI module entry point
│   ├── app.rs           # Main application state and logic
│   ├── ui.rs            # UI layout and rendering
│   ├── events.rs        # Event handling
│   ├── data_collector.rs # Data collection from existing modules
│   └── widgets/        # Custom UI components
│       ├── mod.rs
│       ├── graphs.rs    # Real-time graphs
│       ├── stats.rs     # Statistics panels
│       └── charts.rs    # Charts and visualizations
├── MPMC/               # Existing (unchanged)
├── Core/               # Existing (unchanged)
└── Debug/              # Existing (unchanged)
```

### Design Principles

1. **Zero Core Modifications**: TUI reads data without changing existing modules
2. **Non-Intrusive**: TUI can be removed without affecting functionality
3. **Performance First**: Minimal impact on MPMC operations
4. **Modular**: Clean separation of concerns

## Implementation Guide

### Step 1: Project Setup

#### 1.1 Update Cargo.toml
```toml
[dependencies]
# ... existing dependencies ...

# TUI dependencies
ratatui = "0.24"
crossterm = "0.27"
chrono = "0.4"

# Optional: For better graphs
tui-widgets = "0.2"
```

#### 1.2 Create TUI Module Structure
```bash
mkdir -p src/tui/widgets
touch src/tui/{mod.rs,app.rs,ui.rs,events.rs,data_collector.rs}
touch src/tui/widgets/{mod.rs,graphs.rs,stats.rs,charts.rs}
```

### Step 2: Data Collection Layer

#### 2.1 Data Collector (`src/tui/data_collector.rs`)
The data collector interfaces with existing modules without modifying them:

```rust
// Collect data from existing modules
pub struct DataCollector {
    // Channels for real-time data
    stats_channel: Receiver<ChannelStats>,
    memory_channel: Receiver<MemoryStats>,
    sfu_channel: Receiver<SfuStats>,
}

impl DataCollector {
    pub fn new() -> Self {
        // Initialize collectors that read from existing modules
    }
    
    pub fn collect_channel_stats(&self) -> ChannelStats {
        // Read from MPMC producer/consumer
    }
    
    pub fn collect_memory_stats(&self) -> MemoryStats {
        // Read from Core/SharedMemory
    }
    
    pub fn collect_sfu_stats(&self) -> SfuStats {
        // Read from Core/sfu
    }
}
```

#### 2.2 Data Structures
Define structures to hold collected data:

```rust
#[derive(Debug, Clone)]
pub struct ChannelStats {
    pub channel_id: u32,
    pub producer_count: u32,
    pub consumer_count: u32,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub buffer_utilization: f32,
    pub throughput: f64, // messages per second
}

#[derive(Debug, Clone)]
pub struct MemoryStats {
    pub shared_memory_size: usize,
    pub shared_memory_used: usize,
    pub buffer_memory: usize,
    pub sfu_memory: usize,
}

#[derive(Debug, Clone)]
pub struct SfuStats {
    pub allocated_space: usize,
    pub used_space: usize,
    pub fragment_count: u32,
    pub allocation_rate: f64,
}
```

### Step 3: Application State (`src/tui/app.rs`)

```rust
pub struct App {
    pub data_collector: DataCollector,
    pub should_quit: bool,
    pub selected_channel: Option<u32>,
    pub show_graphs: bool,
    pub update_interval: Duration,
    
    // Data storage
    pub channel_stats: Vec<ChannelStats>,
    pub memory_stats: MemoryStats,
    pub sfu_stats: SfuStats,
    
    // Historical data for graphs
    pub throughput_history: VecDeque<f64>,
    pub memory_history: VecDeque<usize>,
}

impl App {
    pub fn new() -> Result<Self> {
        Ok(Self {
            data_collector: DataCollector::new()?,
            should_quit: false,
            selected_channel: None,
            show_graphs: true,
            update_interval: Duration::from_millis(100),
            // ... initialize other fields
        })
    }
    
    pub fn update(&mut self) -> Result<()> {
        // Collect latest data from all modules
        self.channel_stats = self.data_collector.collect_channel_stats();
        self.memory_stats = self.data_collector.collect_memory_stats();
        self.sfu_stats = self.data_collector.collect_sfu_stats();
        
        // Update historical data
        self.update_history();
        
        Ok(())
    }
    
    pub fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('g') => self.show_graphs = !self.show_graphs,
            KeyCode::Up => self.select_previous_channel(),
            KeyCode::Down => self.select_next_channel(),
            _ => {}
        }
    }
}
```

### Step 4: UI Layout (`src/tui/ui.rs`)

```rust
pub fn render_ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(30), // Top panel - Channel stats
            Constraint::Percentage(40), // Middle panel - Graphs
            Constraint::Percentage(30), // Bottom panel - Memory & SFU
        ])
        .split(f.area());
    
    // Render channel statistics
    render_channel_stats(f, chunks[0], app);
    
    // Render graphs
    if app.show_graphs {
        render_graphs(f, chunks[1], app);
    }
    
    // Render memory and SFU info
    render_system_info(f, chunks[2], app);
}
```

### Step 5: Event Handling (`src/tui/events.rs`)

```rust
pub struct EventHandler {
    pub rx: UnboundedReceiver<Event>,
}

#[derive(Debug)]
pub enum Event {
    Key(KeyEvent),
    Tick,
    Update,
}

impl EventHandler {
    pub fn new(tick_rate: Duration) -> (Self, UnboundedSender<Event>) {
        let (tx, rx) = unbounded_channel();
        let event_tx = tx.clone();
        
        // Keyboard events
        let _thread = thread::spawn(move || {
            loop {
                if crossterm::event::poll(Duration::from_millis(100)).unwrap() {
                    if let Ok(event) = crossterm::event::read() {
                        let _ = event_tx.send(Event::Key(event));
                    }
                }
            }
        });
        
        // Timer events for updates
        let _thread = thread::spawn(move || {
            loop {
                thread::sleep(tick_rate);
                let _ = event_tx.send(Event::Update);
            }
        });
        
        (Self { rx }, tx)
    }
}
```

### Step 6: Main TUI Entry (`src/tui/mod.rs`)

```rust
use std::io;
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};

mod app;
mod ui;
mod events;
mod data_collector;
mod widgets;

pub use app::App;
use events::EventHandler;

pub fn run_tui() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize terminal
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    
    // Create app and event handler
    let mut app = App::new()?;
    let (mut events, _) = EventHandler::new(Duration::from_millis(100));
    
    // Main loop
    loop {
        // Handle events
        match events.rx.recv()? {
            Event::Key(key) => {
                app.handle_key(key);
                if app.should_quit {
                    break;
                }
            }
            Event::Update => {
                app.update()?;
            }
            Event::Tick => {
                // Redraw UI
                terminal.draw(|f| ui::render_ui(f, &app))?;
            }
        }
        
        // Draw UI
        terminal.draw(|f| ui::render_ui(f, &app))?;
    }
    
    // Restore terminal
    terminal.show_cursor()?;
    terminal.clear()?;
    
    Ok(())
}
```

### Step 7: Integration

#### 7.1 Update Main Entry Point
Optionally update `main.rs` to include TUI option:

```rust
use clap::{Arg, Command};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = Command::new("DMXP-MPMC")
        .arg(Arg::new("tui")
            .short('t')
            .long("tui")
            .help("Launch TUI monitor"))
        .get_matches();
    
    if matches.get_flag("tui") {
        // Launch TUI
        dmxp_mpmc::tui::run_tui()?;
    } else {
        // Existing functionality
        println!("DMXP-MPMC running without TUI");
    }
    
    Ok(())
}
```

#### 7.2 Update lib.rs
```rust
// ... existing modules ...

#[cfg(feature = "tui")]
pub mod tui;
```

## Features

### Real-Time Monitoring

1. **Channel Statistics**
   - Active channels count
   - Producer/consumer counts per channel
   - Message throughput rates
   - Buffer utilization percentages

2. **Memory Usage**
   - Shared memory consumption
   - Buffer memory usage
   - SFU allocated space
   - Memory allocation rates

3. **Performance Metrics**
   - Messages per second graphs
   - Latency measurements
   - Error rates
   - Queue depths

### Interactive Controls

- **Arrow Keys**: Navigate between channels
- **G**: Toggle graph display
- **R**: Reset statistics
- **P**: Pause/Resume updates
- **Q**: Quit TUI

### Visualizations

1. **Real-Time Graphs**
   - Throughput over time
   - Memory usage trends
   - Channel activity

2. **Progress Bars**
   - Buffer utilization
   - Memory consumption
   - SFU space usage

3. **Status Indicators**
   - Channel health
   - Connection status
   - Error conditions

## Performance Considerations

### Data Collection Optimization

1. **Non-Blocking Collection**: Use channels to avoid blocking MPMC operations
2. **Sampling Rates**: Configurable update intervals (default 100ms)
3. **Memory Efficiency**: Circular buffers for historical data

### UI Performance

1. **Partial Updates**: Only update changed UI elements
2. **Frame Rate Limiting**: Cap UI refresh rate to prevent CPU usage
3. **Efficient Rendering**: Use ratatui's optimized drawing

## Configuration

### TUI Configuration File (`tui_config.toml`)
```toml
[general]
update_interval_ms = 100
max_history_points = 1000

[display]
show_graphs = true
refresh_rate = 30
theme = "dark"

[thresholds]
memory_warning = 80.0
memory_critical = 95.0
throughput_warning = 1000.0
```

## Testing

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_collection() {
        let collector = DataCollector::new().unwrap();
        let stats = collector.collect_channel_stats();
        assert!(!stats.is_empty());
    }
}
```

### Integration Tests
```rust
#[test]
fn test_tui_with_mpmc() {
    // Test TUI with actual MPMC channels
    // Verify no performance impact
}
```

## Deployment

### Building with TUI
```bash
# Build with TUI feature
cargo build --features tui

# Run with TUI
cargo run --features tui -- --tui
```

### Optional TUI
The TUI is completely optional:
```bash
# Build without TUI (smaller binary)
cargo build --no-default-features

# Run normally
cargo run
```

## Troubleshooting

### Common Issues

1. **Terminal Compatibility**: Ensure terminal supports ANSI codes
2. **Performance**: If TUI impacts performance, increase update intervals
3. **Memory Usage**: Limit history points for long-running sessions

### Debug Mode
Enable debug logging:
```bash
RUST_LOG=debug cargo run --features tui -- --tui
```

## Future Enhancements

1. **Multiple Windows**: Support for multiple TUI windows
2. **Remote Monitoring**: Network-based monitoring capabilities
3. **Alerts**: Configurable thresholds and notifications
4. **Export**: Save statistics to files
5. **Configuration**: Runtime configuration changes

## Conclusion

This TUI implementation provides comprehensive monitoring capabilities without modifying the core DMXP-MPMC functionality. The modular design ensures that the TUI can be easily removed or extended based on requirements.
