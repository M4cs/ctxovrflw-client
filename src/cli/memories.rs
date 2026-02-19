use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
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
use std::collections::HashSet;
use std::io;

use crate::config::Config;
use crate::db;
#[cfg(feature = "pro")]
use crate::db::graph;

// ── Data ────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct MemoryRow {
    id: String,
    content: String,
    memory_type: String,
    tags: Vec<String>,
    subject: Option<String>,
    source: Option<String>,
    agent_id: Option<String>,
    expires_at: Option<String>,
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
    Syncing,
    Graph,
}

struct App {
    memories: Vec<MemoryRow>,
    filtered: Vec<usize>, // indices into memories
    selected: HashSet<String>, // selected memory IDs for bulk ops
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
    graph_entity_name: String,
    graph_entity_type: String,
    graph_relations: Vec<(String, String, String, String, f64, bool)>,
    graph_selected: usize,
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
            selected: HashSet::new(),
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
            graph_entity_name: String::new(),
            graph_entity_type: String::new(),
            graph_relations: Vec::new(),
            graph_selected: 0,
        }
    }

    fn recalc_counts(&mut self) {
        self.total_count = self.memories.len();
        self.synced_count = self.memories.iter().filter(|m| {
            m.synced_at.is_some() && m.synced_at.as_deref() >= Some(m.updated_at.as_str())
        }).count();
        self.unsynced_count = self.memories.iter().filter(|m| m.synced_at.is_none()).count();
        self.modified_count = self.total_count - self.synced_count - self.unsynced_count;
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
                    || m.agent_id.as_ref().map_or(false, |s| s.to_lowercase().contains(&search_lower))
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

    fn toggle_select_current(&mut self) {
        if let Some(mem) = self.selected_memory() {
            let id = mem.id.clone();
            if self.selected.contains(&id) {
                self.selected.remove(&id);
            } else {
                self.selected.insert(id);
            }
            // Move cursor down after toggle
            self.move_down();
        }
    }

    fn select_all_visible(&mut self) {
        for &idx in &self.filtered {
            self.selected.insert(self.memories[idx].id.clone());
        }
    }

    fn deselect_all(&mut self) {
        self.selected.clear();
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
        "SELECT id, content, type, tags, subject, source, agent_id, expires_at, created_at, updated_at, synced_at, deleted
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
            agent_id: row.get(6)?,
            expires_at: row.get(7)?,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
            synced_at: row.get(10)?,
            deleted: row.get::<_, i32>(11)? != 0,
        })
    })?.collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(rows)
}

// ── Entry point ─────────────────────────────────────────────────────────

pub async fn run(cfg: &Config) -> Result<()> {
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

    let res = run_loop(&mut terminal, &mut app, &conn, cfg);

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
    cfg: &Config,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if let Event::Key(key) = event::read()? {
            // On Windows, crossterm fires Press + Release events.
            // Only handle Press to avoid double-processing.
            if key.kind == KeyEventKind::Press {
                match app.mode {
                    Mode::List => handle_list_key(app, key, conn, cfg)?,
                    Mode::Detail => handle_detail_key(app, key),
                    Mode::Search => handle_search_key(app, key),
                    Mode::ConfirmDelete => handle_delete_key(app, key, conn)?,
                    Mode::Graph => handle_graph_key(app, key),
                    Mode::Syncing => {} // non-interactive, will transition back
                }
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

// ── Key Handlers ────────────────────────────────────────────────────────

fn handle_list_key(app: &mut App, key: KeyEvent, conn: &Connection, cfg: &Config) -> Result<()> {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => app.should_quit = true,
        KeyCode::Up | KeyCode::Char('k') => app.move_up(),
        KeyCode::Down | KeyCode::Char('j') => app.move_down(),
        KeyCode::Home => {
            if !app.filtered.is_empty() { app.table_state.select(Some(0)); }
        }
        KeyCode::Char('g') => {
            #[cfg(feature = "pro")]
            {
                if let Some(mem) = app.selected_memory() {
                    if let Some(subject) = mem.subject.clone() {
                        match graph::find_entity(conn, &subject, None) {
                            Ok(entities) if !entities.is_empty() => {
                                let entity = &entities[0];
                                app.graph_entity_name = entity.name.clone();
                                app.graph_entity_type = entity.entity_type.clone();
                                app.graph_relations.clear();
                                app.graph_selected = 0;
                                if let Ok(rels) = graph::get_relations(conn, &entity.id, None, None) {
                                    for (rel, source, target) in &rels {
                                        let is_outgoing = rel.source_id == entity.id;
                                        if is_outgoing {
                                            app.graph_relations.push((
                                                source.name.clone(), source.entity_type.clone(),
                                                rel.relation_type.clone(), target.name.clone(),
                                                rel.confidence, true,
                                            ));
                                        } else {
                                            app.graph_relations.push((
                                                source.name.clone(), source.entity_type.clone(),
                                                rel.relation_type.clone(), target.name.clone(),
                                                rel.confidence, false,
                                            ));
                                        }
                                    }
                                }
                                app.mode = Mode::Graph;
                            }
                            _ => { app.status_msg = Some("No graph data for this memory".into()); }
                        }
                    } else {
                        app.status_msg = Some("No graph data for this memory".into());
                    }
                }
            }
            #[cfg(not(feature = "pro"))]
            {
                app.status_msg = Some("Graph view requires Pro tier".into());
            }
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
        KeyCode::Char(' ') => {
            app.toggle_select_current();
        }
        KeyCode::Char('a') => {
            app.select_all_visible();
            app.status_msg = Some(format!("Selected {} memories", app.selected.len()));
        }
        KeyCode::Char('A') => {
            app.deselect_all();
            app.status_msg = Some("Deselected all".into());
        }
        KeyCode::Char('/') => {
            app.mode = Mode::Search;
            app.status_msg = Some("Type to search, Enter to confirm, Esc to cancel".into());
        }
        KeyCode::Char('s') if !key.modifiers.contains(KeyModifiers::SHIFT) => {
            app.sync_filter = app.sync_filter.next();
            app.apply_filters();
        }
        KeyCode::Char('S') => {
            // Trigger sync
            app.status_msg = Some("Syncing...".into());
            app.mode = Mode::Syncing;

            // We need to temporarily leave raw mode to run sync
            let _ = disable_raw_mode();
            let rt = tokio::runtime::Handle::current();
            let sync_result = rt.block_on(crate::sync::run_silent(cfg));
            let _ = enable_raw_mode();

            match sync_result {
                Ok((pushed, pulled, pull_purged)) => {
                    // Reload memories from DB to reflect sync changes
                    if let Ok(fresh) = load_memories(conn) {
                        app.memories = fresh;
                        app.recalc_counts();
                        app.apply_filters();
                    }
                    if pull_purged > 0 {
                        app.status_msg = Some(format!("Sync complete — pushed {pushed}, pulled {pulled}, purged {pull_purged}"));
                    } else {
                        app.status_msg = Some(format!("Sync complete — pushed {pushed}, pulled {pulled}"));
                    }
                }
                Err(e) => {
                    app.status_msg = Some(format!("Sync failed: {e}"));
                }
            }
            app.mode = Mode::List;
        }
        KeyCode::Char('d') => {
            if !app.selected.is_empty() || app.selected_memory().is_some() {
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
        KeyCode::Char(' ') => {
            // Toggle selection from detail view
            if let Some(mem) = app.selected_memory() {
                let id = mem.id.clone();
                if app.selected.contains(&id) {
                    app.selected.remove(&id);
                } else {
                    app.selected.insert(id);
                }
            }
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
            if !app.selected.is_empty() {
                // Bulk delete all selected
                let count = app.selected.len();
                let ids: Vec<String> = app.selected.drain().collect();
                for id in &ids {
                    db::memories::delete(conn, id)?;
                }
                app.memories.retain(|m| !ids.contains(&m.id));
                app.recalc_counts();
                app.apply_filters();
                app.status_msg = Some(format!("Deleted {count} memories"));
            } else if let Some(mem) = app.selected_memory() {
                // Single delete
                let id = mem.id.clone();
                db::memories::delete(conn, &id)?;
                app.memories.retain(|m| m.id != id);
                app.recalc_counts();
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

fn handle_graph_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => app.mode = Mode::List,
        KeyCode::Up | KeyCode::Char('k') => {
            if app.graph_selected > 0 { app.graph_selected -= 1; }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if !app.graph_relations.is_empty() && app.graph_selected + 1 < app.graph_relations.len() {
                app.graph_selected += 1;
            }
        }
        _ => {}
    }
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

    if app.mode == Mode::Graph {
        let area = centered_rect(70, 70, f.area());
        f.render_widget(Clear, area);
        render_graph(f, app, area);
    }

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

    let mut spans = vec![
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
    ];

    if !app.selected.is_empty() {
        spans.push(Span::raw("│ "));
        spans.push(Span::styled(
            format!("◆ {} selected", app.selected.len()),
            Style::default().fg(Color::Yellow).bold(),
        ));
    }

    let header = Line::from(spans);

    let block = Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray));
    let p = Paragraph::new(header).block(block);
    f.render_widget(p, area);
}

fn render_table(f: &mut Frame, app: &mut App, area: Rect) {
    let header = Row::new(vec![
        Cell::from(" ").style(Style::default().fg(Color::Cyan).bold()),
        Cell::from("ID").style(Style::default().fg(Color::Cyan).bold()),
        Cell::from("Type").style(Style::default().fg(Color::Cyan).bold()),
        Cell::from("Subject").style(Style::default().fg(Color::Cyan).bold()),
        Cell::from("Source").style(Style::default().fg(Color::Cyan).bold()),
        Cell::from("Tags").style(Style::default().fg(Color::Cyan).bold()),
        Cell::from("Content").style(Style::default().fg(Color::Cyan).bold()),
        Cell::from("Sync").style(Style::default().fg(Color::Cyan).bold()),
        Cell::from("Created").style(Style::default().fg(Color::Cyan).bold()),
        Cell::from("Expires").style(Style::default().fg(Color::Cyan).bold()),
    ]).height(1);

    let rows: Vec<Row> = app.filtered.iter().map(|&idx| {
        let m = &app.memories[idx];
        let (sync_label, sync_color) = App::sync_status(m);
        let is_selected = app.selected.contains(&m.id);

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

        let source_short = m.source.as_deref().unwrap_or("—").to_string();
        let expires_short = m.expires_at.as_deref().map(|s| &s[..10]).unwrap_or("—");
        let created_short = &m.created_at[..10];

        let select_marker = if is_selected { "◆" } else { " " };
        let select_color = if is_selected { Color::Yellow } else { Color::DarkGray };

        let row = Row::new(vec![
            Cell::from(select_marker).style(Style::default().fg(select_color)),
            Cell::from(m.id[..8].to_string()).style(Style::default().fg(Color::DarkGray)),
            Cell::from(m.memory_type.clone()),
            Cell::from(m.subject.clone().unwrap_or_else(|| "—".into())).style(Style::default().fg(Color::Magenta)),
            Cell::from(source_short).style(Style::default().fg(Color::DarkGray)),
            Cell::from(tags_str).style(Style::default().fg(Color::Blue)),
            Cell::from(content_preview),
            Cell::from(sync_label).style(Style::default().fg(sync_color)),
            Cell::from(created_short.to_string()).style(Style::default().fg(Color::DarkGray)),
            Cell::from(expires_short.to_string()).style(Style::default().fg(Color::DarkGray)),
        ]);

        if is_selected {
            row.style(Style::default().fg(Color::Yellow))
        } else {
            row
        }
    }).collect();

    let widths = [
        Constraint::Length(2),    // Select marker
        Constraint::Length(10),   // ID
        Constraint::Length(12),   // Type
        Constraint::Length(16),   // Subject
        Constraint::Length(10),   // Source
        Constraint::Length(14),   // Tags
        Constraint::Fill(1),      // Content (takes remaining)
        Constraint::Length(12),   // Sync
        Constraint::Length(12),   // Expires
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
    } else if app.mode == Mode::Syncing {
        Line::from(vec![
            Span::styled(" ⟳ Syncing...", Style::default().fg(Color::Cyan).bold()),
        ])
    } else if let Some(msg) = &app.status_msg {
        Line::from(vec![
            Span::styled(format!(" {msg}"), Style::default().fg(Color::Yellow)),
        ])
    } else {
        let mut spans = vec![
            Span::styled(" ↑↓", Style::default().fg(Color::DarkGray)),
            Span::raw(" nav  "),
            Span::styled("Space", Style::default().fg(Color::DarkGray)),
            Span::raw(" select  "),
            Span::styled("a", Style::default().fg(Color::DarkGray)),
            Span::raw("/"),
            Span::styled("A", Style::default().fg(Color::DarkGray)),
            Span::raw(" all/none  "),
            Span::styled("Enter", Style::default().fg(Color::DarkGray)),
            Span::raw(" view  "),
            Span::styled("/", Style::default().fg(Color::DarkGray)),
            Span::raw(" search  "),
            Span::styled("s", Style::default().fg(Color::DarkGray)),
            Span::raw(" filter  "),
            Span::styled("g", Style::default().fg(Color::DarkGray)),
            Span::raw(" graph  "),
            Span::styled("d", Style::default().fg(Color::DarkGray)),
            Span::raw(" delete  "),
            Span::styled("S", Style::default().fg(Color::DarkGray)),
            Span::raw(" sync  "),
            Span::styled("q", Style::default().fg(Color::DarkGray)),
            Span::raw(" quit"),
        ];

        if !app.selected.is_empty() {
            spans.insert(0, Span::styled(
                format!("◆{} ", app.selected.len()),
                Style::default().fg(Color::Yellow).bold(),
            ));
        }

        Line::from(spans)
    };

    f.render_widget(Paragraph::new(content), area);
}

fn render_detail(f: &mut Frame, app: &App, area: Rect) {
    let mem = match app.selected_memory() {
        Some(m) => m,
        None => return,
    };

    let (sync_label, _) = App::sync_status(mem);
    let is_selected = app.selected.contains(&mem.id);

    let mut lines = vec![
        Line::from(vec![
            Span::styled("ID:       ", Style::default().fg(Color::Cyan).bold()),
            Span::raw(&mem.id),
            if is_selected {
                Span::styled("  ◆ selected", Style::default().fg(Color::Yellow))
            } else {
                Span::raw("")
            },
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
            Span::styled("Agent:    ", Style::default().fg(Color::Cyan).bold()),
            Span::raw(mem.agent_id.as_deref().unwrap_or("—")),
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
            Span::styled("SyncedAt: ", Style::default().fg(Color::Cyan).bold()),
            Span::raw(mem.synced_at.as_deref().unwrap_or("—")),
        ]),
        Line::from(vec![
            Span::styled("Expires:  ", Style::default().fg(Color::Cyan).bold()),
            Span::raw(mem.expires_at.as_deref().unwrap_or("—")),
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
        .title(" Memory Detail (Space: toggle select) ")
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
    let delete_count = if !app.selected.is_empty() {
        app.selected.len()
    } else {
        1
    };

    let desc = if delete_count == 1 && app.selected.is_empty() {
        let id_short = app.selected_memory()
            .map(|m| m.id[..8].to_string())
            .unwrap_or_default();
        format!("memory {id_short}")
    } else {
        format!("{delete_count} selected memories")
    };

    let text = Text::from(vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("  Delete "),
            Span::styled(&desc, Style::default().fg(Color::Red).bold()),
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

fn render_graph(f: &mut Frame, app: &mut App, area: Rect) {
    use ratatui::widgets::{List, ListItem, ListState};

    let title = format!(" Entity: {} ({}) ", app.graph_entity_name, app.graph_entity_type);
    let block = Block::default()
        .title(Span::styled(title, Style::default().fg(Color::Cyan).bold()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    if app.graph_relations.is_empty() {
        let text = Text::from(vec![
            Line::from(""),
            Line::from("  No relations found"),
            Line::from(""),
            Line::from(Span::styled("  Esc: back", Style::default().fg(Color::DarkGray))),
        ]);
        f.render_widget(Paragraph::new(text).block(block), area);
        return;
    }

    let items: Vec<ListItem> = app.graph_relations.iter().map(|(_src_name, _src_type, rel_type, tgt_name, confidence, is_outgoing)| {
        let (arrow, arrow_color, other_name) = if *is_outgoing {
            ("→", Color::Green, tgt_name.as_str())
        } else {
            ("←", Color::Magenta, _src_name.as_str())
        };
        ListItem::new(Line::from(vec![
            Span::styled(format!("  {arrow} "), Style::default().fg(arrow_color)),
            Span::styled(format!("[{rel_type}]"), Style::default().fg(Color::Yellow)),
            Span::raw(" "),
            Span::styled(other_name, Style::default().fg(Color::White)),
            Span::styled(format!("  conf: {confidence:.1}"), Style::default().fg(Color::DarkGray)),
        ]))
    }).collect();

    let mut list_state = ListState::default();
    list_state.select(Some(app.graph_selected));

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(block.inner(area));

    f.render_widget(block, area);

    let list = List::new(items)
        .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD));
    f.render_stateful_widget(list, inner[0], &mut list_state);

    let footer = Line::from(vec![
        Span::styled("  Esc", Style::default().fg(Color::DarkGray)),
        Span::raw(": back  "),
        Span::styled("↑↓", Style::default().fg(Color::DarkGray)),
        Span::raw(": navigate"),
    ]);
    f.render_widget(Paragraph::new(footer), inner[1]);
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
