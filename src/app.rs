//! Terminal UI: channel table, grab & move, move to number, undo/redo.
//! Saving from the UI comes in milestone 4 (see SPEC.md).

use std::path::PathBuf;

use anyhow::Result;
use ratatui::DefaultTerminal;
use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, HighlightSpacing, Row, Table, TableState};

use sl40edit::dbm::DbmFile;
use sl40edit::editor::Editor;

/// Longest number the move-to prompt accepts (the table has at most 5000 slots).
const MAX_PROMPT_DIGITS: usize = 4;

pub struct App {
    path: PathBuf,
    file: DbmFile,
    editor: Editor,
    table: TableState,
    /// Input of the move-to prompt while it's open.
    prompt: Option<String>,
    /// One-shot status message, cleared on the next key press.
    message: Option<String>,
    /// Set after the first `q` with unsaved changes; a second `q` quits.
    quit_armed: bool,
    /// Rows visible in the table body, updated on each draw (used for paging).
    page: usize,
    quit: bool,
}

impl App {
    pub fn new(path: PathBuf, file: DbmFile) -> Self {
        let editor = Editor::new(file.program_order());
        let mut table = TableState::default();
        if !editor.is_empty() {
            table.select(Some(0));
        }
        Self {
            path,
            file,
            editor,
            table,
            prompt: None,
            message: None,
            quit_armed: false,
            page: 10,
            quit: false,
        }
    }

    pub fn run(mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while !self.quit {
            terminal.draw(|f| self.draw(f))?;
            if let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                self.on_key(key);
            }
        }
        Ok(())
    }

    fn cursor(&self) -> usize {
        self.table.selected().unwrap_or(0)
    }

    fn channel_name(&self, pos: usize) -> &str {
        &self.file.channels[self.editor.order()[pos]].name
    }

    fn on_key(&mut self, key: KeyEvent) {
        self.message = None;
        let quit_armed = std::mem::take(&mut self.quit_armed);
        let ctrl_c =
            key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);

        if ctrl_c
            || (key.code == KeyCode::Char('q')
                && self.prompt.is_none()
                && self.editor.grabbed().is_none())
        {
            self.request_quit(quit_armed);
        } else if self.prompt.is_some() {
            self.on_prompt_key(key);
        } else if self.editor.grabbed().is_some() {
            self.on_grab_key(key);
        } else {
            self.on_normal_key(key);
        }
    }

    /// Quit at once if nothing is unsaved; otherwise require a second press.
    fn request_quit(&mut self, quit_armed: bool) {
        // Includes an in-progress grab, which counts as a change too.
        if !self.editor.is_dirty() || quit_armed {
            self.quit = true;
        } else {
            self.quit_armed = true;
            self.message = Some("Unsaved changes will be lost. Press again to quit.".into());
        }
    }

    /// Cursor target for a navigation key, or `None` if `key` isn't one.
    fn nav_target(&self, key: KeyEvent) -> Option<usize> {
        let last = self.editor.len().checked_sub(1)?;
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let half = (self.page / 2).max(1) as isize;
        let full = self.page.max(1) as isize;
        let delta = match key.code {
            KeyCode::Char('d') if ctrl => half,
            KeyCode::Char('u') if ctrl => -half,
            _ if ctrl => return None,
            KeyCode::Char('j') | KeyCode::Down => 1,
            KeyCode::Char('k') | KeyCode::Up => -1,
            KeyCode::PageDown => full,
            KeyCode::PageUp => -full,
            KeyCode::Char('g') | KeyCode::Home => return Some(0),
            KeyCode::Char('G') | KeyCode::End => return Some(last),
            _ => return None,
        };
        Some((self.cursor() as isize + delta).clamp(0, last as isize) as usize)
    }

    fn on_normal_key(&mut self, key: KeyEvent) {
        if let Some(target) = self.nav_target(key) {
            self.table.select(Some(target));
            return;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('r') if ctrl => match self.editor.redo() {
                Some(pos) => self.table.select(Some(pos)),
                None => self.message = Some("Nothing to redo".into()),
            },
            _ if ctrl => {}
            KeyCode::Char(' ') if !self.editor.is_empty() => {
                self.editor.grab(self.cursor());
            }
            KeyCode::Char('m') if !self.editor.is_empty() => self.prompt = Some(String::new()),
            KeyCode::Char('u') => match self.editor.undo() {
                Some(pos) => self.table.select(Some(pos)),
                None => self.message = Some("Nothing to undo".into()),
            },
            _ => {}
        }
    }

    fn on_grab_key(&mut self, key: KeyEvent) {
        if let Some(target) = self.nav_target(key) {
            if let Some(pos) = self.editor.move_grabbed(target) {
                self.table.select(Some(pos));
            }
            return;
        }
        match key.code {
            KeyCode::Char(' ') | KeyCode::Enter => {
                if let Some(pos) = self.editor.drop_grab() {
                    self.table.select(Some(pos));
                }
            }
            KeyCode::Esc => {
                if let Some(pos) = self.editor.cancel_grab() {
                    self.table.select(Some(pos));
                }
            }
            _ => self.message = Some("Moving: space/enter drops, esc cancels".into()),
        }
    }

    fn on_prompt_key(&mut self, key: KeyEvent) {
        let Some(input) = self.prompt.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Char(c @ '0'..='9') if input.len() < MAX_PROMPT_DIGITS => input.push(c),
            KeyCode::Backspace => {
                input.pop();
            }
            KeyCode::Esc => self.prompt = None,
            KeyCode::Enter => {
                let input = self.prompt.take().unwrap_or_default();
                let Ok(n @ 1..) = input.parse::<usize>() else {
                    if !input.is_empty() {
                        self.message = Some(format!("Invalid position '{input}'"));
                    }
                    return;
                };
                let from = self.cursor();
                if let Some(pos) = self.editor.move_to(from, n - 1) {
                    self.table.select(Some(pos));
                    self.message = Some(format!(
                        "Moved '{}' from {} to {}",
                        self.channel_name(pos),
                        from + 1,
                        pos + 1
                    ));
                }
            }
            _ => {}
        }
    }

    fn draw(&mut self, frame: &mut Frame) {
        let [main, status] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(frame.area());

        let rows = self.editor.order().iter().enumerate().map(|(pos, &i)| {
            let ch = &self.file.channels[i];
            let sat = self
                .file
                .satellite(ch.sat_index)
                .map(|s| format!("{} {}", s.name, s.position_label()))
                .unwrap_or_else(|| "?".into());
            let tp = self
                .file
                .transponder(ch.tp_index)
                .map(|t| {
                    format!(
                        "{:>5} {} {:>5}",
                        t.freq_mhz,
                        t.polarisation.letter(),
                        t.symbol_rate
                    )
                })
                .unwrap_or_else(|| "?".into());
            Row::new([format!("{:>4}", pos + 1), ch.name.clone(), sat, tp])
        });

        let grabbing = self.editor.grabbed().is_some();
        let (highlight, symbol) = if grabbing {
            (
                Style::new()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
                "» ",
            )
        } else {
            (Style::new().add_modifier(Modifier::REVERSED), "  ")
        };

        let dirty = if self.editor.is_dirty() { " [+]" } else { "" };
        let table = Table::new(
            rows,
            [
                Constraint::Length(4),
                Constraint::Fill(2),
                Constraint::Fill(1),
                Constraint::Length(13),
            ],
        )
        .header(Row::new(["   #", "Name", "Satellite", "Freq  Pol  SR"]).style(Style::new().bold()))
        .row_highlight_style(highlight)
        .highlight_symbol(symbol)
        .highlight_spacing(HighlightSpacing::Always)
        .block(Block::bordered().title(format!(" sl40edit — {}{dirty} ", self.path.display())));

        // Two border lines plus the header row.
        self.page = main.height.saturating_sub(3) as usize;
        frame.render_stateful_widget(table, main, &mut self.table);
        frame.render_widget(self.status_line(), status);
    }

    fn status_line(&self) -> Line<'_> {
        let pos = self.table.selected().map_or(0, |p| p + 1);
        let n = self.editor.len();

        if let Some(input) = &self.prompt {
            return Line::from(vec![
                Span::raw(" MOVE TO ").black().on_cyan(),
                Span::raw(format!(
                    " '{}' → position: ",
                    self.channel_name(self.cursor())
                )),
                Span::raw(format!("{input}▏")).bold(),
                Span::raw(format!("  (1–{n}, enter ok, esc cancel)")).dim(),
            ]);
        }

        let (badge, hints) = if self.editor.grabbed().is_some() {
            (
                Span::raw(" MOVING ").black().on_yellow(),
                "j/k PgUp/PgDn g/G move · space drop · esc cancel",
            )
        } else {
            (
                Span::raw(" NORMAL ").black().on_blue(),
                "space grab · m move to · u undo · ^r redo · q quit",
            )
        };

        let mut spans = vec![badge, Span::raw(format!(" {pos}/{n} "))];
        if self.editor.is_dirty() {
            spans.push(Span::raw("· modified ").yellow());
        }
        if !self.file.warnings.is_empty() {
            spans.push(Span::raw(format!("· {} warning(s) ", self.file.warnings.len())).yellow());
        }
        match &self.message {
            Some(msg) => spans.push(Span::raw(format!("· {msg}")).bold()),
            None => spans.push(Span::raw(format!("· {hints}")).dim()),
        }
        Line::from(spans)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn app() -> Option<App> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("samples/COMAG_SL40HD_03_11.DBM");
        if !path.exists() {
            eprintln!("skipping: sample missing");
            return None;
        }
        let file = DbmFile::load(&path).unwrap();
        Some(App::new(path, file))
    }

    fn press(app: &mut App, keys: &str) {
        for c in keys.chars() {
            let code = match c {
                '\n' => KeyCode::Enter,
                '\x1b' => KeyCode::Esc,
                c => KeyCode::Char(c),
            };
            app.on_key(KeyEvent::from(code));
        }
    }

    fn names(app: &App, n: usize) -> Vec<&str> {
        (0..n).map(|p| app.channel_name(p)).collect()
    }

    fn render(app: &mut App) -> String {
        let mut term = Terminal::new(TestBackend::new(110, 12)).unwrap();
        term.draw(|f| app.draw(f)).unwrap();
        term.backend()
            .buffer()
            .content()
            .chunks(110)
            .map(|row| row.iter().map(|c| c.symbol()).collect::<String>() + "\n")
            .collect()
    }

    #[test]
    fn renders_sample_table() {
        let Some(mut app) = app() else { return };
        press(&mut app, "j");
        let screen = render(&mut app);
        println!("{screen}");
        assert!(screen.contains("Das Erste HD"));
        assert!(screen.contains("11493 H 22000"));
        assert!(screen.contains(" 2/378 "));
        assert!(screen.contains("NORMAL"));
    }

    #[test]
    fn grab_move_drop_then_undo_redo() {
        let Some(mut app) = app() else { return };
        let before = names(&app, 4)
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();

        press(&mut app, " jj");
        assert!(render(&mut app).contains("MOVING"));
        press(&mut app, " ");
        assert_eq!(
            names(&app, 4),
            [&before[1], &before[2], &before[0], &before[3]]
        );
        assert_eq!(app.cursor(), 2);
        let screen = render(&mut app);
        assert!(
            screen.contains("[+]") && screen.contains("modified"),
            "{screen}"
        );

        press(&mut app, "u");
        assert_eq!(names(&app, 4), before);
        assert_eq!(app.cursor(), 0);
        app.on_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));
        assert_eq!(names(&app, 1), [&before[1]]);
        assert_eq!(app.cursor(), 2);
    }

    #[test]
    fn esc_cancels_grab() {
        let Some(mut app) = app() else { return };
        press(&mut app, " G\x1b");
        assert_eq!(app.channel_name(0), "Das Erste HD");
        assert_eq!(app.cursor(), 0);
        assert!(!app.editor.is_dirty());
    }

    #[test]
    fn move_to_number() {
        let Some(mut app) = app() else { return };
        press(&mut app, "m");
        assert!(render(&mut app).contains("MOVE TO"));
        press(&mut app, "5\n");
        assert_eq!(app.channel_name(4), "Das Erste HD");
        assert_eq!(app.cursor(), 4);
        assert!(render(&mut app).contains("Moved 'Das Erste HD' from 1 to 5"));

        // Clamped to the end; invalid input and esc do nothing.
        press(&mut app, "m9999\n");
        assert_eq!(app.channel_name(377), "Das Erste HD");
        press(&mut app, "m0\n");
        assert_eq!(app.cursor(), 377);
        press(&mut app, "m3\x1b");
        assert_eq!(app.cursor(), 377);
        assert_eq!(app.editor.undo_depth(), 2);
    }

    #[test]
    fn quit_asks_when_dirty() {
        let Some(mut app) = app() else { return };
        press(&mut app, "q");
        assert!(app.quit);

        let Some(mut app) = self::app() else { return };
        press(&mut app, "m2\nq");
        assert!(!app.quit);
        press(&mut app, "q");
        assert!(app.quit);

        // Any other key disarms it.
        let Some(mut app) = self::app() else { return };
        press(&mut app, "m2\nqjq");
        assert!(!app.quit);

        // Ctrl-C works in every mode, with the same confirmation.
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let Some(mut app) = self::app() else { return };
        press(&mut app, " j");
        app.on_key(ctrl_c);
        assert!(!app.quit && app.editor.grabbed().is_some());
        app.on_key(ctrl_c);
        assert!(app.quit);
    }
}
