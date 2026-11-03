// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Benjamin Langguth

//! Reader/writer for the Comag SL40HD `.DBM` channel database (948,368-byte variant).
//!
//! Layout from ChanSort (`ChanSort.Loader.DBM.ini`, `[dbm:948368]`, GPLv3), verified
//! against a real receiver backup. See SPEC.md for the field-level description.
//!
//! The parsed structs are a read-only view. The original bytes are kept in full, and
//! writing only patches the program-number field of each channel and the checksum.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};

use crate::text::decode_dvb_string;

/// Offsets and sizes of the 948,368-byte layout.
pub mod layout {
    pub const FILE_SIZE: usize = 948_368;

    pub const OFF_CHECKSUM: usize = 0x0000;
    pub const OFF_DATA_LENGTH: usize = 0x0002;
    pub const OFF_DATA: usize = 0x0006;

    pub const OFF_SAT_BITMAP: usize = 0x0006;
    pub const OFF_SAT_DATA: usize = 0x0026;
    pub const NUM_SAT: usize = 254;
    pub const LEN_SAT: usize = 76;

    pub const OFF_TP_BITMAP: usize = 0x4b8e;
    pub const OFF_TP_DATA: usize = 0x4d06;
    pub const NUM_TP: usize = 3000;
    pub const LEN_TP: usize = 36;

    pub const OFF_CH_BITMAP: usize = 0x1f2fc;
    pub const OFF_CH_DATA: usize = 0x1f56e;
    pub const NUM_CH: usize = 5000;
    pub const LEN_CH: usize = 164;

    // satellite record
    pub const SAT_NAME: usize = 0;
    pub const SAT_NAME_LEN: usize = 34;
    pub const SAT_ORBITAL_POS: usize = 74;

    // transponder record
    pub const TP_FREQ: usize = 0;
    pub const TP_SYMBOL_RATE: usize = 20;
    pub const TP_POLARISATION: usize = 22;

    // channel record
    pub const CH_NAME: usize = 0;
    pub const CH_NAME_LEN: usize = 64;
    pub const CH_PROG_NR: usize = 64;
    pub const CH_SAT_INDEX: usize = 70;
    pub const CH_TP_INDEX: usize = 72;
    pub const CH_SERVICE_TYPE: usize = 80;
    pub const CH_FLAGS: usize = 81;
    pub const CH_FAVOURITES: usize = 83;
    pub const CH_TSID: usize = 100;
    pub const CH_ONID: usize = 102;
    pub const CH_SID: usize = 104;
}

use layout::*;

#[derive(Debug, Clone)]
pub struct Satellite {
    pub slot: usize,
    pub name: String,
    /// Tenths of a degree; values > 1800 are west (3600 - pos).
    pub orbital_pos: u16,
}

impl Satellite {
    pub fn position_label(&self) -> String {
        let (pos, dir) = if self.orbital_pos > 1800 {
            (3600u16.saturating_sub(self.orbital_pos), 'W')
        } else {
            (self.orbital_pos, 'E')
        };
        format!("{}.{}°{dir}", pos / 10, pos % 10)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Polarisation {
    Horizontal,
    Vertical,
    Unknown(u8),
}

impl Polarisation {
    fn from_byte(b: u8) -> Self {
        match b {
            0 => Self::Horizontal,
            1 => Self::Vertical,
            other => Self::Unknown(other),
        }
    }

    pub fn letter(self) -> char {
        match self {
            Self::Horizontal => 'H',
            Self::Vertical => 'V',
            Self::Unknown(_) => '?',
        }
    }
}

#[derive(Debug, Clone)]
pub struct Transponder {
    pub slot: usize,
    pub freq_mhz: u32,
    pub symbol_rate: u16,
    pub polarisation: Polarisation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceType {
    Tv,
    Radio,
    Other(u8),
}

#[derive(Debug, Clone)]
pub struct Channel {
    /// Record slot in the channel table (stable identity; never changes on reorder).
    pub slot: usize,
    pub name: String,
    /// 0-based program number as stored in the file.
    pub prog_nr: u16,
    pub sat_index: u16,
    pub tp_index: u16,
    pub service_type: ServiceType,
    pub flags: u8,
    pub favourites: u8,
    pub tsid: u16,
    pub onid: u16,
    pub sid: u16,
}

impl Channel {
    pub fn hidden(&self) -> bool {
        self.flags & 0x04 != 0
    }
    pub fn skip(&self) -> bool {
        self.flags & 0x08 != 0
    }
    pub fn locked(&self) -> bool {
        self.flags & 0x10 != 0
    }
}

#[derive(Debug, Clone)]
pub struct DbmFile {
    raw: Vec<u8>,
    pub satellites: Vec<Satellite>,
    pub transponders: Vec<Transponder>,
    /// In slot order.
    pub channels: Vec<Channel>,
    /// Non-fatal oddities found while parsing.
    pub warnings: Vec<String>,
}

fn u16_at(d: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([d[off], d[off + 1]])
}

fn u32_at(d: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([d[off], d[off + 1], d[off + 2], d[off + 3]])
}

fn used_slots(d: &[u8], bitmap: usize, count: usize) -> impl Iterator<Item = usize> + '_ {
    (0..count).filter(move |i| d[bitmap + i / 8] & (1 << (i % 8)) != 0)
}

/// Data length stored in the header, validated against the buffer size.
fn data_length(d: &[u8]) -> Result<usize> {
    ensure!(d.len() >= OFF_DATA, "file too short for header");
    let len = u32_at(d, OFF_DATA_LENGTH) as usize;
    ensure!(
        OFF_DATA + len <= d.len(),
        "header data length {len} exceeds file size {}",
        d.len()
    );
    Ok(len)
}

/// `u16(0x55 + sum of bytes[6 .. 6+len])`
pub fn compute_checksum(d: &[u8]) -> Result<u16> {
    let len = data_length(d)?;
    let sum = d[OFF_DATA..OFF_DATA + len]
        .iter()
        .fold(0x55u32, |acc, &b| acc.wrapping_add(b as u32));
    Ok(sum as u16)
}

pub fn stored_checksum(d: &[u8]) -> u16 {
    u16_at(d, OFF_CHECKSUM)
}

impl DbmFile {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(bytes).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn parse(raw: Vec<u8>) -> Result<Self> {
        if raw.len() != FILE_SIZE {
            bail!(
                "unsupported file size {} bytes (only the {FILE_SIZE}-byte Comag SL40HD layout is supported)",
                raw.len()
            );
        }
        let stored = stored_checksum(&raw);
        let computed = compute_checksum(&raw)?;
        ensure!(
            stored == computed,
            "checksum mismatch: stored {stored:04x}, computed {computed:04x} (file damaged or not an SL40HD backup)"
        );

        let d = &raw;
        let mut warnings = Vec::new();

        let satellites: Vec<Satellite> = used_slots(d, OFF_SAT_BITMAP, NUM_SAT)
            .map(|slot| {
                let r = &d[OFF_SAT_DATA + slot * LEN_SAT..][..LEN_SAT];
                Satellite {
                    slot,
                    name: decode_dvb_string(&r[SAT_NAME..SAT_NAME + SAT_NAME_LEN]),
                    orbital_pos: u16_at(r, SAT_ORBITAL_POS),
                }
            })
            .collect();

        let transponders: Vec<Transponder> = used_slots(d, OFF_TP_BITMAP, NUM_TP)
            .map(|slot| {
                let r = &d[OFF_TP_DATA + slot * LEN_TP..][..LEN_TP];
                Transponder {
                    slot,
                    freq_mhz: u32_at(r, TP_FREQ),
                    symbol_rate: u16_at(r, TP_SYMBOL_RATE),
                    polarisation: Polarisation::from_byte(r[TP_POLARISATION]),
                }
            })
            .collect();

        let channels: Vec<Channel> = used_slots(d, OFF_CH_BITMAP, NUM_CH)
            .map(|slot| {
                let r = &d[OFF_CH_DATA + slot * LEN_CH..][..LEN_CH];
                Channel {
                    slot,
                    name: decode_dvb_string(&r[CH_NAME..CH_NAME + CH_NAME_LEN]),
                    prog_nr: u16_at(r, CH_PROG_NR),
                    sat_index: u16_at(r, CH_SAT_INDEX),
                    tp_index: u16_at(r, CH_TP_INDEX),
                    service_type: match r[CH_SERVICE_TYPE] {
                        1 => ServiceType::Tv,
                        2 => ServiceType::Radio,
                        other => ServiceType::Other(other),
                    },
                    flags: r[CH_FLAGS],
                    favourites: r[CH_FAVOURITES],
                    tsid: u16_at(r, CH_TSID),
                    onid: u16_at(r, CH_ONID),
                    sid: u16_at(r, CH_SID),
                }
            })
            .collect();

        let mut file = DbmFile {
            raw,
            satellites,
            transponders,
            channels,
            warnings: Vec::new(),
        };

        for ch in &file.channels {
            if file.satellite(ch.sat_index).is_none() {
                warnings.push(format!(
                    "channel '{}' references unused satellite slot {}",
                    ch.name, ch.sat_index
                ));
            }
            if file.transponder(ch.tp_index).is_none() {
                warnings.push(format!(
                    "channel '{}' references unused transponder slot {}",
                    ch.name, ch.tp_index
                ));
            }
        }
        let mut progs: Vec<u16> = file.channels.iter().map(|c| c.prog_nr).collect();
        progs.sort_unstable();
        if progs.iter().enumerate().any(|(i, &p)| p as usize != i) {
            warnings.push(
                "program numbers are not a contiguous 0..n-1 sequence; saving will renumber them"
                    .into(),
            );
        }
        file.warnings = warnings;
        Ok(file)
    }

    /// The untouched file contents.
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }

    pub fn satellite(&self, slot: u16) -> Option<&Satellite> {
        self.satellites
            .binary_search_by_key(&(slot as usize), |s| s.slot)
            .ok()
            .map(|i| &self.satellites[i])
    }

    pub fn transponder(&self, slot: u16) -> Option<&Transponder> {
        self.transponders
            .binary_search_by_key(&(slot as usize), |t| t.slot)
            .ok()
            .map(|i| &self.transponders[i])
    }

    /// Indices into `self.channels`, sorted by stored program number (ties by slot).
    pub fn program_order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.channels.len()).collect();
        order.sort_by_key(|&i| (self.channels[i].prog_nr, self.channels[i].slot));
        order
    }

    /// Build a new file image where `order[n]` (an index into `self.channels`) gets
    /// program number `n`. Only the program-number fields and the checksum change.
    pub fn to_bytes_with_order(&self, order: &[usize]) -> Result<Vec<u8>> {
        let n = self.channels.len();
        ensure!(
            order.len() == n,
            "order has {} entries, expected {n}",
            order.len()
        );
        let mut seen = vec![false; n];
        for &i in order {
            ensure!(
                i < n && !seen[i],
                "order is not a permutation of the channel list"
            );
            seen[i] = true;
        }

        let mut out = self.raw.clone();
        for (prog, &i) in order.iter().enumerate() {
            let off = OFF_CH_DATA + self.channels[i].slot * LEN_CH + CH_PROG_NR;
            out[off..off + 2].copy_from_slice(&(prog as u16).to_le_bytes());
        }
        let sum = compute_checksum(&out)?;
        out[OFF_CHECKSUM..OFF_CHECKSUM + 2].copy_from_slice(&sum.to_le_bytes());
        Ok(out)
    }

    /// Write the reordered file to `path` in place.
    ///
    /// Before writing, the new image is parsed again and its order checked. Then
    /// the image goes to `<path>.tmp` and is fsynced. The existing file is moved to
    /// `<path>.bak`, but only if no backup exists yet, so the backup always holds the
    /// pristine original. Finally the tmp file is renamed into place.
    /// Returns the backup path if one was created.
    pub fn save(&self, path: &Path, order: &[usize]) -> Result<Option<PathBuf>> {
        let bytes = self.to_bytes_with_order(order)?;

        let check = DbmFile::parse(bytes.clone()).context("verifying the new image")?;
        let expected: Vec<usize> = order.iter().map(|&i| self.channels[i].slot).collect();
        let actual: Vec<usize> = check
            .program_order()
            .iter()
            .map(|&i| check.channels[i].slot)
            .collect();
        ensure!(
            expected == actual,
            "verification failed: written order differs"
        );

        let tmp = with_suffix(path, ".tmp");
        let bak = with_suffix(path, ".bak");
        {
            let mut f =
                File::create(&tmp).with_context(|| format!("creating {}", tmp.display()))?;
            f.write_all(&bytes)?;
            f.sync_all()?;
        }
        let backup = if path.exists() && !bak.exists() {
            fs::rename(path, &bak).with_context(|| format!("creating backup {}", bak.display()))?;
            Some(bak)
        } else {
            None
        };
        fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
        if let Some(dir) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            // Best effort: persist the renames.
            let _ = File::open(dir).and_then(|d| d.sync_all());
        }
        Ok(backup)
    }
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}
