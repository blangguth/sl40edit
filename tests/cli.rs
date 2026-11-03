// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Benjamin Langguth

//! End-to-end tests of the command-line binary.

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

use sl40edit::dbm::DbmFile;

fn sample_path() -> Option<PathBuf> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/COMAG_SL40HD_03_11.DBM");
    path.exists().then_some(path).or_else(|| {
        eprintln!("skipping: sample missing");
        None
    })
}

fn names_in_order(f: &DbmFile) -> Vec<String> {
    f.program_order()
        .iter()
        .map(|&i| f.channels[i].name.clone())
        .collect()
}

#[test]
fn test_swap_swaps_first_two_and_back() {
    let Some(sample) = sample_path() else { return };
    let dir = std::env::temp_dir().join(format!("sl40edit-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("COMAG_SL40HD.DBM");
    std::fs::copy(&sample, &path).unwrap();
    // The fixture is read-only; the scratch copy must be writable.
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    let original = DbmFile::load(&path).unwrap();
    let before = names_in_order(&original);

    let run = || {
        let out = Command::new(env!("CARGO_BIN_EXE_sl40edit"))
            .arg("--test-swap")
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    };

    let stdout = run();
    assert!(
        stdout.contains("before: 1 = Das Erste HD, 2 = AnixeHD Serie"),
        "{stdout}"
    );
    let after = names_in_order(&DbmFile::load(&path).unwrap());
    assert_eq!(after[0], before[1]);
    assert_eq!(after[1], before[0]);
    assert_eq!(after[2..], before[2..]);
    assert_eq!(
        std::fs::read(dir.join("COMAG_SL40HD.DBM.bak")).unwrap(),
        original.raw()
    );

    // Second run restores the original bytes exactly.
    run();
    assert_eq!(std::fs::read(&path).unwrap(), original.raw());

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn rejects_bad_arguments() {
    let out = Command::new(env!("CARGO_BIN_EXE_sl40edit"))
        .output()
        .unwrap();
    assert!(!out.status.success());
    let out = Command::new(env!("CARGO_BIN_EXE_sl40edit"))
        .arg("--bogus")
        .output()
        .unwrap();
    assert!(!out.status.success());
}
