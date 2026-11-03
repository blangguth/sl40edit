//! Tests against the real receiver backup in `samples/`. The sample is gitignored
//! (personal channel list), so the tests skip if it's missing.

use std::path::PathBuf;

use sl40edit::dbm::{
    DbmFile, Polarisation, ServiceType, compute_checksum, layout, stored_checksum,
};

fn sample() -> Option<DbmFile> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("samples/COMAG_SL40HD_03_11.DBM");
    if !path.exists() {
        eprintln!("skipping: {} not found", path.display());
        return None;
    }
    Some(DbmFile::load(&path).expect("sample should parse"))
}

#[test]
fn checksum_matches_stored_value() {
    let Some(f) = sample() else { return };
    assert_eq!(compute_checksum(f.raw()).unwrap(), stored_checksum(f.raw()));
}

#[test]
fn counts() {
    let Some(f) = sample() else { return };
    assert_eq!(f.satellites.len(), 59);
    assert_eq!(f.transponders.len(), 1493);
    assert_eq!(f.channels.len(), 378);
    assert!(f.channels.iter().all(|c| c.service_type == ServiceType::Tv));
    assert!(
        f.warnings.is_empty(),
        "unexpected warnings: {:?}",
        f.warnings
    );
}

#[test]
fn spot_checks() {
    let Some(f) = sample() else { return };
    let order = f.program_order();
    let first = &f.channels[order[0]];
    assert_eq!(first.name, "Das Erste HD");
    assert_eq!(first.prog_nr, 0);
    assert_eq!((first.onid, first.tsid, first.sid), (1, 1019, 10301));

    let tp = f.transponder(first.tp_index).unwrap();
    assert_eq!(
        (tp.freq_mhz, tp.symbol_rate, tp.polarisation),
        (11493, 22000, Polarisation::Horizontal)
    );
    let sat = f.satellite(first.sat_index).unwrap();
    assert_eq!(sat.name, "ASTRA 1H,1KR,1L,1M");
    assert_eq!(sat.position_label(), "19.2°E");

    let kika = f.channels.iter().find(|c| c.name == "KiKA HD").unwrap();
    assert_eq!(
        f.transponder(kika.tp_index).unwrap().polarisation,
        Polarisation::Vertical
    );

    assert!(f.channels.iter().any(|c| c.name == "SAT.1 Gold Österreich"));
    assert_eq!(f.satellites[0].position_label(), "13.0°E");
}

#[test]
fn unchanged_round_trip_is_byte_identical() {
    let Some(f) = sample() else { return };
    let out = f.to_bytes_with_order(&f.program_order()).unwrap();
    assert!(out == f.raw(), "round trip changed bytes");
}

#[test]
fn reorder_touches_only_prog_nrs_and_checksum() {
    let Some(f) = sample() else { return };
    let mut order = f.program_order();
    order.rotate_right(1); // last channel becomes first

    let out = f.to_bytes_with_order(&order).unwrap();
    let changed: Vec<usize> = (0..out.len()).filter(|&i| out[i] != f.raw()[i]).collect();
    assert!(!changed.is_empty());
    for off in changed {
        let in_checksum = off < 2;
        let in_prog_nr = off >= layout::OFF_CH_DATA && {
            let rel = (off - layout::OFF_CH_DATA) % layout::LEN_CH;
            rel == layout::CH_PROG_NR || rel == layout::CH_PROG_NR + 1
        };
        assert!(
            in_checksum || in_prog_nr,
            "unexpected change at offset {off:#x}"
        );
    }

    let reparsed = DbmFile::parse(out).unwrap();
    let names: Vec<&str> = reparsed
        .program_order()
        .iter()
        .map(|&i| reparsed.channels[i].name.as_str())
        .collect();
    let expected: Vec<&str> = order.iter().map(|&i| f.channels[i].name.as_str()).collect();
    assert_eq!(names, expected);
}

#[test]
fn rejects_non_permutation() {
    let Some(f) = sample() else { return };
    let mut order = f.program_order();
    order[1] = order[0];
    assert!(f.to_bytes_with_order(&order).is_err());
    order.pop();
    assert!(f.to_bytes_with_order(&order).is_err());
}

#[test]
fn rejects_corrupted_file() {
    let Some(f) = sample() else { return };
    let mut bad = f.raw().to_vec();
    bad[0x1000] ^= 0xff;
    let err = DbmFile::parse(bad).unwrap_err().to_string();
    assert!(err.contains("checksum"), "{err}");
    assert!(DbmFile::parse(f.raw()[..1000].to_vec()).is_err());
}

#[test]
fn save_writes_backup_once_and_verifies() {
    let Some(f) = sample() else { return };
    let dir = std::env::temp_dir().join(format!("sl40edit-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("COMAG_SL40HD.DBM");
    std::fs::write(&path, f.raw()).unwrap();

    let mut order = f.program_order();
    order.swap(0, 1);
    let bak = f
        .save(&path, &order)
        .unwrap()
        .expect("first save creates a backup");
    assert_eq!(std::fs::read(&bak).unwrap(), f.raw());

    // A second save must not overwrite the pristine backup.
    let saved = DbmFile::load(&path).unwrap();
    assert!(saved.save(&path, &saved.program_order()).unwrap().is_none());
    assert_eq!(std::fs::read(&bak).unwrap(), f.raw());

    let reloaded = DbmFile::load(&path).unwrap();
    assert_eq!(
        reloaded.channels[reloaded.program_order()[0]].name,
        f.channels[order[0]].name
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
