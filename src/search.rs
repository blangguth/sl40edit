// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Benjamin Langguth

//! Forgiving channel-name search.
//!
//! The query and the names are compared after normalising both: lowercase,
//! diacritics folded to the base letter, and everything except letters and
//! digits dropped. So `sat1` finds "SAT.1 Gold", `zdfneo` finds "zdf_neo HD",
//! and `osterreich` finds "Österreich".

/// A normalised search query. `None` from [`Query::new`] means there's nothing to search for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    raw: String,
    norm: String,
}

impl Query {
    pub fn new(raw: &str) -> Option<Self> {
        let norm = normalize(raw);
        (!norm.is_empty()).then(|| Self {
            raw: raw.to_owned(),
            norm,
        })
    }

    /// The text as typed.
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    pub fn matches(&self, name: &str) -> bool {
        normalize(name).contains(&self.norm)
    }
}

pub fn normalize(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(fold_diacritic)
        .filter(|c| c.is_alphanumeric())
        .collect()
}

fn fold_diacritic(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'ı' => 'i',
        'ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        'ğ' => 'g',
        'ş' => 's',
        other => other,
    }
}

/// A search hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    pub pos: usize,
    /// The search ran past the end (or start) and continued from the other side.
    pub wrapped: bool,
}

/// Find the first position, starting at `start` (inclusive) and going forward or
/// backward with wrap-around, for which `is_match` is true.
pub fn find(
    len: usize,
    start: usize,
    forward: bool,
    mut is_match: impl FnMut(usize) -> bool,
) -> Option<Hit> {
    if len == 0 {
        return None;
    }
    let start = start % len;
    (0..len)
        .map(|k| {
            if forward {
                (start + k) % len
            } else {
                (start + len - k) % len
            }
        })
        .find(|&pos| is_match(pos))
        .map(|pos| Hit {
            pos,
            wrapped: if forward { pos < start } else { pos > start },
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalisation() {
        assert_eq!(normalize("SAT.1 Gold Österreich"), "sat1goldosterreich");
        assert_eq!(normalize("zdf_neo HD"), "zdfneohd");
        assert_eq!(normalize("GUÍA FÁCIL"), "guiafacil");
        assert_eq!(normalize(" .-_ "), "");
    }

    #[test]
    fn query_matching() {
        let q = Query::new("sat1").unwrap();
        assert!(q.matches("SAT.1 Gold"));
        assert!(!q.matches("SAT 2"));
        assert!(Query::new("Das Erste").unwrap().matches("Das Erste HD"));
        assert!(
            Query::new("österreich")
                .unwrap()
                .matches("SAT.1 Gold Osterreich")
        );
        assert!(Query::new("  ").is_none());
        assert_eq!(Query::new("ZDF ").unwrap().as_str(), "ZDF ");
    }

    #[test]
    fn find_forward_backward_wrap() {
        let hits = [false, true, false, true, false];
        let m = |p: usize| hits[p];
        assert_eq!(
            find(5, 0, true, m),
            Some(Hit {
                pos: 1,
                wrapped: false
            })
        );
        assert_eq!(
            find(5, 1, true, m),
            Some(Hit {
                pos: 1,
                wrapped: false
            })
        );
        assert_eq!(
            find(5, 4, true, m),
            Some(Hit {
                pos: 1,
                wrapped: true
            })
        );
        assert_eq!(
            find(5, 2, false, m),
            Some(Hit {
                pos: 1,
                wrapped: false
            })
        );
        assert_eq!(
            find(5, 0, false, m),
            Some(Hit {
                pos: 3,
                wrapped: true
            })
        );
        assert_eq!(
            find(5, 7, true, m),
            Some(Hit {
                pos: 3,
                wrapped: false
            })
        ); // start wraps
        assert_eq!(find(5, 0, true, |_| false), None);
        assert_eq!(find(0, 0, true, |_| true), None);
    }
}
