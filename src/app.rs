//! Terminal UI. Read-only so far (milestone 2): it shows the channel table and
//! supports navigation. Editing lands in milestone 3 (see SPEC.md).

use std::path::PathBuf;

use anyhow::Result;
use ratatui::DefaultTerminal;
use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Row, Table, TableState};

use sl40edit::dbm::DbmFile;

pub struct App {
    path: PathBuf,
    file: DbmFile,
    /// Channel indices (into `file.channels`) in current program order.
    order: Vec<usize>,
    table: TableState,
    /// Rows visible in the table body, updated on each draw (used for paging).
    page: usize,
    quit: bool,
}

impl App {
    pub fn new(path: PathBuf, file: DbmFile) -> Self {
        let order = file.program_order();
        let mut table = TableState::default();
        if !order.is_empty() {
            table.select(Some(0));
        }
        Self {
            path,
            file,
            order,
            table,
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

    fn on_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let half = (self.page / 2).max(1) as isize;
        let full = self.page.max(1) as isize;
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('c') if ctrl => self.quit = true,
            KeyCode::Char('j') | KeyCode::Down => self.move_cursor(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_cursor(-1),
            KeyCode::Char('d') if ctrl => self.move_cursor(half),
            KeyCode::Char('u') if ctrl => self.move_cursor(-half),
            KeyCode::PageDown => self.move_cursor(full),
            KeyCode::PageUp => self.move_cursor(-full),
            KeyCode::Char('g') | KeyCode::Home => self.table.select(Some(0)),
            KeyCode::Char('G') | KeyCode::End => self.table.select(self.order.len().checked_sub(1)),
            _ => {}
        }
    }

    fn move_cursor(&mut self, delta: isize) {
        let Some(last) = self.order.len().checked_sub(1) else {
            return;
        };
        let cur = self.table.selected().unwrap_or(0) as isize;
        self.table
            .select(Some((cur + delta).clamp(0, last as isize) as usize));
    }

    fn draw(&mut self, frame: &mut Frame) {
        let [main, status] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(frame.area());

        let rows = self.order.iter().enumerate().map(|(pos, &i)| {
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
        .row_highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .block(Block::bordered().title(format!(" sl40edit — {} ", self.path.display())));

        // Two border lines plus the header row.
        self.page = main.height.saturating_sub(3) as usize;
        frame.render_stateful_widget(table, main, &mut self.table);

        let pos = self.table.selected().map_or(0, |p| p + 1);
        let mut spans = vec![
            Span::raw(format!(" {pos}/{} ", self.order.len())),
            Span::raw("· checksum OK "),
        ];
        if !self.file.warnings.is_empty() {
            spans.push(Span::raw(format!("· {} warning(s) ", self.file.warnings.len())).yellow());
        }
        spans.push(Span::raw("· read-only preview · j/k g/G PgUp/PgDn · q quit").dim());
        frame.render_widget(Line::from(spans), status);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn renders_sample_table() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("samples/COMAG_SL40HD_03_11.DBM");
        if !path.exists() {
            eprintln!("skipping: sample missing");
            return;
        }
        let mut app = App::new(path.clone(), DbmFile::load(&path).unwrap());
        let mut term = Terminal::new(TestBackend::new(110, 12)).unwrap();
        term.draw(|f| app.draw(f)).unwrap();
        app.on_key(KeyEvent::from(KeyCode::Char('j')));
        term.draw(|f| app.draw(f)).unwrap();

        let screen: String = term
            .backend()
            .buffer()
            .content()
            .chunks(110)
            .map(|row| row.iter().map(|c| c.symbol()).collect::<String>() + "\n")
            .collect();
        println!("{screen}");
        assert!(screen.contains("Das Erste HD"));
        assert!(screen.contains("11493 H 22000"));
        assert!(screen.contains(" 2/378 "));
    }
}
