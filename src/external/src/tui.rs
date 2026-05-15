//! Full-screen TUI built on ratatui + crossterm.
//!
//! - US-010 introduced the four-tab scaffolding (Add / Remove / List / Update)
//!   with empty placeholder bodies and the q/Esc quit path.
//! - US-012 fills in the Add tab: catalog rows for skills + agents.md
//!   integrations, multi-select with Space, per-row installed/compat
//!   markers, Enter-triggered install, and an Overwrite/Skip/Rename modal
//!   when a skill's target folder already exists.

use std::io::{self, IsTerminal, Stdout};
use std::path::PathBuf;

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
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap},
    Frame, Terminal,
};

use crate::catalog::{self, CatalogEntry};
use crate::harness::{self, Harness};
use crate::installer::{self, OverwriteAction, SkillInstallOutcome};
use crate::state::ProjectState;

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

/// Map a [`Harness`] to its registry-side tag string used in
/// `harness_compatibility` manifest fields.
pub(crate) fn harness_tag(h: Harness) -> &'static str {
    match h {
        Harness::ClaudeCode => "claude-code",
        Harness::Codex => "codex",
        Harness::OpenCode => "opencode",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowKind {
    Skill,
    AgentsMd,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AddRow {
    pub(crate) kind: RowKind,
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) description: String,
    pub(crate) compat: Vec<String>,
    pub(crate) installed: bool,
    pub(crate) incompatible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StatusMsg {
    Info(String),
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConflictModal {
    pub(crate) conflicts: Vec<String>,
}

#[derive(Debug)]
pub(crate) struct AddTabState {
    pub(crate) rows: Vec<AddRow>,
    pub(crate) cursor: usize,
    pub(crate) selected: Vec<bool>,
    pub(crate) status: Option<StatusMsg>,
    pub(crate) modal: Option<ConflictModal>,
}

impl AddTabState {
    fn new(rows: Vec<AddRow>) -> Self {
        let selected = vec![false; rows.len()];
        Self {
            rows,
            cursor: 0,
            selected,
            status: None,
            modal: None,
        }
    }

    fn refresh(&mut self, rows: Vec<AddRow>) {
        let cursor = self.cursor.min(rows.len().saturating_sub(1));
        self.selected = vec![false; rows.len()];
        self.rows = rows;
        self.cursor = cursor;
    }

    pub(crate) fn move_down(&mut self) {
        if self.cursor + 1 < self.rows.len() {
            self.cursor += 1;
        }
    }

    pub(crate) fn move_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub(crate) fn toggle_selected(&mut self) {
        if let Some(row) = self.rows.get(self.cursor) {
            if !row.incompatible {
                let cur = self.selected[self.cursor];
                self.selected[self.cursor] = !cur;
            }
        }
    }

    pub(crate) fn selected_rows(&self) -> Vec<&AddRow> {
        self.rows
            .iter()
            .enumerate()
            .filter_map(|(i, r)| if self.selected[i] { Some(r) } else { None })
            .collect()
    }
}

/// Pure: build the Add-tab rows from the embedded catalog plus the project's
/// installed-items state. Rows whose `harness_compatibility` doesn't include
/// the detected harness are flagged `incompatible`.
pub(crate) fn build_add_rows(
    skills: &[CatalogEntry],
    agents_md: &[CatalogEntry],
    state: &ProjectState,
    harness: Option<Harness>,
) -> Vec<AddRow> {
    let mut out = Vec::with_capacity(skills.len() + agents_md.len());
    for entry in skills {
        out.push(make_row(entry, RowKind::Skill, state, harness));
    }
    for entry in agents_md {
        out.push(make_row(entry, RowKind::AgentsMd, state, harness));
    }
    out
}

fn make_row(
    entry: &CatalogEntry,
    kind: RowKind,
    state: &ProjectState,
    harness: Option<Harness>,
) -> AddRow {
    let installed = match kind {
        RowKind::Skill => state.installed_skills.iter().any(|e| e.name == entry.name),
        RowKind::AgentsMd => state
            .installed_agents_md
            .iter()
            .any(|e| e.name == entry.name),
    };
    let compat: Vec<String> = entry
        .harness_compatibility
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    let incompatible = match harness {
        Some(h) if !compat.is_empty() => !compat.iter().any(|c| c == harness_tag(h)),
        _ => false,
    };
    AddRow {
        kind,
        name: entry.name.to_string(),
        version: entry.version.to_string(),
        description: entry.description.to_string(),
        compat,
        installed,
        incompatible,
    }
}

pub struct App {
    current_tab: Tab,
    harness: Option<Harness>,
    project_root: PathBuf,
    should_quit: bool,
    add: AddTabState,
}

impl App {
    pub fn new(project_root: PathBuf, harness: Option<Harness>) -> Self {
        let state = ProjectState::load(&project_root).unwrap_or_default();
        let rows = build_add_rows(catalog::SKILLS, catalog::AGENTS_MD, &state, harness);
        Self {
            current_tab: Tab::Add,
            harness,
            project_root,
            should_quit: false,
            add: AddTabState::new(rows),
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
        if self.add.modal.is_some() && self.current_tab == Tab::Add {
            self.handle_modal_key(code);
            return;
        }
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
            KeyCode::Up => {
                if self.current_tab == Tab::Add && !self.tab_disabled(Tab::Add) {
                    self.add.move_up();
                }
            }
            KeyCode::Down => {
                if self.current_tab == Tab::Add && !self.tab_disabled(Tab::Add) {
                    self.add.move_down();
                }
            }
            KeyCode::Char(' ') => {
                if self.current_tab == Tab::Add && !self.tab_disabled(Tab::Add) {
                    self.add.toggle_selected();
                }
            }
            KeyCode::Enter => {
                if self.current_tab == Tab::Add && !self.tab_disabled(Tab::Add) {
                    self.trigger_install();
                }
            }
            _ => {}
        }
    }

    fn handle_modal_key(&mut self, code: KeyCode) {
        let action = match code {
            KeyCode::Char('o') | KeyCode::Char('O') => Some(OverwriteAction::Overwrite),
            KeyCode::Char('s') | KeyCode::Char('S') => Some(OverwriteAction::Skip),
            KeyCode::Char('r') | KeyCode::Char('R') => Some(OverwriteAction::Rename),
            KeyCode::Esc => {
                self.add.modal = None;
                return;
            }
            _ => None,
        };
        if let Some(action) = action {
            self.add.modal = None;
            self.run_pending_installs(action);
        }
    }

    fn trigger_install(&mut self) {
        let Some(harness) = self.harness else {
            self.add.status = Some(StatusMsg::Error(
                "No harness detected — cannot install".into(),
            ));
            return;
        };

        let selected = self.add.selected_rows();
        if selected.is_empty() {
            self.add.status = Some(StatusMsg::Info("Nothing selected.".into()));
            return;
        }

        // Skill conflicts: any selected skill whose target folder already
        // exists on disk. agents.md installs are idempotent so they never
        // conflict.
        let conflicts: Vec<String> = selected
            .iter()
            .filter(|r| {
                r.kind == RowKind::Skill
                    && installer::skill_target_exists(&self.project_root, harness, &r.name)
            })
            .map(|r| r.name.clone())
            .collect();

        if !conflicts.is_empty() {
            self.add.modal = Some(ConflictModal { conflicts });
            return;
        }

        self.run_pending_installs(OverwriteAction::Skip);
    }

    fn run_pending_installs(&mut self, on_conflict: OverwriteAction) {
        let Some(harness) = self.harness else { return };
        let project_root = self.project_root.clone();

        let pending: Vec<AddRow> = self
            .add
            .selected_rows()
            .into_iter()
            .cloned()
            .collect();

        let mut installed = 0usize;
        let mut skipped = 0usize;
        let mut renamed = 0usize;
        let mut errors: Vec<String> = Vec::new();
        let source_url = format!(
            "https://github.com/{}/releases/tag/{}",
            installer::REGISTRY_REPO,
            installer::REGISTRY_TAG
        );

        for row in &pending {
            match row.kind {
                RowKind::Skill => match installer::fetch_and_install_skill(
                    &project_root,
                    harness,
                    &row.name,
                    &row.version,
                    &source_url,
                    on_conflict,
                ) {
                    Ok(SkillInstallOutcome::Installed { .. }) => installed += 1,
                    Ok(SkillInstallOutcome::Skipped) => skipped += 1,
                    Ok(SkillInstallOutcome::Renamed { .. }) => renamed += 1,
                    Err(e) => errors.push(format!("{}: {}", row.name, e)),
                },
                RowKind::AgentsMd => match installer::fetch_and_install_agents_md(
                    &project_root,
                    harness,
                    &row.name,
                    &row.version,
                    &source_url,
                ) {
                    Ok(()) => installed += 1,
                    Err(e) => errors.push(format!("{}: {}", row.name, e)),
                },
            }
        }

        self.refresh_add_rows();

        let mut parts: Vec<String> = Vec::new();
        if installed > 0 {
            parts.push(format!("installed {installed}"));
        }
        if skipped > 0 {
            parts.push(format!("skipped {skipped}"));
        }
        if renamed > 0 {
            parts.push(format!("renamed {renamed}"));
        }
        if !parts.is_empty() && errors.is_empty() {
            self.add.status = Some(StatusMsg::Info(parts.join(", ")));
        } else if !errors.is_empty() {
            let mut msg = parts.join(", ");
            if !msg.is_empty() {
                msg.push_str("; ");
            }
            msg.push_str(&format!("{} error(s): {}", errors.len(), errors.join("; ")));
            self.add.status = Some(StatusMsg::Error(msg));
        } else {
            self.add.status = Some(StatusMsg::Info("Done.".into()));
        }
    }

    fn refresh_add_rows(&mut self) {
        let state = ProjectState::load(&self.project_root).unwrap_or_default();
        let rows = build_add_rows(catalog::SKILLS, catalog::AGENTS_MD, &state, self.harness);
        self.add.refresh(rows);
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
/// allows the integration tests to launch the binary without a pty.
pub fn run() -> io::Result<()> {
    if !io::stdout().is_terminal() {
        return Ok(());
    }

    let project_root = std::env::current_dir()?;
    let detected = harness::detect(&project_root);
    let app = App::new(project_root, detected);
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
            Constraint::Length(2),
        ])
        .split(f.area());

    render_header(f, app, layout[0]);
    render_tabs(f, app, layout[1]);
    render_body(f, app, layout[2]);
    render_footer(f, app, layout[3]);

    if app.current_tab == Tab::Add {
        if let Some(modal) = &app.add.modal {
            render_conflict_modal(f, modal);
        }
    }
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
    if app.tab_disabled(app.current_tab) {
        let msg = format!(
            "{} is disabled — no harness detected in this project.",
            app.current_tab.title()
        );
        let body = Paragraph::new(Span::styled(msg, Style::default().fg(Color::DarkGray)))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(app.current_tab.title()),
            )
            .wrap(Wrap { trim: true });
        f.render_widget(body, area);
        return;
    }

    match app.current_tab {
        Tab::Add => render_add(f, app, area),
        Tab::Remove | Tab::List | Tab::Update => {
            let body_text = format!("{} tab (placeholder)", app.current_tab.title());
            let body = Paragraph::new(Span::raw(body_text)).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(app.current_tab.title()),
            );
            f.render_widget(body, area);
        }
    }
}

fn render_add(f: &mut Frame, app: &App, area: Rect) {
    let body_area = Block::default()
        .borders(Borders::ALL)
        .title("Add — [Space] toggle  [Enter] install");
    let inner = body_area.inner(area);
    f.render_widget(body_area, area);

    if app.add.rows.is_empty() {
        let p = Paragraph::new("Catalog is empty.").style(Style::default().fg(Color::DarkGray));
        f.render_widget(p, inner);
        return;
    }

    let mut items: Vec<ListItem> = Vec::with_capacity(app.add.rows.len() + 2);
    let mut last_kind: Option<RowKind> = None;
    for (idx, row) in app.add.rows.iter().enumerate() {
        if last_kind != Some(row.kind) {
            let header = match row.kind {
                RowKind::Skill => "── Skills ──",
                RowKind::AgentsMd => "── agents.md ──",
            };
            items.push(ListItem::new(Span::styled(
                header,
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )));
            last_kind = Some(row.kind);
        }
        items.push(ListItem::new(format_row(row, app.add.selected[idx])).style(row_style(row)));
    }

    // ListState selection maps to row index + header offset.
    let mut list_state = ListState::default();
    list_state.select(Some(cursor_to_list_index(&app.add.rows, app.add.cursor)));

    let list = List::new(items).highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );
    f.render_stateful_widget(list, inner, &mut list_state);
}

fn cursor_to_list_index(rows: &[AddRow], cursor: usize) -> usize {
    // Account for inserted section-header items: one header before the first
    // Skill row (if any) and one before the first AgentsMd row (if any).
    let mut idx = cursor;
    let mut last_kind: Option<RowKind> = None;
    for (i, r) in rows.iter().enumerate() {
        if last_kind != Some(r.kind) {
            if i <= cursor {
                idx += 1;
            }
            last_kind = Some(r.kind);
        }
    }
    idx
}

fn format_row(row: &AddRow, selected: bool) -> String {
    let mark = if selected { "[x]" } else { "[ ]" };
    let max_desc = 50usize;
    let desc: String = if row.description.chars().count() > max_desc {
        let truncated: String = row.description.chars().take(max_desc - 1).collect();
        format!("{truncated}…")
    } else {
        row.description.clone()
    };
    let mut tags: Vec<String> = Vec::new();
    if row.installed {
        tags.push("[installed]".into());
    }
    if row.incompatible {
        let only = if row.compat.len() == 1 {
            format!("[{} only]", row.compat[0])
        } else {
            format!("[{} only]", row.compat.join("/"))
        };
        tags.push(only);
    }
    let tags_str = if tags.is_empty() {
        String::new()
    } else {
        format!("  {}", tags.join(" "))
    };
    format!("{mark} {} v{}  {}{}", row.name, row.version, desc, tags_str)
}

fn row_style(row: &AddRow) -> Style {
    if row.incompatible {
        Style::default().fg(Color::DarkGray)
    } else if row.installed {
        Style::default().fg(Color::Green)
    } else {
        Style::default()
    }
}

fn render_footer(f: &mut Frame, app: &App, area: Rect) {
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    let mut parts: Vec<String> = match app.current_tab {
        Tab::Add => vec![
            "[↑/↓] move".into(),
            "[Space] toggle".into(),
            "[Enter] install".into(),
            "[Tab] switch".into(),
            "[q/Esc] quit".into(),
        ],
        _ => vec![
            "[Tab/Shift+Tab] switch".into(),
            "[1-4] jump".into(),
            "[q/Esc] quit".into(),
        ],
    };
    if app.tab_disabled(app.current_tab) {
        parts.insert(0, "(disabled — no harness)".into());
    }
    let footer = Paragraph::new(Line::from(parts.join("   ")));
    f.render_widget(footer, split[0]);

    if app.current_tab == Tab::Add {
        if let Some(msg) = &app.add.status {
            let (text, style) = match msg {
                StatusMsg::Info(s) => (s.as_str(), Style::default().fg(Color::Green)),
                StatusMsg::Error(s) => (s.as_str(), Style::default().fg(Color::Red)),
            };
            let toast = Paragraph::new(Span::styled(text, style)).wrap(Wrap { trim: true });
            f.render_widget(toast, split[1]);
        }
    }
}

fn render_conflict_modal(f: &mut Frame, modal: &ConflictModal) {
    let area = centered_rect(60, 30, f.area());
    f.render_widget(Clear, area);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        "Target folder(s) already exist:",
        Style::default().add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));
    for name in &modal.conflicts {
        lines.push(Line::from(format!("  • {name}")));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "[O]verwrite   [S]kip   [R]ename   [Esc] cancel",
        Style::default().fg(Color::Cyan),
    )));
    let widget = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Conflict")
                .style(Style::default().fg(Color::Yellow)),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(widget, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::InstalledItem;
    use tempfile::TempDir;

    fn entry(
        name: &'static str,
        version: &'static str,
        description: &'static str,
        compat: &'static [&'static str],
        source_path: &'static str,
    ) -> CatalogEntry {
        CatalogEntry {
            name,
            description,
            version,
            harness_compatibility: compat,
            source_path,
        }
    }

    fn fresh_app(harness: Option<Harness>) -> App {
        let dir = TempDir::new().unwrap();
        // Leak the TempDir guard so the dir lives for the duration of the
        // test; the test only cares about the path.
        let path = dir.keep();
        App::new(path, harness)
    }

    #[test]
    fn tab_cycle_forward_wraps() {
        let mut app = fresh_app(Some(Harness::ClaudeCode));
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
        let mut app = fresh_app(Some(Harness::ClaudeCode));
        app.prev_tab();
        assert_eq!(app.current_tab, Tab::Update);
        app.prev_tab();
        assert_eq!(app.current_tab, Tab::List);
    }

    #[test]
    fn number_keys_jump_directly() {
        let mut app = fresh_app(Some(Harness::Codex));
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
        let mut app = fresh_app(None);
        app.handle_key(KeyCode::Char('q'), KeyModifiers::NONE);
        assert!(app.should_quit);
    }

    #[test]
    fn esc_quits() {
        let mut app = fresh_app(None);
        app.handle_key(KeyCode::Esc, KeyModifiers::NONE);
        assert!(app.should_quit);
    }

    #[test]
    fn shift_tab_goes_back() {
        let mut app = fresh_app(None);
        app.handle_key(KeyCode::Tab, KeyModifiers::SHIFT);
        assert_eq!(app.current_tab, Tab::Update);
    }

    #[test]
    fn back_tab_goes_back() {
        let mut app = fresh_app(None);
        app.handle_key(KeyCode::BackTab, KeyModifiers::NONE);
        assert_eq!(app.current_tab, Tab::Update);
    }

    #[test]
    fn add_and_remove_disabled_without_harness() {
        let app = fresh_app(None);
        assert!(app.tab_disabled(Tab::Add));
        assert!(app.tab_disabled(Tab::Remove));
        assert!(!app.tab_disabled(Tab::List));
        assert!(!app.tab_disabled(Tab::Update));
    }

    #[test]
    fn all_tabs_enabled_with_harness() {
        for h in [Harness::ClaudeCode, Harness::Codex, Harness::OpenCode] {
            let app = fresh_app(Some(h));
            for t in Tab::ALL {
                assert!(!app.tab_disabled(t));
            }
        }
    }

    #[test]
    fn unrelated_keys_do_not_quit_or_switch() {
        let mut app = fresh_app(Some(Harness::ClaudeCode));
        app.handle_key(KeyCode::Char('x'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Char('5'), KeyModifiers::NONE);
        // Enter without selection just sets a status msg, doesn't quit/switch.
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(!app.should_quit);
        assert_eq!(app.current_tab, Tab::Add);
    }

    // -- Add tab data model ---------------------------------------------------

    #[test]
    fn build_add_rows_marks_installed_skills_and_agents() {
        let skills = vec![entry("foo", "0.1.0", "foo desc", &[], "skills/foo")];
        let agents = vec![entry("bar", "0.2.0", "bar desc", &[], "agents.md/bar")];
        let state = ProjectState {
            installed_skills: vec![InstalledItem {
                name: "foo".into(),
                version: "0.1.0".into(),
                source_url: "u".into(),
                install_path: Some(".claude/skills/foo/".into()),
                delimiter_id: None,
            }],
            installed_agents_md: vec![],
            ..Default::default()
        };
        let rows = build_add_rows(&skills, &agents, &state, Some(Harness::ClaudeCode));
        assert_eq!(rows.len(), 2);
        let foo = rows.iter().find(|r| r.name == "foo").unwrap();
        assert_eq!(foo.kind, RowKind::Skill);
        assert!(foo.installed);
        assert!(!foo.incompatible);
        let bar = rows.iter().find(|r| r.name == "bar").unwrap();
        assert_eq!(bar.kind, RowKind::AgentsMd);
        assert!(!bar.installed);
    }

    #[test]
    fn build_add_rows_marks_incompatible_when_harness_not_in_compat() {
        let skills = vec![entry(
            "claude-only",
            "0.1.0",
            "claude only",
            &["claude-code"],
            "skills/claude-only",
        )];
        let agents: Vec<CatalogEntry> = vec![];
        let state = ProjectState::default();

        let rows_codex = build_add_rows(&skills, &agents, &state, Some(Harness::Codex));
        assert!(rows_codex[0].incompatible);

        let rows_claude = build_add_rows(&skills, &agents, &state, Some(Harness::ClaudeCode));
        assert!(!rows_claude[0].incompatible);
    }

    #[test]
    fn build_add_rows_empty_compat_is_universal() {
        let skills = vec![entry("any", "0.1.0", "any harness", &[], "skills/any")];
        let agents: Vec<CatalogEntry> = vec![];
        let state = ProjectState::default();
        for h in [Harness::ClaudeCode, Harness::Codex, Harness::OpenCode] {
            let rows = build_add_rows(&skills, &agents, &state, Some(h));
            assert!(!rows[0].incompatible, "harness {h:?}");
        }
    }

    #[test]
    fn toggle_selected_skips_incompatible_rows() {
        let rows = vec![
            AddRow {
                kind: RowKind::Skill,
                name: "bad".into(),
                version: "0.1.0".into(),
                description: "".into(),
                compat: vec!["codex".into()],
                installed: false,
                incompatible: true,
            },
            AddRow {
                kind: RowKind::Skill,
                name: "good".into(),
                version: "0.1.0".into(),
                description: "".into(),
                compat: vec![],
                installed: false,
                incompatible: false,
            },
        ];
        let mut s = AddTabState::new(rows);
        s.toggle_selected();
        assert!(!s.selected[0], "incompatible row must not toggle");
        s.move_down();
        s.toggle_selected();
        assert!(s.selected[1]);
    }

    #[test]
    fn cursor_move_clamps_at_boundaries() {
        let rows = vec![
            AddRow {
                kind: RowKind::Skill,
                name: "a".into(),
                version: "0.1.0".into(),
                description: "".into(),
                compat: vec![],
                installed: false,
                incompatible: false,
            },
            AddRow {
                kind: RowKind::Skill,
                name: "b".into(),
                version: "0.1.0".into(),
                description: "".into(),
                compat: vec![],
                installed: false,
                incompatible: false,
            },
        ];
        let mut s = AddTabState::new(rows);
        s.move_up();
        assert_eq!(s.cursor, 0);
        s.move_down();
        assert_eq!(s.cursor, 1);
        s.move_down();
        assert_eq!(s.cursor, 1, "does not run off the end");
    }

    #[test]
    fn trigger_install_with_nothing_selected_reports_info() {
        let mut app = fresh_app(Some(Harness::ClaudeCode));
        app.trigger_install();
        match app.add.status {
            Some(StatusMsg::Info(s)) => assert!(s.contains("Nothing selected")),
            other => panic!("expected Info status, got {other:?}"),
        }
    }

    #[test]
    fn trigger_install_with_skill_conflict_opens_modal() {
        let dir = TempDir::new().unwrap();
        let project_root = dir.path().to_path_buf();
        // Pre-seed: a target folder for a catalog skill name. We use the
        // first catalog skill since the embedded catalog has at least one
        // entry (example-skill from US-001).
        let skill_name = catalog::SKILLS[0].name;
        std::fs::create_dir_all(
            project_root
                .join(".claude/skills")
                .join(skill_name),
        )
        .unwrap();

        let mut app = App::new(project_root, Some(Harness::ClaudeCode));
        // Find the row for the seeded skill and select it.
        let idx = app
            .add
            .rows
            .iter()
            .position(|r| r.kind == RowKind::Skill && r.name == skill_name)
            .unwrap();
        app.add.cursor = idx;
        app.add.toggle_selected();
        assert!(app.add.selected[idx]);

        app.trigger_install();
        let modal = app.add.modal.as_ref().expect("conflict modal opens");
        assert!(modal.conflicts.iter().any(|n| n == skill_name));
    }

    #[test]
    fn modal_esc_dismisses_without_install() {
        let dir = TempDir::new().unwrap();
        let mut app = App::new(dir.path().to_path_buf(), Some(Harness::ClaudeCode));
        app.add.modal = Some(ConflictModal {
            conflicts: vec!["x".into()],
        });
        app.handle_key(KeyCode::Esc, KeyModifiers::NONE);
        assert!(app.add.modal.is_none());
        assert!(!app.should_quit, "Esc only closes modal, not the app");
    }

    #[test]
    fn harness_tag_mapping() {
        assert_eq!(harness_tag(Harness::ClaudeCode), "claude-code");
        assert_eq!(harness_tag(Harness::Codex), "codex");
        assert_eq!(harness_tag(Harness::OpenCode), "opencode");
    }

    #[test]
    fn format_row_includes_markers() {
        let row = AddRow {
            kind: RowKind::Skill,
            name: "demo".into(),
            version: "1.0.0".into(),
            description: "a demo".into(),
            compat: vec!["claude-code".into()],
            installed: true,
            incompatible: false,
        };
        let s = format_row(&row, true);
        assert!(s.contains("[x]"));
        assert!(s.contains("demo"));
        assert!(s.contains("v1.0.0"));
        assert!(s.contains("[installed]"));
    }

    #[test]
    fn format_row_truncates_long_description() {
        let row = AddRow {
            kind: RowKind::Skill,
            name: "demo".into(),
            version: "1.0.0".into(),
            description: "x".repeat(200),
            compat: vec![],
            installed: false,
            incompatible: false,
        };
        let s = format_row(&row, false);
        // 50-char cap with the trailing ellipsis (so the text doesn't grow
        // unboundedly).
        assert!(s.contains("…"));
        // The body should be shorter than the full description.
        assert!(s.len() < 200);
    }

    #[test]
    fn format_row_shows_incompatible_tag() {
        let row = AddRow {
            kind: RowKind::Skill,
            name: "demo".into(),
            version: "1.0.0".into(),
            description: "".into(),
            compat: vec!["claude-code".into()],
            installed: false,
            incompatible: true,
        };
        let s = format_row(&row, false);
        assert!(s.contains("[claude-code only]"));
    }
}
