use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Gauge, Paragraph, Row, Sparkline, Table, TableState,
    },
    Frame,
};

use crate::app::App;
use crate::data::ChannelSnapshot;

// ── colour helpers ────────────────────────────────────────────────────────────

fn fill_color(pct: f64) -> Color {
    if pct >= 90.0 {
        Color::Red
    } else if pct >= 70.0 {
        Color::Yellow
    } else {
        Color::Green
    }
}

fn fmt_bytes(b: u64) -> String {
    if b >= 1 << 30 {
        format!("{:.2} GB", b as f64 / (1u64 << 30) as f64)
    } else if b >= 1 << 20 {
        format!("{:.2} MB", b as f64 / (1u64 << 20) as f64)
    } else if b >= 1 << 10 {
        format!("{:.2} KB", b as f64 / (1u64 << 10) as f64)
    } else {
        format!("{} B", b)
    }
}

// ── top-level render ──────────────────────────────────────────────────────────

pub fn render(f: &mut Frame, app: &App) {
    let show_graph = app.show_graph && !app.snapshot.channels.is_empty();

    // Vertical split: channel table | (optional graph) | sfb panel | help bar
    let constraints = if show_graph {
        vec![
            Constraint::Fill(1),    // channel table
            Constraint::Length(10), // sparkline graph
            Constraint::Length(9),  // sfb panel
            Constraint::Length(1),  // help bar
        ]
    } else {
        vec![
            Constraint::Fill(1),
            Constraint::Length(9),
            Constraint::Length(1),
        ]
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(f.area());

    let (graph_chunk, sfb_chunk, help_chunk) = if show_graph {
        (Some(chunks[1]), chunks[2], chunks[3])
    } else {
        (None, chunks[1], chunks[2])
    };

    render_channel_table(f, app, chunks[0]);

    if let Some(gc) = graph_chunk {
        render_graph(f, app, gc);
    }

    render_sfb(f, app, sfb_chunk);
    render_help(f, app, help_chunk);
}

// ── channel table ─────────────────────────────────────────────────────────────

fn render_channel_table(f: &mut Frame, app: &App, area: Rect) {
    let header_cells = ["ID", "Capacity", "Fill", "Fill %", "Msgs/s", "Overflow"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().add_modifier(Modifier::BOLD)));
    let header = Row::new(header_cells)
        .style(Style::default().fg(Color::Cyan))
        .height(1)
        .bottom_margin(1);

    let rows: Vec<Row> = app
        .snapshot
        .channels
        .iter()
        .enumerate()
        .map(|(i, ch)| build_channel_row(ch, i == app.selected))
        .collect();

    let title = format!(
        " Channels ({}) — SHM used: {} / {} ",
        app.snapshot.channels.len(),
        fmt_bytes(app.snapshot.shm_used_bytes as u64),
        fmt_bytes(app.snapshot.shm_total_bytes as u64),
    );

    let table = Table::new(
        rows,
        [
            Constraint::Length(6),   // ID
            Constraint::Length(10),  // Capacity
            Constraint::Fill(1),     // Fill (progress bar)
            Constraint::Length(8),   // Fill %
            Constraint::Length(12),  // Msgs/s
            Constraint::Length(10),  // Overflow
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(title))
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    let mut state = TableState::default().with_selected(Some(app.selected));
    f.render_stateful_widget(table, area, &mut state);
}

fn build_channel_row(ch: &ChannelSnapshot, selected: bool) -> Row<'static> {
    let color = fill_color(ch.fill_pct);
    let base = if selected {
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };

    // Inline ASCII progress bar (20 chars wide)
    let bar_width = 20usize;
    let filled = ((ch.fill_pct / 100.0) * bar_width as f64).round() as usize;
    let filled = filled.min(bar_width);
    let bar = format!(
        "[{}{}]",
        "#".repeat(filled),
        " ".repeat(bar_width - filled)
    );

    let overflow_str = if ch.has_overflow { "YES" } else { "no" };
    let overflow_style = if ch.has_overflow {
        Style::default().fg(Color::Magenta)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    Row::new(vec![
        Cell::from(format!("{}", ch.id)).style(base),
        Cell::from(format!("{}", ch.capacity)).style(base),
        Cell::from(bar).style(Style::default().fg(color)),
        Cell::from(format!("{:.1}%", ch.fill_pct))
            .style(Style::default().fg(color)),
        Cell::from(format!("{:.0}", ch.msgs_per_sec)).style(base),
        Cell::from(overflow_str).style(overflow_style),
    ])
}

// ── sparkline graph ───────────────────────────────────────────────────────────

fn render_graph(f: &mut Frame, app: &App, area: Rect) {
    let idx = app.selected;
    let history = app
        .snapshot
        .throughput_history
        .get(idx)
        .cloned()
        .unwrap_or_default();

    let data: Vec<u64> = history
        .iter()
        .map(|v| v.round().max(0.0) as u64)
        .collect();

    let ch = app.snapshot.channels.get(idx);
    let title = ch
        .map(|c| format!(" Channel {} — throughput (msgs/s) ", c.id))
        .unwrap_or_else(|| " Throughput ".to_string());

    let sparkline = Sparkline::default()
        .block(Block::default().borders(Borders::ALL).title(title))
        .data(&data)
        .style(Style::default().fg(Color::Cyan));

    f.render_widget(sparkline, area);
}

// ── sfb panel ────────────────────────────────────────────────────────────────

fn render_sfb(f: &mut Frame, app: &App, area: Rect) {
    let sfb = &app.snapshot.sfb;

    // Split into two columns: stats text | capacity gauge
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Fill(1), Constraint::Length(40)])
        .split(area);

    // Left: text stats
    let cap_str = fmt_bytes(sfb.active_capacity_bytes);
    let data_str = fmt_bytes(sfb.active_data_bytes);
    let free_str = fmt_bytes(sfb.free_space_bytes);
    let frag_pct = sfb.fragmentation_ratio * 100.0;

    let lines = vec![
        Line::from(vec![
            Span::styled("  Active pages : ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}", sfb.active_pages),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Capacity     : ", Style::default().fg(Color::DarkGray)),
            Span::styled(cap_str, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  Used data    : ", Style::default().fg(Color::DarkGray)),
            Span::styled(data_str, Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            Span::styled("  Free space   : ", Style::default().fg(Color::DarkGray)),
            Span::styled(free_str, Style::default().fg(Color::Green)),
        ]),
        Line::from(vec![
            Span::styled("  Fragmentation: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{:.1}%", frag_pct),
                Style::default().fg(fill_color(frag_pct)),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Appends/s    : ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{:.0}", sfb.appends_per_sec),
                Style::default().fg(Color::Cyan),
            ),
        ]),
    ];

    let para = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" SFB — Stable Fragmented Buffer "),
    );
    f.render_widget(para, cols[0]);

    // Right: capacity gauge
    let used_pct = if sfb.active_capacity_bytes > 0 {
        (sfb.active_data_bytes as f64 / sfb.active_capacity_bytes as f64).min(1.0)
    } else {
        0.0
    };

    let gauge_color = fill_color(used_pct * 100.0);
    let label = format!(
        "{} / {}",
        fmt_bytes(sfb.active_data_bytes),
        fmt_bytes(sfb.active_capacity_bytes)
    );

    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" SFB capacity "),
        )
        .gauge_style(Style::default().fg(gauge_color))
        .ratio(used_pct)
        .label(label);

    f.render_widget(gauge, cols[1]);
}

// ── help bar ──────────────────────────────────────────────────────────────────

fn render_help(f: &mut Frame, app: &App, area: Rect) {
    let paused_indicator = if app.paused {
        Span::styled("  PAUSED  ", Style::default().fg(Color::Black).bg(Color::Yellow))
    } else {
        Span::styled("          ", Style::default())
    };

    let line = Line::from(vec![
        paused_indicator,
        Span::styled(
            "  ↑/↓ select   p pause   g graph   q quit",
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    let para = Paragraph::new(line);
    f.render_widget(para, area);
}
