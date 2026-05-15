//! Full-screen TUI built on ratatui + crossterm. Implemented in US-010.
//!
//! Scaffolding only: four tabs (Add / Remove / List / Update) with empty
//! placeholder bodies. Later stories (US-012/US-013/US-014/US-015) fill in
//! the tab contents and wire up install/remove/list/update logic.

use std::io::{self, IsTerminal, Stdout};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Tabs},
    Frame, Terminal,
};

use crate::harness::{self, Harness};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Add,
    Remove,
    List,
    Update,
}

impl Tab {
    const ALL: [Tab; 4] = [Tab::Add, Tab::Remove, Tab::List, Tab::Update];

    fn index(self) -> usize {
        match self {
            Tab::Add => 0,
            Tab::Remove => 1,
            Tab::List => 2,
            Tab::Update => 3,
        }
    }

    fn from_index(i: usize) -> Tab {
        Tab::ALL[i % Tab::ALL.len()]
    }

    fn title(self) -> &'static str {
        match self {
            Tab::Add => "Add",
            Tab::Remove => "Remove",
            Tab::List => "List",
            Tab::Update => "Update",
        }
    }

    fn requires_harness(self) -> bool {
        matches!(self, Tab::Add | Tab::Remove)
    }
}

pub struct App {
    current_tab: Tab,
    harness: Option<Harness>,
    should_quit: bool,
}

impl App {
    pub fn new(harness: Option<Harness>) -> Self {
        Self {
            current_tab: Tab::Add,
            harness,
            should_quit: false,
        }
    }

    fn next_tab(&mut self) {
        let i = self.current_tab.index();
        self.current_tab = Tab::from_index((i + 1) % Tab::ALL.len());
    }

    fn prev_tab(&mut self) {
        let i = self.current_tab.index();
        self.current_tab = Tab::from_index((i + Tab::ALL.len() - 1) % Tab::ALL.len());
    }

    fn select_tab(&mut self, idx: usize) {
        if idx < Tab::ALL.len() {
            self.current_tab = Tab::from_index(idx);
        }
    }

    fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Tab => {
                if modifiers.contains(KeyModifiers::SHIFT) {
                    self.prev_tab();
                } else {
                    self.next_tab();
                }
            }
            KeyCode::BackTab => self.prev_tab(),
            KeyCode::Char('1') => self.select_tab(0),
            KeyCode::Char('2') => self.select_tab(1),
            KeyCode::Char('3') => self.select_tab(2),
            KeyCode::Char('4') => self.select_tab(3),
            _ => {}
        }
    }

    fn tab_disabled(&self, tab: Tab) -> bool {
        tab.requires_harness() && self.harness.is_none()
    }
}

/// Entry point. Detects the harness in the current working directory, sets up
/// the terminal, and runs the event loop until the user quits.
///
/// In non-TTY contexts (e.g. piped stdin/stdout under `assert_cmd`) the TUI is
/// skipped and `Ok(())` is returned. This keeps the binary scriptable and
/// allows the integration test in US-010 to launch the binary without a pty.
pub fn run() -> io::Result<()> {
    if !io::stdout().is_terminal() {
        return Ok(());
    }

    let project_root = std::env::current_dir()?;
    let detected = harness::detect(&project_root);
    let app = App::new(detected);
    run_app(app)
}

fn run_app(mut app: App) -> io::Result<()> {
    let mut terminal = setup_terminal()?;
    let result = event_loop(&mut terminal, &mut app);
    let restore = restore_terminal(&mut terminal);
    result.and(restore)
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Terminal::new(CrosstermBackend::new(stdout))
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
) -> io::Result<()> {
    while !app.should_quit {
        terminal.draw(|f| ui(f, app))?;
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                app.handle_key(key.code, key.modifiers);
            }
        }
    }
    Ok(())
}

fn ui(f: &mut Frame, app: &App) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(f.area());

    render_header(f, app, layout[0]);
    render_tabs(f, app, layout[1]);
    render_body(f, app, layout[2]);
    render_footer(f, app, layout[3]);
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let (label, style) = match app.harness {
        Some(Harness::ClaudeCode) => ("Harness: Claude Code", Style::default().fg(Color::Green)),
        Some(Harness::Codex) => ("Harness: Codex", Style::default().fg(Color::Green)),
        Some(Harness::OpenCode) => ("Harness: OpenCode", Style::default().fg(Color::Green)),
        None => ("No harness detected", Style::default().fg(Color::Yellow)),
    };
    let header = Paragraph::new(Span::styled(label, style))
        .block(Block::default().borders(Borders::ALL).title("instinctagents"));
    f.render_widget(header, area);
}

fn render_tabs(f: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<Line> = Tab::ALL
        .iter()
        .map(|t| {
            let style = if app.tab_disabled(*t) {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
            };
            Line::styled(t.title(), style)
        })
        .collect();
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL))
        .select(app.current_tab.index())
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Cyan),
        );
    f.render_widget(tabs, area);
}

fn render_body(f: &mut Frame, app: &App, area: Rect) {
    let disabled = app.tab_disabled(app.current_tab);
    let body_text = if disabled {
        format!(
            "{} is disabled — no harness detected in this project.",
            app.current_tab.title()
        )
    } else {
        format!("{} tab (placeholder)", app.current_tab.title())
    };
    let style = if disabled {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default()
    };
    let body = Paragraph::new(Span::styled(body_text, style)).block(
        Block::default()
            .borders(Borders::ALL)
            .title(app.current_tab.title()),
    );
    f.render_widget(body, area);
}

fn render_footer(f: &mut Frame, app: &App, area: Rect) {
    let mut parts: Vec<String> = vec![
        "[Tab/Shift+Tab] switch".into(),
        "[1-4] jump".into(),
        "[q/Esc] quit".into(),
    ];
    if app.tab_disabled(app.current_tab) {
        parts.insert(0, "(disabled — no harness)".into());
    }
    let footer = Paragraph::new(Line::from(parts.join("   ")));
    f.render_widget(footer, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_cycle_forward_wraps() {
        let mut app = App::new(Some(Harness::ClaudeCode));
        assert_eq!(app.current_tab, Tab::Add);
        app.next_tab();
        assert_eq!(app.current_tab, Tab::Remove);
        app.next_tab();
        assert_eq!(app.current_tab, Tab::List);
        app.next_tab();
        assert_eq!(app.current_tab, Tab::Update);
        app.next_tab();
        assert_eq!(app.current_tab, Tab::Add);
    }

    #[test]
    fn tab_cycle_backward_wraps() {
        let mut app = App::new(Some(Harness::ClaudeCode));
        app.prev_tab();
        assert_eq!(app.current_tab, Tab::Update);
        app.prev_tab();
        assert_eq!(app.current_tab, Tab::List);
    }

    #[test]
    fn number_keys_jump_directly() {
        let mut app = App::new(Some(Harness::Codex));
        app.handle_key(KeyCode::Char('3'), KeyModifiers::NONE);
        assert_eq!(app.current_tab, Tab::List);
        app.handle_key(KeyCode::Char('1'), KeyModifiers::NONE);
        assert_eq!(app.current_tab, Tab::Add);
        app.handle_key(KeyCode::Char('4'), KeyModifiers::NONE);
        assert_eq!(app.current_tab, Tab::Update);
        app.handle_key(KeyCode::Char('2'), KeyModifiers::NONE);
        assert_eq!(app.current_tab, Tab::Remove);
    }

    #[test]
    fn q_quits() {
        let mut app = App::new(None);
        app.handle_key(KeyCode::Char('q'), KeyModifiers::NONE);
        assert!(app.should_quit);
    }

    #[test]
    fn esc_quits() {
        let mut app = App::new(None);
        app.handle_key(KeyCode::Esc, KeyModifiers::NONE);
        assert!(app.should_quit);
    }

    #[test]
    fn shift_tab_goes_back() {
        let mut app = App::new(None);
        app.handle_key(KeyCode::Tab, KeyModifiers::SHIFT);
        assert_eq!(app.current_tab, Tab::Update);
    }

    #[test]
    fn back_tab_goes_back() {
        let mut app = App::new(None);
        app.handle_key(KeyCode::BackTab, KeyModifiers::NONE);
        assert_eq!(app.current_tab, Tab::Update);
    }

    #[test]
    fn add_and_remove_disabled_without_harness() {
        let app = App::new(None);
        assert!(app.tab_disabled(Tab::Add));
        assert!(app.tab_disabled(Tab::Remove));
        assert!(!app.tab_disabled(Tab::List));
        assert!(!app.tab_disabled(Tab::Update));
    }

    #[test]
    fn all_tabs_enabled_with_harness() {
        for h in [Harness::ClaudeCode, Harness::Codex, Harness::OpenCode] {
            let app = App::new(Some(h));
            for t in Tab::ALL {
                assert!(!app.tab_disabled(t));
            }
        }
    }

    #[test]
    fn unrelated_keys_do_not_quit_or_switch() {
        let mut app = App::new(Some(Harness::ClaudeCode));
        app.handle_key(KeyCode::Char('x'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Char('5'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(!app.should_quit);
        assert_eq!(app.current_tab, Tab::Add);
    }
}
