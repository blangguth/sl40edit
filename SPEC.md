# sl40edit — channel list editor for the Comag SL40HD

A Linux terminal app (Rust + ratatui) to reorder the channel list of a
**Comag SL40HD** satellite receiver, using the database backup the receiver
writes to USB.

Started as a rebuild of the Windows-only
[ComagListEditor](https://sourceforge.net/p/comaglisteditor/code/HEAD/tree/trunk/).
That tool targets the older PVR2/100CI `CHANLIST.BIN` format, which the
SL40HD does not use. The file format below comes from
[ChanSort](https://github.com/PredatH0r/ChanSort) (GPLv3,
`source/ChanSort.Loader.DBM/ChanSort.Loader.DBM.ini`, section `[dbm:948368]`).
It was checked against a real backup, `samples/COMAG_SL40HD_03_11.DBM`.

## Workflow

1. Receiver: *DB management → Backup to USB* writes `COMAG_SL40HD_<dd>_<mm>.DBM`.
2. `sl40edit /media/sdc1/COMAG_SL40HD_03_11.DBM`, then reorder and save.
3. Receiver: *DB management → Load from USB*. It accepts the dated filename
   as written (confirmed 2026-11-03), so there's no need to rename.

## Scope

### v1
- Show all channels in program order, with columns `#`, Name, Satellite, Freq/Pol/SR.
- Reorder channels by hand:
  - **Grab & move:** `space` grabs the channel under the cursor. Movement keys
    then move it, and the other channels shift to make room. `space` drops it
    and `esc` cancels the move.
  - **Move to number:** `m`, type a number, `enter`. The channel moves to that
    position and the others shift.
- `/` incremental search by name. While you type, the cursor jumps to the
  first match at or after where the search started. `↓`/`↑` (or
  `tab`/`shift-tab`) step through the matches, `enter` confirms, and `esc`
  returns to the starting point. After confirming, `n`/`N` go to the next or
  previous match with wrap-around, matching names are highlighted, and the
  status bar shows `/query 2/7`. `esc` in normal mode clears the search.
  Matching ignores case, diacritics and punctuation: `sat1` finds
  "SAT.1 Gold", `zdfneo` finds "zdf_neo HD", `osterreich` finds
  "Österreich".
- Undo and redo (`u` / `ctrl-r`) across the whole session.
- Save in place (`w`), keeping a `.bak` copy of the original. Quit with `q`,
  which asks for confirmation when there are unsaved changes.

### Not in v1
Deleting, renaming, favourites, editing the hide/skip/lock flags, automatic
sorting, CSV import/export, and other receivers or file sizes. The format
layer keeps all bytes intact so these can be added later without
re-engineering.

## Keys

| Key | Normal mode | Grab mode |
|---|---|---|
| `j`/`k`, `↓`/`↑` | cursor down/up | move the grabbed channel down/up |
| `ctrl-d`/`ctrl-u`, `PgDn`/`PgUp` | half or full page | move by a page |
| `g`/`G`, `Home`/`End` | first/last | move to the top/bottom |
| `space` | grab | drop |
| `esc` | clear search | cancel (put the channel back) |
| `m` | move to number… | — |
| `/`, `n`, `N` | search, next, previous | — |
| `u`, `ctrl-r` | undo, redo | — |
| `w` | save | — |
| `q` | quit | — |
| `?` | help | help |

## File format (948,368-byte variant)

All integers are little-endian. "Bitmap" means one bit per record slot, LSB
first: bit `i` set means slot `i` is in use. Records in unused slots are
leftovers and must be ignored.

| Offset | Size | Content |
|---|---|---|
| `0x00000` | u16 | checksum |
| `0x00002` | u32 | data length `L` (948,360 in the sample) |
| `0x00006` | 32 | satellite bitmap (254 slots) |
| `0x00026` | 254 × 76 | satellite records |
| `0x04B8E` | 376 | transponder bitmap (3000 slots; bits 3000–3007 are set and ignored) |
| `0x04D06` | 3000 × 36 | transponder records |
| `0x1F2FC` | 626 | channel bitmap (5000 slots; bits 5000–5007 are set and ignored) |
| `0x1F56E` | 5000 × 164 | channel records |
| … | | trailing data, mostly `0xFF` padding, preserved as-is |

**Checksum:** `u16(0x55 + Σ bytes[6 .. 6+L])`, truncated to 16 bits.

**Satellite record (76 B):** name `[0..34]` (NUL-padded), LNB low/high
frequency u16 at 34/36, orbital position u16 at 74, in tenths of a degree
(e.g. 130 = 13.0°E). Values above 1800 mean west: `3600 − pos`, so
3530 = 7.0°W. The ini says `offOribalPos` (typo), so ChanSort never reads this
field. The value was verified by hand.

**Transponder record (36 B):** frequency u32 at 0 (MHz, truncated, e.g.
11493), symbol rate u16 at 20 (kS/s), polarisation u8 at 22 (0 = H,
1 = V). Polarisation is *inferred* from known transponders (11347 V vs.
11361/11493/12187/12544 H) and is not in ChanSort. Bytes 4–19 and 24–35
appear to contain stale RAM pointers and are not interpreted.

**Channel record (164 B):**

| Off | Type | Field |
|---|---|---|
| 0 | 64 B | name, DVB string (see below) |
| 64 | u16 | program number, **0-based** |
| 66 | u16 | LCN |
| 70 | u16 | satellite index |
| 72 | u16 | transponder index |
| 80 | u8 | service type (1 TV, 2 radio, other = data) |
| 81 | u8 | flags: `0x04` hide, `0x08` skip, `0x10` lock (other bits unknown) |
| 83 | u8 | favourites bits (ChanSort: A=0x01, B=0x04, C=0x08, D=0x10) |
| 100/102/104 | u16 | TSID / ONID / SID |
| 108/110 | u16 | PCR PID / video PID |

**Names:** follow the DVB text rules (EN 300 468 Annex A). A leading `0x05`
selects ISO-8859-9. With no prefix byte, the text is treated as ISO-8859-1.
`0x86`/`0x87` mark the short-name part and are stripped for display. v1
treats names as read-only, so they are never re-encoded.

### Facts about the sample
59 satellites, 1493 transponders, 378 channels (all TV), program numbers
0–377 with no gaps or duplicates, no favourites set. Flag byte 81 is `0x01`
on every channel except one (`0x21`, Das Erste HD at program 0, which is
probably the last-watched channel).

## How saving works

- Reordering rewrites **only** the u16 at channel offset 64. All other bytes
  are copied through unchanged.
- ChanSort marks some other DBM variants with `reorderPhysically=true`, which
  writes records in the new order. This variant isn't marked, and renumbering
  alone is enough: **confirmed on the receiver on 2026-11-03** (see the open
  questions below).
- Before writing, the app checks that the program numbers are a permutation of
  `0..n-1`, recalculates the checksum, then parses the new image again and
  compares the result.
- It writes `<file>.tmp`, fsyncs, renames the original to `<file>.bak` (only
  if no `.bak` exists yet, so the `.bak` always keeps the pristine original),
  then renames the tmp file into place.

## Testing

- **Round trip:** parse the sample and serialize it with no changes. The bytes
  must be identical.
- **Checksum:** recalculating it on the sample reproduces the stored value.
- **Parse facts:** the counts and spot checks listed under "Facts about the
  sample".
- **Reorder:** after a move, the program numbers are still a permutation and
  only the bytes at channel offsets 64–65 (and the checksum) differ.
- The sample is gitignored (it's a personal channel list). Tests that need it
  skip with a message when it is missing.

## Open questions (receiver tests)

1. ~~Does renumbering alone change the order on the receiver, or do the
   records have to be physically reordered?~~ **Answered on 2026-11-03:**
   renumbering is enough. After `--test-swap`, *Load from USB* accepted the
   file, which also confirms the checksum, and channel 1 became AnixeHD Serie.
   No physical reordering is needed.
2. What does bit `0x20` of flag byte 81 mean (Das Erste HD)? It moved with
   the record during the swap test and caused no visible problems.
3. ~~Which filename does *Load from USB* expect?~~ **Answered on 2026-11-03:**
   it accepts the dated name `COMAG_SL40HD_03_11.DBM` unchanged.

## Milestones

1. **Format core** (done in the scaffold): parse, serialize, checksum, reorder
   by program numbers, tests.
2. **Read-only TUI** (done in the scaffold): table, navigation, status bar.
3. **Editing** (done): grab and move, move to number, undo and redo
   (`src/editor.rs`). A whole grab counts as one undo step.
4. Save from the UI (`w`) with backup and verification. The quit
   confirmation is already in place: `q` or `ctrl-c` with unsaved changes
   asks for a second press.
5. **Search** (done, `src/search.rs`), help overlay (`?`) still open.
6. ~~First receiver test~~ done on 2026-11-03 (`sl40edit --test-swap FILE.DBM`
   swaps channels 1 and 2 in place; run it again to swap back).

## License

GPL-3.0-or-later. The format knowledge comes from ChanSort (GPLv3, ©
PredatH0r).
