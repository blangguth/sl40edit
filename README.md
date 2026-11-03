# sl40edit

A terminal channel list editor for the **Comag SL40HD** satellite receiver,
for Linux.

The SL40HD can back up its channel database to a USB stick
(*DB management → Backup to USB*), but the receiver's own menus make
reordering hundreds of channels tedious. `sl40edit` opens that backup, lets
you reorder channels with the keyboard, and writes it back, ready for
*Load from USB*.

```
┌ sl40edit — /media/sdc1/COMAG_SL40HD_03_11.DBM [+] ───────────────────────────────────────────┐
│     # Name                                        Satellite                     Freq  Pol  SR│
│     1 Das Erste HD                                ASTRA 1H,1KR,1L,1M 19.2°E     11493 H 22000│
│  »  2 ZDF HD                                      ASTRA 1H,1KR,1L,1M 19.2°E     11361 H 22000│
│     3 AnixeHD Serie                               ASTRA 1H,1KR,1L,1M 19.2°E     10773 H 22000│
└──────────────────────────────────────────────────────────────────────────────────────────────┘
 MOVING  2/378 · modified · j/k g/G move · / n N after match · space drop · esc cancel · ? help
```

## Status

Works and has been tested on a real receiver: lists reordered with
`sl40edit` load via *Load from USB* and show the new order.

Supported: the **948,368-byte** `.DBM` backup of the Comag SL40HD. Other
receivers or file sizes are rejected on load. Other MStar-based receivers
that write the same layout (ChanSort lists e.g. the Xoro HRS 8520) may
work but haven't been tested.

Not yet supported: deleting or renaming channels, favourite lists, the
hide/skip/lock flags. The file layer keeps every byte intact, so these can be
added later.

## Install

You need a Rust toolchain (1.88 or newer), e.g. from
[rustup.rs](https://rustup.rs).

```sh
git clone https://github.com/blangguth/sl40edit
cd sl40edit
cargo install --path .
```

## Usage

1. On the receiver: *DB management → Backup to USB*. This writes a file such as
   `COMAG_SL40HD_03_11.DBM` to the stick.
2. Plug the stick into your computer and run:
   ```sh
   sl40edit /media/<stick>/COMAG_SL40HD_03_11.DBM
   ```
3. Reorder, press `w` to save, `q` to quit, then run `sync` before unplugging.
4. On the receiver: *DB management → Load from USB*. The receiver accepts
   the dated filename as is.

The first save keeps your original as `COMAG_SL40HD_03_11.DBM.bak` next to
it. Later saves never overwrite that backup.

### Keys

| Key | |
|---|---|
| `j`/`k`, `↓`/`↑`, `PgDn`/`PgUp`, `ctrl-d`/`ctrl-u`, `g`/`G` | move the cursor |
| `space` | grab a channel. Movement keys then move it, `space` drops it, `esc` cancels |
| `m` | move the channel to a position number |
| `/` | search. Matches as you type, ignoring case, accents and punctuation (`sat1` finds "SAT.1 Gold"). `↓`/`↑` step through matches, `enter` keeps the result, `esc` goes back |
| `n` / `N` | next / previous match |
| `/`, `n`, `N` *while moving* | place the grabbed channel right after the matching channel |
| `u` / `ctrl-r` | undo / redo |
| `w` | save |
| `q` | quit (asks again if there are unsaved changes) |
| `?` | help |

A quick way to build your order from the top down: grab a channel, type
`/` and the name of the channel it should follow, press `enter`, then `space`.

### Receiver test mode

```sh
sl40edit --test-swap FILE.DBM
```

Swaps channels 1 and 2 and saves (with the same `.bak` handling). Useful to
check that your receiver and firmware accept files written by `sl40edit`.
Run it again to swap back.

## Safety

* The file is validated on load: exact size, plus the checksum the receiver
  uses. Anything else is refused.
* Reordering changes only the program-number field of each channel record and
  the checksum. All other bytes are copied unchanged.
* Before writing, the new file is parsed again and checked. It's written to a
  temporary file first and then renamed into place.
* If the file on disk changed after you opened it (for example, you made a
  new receiver backup in the meantime), `w` refuses to overwrite it.

Still: this is an unofficial tool. Keep your original backup.

## File format

The layout of the SL40HD `.DBM` file comes from
[ChanSort](https://github.com/PredatH0r/ChanSort) by PredatH0r (GPLv3),
section `[dbm:948368]` of `ChanSort.Loader.DBM.ini`. It was verified against
a real backup and extended (orbital position, polarisation). See
[SPEC.md](SPEC.md) for the field-by-field description and design notes.

## Development

```sh
cargo test
cargo clippy --all-targets
```

Most tests need a real receiver backup, which isn't included because it's a
personal channel list. Put an unmodified SL40HD backup at
`testdata/COMAG_SL40HD_03_11.DBM` (read-only). Without it, those tests skip.
Several tests check the specific contents of the author's backup, so with a
different file some of them will fail.

## License

Copyright (C) 2026 Benjamin Langguth

GPL-3.0-only (GNU General Public License, version 3). See [LICENSE](LICENSE).

This project is not affiliated with or endorsed by Comag. "Comag" and
"SL40HD" are used only to identify the supported device.
