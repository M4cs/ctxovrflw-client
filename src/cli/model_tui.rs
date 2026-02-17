use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState, Wrap},
    Frame, Terminal,
};
use std::io::{self, Write as _};

use crate::config::Config;
use crate::embed::models::{self, EmbeddingModel, MODELS};

// ── Types ───────────────────────────────────────────────────────────────

#[derive(PartialEq)]
enum Mode {
    Browse,
    Detail,
    ConfirmSwitch,
}

struct App {
    models: Vec<&'static EmbeddingModel>,
    current_model: String,
    table_state: TableState,
    mode: Mode,
    should_quit: bool,
    status_msg: Option<(String, Color)>,
    detail_scroll: u16,
    /// Set when user confirms a switch — async work happens after TUI exits
    switch_to: Option<String>,
}

impl App {
    fn new(current_model: String) -> Self {
        let models: Vec<&'static EmbeddingModel> = MODELS.iter().collect();
        let mut table_state = TableState::default();

        // Start selection on current model
        let current_idx = models.iter().position(|m| m.id == current_model).unwrap_or(0);
        table_state.select(Some(current_idx));

        App {
            models,
            current_model,
            table_state,
            mode: Mode::Browse,
            should_quit: false,
            status_msg: None,
            detail_scroll: 0,
            switch_to: None,
        }
    }

    fn selected_model(&self) -> Option<&'static EmbeddingModel> {
        self.table_state.selected().and_then(|i| self.models.get(i).copied())
    }

    fn selected_is_current(&self) -> bool {
        self.selected_model().map_or(false, |m| m.id == self.current_model)
    }
}

// ── Entry point ─────────────────────────────────────────────────────────

pub async fn run(_cfg: &Config) -> Result<()> {
    let cfg = Config::load().unwrap_or_default();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(cfg.embedding_model.clone());

    // Main loop
    while !app.should_quit {
        terminal.draw(|f| draw(f, &mut app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            handle_key(&mut app, key);
        }
    }

    // Cleanup TUI
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    // If user chose to switch, do the async work outside of TUI
    if let Some(model_id) = app.switch_to {
        super::model::switch(&model_id).await?;
    }

    Ok(())
}

// ── Key handling ────────────────────────────────────────────────────────

fn handle_key(app: &mut App, key: KeyEvent) {
    // Global quit
    if key.code == KeyCode::Char('q') && app.mode == Mode::Browse {
        app.should_quit = true;
        return;
    }
    if key.code == KeyCode::Esc {
        match app.mode {
            Mode::Detail | Mode::ConfirmSwitch => app.mode = Mode::Browse,
            Mode::Browse => app.should_quit = true,
            _ => {}
        }
        return;
    }

    match app.mode {
        Mode::Browse => handle_browse(app, key),
        Mode::Detail => handle_detail(app, key),
        Mode::ConfirmSwitch => handle_confirm(app, key),
        _ => {}
    }
}

fn handle_browse(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            let i = app.table_state.selected().unwrap_or(0);
            if i > 0 {
                app.table_state.select(Some(i - 1));
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let i = app.table_state.selected().unwrap_or(0);
            if i + 1 < app.models.len() {
                app.table_state.select(Some(i + 1));
            }
        }
        KeyCode::Home | KeyCode::Char('g') => {
            app.table_state.select(Some(0));
        }
        KeyCode::End | KeyCode::Char('G') => {
            if !app.models.is_empty() {
                app.table_state.select(Some(app.models.len() - 1));
            }
        }
        KeyCode::Enter => {
            if app.selected_is_current() {
                app.status_msg = Some(("Already using this model".into(), Color::Yellow));
            } else {
                app.mode = Mode::ConfirmSwitch;
            }
        }
        KeyCode::Char(' ') | KeyCode::Char('d') => {
            app.detail_scroll = 0;
            app.mode = Mode::Detail;
        }
        _ => {}
    }
}

fn handle_detail(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            app.detail_scroll = app.detail_scroll.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.detail_scroll = app.detail_scroll.saturating_add(1);
        }
        KeyCode::Enter => {
            if !app.selected_is_current() {
                app.mode = Mode::ConfirmSwitch;
            }
        }
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char(' ') | KeyCode::Char('d') => {
            app.mode = Mode::Browse;
        }
        _ => {}
    }
}

fn handle_confirm(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
            if let Some(model) = app.selected_model() {
                app.switch_to = Some(model.id.to_string());
                app.should_quit = true;
            }
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.mode = Mode::Browse;
        }
        _ => {}
    }
}

// ── Drawing ─────────────────────────────────────────────────────────────

fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // header
            Constraint::Min(10),    // model list
            Constraint::Length(3),  // details bar
            Constraint::Length(1),  // help
        ])
        .split(f.area());

    draw_header(f, app, chunks[0]);
    draw_model_table(f, app, chunks[1]);
    draw_info_bar(f, app, chunks[2]);
    draw_help(f, app, chunks[3]);

    // Overlays
    match app.mode {
        Mode::Detail => draw_detail_popup(f, app),
        Mode::ConfirmSwitch => draw_confirm_popup(f, app),
        _ => {}
    }

    // Clear status after drawing
    if app.status_msg.is_some() {
        // status will show for one frame then clear on next key
    }
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let current = models::get_model(&app.current_model)
        .map(|m| format!("{} ({}, {}d)", m.name, format_size(m.size_mb), m.dim))
        .unwrap_or_else(|| app.current_model.clone());

    let header = Paragraph::new(Line::from(vec![
        Span::styled(" Current: ", Style::default().fg(Color::DarkGray)),
        Span::styled(current, Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" 🧠 Embedding Models ")
            .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    f.render_widget(header, area);
}

fn draw_model_table(f: &mut Frame, app: &mut App, area: Rect) {
    let header = Row::new(vec![
        Cell::from(" "),
        Cell::from("Model").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Dims").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Size").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Description").style(Style::default().add_modifier(Modifier::BOLD)),
    ])
    .style(Style::default().fg(Color::DarkGray))
    .height(1);

    let rows: Vec<Row> = app
        .models
        .iter()
        .map(|model| {
            let is_current = model.id == app.current_model;
            let marker = if is_current { "✓" } else { " " };
            let marker_style = if is_current {
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            // Color by size tier
            let size_color = if model.size_mb <= 35 {
                Color::Green
            } else if model.size_mb <= 150 {
                Color::Yellow
            } else {
                Color::Red
            };

            let dim_color = if model.dim <= 384 {
                Color::White
            } else if model.dim <= 768 {
                Color::Cyan
            } else {
                Color::Magenta
            };

            let name_style = if is_current {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::White)
            };

            Row::new(vec![
                Cell::from(marker).style(marker_style),
                Cell::from(model.name).style(name_style),
                Cell::from(format!("{}", model.dim)).style(Style::default().fg(dim_color)),
                Cell::from(format_size(model.size_mb)).style(Style::default().fg(size_color)),
                Cell::from(model.description).style(Style::default().fg(Color::DarkGray)),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(2),
        Constraint::Length(28),
        Constraint::Length(6),
        Constraint::Length(8),
        Constraint::Min(20),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .row_highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );

    f.render_stateful_widget(table, area, &mut app.table_state);
}

fn draw_info_bar(f: &mut Frame, app: &App, area: Rect) {
    let content = if let Some((ref msg, color)) = app.status_msg {
        Line::from(Span::styled(format!(" {}", msg), Style::default().fg(color)))
    } else if let Some(model) = app.selected_model() {
        let downloaded = is_model_downloaded(model);
        let status = if downloaded { "✓ Downloaded" } else { "⬇ Not downloaded" };
        let status_color = if downloaded { Color::Green } else { Color::Yellow };
        let prefix_info = if model.requires_prefix {
            format!(" │ Prefix: \"{}\"", model.query_prefix.unwrap_or(""))
        } else {
            String::new()
        };
        let inputs = if model.num_inputs == 2 { "XLM-R" } else { "BERT" };

        Line::from(vec![
            Span::styled(format!(" {} ", model.id), Style::default().fg(Color::Cyan)),
            Span::styled("│ ", Style::default().fg(Color::DarkGray)),
            Span::styled(status, Style::default().fg(status_color)),
            Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{} arch", inputs), Style::default().fg(Color::DarkGray)),
            Span::styled(prefix_info, Style::default().fg(Color::DarkGray)),
        ])
    } else {
        Line::from("")
    };

    let bar = Paragraph::new(content).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    f.render_widget(bar, area);
}

fn draw_help(f: &mut Frame, app: &App, area: Rect) {
    let help = match app.mode {
        Mode::Browse => {
            vec![
                Span::styled(" ↑↓", Style::default().fg(Color::Cyan)),
                Span::styled(" Navigate  ", Style::default().fg(Color::DarkGray)),
                Span::styled("Enter", Style::default().fg(Color::Cyan)),
                Span::styled(" Switch  ", Style::default().fg(Color::DarkGray)),
                Span::styled("Space/d", Style::default().fg(Color::Cyan)),
                Span::styled(" Details  ", Style::default().fg(Color::DarkGray)),
                Span::styled("q", Style::default().fg(Color::Cyan)),
                Span::styled(" Quit", Style::default().fg(Color::DarkGray)),
            ]
        }
        Mode::Detail => {
            vec![
                Span::styled(" ↑↓", Style::default().fg(Color::Cyan)),
                Span::styled(" Scroll  ", Style::default().fg(Color::DarkGray)),
                Span::styled("Enter", Style::default().fg(Color::Cyan)),
                Span::styled(" Switch  ", Style::default().fg(Color::DarkGray)),
                Span::styled("Esc", Style::default().fg(Color::Cyan)),
                Span::styled(" Back", Style::default().fg(Color::DarkGray)),
            ]
        }
        Mode::ConfirmSwitch => {
            vec![
                Span::styled(" y", Style::default().fg(Color::Green)),
                Span::styled(" Confirm  ", Style::default().fg(Color::DarkGray)),
                Span::styled("n/Esc", Style::default().fg(Color::Red)),
                Span::styled(" Cancel", Style::default().fg(Color::DarkGray)),
            ]
        }
        _ => vec![],
    };

    let help_line = Paragraph::new(Line::from(help));
    f.render_widget(help_line, area);
}

fn draw_detail_popup(f: &mut Frame, app: &App) {
    let model = match app.selected_model() {
        Some(m) => m,
        None => return,
    };

    let area = centered_rect(60, 70, f.area());
    f.render_widget(Clear, area);

    let is_current = model.id == app.current_model;
    let downloaded = is_model_downloaded(model);

    let mut lines = vec![
        Line::from(vec![
            Span::styled("Name:       ", Style::default().fg(Color::DarkGray)),
            Span::styled(model.name, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("ID:         ", Style::default().fg(Color::DarkGray)),
            Span::styled(model.id, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Dimensions: ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{}", model.dim), Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("Size:       ", Style::default().fg(Color::DarkGray)),
            Span::styled(format_size(model.size_mb), Style::default().fg(size_color(model.size_mb))),
        ]),
        Line::from(vec![
            Span::styled("Arch:       ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if model.num_inputs == 2 { "XLM-RoBERTa (2 inputs)" } else { "BERT (3 inputs)" },
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Description:", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled(format!("  {}", model.description), Style::default().fg(Color::White)),
        ]),
        Line::from(""),
    ];

    if model.requires_prefix {
        lines.push(Line::from(vec![
            Span::styled("Prefix:     ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("\"{}\"", model.query_prefix.unwrap_or("")),
                Style::default().fg(Color::Yellow),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled(
                "  (automatically prepended to queries)",
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
            ),
        ]));
        lines.push(Line::from(""));
    }

    // Status
    lines.push(Line::from(vec![
        Span::styled("Status:     ", Style::default().fg(Color::DarkGray)),
        if is_current {
            Span::styled("● Active", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
        } else if downloaded {
            Span::styled("◉ Downloaded", Style::default().fg(Color::Blue))
        } else {
            Span::styled("○ Not downloaded", Style::default().fg(Color::Yellow))
        },
    ]));

    // Download source
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("Source:     ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            short_url(model.onnx_url),
            Style::default().fg(Color::DarkGray),
        ),
    ]));

    let title = format!(" {} ", model.name);
    let popup = Paragraph::new(lines)
        .scroll((app.detail_scroll, 0))
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
                .border_style(Style::default().fg(Color::Cyan)),
        );

    f.render_widget(popup, area);
}

fn draw_confirm_popup(f: &mut Frame, app: &App) {
    let model = match app.selected_model() {
        Some(m) => m,
        None => return,
    };

    let area = centered_rect(50, 30, f.area());
    f.render_widget(Clear, area);

    let downloaded = is_model_downloaded(model);
    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Switch to ", Style::default().fg(Color::White)),
            Span::styled(model.name, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled("?", Style::default().fg(Color::White)),
        ]),
        Line::from(""),
    ];

    if !downloaded {
        lines.push(Line::from(vec![
            Span::styled(
                format!("  ⬇ Will download ~{}", format_size(model.size_mb)),
                Style::default().fg(Color::Yellow),
            ),
        ]));
    }

    lines.push(Line::from(vec![
        Span::styled(
            "  🔄 All memories will be re-embedded",
            Style::default().fg(Color::Yellow),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled(
            "  ⚠ Daemon must be stopped first",
            Style::default().fg(Color::Red),
        ),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  Continue? ", Style::default().fg(Color::White)),
        Span::styled("[y/n]", Style::default().fg(Color::DarkGray)),
    ]));

    let popup = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Confirm Switch ")
            .title_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
            .border_style(Style::default().fg(Color::Yellow)),
    );

    f.render_widget(popup, area);
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn format_size(mb: usize) -> String {
    if mb >= 1000 {
        format!("{:.1} GB", mb as f64 / 1024.0)
    } else {
        format!("{} MB", mb)
    }
}

fn size_color(mb: usize) -> Color {
    if mb <= 35 {
        Color::Green
    } else if mb <= 150 {
        Color::Yellow
    } else {
        Color::Red
    }
}

fn short_url(url: &str) -> String {
    // "https://huggingface.co/Xenova/all-MiniLM-L6-v2/..." → "Xenova/all-MiniLM-L6-v2"
    url.strip_prefix("https://huggingface.co/")
        .and_then(|rest| {
            let parts: Vec<&str> = rest.splitn(4, '/').collect();
            if parts.len() >= 2 {
                Some(format!("huggingface.co/{}/{}", parts[0], parts[1]))
            } else {
                None
            }
        })
        .unwrap_or_else(|| url.to_string())
}

fn is_model_downloaded(model: &EmbeddingModel) -> bool {
    if let Ok(model_dir) = Config::model_dir() {
        let subdir = model_dir.join(model.id);
        subdir.join("model.onnx").exists() && subdir.join("tokenizer.json").exists()
    } else {
        false
    }
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
