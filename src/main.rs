mod app;

use std::path::PathBuf;

use anyhow::{Result, bail};
use sl40edit::dbm::DbmFile;

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let (Some(path), None) = (args.next(), args.next()) else {
        bail!("usage: sl40edit <COMAG_SL40HD_xx_xx.DBM>");
    };
    let path = PathBuf::from(path);
    // Load before touching the terminal so errors print normally.
    let file = DbmFile::load(&path)?;

    let mut terminal = ratatui::init();
    let result = app::App::new(path, file).run(&mut terminal);
    ratatui::restore();
    result
}
