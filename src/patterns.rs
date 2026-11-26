//! Text pattern detectors for single sentences.
//!
//! Each detector receives one sentence of visible text (atoms already
//! replaced by a placeholder) and returns hits with byte ranges relative to
//! that text. Detectors only flag; none rewrites text (PASTI-LINT-006).

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::LazyLock;

use regex::Regex;

use crate::diagnostic::{Autofix, Certainty, Method, Outcome};
use crate::glossary::words;

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub range: Range<usize>,
    pub outcome: Outcome,
    pub certainty: Certainty,
    pub method: Method,
    pub message: String,
    pub reason: &'static str,
    pub autofix: Autofix,
}

/// PASTI-GRA-001 pairs with no alternative reading as whole words:
/// prepositions written attached, and passive `di-` written apart.
/// `keluar`/`ke luar` is deliberately absent: both are valid (Doc 04 §3.1).
pub const SPELLING_PAIRS: &[(&str, &str)] = &[
    ("disini", "di sini"),
    ("disana", "di sana"),
    ("disitu", "di situ"),
    ("dimana", "di mana"),
    ("kemana", "ke mana"),
    ("darimana", "dari mana"),
    ("kesini", "ke sini"),
    ("kesana", "ke sana"),
    ("kesitu", "ke situ"),
    ("diatas", "di atas"),
    ("dibawah", "di bawah"),
    ("didalam", "di dalam"),
    ("diluar", "di luar"),
    ("didepan", "di depan"),
    ("dibelakang", "di belakang"),
    ("disamping", "di samping"),
    ("diantara", "di antara"),
    ("diantaranya", "di antaranya"),
    ("keatas", "ke atas"),
    ("kebawah", "ke bawah"),
    ("kedalam", "ke dalam"),
    ("kebelakang", "ke belakang"),
    ("di pasang", "dipasang"),
    ("di gunakan", "digunakan"),
    ("di lakukan", "dilakukan"),
    ("di periksa", "diperiksa"),
    ("di simpan", "disimpan"),
    ("di hapus", "dihapus"),
    ("di kirim", "dikirim"),
    ("di terima", "diterima"),
    ("di ganti", "diganti"),
    ("di pilih", "dipilih"),
    ("di tekan", "ditekan"),
    ("di jalankan", "dijalankan"),
    ("di matikan", "dimatikan"),
    ("di nyalakan", "dinyalakan"),
    ("di atur", "diatur"),
    ("di ukur", "diukur"),
    ("di catat", "dicatat"),
    ("di bersihkan", "dibersihkan"),
    ("di perbaiki", "diperbaiki"),
    ("di lepas", "dilepas"),
    ("di klik", "diklik"),
    ("di buka", "dibuka"),
    ("di pastikan", "dipastikan"),
    ("di tambahkan", "ditambahkan"),
    ("di hubungkan", "dihubungkan"),
    ("di sambungkan", "disambungkan"),
];

/// Unit symbols with case as published (BIPM SI Brochure and common data units).
/// Multi-letter symbols give high certainty when glued to a number; single
/// letters (`m`, `s`, `A`…) also appear in product codes, so they give medium.
pub const UNITS: &[&str] = &[
    "kg", "mg", "µg", "g", "km", "cm", "mm", "µm", "nm", "m", "ms", "µs", "ns", "s", "min", "h", "mL", "ml", "kL", "L",
    "l", "kPa", "MPa", "hPa", "Pa", "bar", "mbar", "kV", "mV", "V", "mA", "A", "kW", "MW", "W", "kWh", "Wh", "Hz",
    "kHz", "MHz", "GHz", "°C", "K", "N", "kN", "Nm", "J", "kJ", "MJ", "dB", "mol", "lx", "rpm", "psi", "kB", "MB",
    "GB", "TB",
];

const EDGE_PUNCT: &[char] = &[
    ',', '.', ';', ':', '!', '?', ')', '(', '"', '\'', '“', '”', '‘', '’', '[', ']',
];

/// Whitespace-separated tokens with edge punctuation trimmed.
fn tokens(text: &str) -> Vec<(Range<usize>, &str)> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices().chain(std::iter::once((text.len(), ' '))) {
        if c.is_whitespace() {
            if let Some(s) = start.take() {
                let raw = &text[s..i];
                let mut lead = raw.len() - raw.trim_start_matches(EDGE_PUNCT).len();
                // Keep a leading decimal separator (`,5`) so a missing zero is visible.
                if lead > 0
                    && raw[..lead].ends_with([',', '.'])
                    && raw[lead..].starts_with(|c: char| c.is_ascii_digit())
                {
                    lead -= 1;
                }
                let t = raw[lead..].trim_end_matches(EDGE_PUNCT);
                if !t.is_empty() {
                    out.push((s + lead..s + lead + t.len(), t));
                }
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    out
}

static NUM_UNIT_GLUED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(-?\d+(?:[.,]\d+)?)(\D.*)$").unwrap());
static NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^-?\d+(?:[.,]\d+)?$").unwrap());
static DOT_DECIMAL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^-?\d+\.\d+$").unwrap());
static COMMA_GROUP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^-?\d{1,3},\d{3}$").unwrap());
static MISSING_ZERO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^-?[.,]\d+$").unwrap());

/// PASTI-NUM-002: a value and its unit symbol must be separated by a space
/// (`80 °C`, `5 kg`), and the symbol keeps its published case.
pub fn unit_symbols(text: &str) -> Vec<Hit> {
    let mut hits = Vec::new();
    let toks = tokens(text);
    for (i, (r, t)) in toks.iter().enumerate() {
        if let Some(c) = NUM_UNIT_GLUED.captures(t)
            && let Some(unit) = UNITS.iter().find(|u| **u == &c[2])
        {
            let multi = unit.chars().count() > 1;
            hits.push(Hit {
                range: r.clone(),
                outcome: if multi {
                    Outcome::Violation
                } else {
                    Outcome::PossibleViolation
                },
                certainty: if multi { Certainty::High } else { Certainty::Medium },
                method: Method::Deterministic,
                message: format!(
                    "«{t}»: pisahkan angka dan lambang satuan dengan spasi («{} {unit}»){}.",
                    &c[1],
                    if multi {
                        ""
                    } else {
                        "; pastikan ini memang satuan, bukan kode"
                    }
                ),
                reason: "NUM_UNIT_NO_SPACE",
                autofix: Autofix::ReviewRequired,
            });
            continue;
        }
        // Case check on `<number> <unit>`.
        if NUMBER.is_match(t)
            && let Some((ur, u)) = toks.get(i + 1)
            && !UNITS.contains(u)
            && u.chars().count() > 1
        {
            let lower = u.to_lowercase();
            let candidates: Vec<&&str> = UNITS.iter().filter(|x| x.to_lowercase() == lower).collect();
            if candidates.len() == 1 {
                hits.push(Hit {
                    range: ur.clone(),
                    outcome: Outcome::PossibleViolation,
                    certainty: Certainty::Medium,
                    method: Method::Deterministic,
                    message: format!(
                        "Lambang satuan «{u}» tidak sesuai kapitalisasi terbitan; kemungkinan maksudnya «{}». Huruf besar/kecil dapat mengubah arti awalan.",
                        candidates[0]
                    ),
                    reason: "NUM_UNIT_CASE",
                    autofix: Autofix::ReviewRequired,
                });
            }
        }
    }
    hits
}

/// PASTI-NUM-001: Indonesian prose uses a decimal comma and a leading zero.
/// Separators that could mean thousands or decimals are not guessed.
pub fn number_format(text: &str) -> Vec<Hit> {
    let mut hits = Vec::new();
    for (r, t) in tokens(text) {
        let (outcome, certainty, reason, message) = if MISSING_ZERO.is_match(t) {
            (
                Outcome::Violation,
                Certainty::High,
                "NUM_MISSING_LEADING_ZERO",
                format!(
                    "«{t}»: tulis nol sebelum pecahan, misalnya «0,{}».",
                    &t[t.find(['.', ',']).unwrap() + 1..]
                ),
            )
        } else if DOT_DECIMAL.is_match(t) {
            let frac = &t[t.find('.').unwrap() + 1..];
            if frac.len() == 3 {
                (
                    Outcome::PossibleViolation,
                    Certainty::Low,
                    "NUM_AMBIGUOUS_SEPARATOR",
                    format!(
                        "«{t}» dapat berarti ribuan (pemisah titik) atau desimal (titik); nyatakan kebijakan pemisah pada profil atau tulis tanpa pemisah ribuan."
                    ),
                )
            } else {
                (
                    Outcome::PossibleViolation,
                    Certainty::Medium,
                    "NUM_DECIMAL_POINT",
                    format!(
                        "«{t}»: profil bahasa Indonesia memakai koma desimal («{}»). Jika titik disyaratkan sumber atau kode, tandai sebagai kode.",
                        t.replace('.', ",")
                    ),
                )
            }
        } else if COMMA_GROUP.is_match(t) {
            (
                Outcome::PossibleViolation,
                Certainty::Low,
                "NUM_AMBIGUOUS_SEPARATOR",
                format!(
                    "«{t}» dapat berarti desimal (koma) atau ribuan (pemisah koma lintas locale); pastikan maksud sumber."
                ),
            )
        } else {
            continue;
        };
        hits.push(Hit {
            range: r,
            outcome,
            certainty,
            method: Method::Deterministic,
            message,
            reason,
            autofix: Autofix::Forbidden,
        });
    }
    hits
}

/// Comparison phrases that do not say whether the bound is strict or
/// inclusive. Mapped phrases (Doc 06 NUM-003, Doc 10) are never flagged.
pub const UNMAPPED_COMPARISONS: &[&str] = &[
    "di atas",
    "di bawah",
    "minimal",
    "maksimal",
    "minimum",
    "maksimum",
    "tidak lebih dari",
    "tidak kurang dari",
    "paling sedikit",
    "setidaknya",
    "sekurangnya",
];

/// PASTI-NUM-003: a numeric bound must use a phrase whose strictness is defined.
pub fn comparisons(text: &str, defined: &[String]) -> Vec<Hit> {
    let ws = words(text);
    let lower: Vec<String> = ws.iter().map(|r| text[r.clone()].to_lowercase()).collect();
    let mut hits = Vec::new();
    let mut i = 0;
    while i < ws.len() {
        let mut advanced = false;
        for phrase in UNMAPPED_COMPARISONS {
            if defined.iter().any(|d| d.eq_ignore_ascii_case(phrase)) {
                continue;
            }
            let parts: Vec<&str> = phrase.split(' ').collect();
            let n = parts.len();
            if i + n < ws.len()
                && lower[i..i + n].iter().zip(&parts).all(|(a, b)| a == b)
                && ws[i + n].start > ws[i + n - 1].end
                && text[ws[i + n].clone()].starts_with(|c: char| c.is_ascii_digit())
                && text[ws[i + n - 1].end..ws[i + n].start]
                    .chars()
                    .all(|c| c.is_whitespace() || c == '-')
            {
                let r = ws[i].start..ws[i + n].end;
                hits.push(Hit {
                    range: r.clone(),
                    outcome: Outcome::PossibleViolation,
                    certainty: Certainty::Medium,
                    method: Method::Heuristic,
                    message: format!(
                        "«{}»: ungkapan batas belum dipetakan ke batas ketat atau termasuk. Gunakan «lebih dari» (>), «sekurang-kurangnya» (≥), «kurang dari» (<), atau «paling banyak» (≤) sesuai sumber, atau definisikan ungkapan ini pada profil.",
                        &text[r]
                    ),
                    reason: "NUM_UNMAPPED_COMPARISON",
                    autofix: Autofix::Forbidden,
                });
                i += n;
                advanced = true;
                break;
            }
        }
        if !advanced {
            i += 1;
        }
    }
    hits
}

static RANGE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[\s(])(-?\d+(?:,\d+)?)(\s*(?:°C|[A-Za-zµ°%]+))?\s*(?:[–—]|-|\s(?:s\.d\.|sampai dengan|sampai|hingga)\s)\s*(-?\d+(?:,\d+)?)(\s*(?:°C|%|[A-Za-zµ]+))?")
        .unwrap()
});

/// Words that introduce references (`langkah 4–6`), not measured ranges.
const REFERENCE_WORDS: &[&str] = &[
    "langkah", "halaman", "hlm", "bab", "bagian", "baris", "nomor", "no", "tabel", "gambar", "butir", "poin", "pasal",
    "ayat", "kolom", "tahun",
];

/// PASTI-NUM-004: a range must state both bounds, the unit, and whether the
/// bounds are included. `in_table` lowers certainty: a table note may define it.
pub fn ranges(text: &str, in_table: bool) -> Vec<Hit> {
    let mut hits = Vec::new();
    for c in RANGE_RE.captures_iter(text) {
        let whole = c.get(0).unwrap();
        let start = c.get(1).unwrap().start();
        let end = whole.end();
        // A date-like or identifier continuation (`26-11-2026`, `10-20/x`) is not a range.
        if text[end..].starts_with(['-', '/', '.']) && text[end..].chars().nth(1).is_some_and(|ch| ch.is_ascii_digit())
        {
            continue;
        }
        let prev_word = words(&text[..start]).last().map(|r| text[r.clone()].to_lowercase());
        if prev_word.as_deref().is_some_and(|w| REFERENCE_WORDS.contains(&w)) {
            continue;
        }
        let unit_unknown = c.get(2).is_none() && c.get(4).is_none();
        hits.push(Hit {
            range: start..end,
            outcome: Outcome::PossibleViolation,
            certainty: if in_table { Certainty::Low } else { Certainty::Medium },
            method: Method::Heuristic,
            message: format!(
                "Rentang «{}» belum menyatakan apakah kedua batas termasuk{}; misalnya tulis «sekurang-kurangnya … dan paling banyak …» untuk rentang tertutup, atau jelaskan notasinya.",
                &text[start..end],
                if unit_unknown { " dan satuannya" } else { "" }
            ),
            reason: "NUM_RANGE_INCLUSIVITY",
            autofix: Autofix::Forbidden,
        });
    }
    hits
}

/// PASTI-GRA-001 against the built-in list plus project pairs.
pub fn spelling(text: &str, extra: &[(String, String)]) -> Vec<Hit> {
    let mut table: BTreeMap<Vec<String>, String> = BTreeMap::new();
    for (w, r) in SPELLING_PAIRS
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .chain(extra.iter().cloned())
    {
        table.insert(w.split_whitespace().map(str::to_lowercase).collect(), r);
    }
    let max = table.keys().map(Vec::len).max().unwrap_or(1);
    let ws = words(text);
    let mut hits = Vec::new();
    let mut i = 0;
    while i < ws.len() {
        let mut n = max.min(ws.len() - i);
        let mut matched = false;
        while n > 0 {
            let spaced = ws[i..i + n].windows(2).all(|w| &text[w[0].end..w[1].start] == " ");
            let key: Vec<String> = ws[i..i + n].iter().map(|r| text[r.clone()].to_lowercase()).collect();
            if spaced && let Some(right) = table.get(&key) {
                let r = ws[i].start..ws[i + n - 1].end;
                hits.push(Hit {
                    range: r.clone(),
                    outcome: Outcome::Violation,
                    certainty: Certainty::High,
                    method: Method::Deterministic,
                    message: format!(
                        "«{}»: tulis «{right}» (imbuhan serangkai, kata depan terpisah; EYD V).",
                        &text[r]
                    ),
                    reason: "GRA_AFFIX_PREPOSITION",
                    autofix: Autofix::ReviewRequired,
                });
                i += n;
                matched = true;
                break;
            }
            n -= 1;
        }
        if !matched {
            i += 1;
        }
    }
    hits
}

static NUMERAL_REDUP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\p{Ll}{2,}2$").unwrap());

/// PASTI-GRA-011: reduplication is written in full with a hyphen
/// (`kabel-kabel`), not with `2` or as two separate words.
pub fn reduplication(text: &str) -> Vec<Hit> {
    let mut hits = Vec::new();
    for (r, t) in tokens(text) {
        if NUMERAL_REDUP.is_match(t) {
            let base = &t[..t.len() - 1];
            hits.push(Hit {
                range: r,
                outcome: Outcome::PossibleViolation,
                certainty: Certainty::Medium,
                method: Method::Deterministic,
                message: format!("«{t}»: tulis bentuk ulang lengkap dengan tanda hubung, «{base}-{base}», jika ini memang bentuk ulang."),
                reason: "GRA_REDUP_NUMERAL",
                autofix: Autofix::ReviewRequired,
            });
        }
    }
    let ws = words(text);
    let mut last_end = 0;
    for w in ws.windows(2) {
        let (a, b) = (&text[w[0].clone()], &text[w[1].clone()]);
        if &text[w[0].end..w[1].start] == " "
            && w[0].start >= last_end
            && a.chars().count() >= 2
            && a.chars().all(char::is_alphabetic)
            && a.to_lowercase() == b.to_lowercase()
        {
            last_end = w[1].end;
            hits.push(Hit {
                range: w[0].start..w[1].end,
                outcome: Outcome::PossibleViolation,
                certainty: Certainty::Medium,
                method: Method::Deterministic,
                message: format!("«{a} {b}»: bentuk ulang ditulis dengan tanda hubung («{a}-{b}»); jika kata terulang tidak sengaja, hapus salah satunya."),
                reason: "GRA_REDUP_SPACED",
                autofix: Autofix::ReviewRequired,
            });
        }
    }
    hits.sort_by_key(|h| h.range.start);
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reasons(h: &[Hit]) -> Vec<&'static str> {
        h.iter().map(|h| h.reason).collect()
    }

    #[test]
    fn units_glued_and_case() {
        let h = unit_symbols("Jika suhu melebihi 80°C, tambah 5kg dan 2m.");
        assert_eq!(reasons(&h), ["NUM_UNIT_NO_SPACE"; 3]);
        assert_eq!(h[0].certainty, Certainty::High);
        assert_eq!(h[2].certainty, Certainty::Medium);
        assert!(unit_symbols("Atur ke 80 °C dan 5 kg, sudut 30°, naik 5%.").is_empty());
        assert_eq!(reasons(&unit_symbols("Tambah 5 KG dan 2 Kpa.")), ["NUM_UNIT_CASE"; 2]);
        // Product codes are not units: `4G`, `3D`, `x86`.
        assert!(unit_symbols("Modul 4G dan cetak 3D pada x86.").is_empty());
    }

    #[test]
    fn number_formats() {
        assert_eq!(
            reasons(&number_format("Tambahkan 2.5 mL lalu ,5 mL dan 1.000 g.")),
            [
                "NUM_DECIMAL_POINT",
                "NUM_MISSING_LEADING_ZERO",
                "NUM_AMBIGUOUS_SEPARATOR"
            ]
        );
        assert!(number_format("Pasang v1.2 dengan 0,5 mL dan versi 1.2.3.").is_empty());
    }

    #[test]
    fn unmapped_comparisons_only_before_numbers() {
        let h = comparisons("Jika suhu di atas 80 °C, tunggu minimal 5 menit.", &[]);
        assert_eq!(h.len(), 2);
        assert!(comparisons("Letakkan di atas meja. Jika suhu lebih dari 80 °C, berhenti.", &[]).is_empty());
        assert_eq!(comparisons("tunggu minimal 5 menit", &["minimal".into()]).len(), 0);
    }

    #[test]
    fn ranges_skip_references_and_dates() {
        assert_eq!(ranges("Jaga suhu 10–20 °C.", false).len(), 1);
        assert_eq!(ranges("Jaga suhu 10 sampai 20 °C.", false).len(), 1);
        assert!(ranges("Jika indikator MERAH menyala, lakukan langkah 4–6.", false).is_empty());
        assert!(ranges("Tanggal 26-11-2026.", false).is_empty());
        assert!(ranges("Katup V-01 terbuka.", false).is_empty());
    }

    #[test]
    fn spelling_pairs_whole_words_only() {
        let h = spelling("Sensor di pasang disini, bukan dikeluarkan.", &[]);
        assert_eq!(h.len(), 2);
        assert!(h[0].message.contains("dipasang"));
        assert!(spelling("Pasang sensor di panel. Simpan berkas di direktori kerja.", &[]).is_empty());
        let extra = vec![("dipanel".to_string(), "di panel".to_string())];
        assert_eq!(spelling("Pasang sensor dipanel.", &extra).len(), 1);
    }

    #[test]
    fn reduplication_forms() {
        assert_eq!(
            reasons(&reduplication("Periksa kabel2 dan baut baut.")),
            ["GRA_REDUP_NUMERAL", "GRA_REDUP_SPACED"]
        );
        assert!(reduplication("Periksa sensor-sensor dan IPv2 serta mp3.").is_empty());
    }
}
