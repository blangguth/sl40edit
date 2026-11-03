//! DVB string decoding (EN 300 468, Annex A) — just enough for channel and satellite names.

/// Decode a NUL-padded DVB text field for display.
///
/// * `0x05` prefix → ISO-8859-9 (seen on the SL40HD)
/// * `0x15` prefix → UTF-8
/// * `0x10 0x00 0x09` → ISO-8859-9
/// * no prefix (or other tables) → ISO-8859-1. That's what this receiver
///   effectively uses; strict DVB would mean ISO 6937.
///
/// Control codes 0x80–0x9F are dropped (0x86/0x87 mark the short name), and 0x8A
/// (CR/LF) becomes a space.
pub fn decode_dvb_string(field: &[u8]) -> String {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    let bytes = &field[..end];

    let (table, body) = match bytes {
        [0x05, rest @ ..] => (Table::Latin5, rest),
        [0x15, rest @ ..] => return String::from_utf8_lossy(rest).into_owned(),
        [0x10, 0x00, 0x09, rest @ ..] => (Table::Latin5, rest),
        [0x10, _, _, rest @ ..] => (Table::Latin1, rest),
        [0x01..=0x0b, rest @ ..] => (Table::Latin1, rest),
        _ => (Table::Latin1, bytes),
    };

    body.iter()
        .filter_map(|&b| match b {
            0x8a => Some(' '),
            0x80..=0x9f => None,
            _ => Some(table.decode(b)),
        })
        .collect()
}

#[derive(Clone, Copy)]
enum Table {
    Latin1,
    /// ISO-8859-9: Latin-1 with six Turkish letters swapped in.
    Latin5,
}

impl Table {
    fn decode(self, b: u8) -> char {
        if let Table::Latin5 = self {
            match b {
                0xd0 => return 'Ğ',
                0xdd => return 'İ',
                0xde => return 'Ş',
                0xf0 => return 'ğ',
                0xfd => return 'ı',
                0xfe => return 'ş',
                _ => {}
            }
        }
        b as char // Latin-1 maps 1:1 onto U+0000..U+00FF
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_latin1_with_padding() {
        assert_eq!(decode_dvb_string(b"GU\xcdA\0\0\0garbage"), "GUÍA");
    }

    #[test]
    fn latin5_prefix() {
        assert_eq!(
            decode_dvb_string(b"\x05SAT.1 Gold \xd6sterreich\0"),
            "SAT.1 Gold Österreich"
        );
        assert_eq!(decode_dvb_string(b"\x05\xdeimdi\0"), "Şimdi");
    }

    #[test]
    fn strips_emphasis_markers() {
        assert_eq!(
            decode_dvb_string(b"\x05\x86DELUXE\x87 MUSIC"),
            "DELUXE MUSIC"
        );
    }

    #[test]
    fn utf8_prefix() {
        assert_eq!(decode_dvb_string("\u{15}Café".as_bytes()), "Café");
    }
}
