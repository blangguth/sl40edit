// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Benjamin Langguth

mod app;

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use sl40edit::dbm::DbmFile;

const USAGE: &str = "\
usage: sl40edit <FILE.DBM>               open the editor
       sl40edit --test-swap <FILE.DBM>   swap channels 1 and 2 and save in place (receiver test)";

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.as_slice() {
        [flag, path] if flag == "--test-swap" => test_swap(Path::new(path)),
        [flag] if flag == "-h" || flag == "--help" => {
            println!("{USAGE}");
            Ok(())
        }
        [path] if !path.to_string_lossy().starts_with('-') => edit(PathBuf::from(path)),
        _ => bail!("{USAGE}"),
    }
}

fn edit(path: PathBuf) -> Result<()> {
    // Load before touching the terminal so errors print normally.
    let file = DbmFile::load(&path)?;
    let mut terminal = ratatui::init();
    let result = app::App::new(path, file).run(&mut terminal);
    ratatui::restore();
    result
}

/// Swap the first two channels and save. Used to check on the receiver whether
/// rewriting program numbers alone changes the order (SPEC.md, open question 1).
fn test_swap(path: &Path) -> Result<()> {
    let file = DbmFile::load(path)?;
    let mut order = file.program_order();
    if order.len() < 2 {
        bail!("need at least two channels, found {}", order.len());
    }
    let name = |i: usize| file.channels[i].name.as_str();
    println!("before: 1 = {}, 2 = {}", name(order[0]), name(order[1]));
    order.swap(0, 1);

    let backup = file.save(path, &order)?;
    println!("after:  1 = {}, 2 = {}", name(order[0]), name(order[1]));
    match backup {
        Some(bak) => println!(
            "saved {} (original kept as {})",
            path.display(),
            bak.display()
        ),
        None => println!("saved {} (existing .bak left untouched)", path.display()),
    }
    println!(
        "\nOn the receiver: DB management → Load from USB, then check whether\n\
         channel 1 is now '{}'. Running this again swaps them back.",
        name(order[0])
    );
    Ok(())
}
