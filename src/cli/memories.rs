use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Cell, Clear, Paragraph, Row, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Table, TableState, Wrap,
    },
    Frame, Terminal,
};
use rusqlite::{params, Connection};
use std::io;

use crate::config::Config;
use crate::db;

// ── Data ────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct MemoryRow {
    id: String,
    content: String,
    memory_type: String,
    tags: Vec<String>,
    subject: Option<String>,
    source: Option<String>,
    created_at: String,
    updated_at: String,
    synced_at: Option<String>,
    #[allow(dead_code)]
    deleted: bool,
}

#[derive(PartialEq, Clone, Copy)]
enum SyncFilter {
    All,
    Synced,
    Unsynced,
    Modified,
}

impl SyncFilter {
    fn label(&self) -> &str {
        match self {
            SyncFilter::All => "All",
            SyncFilter::Synced => "Synced",
            SyncFilter::Unsynced => "Unsynced",
            SyncFilter::Modified => "Modified",
        }
    }

    fn next(&self) -> Self {
        match self {
            SyncFilter::All => SyncFilter::Synced,
            SyncFilter::Synced => SyncFilter::Unsynced,
            SyncFilter::Unsynced => SyncFilter::Modified,
            SyncFilter::Modified => SyncFilter::All,
        }
    }
}

#[derive(PartialEq)]
enum Mode {
    List,
    Detail,
    Search,
    ConfirmDelete,
}

struct App {
    memories: Vec<MemoryRow>,
    filtered: Vec<usize>, // indices into memories
    table_state: TableState,
    search: String,
    sync_filter: SyncFilter,
    mode: Mode,
    detail_scroll: u16,
    should_quit: bool,
    status_msg: Option<String>,
    total_count: usize,
    synced_count: usize,
    unsynced_count: usize,
    modified_count: usize,
}

impl App {
    fn new(memories: Vec<MemoryRow>) -> Self {
        let total_count = memories.len();
        let synced_count = memories.iter().filter(|m| {
            m.synced_at.is_some() && m.synced_at.as_deref() >= Some(m.updated_at.as_str())
        }).count();
        let unsynced_count = memories.iter().filter(|m| m.synced_at.is_none()).count();
        let modified_count = total_count - synced_count - unsynced_count;

        let filtered: Vec<usize> = (0..memories.len()).collect();
        let mut table_state = TableState::default();
        if !filtered.is_empty() {
            table_state.select(Some(0));
        }

        App {
            memories,
            filtered,
            table_state,
            search: String::new(),
            sync_filter: SyncFilter::All,
            mode: Mode::List,
            detail_scroll: 0,
            should_quit: false,
            status_msg: None,
            total_count,
            synced_count,
            unsynced_count,
            modified_count,
        }
    }

    fn apply_filters(&mut self) {
        let search_lower = self.search.to_lowercase();
        self.filtered = self.memories.iter().enumerate()
            .filter(|(_, m)| {
                // Sync filter
                let sync_ok = match self.sync_filter {
                    SyncFilter::All => true,
                    SyncFilter::Synced => {
                        m.synced_at.is_some() && m.synced_at.as_deref() >= Some(m.updated_at.as_str())
                    }
                    SyncFilter::Unsynced => m.synced_at.is_none(),
                    SyncFilter::Modified => {
                        m.synced_at.is_some() && m.synced_at.as_deref() < Some(m.updated_at.as_str())
                    }
                };
                if !sync_ok { return false; }

                // Text search
                if search_lower.is_empty() { return true; }
                m.content.to_lowercase().contains(&search_lower)
                    || m.tags.iter().any(|t| t.to_lowercase().contains(&search_lower))
                    || m.subject.as_ref().map_or(false, |s| s.to_lowercase().contains(&search_lower))
                    || m.memory_type.to_lowercase().contains(&search_lower)
                    || m.source.as_ref().map_or(false, |s| s.to_lowercase().contains(&search_lower))
            })
            .map(|(i, _)| i)
            .collect();

        // Reset selection
        if self.filtered.is_empty() {
            self.table_state.select(None);
        } else {
            self.table_state.select(Some(0));
        }
    }

    fn selected_memory(&self) -> Option<&MemoryRow> {
        self.table_state.selected()
            .and_then(|i| self.filtered.get(i))
            .map(|&idx| &self.memories[idx])
    }

    fn sync_status(m: &MemoryRow) -> (&str, Color) {
        match &m.synced_at {
            None => ("✗ unsynced", Color::Red),
            Some(sa) if sa.as_str() >= m.updated_at.as_str() => ("✓ synced", Color::Green),
            Some(_) => ("↻ modified", Color::Yellow),
        }
    }

    fn move_up(&mut self) {
        if let Some(sel) = self.table_state.selected() {
            if sel > 0 {
                self.table_state.select(Some(sel - 1));
            }
        }
    }

    fn move_down(&mut self) {
        if let Some(sel) = self.table_state.selected() {
            if sel + 1 < self.filtered.len() {
                self.table_state.select(Some(sel + 1));
            }
        }
    }
}

// ── Loading ─────────────────────────────────────────────────────────────

fn load_memories(conn: &Connection) -> Result<Vec<MemoryRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, content, type, tags, subject, source, created_at, updated_at, synced_at, deleted
         FROM memories WHERE deleted = 0
         ORDER BY created_at DESC"
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(MemoryRow {
            id: row.get(0)?,
            content: row.get(1)?,
            memory_type: row.get(2)?,
            tags: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
            subject: row.get(4)?,
            source: row.get(5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
            synced_at: row.get(8)?,
            deleted: row.get::<_, i32>(9)? != 0,
        })
    })?.collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(rows)
}

// ── Entry point ─────────────────────────────────────────────────────────

pub async fn run(_cfg: &Config) -> Result<()> {
    let conn = db::open()?;
    let memories = load_memories(&conn)?;

    if memories.is_empty() {
        println!("No memories stored yet. Use `ctxovrflw remember` to add some.");
        return Ok(());
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(memories);

    let res = run_loop(&mut terminal, &mut app, &conn);

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    res
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    conn: &Connection,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if let Event::Key(key) = event::read()? {
            match app.mode {
                Mode::List => handle_list_key(app, key, conn)?,
                Mode::Detail => handle_detail_key(app, key),
                Mode::Search => handle_search_key(app, key),
                Mode::ConfirmDelete => handle_delete_key(app, key, conn)?,
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

// ── Key Handlers ────────────────────────────────────────────────────────

fn handle_list_key(app: &mut App, key: KeyEvent, _conn: &Connection) -> Result<()> {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => app.should_quit = true,
        KeyCode::Up | KeyCode::Char('k') => app.move_up(),
        KeyCode::Down | KeyCode::Char('j') => app.move_down(),
        KeyCode::Home | KeyCode::Char('g') => {
            if !app.filtered.is_empty() { app.table_state.select(Some(0)); }
        }
        KeyCode::End | KeyCode::Char('G') => {
            if !app.filtered.is_empty() { app.table_state.select(Some(app.filtered.len() - 1)); }
        }
        KeyCode::Enter => {
            if app.selected_memory().is_some() {
                app.detail_scroll = 0;
                app.mode = Mode::Detail;
            }
        }
        KeyCode::Char('/') => {
            app.mode = Mode::Search;
            app.status_msg = Some("Type to search, Enter to confirm, Esc to cancel".into());
        }
        KeyCode::Char('s') => {
            app.sync_filter = app.sync_filter.next();
            app.apply_filters();
        }
        KeyCode::Char('d') => {
            if app.selected_memory().is_some() {
                app.mode = Mode::ConfirmDelete;
            }
        }
        _ => {}
    }
    Ok(())
}

fn handle_detail_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Backspace => app.mode = Mode::List,
        KeyCode::Up | KeyCode::Char('k') => {
            app.detail_scroll = app.detail_scroll.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.detail_scroll = app.detail_scroll.saturating_add(1);
        }
        _ => {}
    }
}

fn handle_search_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.search.clear();
            app.apply_filters();
            app.mode = Mode::List;
            app.status_msg = None;
        }
        KeyCode::Enter => {
            app.mode = Mode::List;
            app.status_msg = if app.search.is_empty() {
                None
            } else {
                Some(format!("Filter: \"{}\" ({} results)", app.search, app.filtered.len()))
            };
        }
        KeyCode::Backspace => {
            app.search.pop();
            app.apply_filters();
        }
        KeyCode::Char(c) => {
            app.search.push(c);
            app.apply_filters();
        }
        _ => {}
    }
}

fn handle_delete_key(app: &mut App, key: KeyEvent, conn: &Connection) -> Result<()> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            if let Some(mem) = app.selected_memory() {
                let id = mem.id.clone();
                db::memories::delete(conn, &id)?;
                // Remove from memories and rebuild filters
                app.memories.retain(|m| m.id != id);
                app.apply_filters();
                app.status_msg = Some(format!("Deleted memory {}", &id[..8]));
            }
            app.mode = Mode::List;
        }
        _ => {
            app.mode = Mode::List;
            app.status_msg = Some("Delete cancelled".into());
        }
    }
    Ok(())
}

// ── UI ──────────────────────────────────────────────────────────────────

fn ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // header
            Constraint::Min(5),    // table
            Constraint::Length(2), // footer / status
        ])
        .split(f.area());

    render_header(f, app, chunks[0]);
    render_table(f, app, chunks[1]);
    render_footer(f, app, chunks[2]);

    // Overlay for detail view
    if app.mode == Mode::Detail {
        let area = centered_rect(80, 80, f.area());
        f.render_widget(Clear, area);
        render_detail(f, app, area);
    }

    // Overlay for delete confirmation
    if app.mode == Mode::ConfirmDelete {
        let area = centered_rect(50, 20, f.area());
        f.render_widget(Clear, area);
        render_delete_confirm(f, app, area);
    }
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let sync_style = |filter: SyncFilter, current: SyncFilter| -> Style {
        if filter == current {
            Style::default().fg(Color::Black).bg(Color::Cyan).bold()
        } else {
            Style::default().fg(Color::DarkGray)
        }
    };

    let header = Line::from(vec![
        Span::styled(" ctxovrflw ", Style::default().fg(Color::Cyan).bold()),
        Span::raw("│ "),
        Span::styled(format!("{} memories", app.total_count), Style::default().fg(Color::White)),
        Span::raw(" │ "),
        Span::styled(format!("✓{}", app.synced_count), Style::default().fg(Color::Green)),
        Span::raw(" "),
        Span::styled(format!("✗{}", app.unsynced_count), Style::default().fg(Color::Red)),
        Span::raw(" "),
        Span::styled(format!("↻{}", app.modified_count), Style::default().fg(Color::Yellow)),
        Span::raw(" │ Filter: "),
        Span::styled(
            format!(" {} ", app.sync_filter.label()),
            sync_style(app.sync_filter, app.sync_filter),
        ),
        Span::styled(" [s] ", Style::default().fg(Color::DarkGray)),
    ]);

    let block = Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray));
    let p = Paragraph::new(header).block(block);
    f.render_widget(p, area);
}

fn render_table(f: &mut Frame, app: &mut App, area: Rect) {
    let header = Row::new(vec![
        Cell::from("ID").style(Style::default().fg(Color::Cyan).bold()),
        Cell::from("Type").style(Style::default().fg(Color::Cyan).bold()),
        Cell::from("Subject").style(Style::default().fg(Color::Cyan).bold()),
        Cell::from("Tags").style(Style::default().fg(Color::Cyan).bold()),
        Cell::from("Content").style(Style::default().fg(Color::Cyan).bold()),
        Cell::from("Sync").style(Style::default().fg(Color::Cyan).bold()),
        Cell::from("Created").style(Style::default().fg(Color::Cyan).bold()),
    ]).height(1);

    let rows: Vec<Row> = app.filtered.iter().map(|&idx| {
        let m = &app.memories[idx];
        let (sync_label, sync_color) = App::sync_status(m);

        let content_preview: String = m.content.chars().take(60).collect::<String>()
            .replace('\n', " ");
        let content_preview = if m.content.len() > 60 {
            format!("{}…", content_preview)
        } else {
            content_preview
        };

        let tags_str = if m.tags.is_empty() {
            "—".to_string()
        } else {
            m.tags.join(", ")
        };

        // Parse and format date compactly
        let created_short = &m.created_at[..10]; // YYYY-MM-DD

        Row::new(vec![
            Cell::from(m.id[..8].to_string()).style(Style::default().fg(Color::DarkGray)),
            Cell::from(m.memory_type.clone()),
            Cell::from(m.subject.clone().unwrap_or_else(|| "—".into())).style(Style::default().fg(Color::Magenta)),
            Cell::from(tags_str).style(Style::default().fg(Color::Blue)),
            Cell::from(content_preview),
            Cell::from(sync_label).style(Style::default().fg(sync_color)),
            Cell::from(created_short.to_string()).style(Style::default().fg(Color::DarkGray)),
        ])
    }).collect();

    let widths = [
        Constraint::Length(10),   // ID
        Constraint::Length(12),   // Type
        Constraint::Length(16),   // Subject
        Constraint::Length(16),   // Tags
        Constraint::Fill(1),     // Content (takes remaining)
        Constraint::Length(12),   // Sync
        Constraint::Length(12),   // Created
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)))
        .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
        .highlight_symbol("▸ ");

    f.render_stateful_widget(table, area, &mut app.table_state);

    // Scrollbar
    let total = app.filtered.len();
    if total > 0 {
        let mut scrollbar_state = ScrollbarState::new(total)
            .position(app.table_state.selected().unwrap_or(0));
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            area.inner(ratatui::layout::Margin { vertical: 1, horizontal: 0 }),
            &mut scrollbar_state,
        );
    }
}

fn render_footer(f: &mut Frame, app: &App, area: Rect) {
    let content = if app.mode == Mode::Search {
        Line::from(vec![
            Span::styled(" / ", Style::default().fg(Color::Cyan).bold()),
            Span::raw(&app.search),
            Span::styled("▌", Style::default().fg(Color::Cyan)),
        ])
    } else if let Some(msg) = &app.status_msg {
        Line::from(vec![
            Span::styled(format!(" {msg}"), Style::default().fg(Color::Yellow)),
        ])
    } else {
        Line::from(vec![
            Span::styled(" ↑↓", Style::default().fg(Color::DarkGray)),
            Span::raw(" navigate  "),
            Span::styled("Enter", Style::default().fg(Color::DarkGray)),
            Span::raw(" view  "),
            Span::styled("/", Style::default().fg(Color::DarkGray)),
            Span::raw(" search  "),
            Span::styled("s", Style::default().fg(Color::DarkGray)),
            Span::raw(" sync filter  "),
            Span::styled("d", Style::default().fg(Color::DarkGray)),
            Span::raw(" delete  "),
            Span::styled("q", Style::default().fg(Color::DarkGray)),
            Span::raw(" quit"),
        ])
    };

    f.render_widget(Paragraph::new(content), area);
}

fn render_detail(f: &mut Frame, app: &App, area: Rect) {
    let mem = match app.selected_memory() {
        Some(m) => m,
        None => return,
    };

    let (sync_label, _) = App::sync_status(mem);

    let mut lines = vec![
        Line::from(vec![
            Span::styled("ID:       ", Style::default().fg(Color::Cyan).bold()),
            Span::raw(&mem.id),
        ]),
        Line::from(vec![
            Span::styled("Type:     ", Style::default().fg(Color::Cyan).bold()),
            Span::raw(&mem.memory_type),
        ]),
        Line::from(vec![
            Span::styled("Subject:  ", Style::default().fg(Color::Cyan).bold()),
            Span::raw(mem.subject.as_deref().unwrap_or("—")),
        ]),
        Line::from(vec![
            Span::styled("Source:   ", Style::default().fg(Color::Cyan).bold()),
            Span::raw(mem.source.as_deref().unwrap_or("—")),
        ]),
        Line::from(vec![
            Span::styled("Tags:     ", Style::default().fg(Color::Cyan).bold()),
            Span::raw(if mem.tags.is_empty() { "—".to_string() } else { mem.tags.join(", ") }),
        ]),
        Line::from(vec![
            Span::styled("Sync:     ", Style::default().fg(Color::Cyan).bold()),
            Span::raw(sync_label),
        ]),
        Line::from(vec![
            Span::styled("Created:  ", Style::default().fg(Color::Cyan).bold()),
            Span::raw(&mem.created_at),
        ]),
        Line::from(vec![
            Span::styled("Updated:  ", Style::default().fg(Color::Cyan).bold()),
            Span::raw(&mem.updated_at),
        ]),
        Line::from(""),
        Line::from(Span::styled("── Content ──", Style::default().fg(Color::Cyan).bold())),
        Line::from(""),
    ];

    // Add content lines
    for line in mem.content.lines() {
        lines.push(Line::from(line.to_string()));
    }

    let block = Block::default()
        .title(" Memory Detail ")
        .title_style(Style::default().fg(Color::Cyan).bold())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let p = Paragraph::new(Text::from(lines))
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll, 0));

    f.render_widget(p, area);
}

fn render_delete_confirm(f: &mut Frame, app: &App, area: Rect) {
    let id_short = app.selected_memory()
        .map(|m| m.id[..8].to_string())
        .unwrap_or_default();

    let text = Text::from(vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("  Delete memory "),
            Span::styled(&id_short, Style::default().fg(Color::Red).bold()),
            Span::raw("?"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  y", Style::default().fg(Color::Red).bold()),
            Span::raw(" confirm  "),
            Span::styled("any key", Style::default().fg(Color::DarkGray)),
            Span::raw(" cancel"),
        ]),
    ]);

    let block = Block::default()
        .title(" Confirm Delete ")
        .title_style(Style::default().fg(Color::Red).bold())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red));

    f.render_widget(Paragraph::new(text).block(block), area);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
