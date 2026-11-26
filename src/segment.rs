//! Reproducible segmentation and counting (Doc 06 §3, PASTI-TOK-002/003).
//!
//! Works on a block's visible text, where atoms are already single
//! placeholder characters. Offsets are visible-text byte offsets; callers map
//! them back to source with [`crate::document::Block::src_range`].

use std::ops::Range;

use crate::document::ATOM;

/// Unit symbols that count as a unit even without letters or digits.
const UNIT_SYMBOLS: &[char] = &['%', '‰', '°', '′', '″', 'Ω', 'µ', 'μ'];
const CLOSERS: &[char] = &['"', '\'', ')', ']', '}', '”', '’', '»', '›'];
const OPENERS: &[char] = &['"', '\'', '(', '[', '{', '“', '‘', '«', '‹'];

/// Registered abbreviations, compared case-insensitively including the final dot.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Abbreviations {
    /// Never end a sentence mid-block (e.g. `No.`).
    pub non_terminal: Vec<String>,
    /// May or may not end a sentence (e.g. `dll.`); never split, and the
    /// uncertainty is reported (PASTI-TOK-002).
    pub ambiguous: Vec<String>,
}

impl Default for Abbreviations {
    fn default() -> Self {
        let v = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect();
        Abbreviations {
            non_terminal: v(&[
                "No.", "Hlm.", "Jl.", "Bpk.", "Dr.", "Prof.", "Ir.", "mis.", "Yth.", "a.n.", "u.p.", "s.d.", "Telp.",
                "Gbr.", "Tab.", "Vol.", "Ref.", "Rev.", "Ver.", "Kol.",
            ]),
            ambiguous: v(&["dll.", "dsb.", "dst.", "dkk.", "tsb.", "etc."]),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sentence {
    pub vis: Range<usize>,
    /// Ends with `.`, `?` or `!`; otherwise it is a block-final fragment.
    pub terminated: bool,
    /// Counted units (PASTI-TOK-003), in order.
    pub units: Vec<Range<usize>>,
    /// Ambiguous abbreviations inside the sentence that were not split.
    pub ambiguous_abbrevs: Vec<Range<usize>>,
}

pub fn sentences(text: &str, abbr: &Abbreviations) -> Vec<Sentence> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut ambiguous = Vec::new();
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (pos, c) = chars[i];
        let next = chars.get(i + 1).map(|&(_, c)| c);
        let is_end = match c {
            '?' | '!' => next.is_none_or(|n| n.is_whitespace() || CLOSERS.contains(&n)),
            '.' => {
                let prev = i.checked_sub(1).map(|j| chars[j].1);
                let followed_ok = next.is_none_or(|n| n.is_whitespace() || CLOSERS.contains(&n));
                let ellipsis = prev == Some('.') || next == Some('.');
                if !followed_ok || ellipsis {
                    false
                } else {
                    let token = token_before(text, pos + 1);
                    if contains_ci(&abbr.non_terminal, token) {
                        false
                    } else if contains_ci(&abbr.ambiguous, token) {
                        let rest_blank = text[pos + 1..].trim_start_matches(CLOSERS).trim().is_empty();
                        if !rest_blank {
                            ambiguous.push(pos + 1 - token.len()..pos + 1);
                        }
                        false
                    } else {
                        true
                    }
                }
            }
            _ => false,
        };
        i += 1;
        if is_end {
            // Closing quotes and brackets after the terminator stay with it.
            while i < chars.len() && CLOSERS.contains(&chars[i].1) {
                i += 1;
            }
            let end = chars.get(i).map_or(text.len(), |&(p, _)| p);
            push_sentence(&mut out, text, start..end, true, std::mem::take(&mut ambiguous));
            start = end;
        }
    }
    if start < text.len() {
        let terminated = text[start..].trim_end().ends_with(['.', '?', '!'])
            || text[start..]
                .trim_end()
                .trim_end_matches(CLOSERS)
                .ends_with(['.', '?', '!']);
        push_sentence(&mut out, text, start..text.len(), terminated, ambiguous);
    }
    out
}

fn push_sentence(
    out: &mut Vec<Sentence>,
    text: &str,
    r: Range<usize>,
    terminated: bool,
    ambiguous_abbrevs: Vec<Range<usize>>,
) {
    let slice = &text[r.clone()];
    let lead = slice.len() - slice.trim_start().len();
    let trail = slice.len() - slice.trim_end().len();
    if lead == slice.len() {
        return;
    }
    let vis = r.start + lead..r.end - trail;
    let units = units(text, vis.clone());
    out.push(Sentence {
        vis,
        terminated,
        units,
        ambiguous_abbrevs,
    });
}

/// Whitespace-separated tokens in `range` that count as units: they contain a
/// letter, a digit, an atom, or a recognised unit symbol. Standalone
/// punctuation and operators (`>`, `≤`, `±`, `–`) do not count.
pub fn units(text: &str, range: Range<usize>) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut tok_start: Option<usize> = None;
    let slice = &text[range.clone()];
    for (i, c) in slice.char_indices().chain(std::iter::once((slice.len(), ' '))) {
        if c.is_whitespace() {
            if let Some(s) = tok_start.take() {
                let tok = &slice[s..i];
                if tok
                    .chars()
                    .any(|c| c.is_alphanumeric() || c == ATOM || UNIT_SYMBOLS.contains(&c))
                {
                    out.push(range.start + s..range.start + i);
                }
            }
        } else if tok_start.is_none() {
            tok_start = Some(i);
        }
    }
    out
}

/// The whitespace-delimited token ending at `end`, without opening punctuation.
fn token_before(text: &str, end: usize) -> &str {
    let start = text[..end]
        .rfind(char::is_whitespace)
        .map_or(0, |p| p + text[p..].chars().next().unwrap().len_utf8());
    text[start..end].trim_start_matches(OPENERS)
}

fn contains_ci(list: &[String], token: &str) -> bool {
    !token.is_empty()
        && list
            .iter()
            .any(|a| a.eq_ignore_ascii_case(token) || a.to_lowercase() == token.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Mode;
    use crate::document::{InputFormat, parse};

    fn count(md: &str) -> Vec<usize> {
        let d = parse(md, InputFormat::Markdown, Mode::PROC).unwrap();
        let abbr = Abbreviations::default();
        d.blocks
            .iter()
            .flat_map(|b| sentences(&b.text, &abbr))
            .map(|s| s.units.len())
            .collect()
    }

    #[test]
    fn doc06_binding_counts() {
        // Doc 06 §4.1, each line one sentence.
        let cases = [
            ("Periksa sensor-sensor itu.", 3),
            ("Periksa katupnya.", 2),
            ("Atur batas waktu menjadi 2,5 detik.", 6),
            ("Jika suhu melebihi 80 °C, matikan pompa.", 7),
            ("Jika suhu melebihi 80°C, matikan pompa.", 6),
            ("Klik “Mulai Ulang”.", 3),
            ("Gunakan batas (80 °C).", 4),
            ("Jalankan `tool --mode cepat`.", 2),
            ("No. komponen adalah 12.", 4),
        ];
        for (text, n) in cases {
            assert_eq!(count(text), vec![n], "{text}");
        }
    }

    #[test]
    fn doc06_exact_boundaries() {
        let proc20 = "Jika panel utama sudah dingin dan sumber daya sudah terputus, lepaskan penutup bagian atas dengan alat yang ditentukan dalam prosedur.";
        assert_eq!(count(proc20), vec![20]);
        assert_eq!(count(&proc20.replace("prosedur.", "prosedur ini.")), vec![21]);
        let desc25 = "Pompa utama mengalirkan air dari tangki penyimpanan menuju pipa distribusi ketika katup masuk terbuka dan sensor tekanan menunjukkan nilai dalam batas operasi yang telah ditetapkan.";
        assert_eq!(count(desc25), vec![25]);
        assert_eq!(
            count(&desc25.replace("ditetapkan.", "ditetapkan sebelumnya.")),
            vec![26]
        );
    }

    #[test]
    fn numbers_versions_and_ellipsis_do_not_split() {
        // `tunggu...` keeps the ellipsis inside one sentence.
        assert_eq!(count("Pasang v1.2 pada 2.5 detik lalu tunggu... Selesai."), vec![8]);
        assert_eq!(count("Apa ini? Ya!"), vec![2, 1]);
    }

    #[test]
    fn whitespace_kinds_and_standalone_operators() {
        // NBSP and narrow NBSP separate units; `>` and `±` alone do not count;
        // `%` alone counts; `10 000` is two units.
        assert_eq!(count("Nilai\u{00A0}>\u{202F}10 000 ± 5 %."), vec![5]);
    }

    #[test]
    fn closing_quote_stays_with_sentence_and_ambiguous_abbrev_reported() {
        let abbr = Abbreviations::default();
        let s = sentences("Klik “OK.” Lalu tutup.", &abbr);
        assert_eq!(s.len(), 2);
        assert_eq!(&"Klik “OK.” Lalu tutup."[s[0].vis.clone()], "Klik “OK.”");
        let s = sentences("Siapkan kabel, baut, dll. Lalu pasang.", &abbr);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].ambiguous_abbrevs.len(), 1);
    }
}
