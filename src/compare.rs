//! Rewrite comparison (Doc 07): checks whether a candidate rewrite keeps the
//! protected meaning elements of its source (PASTI-SEM-001…004, NUM-006).
//!
//! Every automated finding is a `possible_violation`: a candidate for review,
//! never the final verdict (Doc 09 §4.2 keeps manual verdicts separate).
//! Literal differences in marker, value, unit, and operator multisets are
//! deterministic/high; condition-scope alignment after a sentence split is
//! heuristic/medium. Nothing here rewrites text, so autofix is always
//! forbidden.
//!
//! Classification choices:
//! - `belum` is classified only as an aspect marker (PASTI-SEM-003), never
//!   additionally as negation, so one word yields at most one finding.
//!   Replacing `belum` with `tidak` therefore shows up as an aspect marker
//!   removed plus a negation marker added.
//! - Markers are compared by meaning class, not spelling: `harus`→`wajib`
//!   (both obligation) or `jika`→`apabila` (both condition) is the "clearer
//!   form" Doc 07 PASTI-SEM-002 allows; a change of class is reported.
//! - Multiword phrases match longest-first across all categories, so
//!   `tidak wajib` is one modality marker, not negation plus obligation.

use std::collections::{BTreeSet, HashSet};

use crate::coords::SourceMap;
use crate::diagnostic::{Autofix, Certainty, Diagnostic, Finding, Method, Mode, Outcome, Range, sha256_hex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cat {
    Modality,
    Aspect,
    Scope,
    Comparison,
}

impl Cat {
    fn rule(self) -> &'static str {
        match self {
            Cat::Modality => "PASTI-SEM-002",
            Cat::Aspect => "PASTI-SEM-003",
            Cat::Scope => "PASTI-SEM-004",
            Cat::Comparison => "PASTI-SEM-001",
        }
    }

    fn category(self) -> &'static str {
        match self {
            Cat::Modality => "meaning.modality",
            Cat::Aspect => "meaning.aspect",
            Cat::Scope => "meaning.scope",
            Cat::Comparison => "meaning.comparison",
        }
    }

    fn reasons(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Cat::Comparison => ("OPERATOR_REMOVED", "OPERATOR_ADDED", "OPERATOR_CHANGED"),
            _ => ("MARKER_REMOVED", "MARKER_ADDED", "MARKER_CHANGED"),
        }
    }

    fn removed_action(self) -> &'static str {
        match self {
            Cat::Modality => "Pertahankan modalitas sumber atau telaah dasar perubahannya.",
            Cat::Aspect => "Telaah apakah konteks yang tersisa menyatakan makna yang sama.",
            Cat::Scope => "Telaah dan pertahankan cakupan sumber.",
            Cat::Comparison => "Pertahankan operator batas sumber.",
        }
    }

    fn added_effect(self) -> &'static str {
        match self {
            Cat::Modality => "tingkat kewajiban atau kepastian dapat berubah",
            Cat::Aspect => "status waktu atau keadaan dapat berubah",
            Cat::Scope => "jangkauan negasi, syarat, atau cakupan dapat berubah",
            Cat::Comparison => "batas perbandingan baru dapat mengubah keputusan",
        }
    }

    fn changed_effect(self) -> &'static str {
        match self {
            Cat::Modality => {
                "jenis modalitas berubah; pertahankan modalitas sumber kecuali perubahan kebijakan disahkan terpisah."
            }
            Cat::Aspect => {
                "aspek atau hubungan waktu berubah; telaah apakah perbedaan itu memengaruhi tindakan atau klaim."
            }
            Cat::Scope => "jangkauan negasi, syarat, atau pengecualian berubah; telaah dan pertahankan cakupan sumber.",
            Cat::Comparison => {
                "operator batas berubah sehingga nilai tepat pada batas dapat menghasilkan keputusan berbeda (PASTI-NUM-003); pertahankan operator sumber."
            }
        }
    }
}

/// A meaning class: markers in the same class are interchangeable forms.
#[derive(Debug)]
struct Class {
    key: &'static str,
    cat: Cat,
    label: &'static str,
    /// Consequence when the marker disappears.
    removed_effect: &'static str,
}

const fn class(key: &'static str, cat: Cat, label: &'static str, removed_effect: &'static str) -> Class {
    Class {
        key,
        cat,
        label,
        removed_effect,
    }
}

static CLASSES: &[Class] = &[
    class(
        "possibility",
        Cat::Modality,
        "ketidakpastian",
        "dugaan dapat berubah menjadi fakta",
    ),
    class(
        "obligation",
        Cat::Modality,
        "kewajiban",
        "kewajiban dapat hilang atau melemah",
    ),
    class(
        "permission",
        Cat::Modality,
        "izin",
        "izin dapat hilang atau berubah jenis",
    ),
    class(
        "ability",
        Cat::Modality,
        "kemampuan",
        "pernyataan kemampuan dapat hilang",
    ),
    class(
        "ability_or_possibility",
        Cat::Modality,
        "kemampuan atau kemungkinan",
        "kemampuan atau kemungkinan dapat berubah menjadi kepastian",
    ),
    class("advice", Cat::Modality, "saran", "tingkat saran dapat hilang"),
    class(
        "no_obligation",
        Cat::Modality,
        "ketiadaan kewajiban",
        "ketiadaan kewajiban dapat berubah menjadi pernyataan lain",
    ),
    class("prohibition", Cat::Modality, "larangan", "larangan dapat hilang"),
    class(
        "perfect",
        Cat::Aspect,
        "aspek selesai",
        "status yang telah tercapai dapat hilang",
    ),
    class(
        "progressive",
        Cat::Aspect,
        "aspek berlangsung",
        "keterangan proses yang sedang berlangsung dapat hilang",
    ),
    class(
        "continuative",
        Cat::Aspect,
        "aspek berlanjut",
        "keterangan keadaan yang masih berlanjut dapat hilang",
    ),
    class(
        "not_yet",
        Cat::Aspect,
        "aspek belum tercapai",
        "status sementara dapat kehilangan batas waktunya",
    ),
    class(
        "future",
        Cat::Aspect,
        "waktu mendatang",
        "kejadian mendatang dapat berubah menjadi keadaan sekarang",
    ),
    class(
        "completed",
        Cat::Aspect,
        "keadaan selesai",
        "keadaan selesai dapat hilang",
    ),
    class(
        "success",
        Cat::Aspect,
        "keadaan berhasil",
        "klaim hasil berhasil dapat hilang",
    ),
    class(
        "negation",
        Cat::Scope,
        "negasi",
        "negasi dapat hilang sehingga makna berbalik",
    ),
    class(
        "nominal_negation",
        Cat::Scope,
        "negasi nomina",
        "negasi dapat hilang sehingga makna berbalik",
    ),
    class(
        "prohibitive",
        Cat::Scope,
        "larangan",
        "larangan dapat berubah menjadi perintah",
    ),
    class(
        "condition",
        Cat::Scope,
        "syarat",
        "syarat dapat hilang sehingga tindakan tampak tanpa syarat",
    ),
    class(
        "only_if",
        Cat::Scope,
        "syarat perlu",
        "syarat perlu dapat melemah atau hilang",
    ),
    class(
        "exception",
        Cat::Scope,
        "pengecualian",
        "pengecualian dapat hilang sehingga cakupan tindakan meluas",
    ),
    class(
        "before",
        Cat::Scope,
        "urutan sebelum",
        "urutan atau prasyarat waktu dapat hilang",
    ),
    class(
        "after",
        Cat::Scope,
        "urutan setelah",
        "urutan atau prasyarat penyelesaian dapat hilang",
    ),
    class("until", Cat::Scope, "batas akhir", "kondisi akhir dapat hilang"),
    class(
        "universal",
        Cat::Scope,
        "kuantor menyeluruh",
        "cakupan kuantor dapat berubah",
    ),
    class(
        "not_all",
        Cat::Scope,
        "kuantor sebagian",
        "cakupan kuantor dapat berubah",
    ),
    class(
        "gt",
        Cat::Comparison,
        "batas ketat (>)",
        "batas perbandingan dapat hilang",
    ),
    class(
        "ge",
        Cat::Comparison,
        "batas termasuk (≥)",
        "batas perbandingan dapat hilang",
    ),
    class(
        "lt",
        Cat::Comparison,
        "batas ketat (<)",
        "batas perbandingan dapat hilang",
    ),
    class(
        "le",
        Cat::Comparison,
        "batas termasuk (≤)",
        "batas perbandingan dapat hilang",
    ),
    class("eq", Cat::Comparison, "kesamaan (=)", "batas perbandingan dapat hilang"),
];

/// Lowercase phrase (words joined by one space) or symbol → class key.
static LEXICON: &[(&str, &str)] = &[
    ("mungkin", "possibility"),
    ("barangkali", "possibility"),
    ("kemungkinan", "possibility"),
    ("diduga", "possibility"),
    ("harus", "obligation"),
    ("wajib", "obligation"),
    ("perlu", "obligation"),
    ("boleh", "permission"),
    ("mampu", "ability"),
    ("dapat", "ability_or_possibility"),
    ("bisa", "ability_or_possibility"),
    ("sebaiknya", "advice"),
    ("hendaknya", "advice"),
    ("tidak wajib", "no_obligation"),
    ("tidak perlu", "no_obligation"),
    ("tidak harus", "no_obligation"),
    ("tidak boleh", "prohibition"),
    ("harus tidak", "prohibition"),
    ("sudah", "perfect"),
    ("telah", "perfect"),
    ("sedang", "progressive"),
    ("masih", "continuative"),
    ("belum", "not_yet"),
    ("akan", "future"),
    ("selesai", "completed"),
    ("berhasil", "success"),
    ("tidak", "negation"),
    ("bukan", "nominal_negation"),
    ("jangan", "prohibitive"),
    ("jika", "condition"),
    ("apabila", "condition"),
    ("bila", "condition"),
    ("hanya jika", "only_if"),
    ("kecuali", "exception"),
    ("sebelum", "before"),
    ("setelah", "after"),
    ("sesudah", "after"),
    ("sampai", "until"),
    ("hingga", "until"),
    ("semua", "universal"),
    ("setiap", "universal"),
    ("tidak semua", "not_all"),
    ("lebih dari", "gt"),
    ("lebih besar dari", "gt"),
    ("lebih besar daripada", "gt"),
    ("melebihi", "gt"),
    ("di atas", "gt"),
    (">", "gt"),
    ("kurang dari", "lt"),
    ("lebih kecil dari", "lt"),
    ("lebih kecil daripada", "lt"),
    ("di bawah", "lt"),
    ("<", "lt"),
    ("sekurang-kurangnya", "ge"),
    ("paling sedikit", "ge"),
    ("minimal", "ge"),
    ("setidaknya", "ge"),
    ("atau lebih", "ge"),
    ("tidak kurang dari", "ge"),
    (">=", "ge"),
    ("≥", "ge"),
    ("paling banyak", "le"),
    ("maksimal", "le"),
    ("tidak lebih dari", "le"),
    ("tidak melebihi", "le"),
    ("atau kurang", "le"),
    ("<=", "le"),
    ("≤", "le"),
    ("sama dengan", "eq"),
    ("=", "eq"),
];

const MAX_PHRASE_WORDS: usize = 3;

/// Unit symbols recognised after a number (case-sensitive: capitalisation is
/// part of the symbol, PASTI-NUM-002).
static UNITS: &[&str] = &[
    "%", "‰", "°", "°C", "°F", "K", "Pa", "hPa", "kPa", "MPa", "bar", "mbar", "psi", "mL", "ml", "L", "l", "µL", "μL",
    "dL", "dl", "cL", "cl", "kL", "kl", "g", "mg", "kg", "µg", "μg", "t", "mm", "cm", "m", "km", "µm", "μm", "nm",
    "m²", "cm²", "mm²", "m³", "cm³", "s", "ms", "µs", "μs", "ns", "min", "h", "jam", "menit", "detik", "hari",
    "minggu", "bulan", "tahun", "V", "mV", "kV", "A", "mA", "W", "kW", "MW", "Wh", "kWh", "J", "kJ", "MJ", "Hz", "kHz",
    "MHz", "GHz", "rpm", "N", "kN", "Nm", "dB", "B", "kB", "KB", "MB", "GB", "TB", "bit", "lx", "lux", "cd", "mol",
    "Ω", "kΩ", "MΩ",
];

/// Function words ignored when aligning sentences by content.
static STOPWORDS: &[&str] = &[
    "dan",
    "atau",
    "yang",
    "lalu",
    "kemudian",
    "serta",
    "untuk",
    "dengan",
    "pada",
    "dari",
    "itu",
    "ini",
    "juga",
    "agar",
    "supaya",
    "karena",
    "oleh",
    "dalam",
    "akan",
    "para",
    "tersebut",
    "adalah",
    "ialah",
    "merupakan",
];

/// Registered abbreviations whose period does not end a sentence (TOK-002).
static ABBREVIATIONS: &[&str] = &[
    "no", "dll", "dsb", "dst", "mis", "hlm", "tsb", "yth", "sdr", "dr", "ir", "prof", "nb",
];

/// Condition markers whose scope the shared-condition check follows.
const CONDITION_CLASSES: &[&str] = &["condition", "only_if", "exception"];

fn class_of(key: &str) -> &'static Class {
    CLASSES
        .iter()
        .find(|c| c.key == key)
        .expect("lexicon class is registered")
}

fn lookup(phrase: &str) -> Option<&'static Class> {
    LEXICON.iter().find(|(p, _)| *p == phrase).map(|(_, k)| class_of(k))
}

fn is_single_word_marker(word: &str) -> bool {
    LEXICON.iter().any(|(p, _)| *p == word)
}

// ---------------------------------------------------------------------------
// Tokens and sentences
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokKind {
    /// Letters only (hyphenated compounds included).
    Word,
    /// Contains at least one ASCII digit: a number or identifier.
    Alnum,
    /// Unit or operator symbol.
    Sym,
}

#[derive(Debug, Clone, Copy)]
struct Tok {
    start: usize,
    end: usize,
    kind: TokKind,
}

fn tokenize(text: &str) -> Vec<Tok> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let n = chars.len();
    let byte_at = |k: usize| if k < n { chars[k].0 } else { text.len() };
    let mut toks = Vec::new();
    let mut i = 0;
    while i < n {
        let c = chars[i].1;
        let next = chars.get(i + 1).map(|p| p.1);
        let negative = matches!(c, '-' | '\u{2212}')
            && next.is_some_and(|d| d.is_ascii_digit())
            && (i == 0 || chars[i - 1].1.is_whitespace() || chars[i - 1].1 == '(');
        if c.is_alphanumeric() || negative {
            let mut j = i + 1;
            loop {
                while j < n && chars[j].1.is_alphanumeric() {
                    j += 1;
                }
                // Internal connectors keep 2,5 / v1.2 / 14:02 / sekurang-kurangnya whole.
                if j + 1 < n
                    && matches!(chars[j].1, '-' | '.' | ',' | ':' | '/' | '\'')
                    && chars[j + 1].1.is_alphanumeric()
                {
                    j += 1;
                    continue;
                }
                break;
            }
            let (start, end) = (byte_at(i), byte_at(j));
            let kind = if text[start..end].bytes().any(|b| b.is_ascii_digit()) {
                TokKind::Alnum
            } else {
                TokKind::Word
            };
            toks.push(Tok { start, end, kind });
            i = j;
            continue;
        }
        let len = match (c, next) {
            ('°', Some('C' | 'F')) | ('>' | '<', Some('=')) => 2,
            ('°' | '%' | '‰' | '>' | '<' | '=' | '≥' | '≤', _) => 1,
            _ => 0,
        };
        if len > 0 {
            toks.push(Tok {
                start: byte_at(i),
                end: byte_at(i + len),
                kind: TokKind::Sym,
            });
            i += len;
        } else {
            i += 1;
        }
    }
    toks
}

/// `1.` / `2)` at the start of a line or right after a sentence end.
fn is_list_marker(text: &str, tok: &Tok) -> bool {
    if tok.kind != TokKind::Alnum || !text[tok.start..tok.end].bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let after = &text[tok.end..];
    let mut it = after.chars();
    if !matches!(it.next(), Some('.' | ')')) || !it.next().is_none_or(char::is_whitespace) {
        return false;
    }
    let before = &text[..tok.start];
    let line_prefix = before.rsplit('\n').next().unwrap_or("");
    if line_prefix.trim().is_empty() {
        return true;
    }
    let trimmed = before.trim_end();
    trimmed.len() < before.len() && trimmed.ends_with(['.', ':', '!', '?'])
}

/// Whether a (left-trimmed) line opens a list item: `1. `, `2) `, `- `, `* `, `• `.
fn starts_list_item(s: &str) -> bool {
    let digits = s.bytes().take_while(u8::is_ascii_digit).count();
    let rest = &s[digits..];
    let mut it = rest.chars();
    let marker = if digits > 0 {
        matches!(it.next(), Some('.' | ')'))
    } else {
        matches!(it.next(), Some('-' | '*' | '•'))
    };
    marker && it.next().is_none_or(char::is_whitespace)
}

/// Sentence spans (byte offsets, trimmed). '.', '!' and '?' end a sentence
/// when followed (after closing quotes/brackets) by whitespace or the end,
/// unless they belong to a list marker or a registered abbreviation; a line
/// break ends a sentence before a blank line or a list item.
fn split_sentences(text: &str, list_dots: &HashSet<usize>) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut seg = 0;
    let mut i = 0;
    while i < text.len() {
        let c = text[i..].chars().next().expect("char boundary");
        let clen = c.len_utf8();
        match c {
            '.' | '!' | '?' if !list_dots.contains(&i) => {
                let mut end = i + clen;
                for cc in text[end..].chars() {
                    if matches!(cc, '"' | '\'' | '”' | '’' | ')' | ']' | '»') {
                        end += cc.len_utf8();
                    } else {
                        break;
                    }
                }
                let at_break = text[end..].chars().next().is_none_or(char::is_whitespace);
                let word_before = text[..i].rsplit(|ch: char| !ch.is_alphanumeric()).next().unwrap_or("");
                let abbreviation = c == '.' && ABBREVIATIONS.contains(&word_before.to_lowercase().as_str());
                if at_break && !abbreviation {
                    push_trimmed(text, seg, end, &mut out);
                    seg = end;
                }
                i = end;
                continue;
            }
            '\n' => {
                let next_line = text[i + 1..].split('\n').next().unwrap_or("");
                if next_line.trim().is_empty() || starts_list_item(next_line.trim_start()) {
                    push_trimmed(text, seg, i, &mut out);
                    seg = i + 1;
                }
            }
            _ => {}
        }
        i += clen;
    }
    push_trimmed(text, seg, text.len(), &mut out);
    out
}

fn push_trimmed(text: &str, start: usize, end: usize, out: &mut Vec<(usize, usize)>) {
    if start >= end {
        return;
    }
    let slice = &text[start..end];
    let lead = slice.len() - slice.trim_start().len();
    let t = slice.trim();
    if !t.is_empty() {
        out.push((start + lead, start + lead + t.len()));
    }
}

fn is_number(s: &str) -> bool {
    let digits = s.strip_prefix(['-', '\u{2212}']).unwrap_or(s);
    !digits.is_empty()
        && digits
            .split(['.', ','])
            .all(|g| !g.is_empty() && g.bytes().all(|b| b.is_ascii_digit()))
}

/// Comma is the decimal separator (PASTI-NUM-001). A single dot becomes a
/// decimal comma unless exactly three digits follow it (`1.000` may be a
/// thousands separator, so it is kept literal instead of guessed).
fn normalize_number(s: &str) -> String {
    let s = s.replace('\u{2212}', "-");
    let seps: Vec<usize> = s.match_indices(['.', ',']).map(|(i, _)| i).collect();
    if let [only] = seps[..]
        && s[only..].starts_with('.')
        && s.len() - only - 1 != 3
    {
        return format!("{},{}", &s[..only], &s[only + 1..]);
    }
    s
}

/// SI allows both `l` and `L` for the litre, so litre units compare equal
/// across that case (`ml`≡`mL`, `kl`≡`kL`); every other case difference
/// (e.g. the `M`/`m` prefix) stays significant.
fn normalize_unit(u: &str) -> String {
    match u.strip_suffix(['l', 'L']) {
        Some(prefix @ ("" | "m" | "µ" | "μ" | "c" | "d" | "k")) => format!("{prefix}L"),
        _ => u.to_string(),
    }
}

/// `5kg` → (`5`, `kg`) when the suffix is a known unit.
fn split_attached_unit(s: &str) -> Option<(&str, &str)> {
    let idx = s
        .char_indices()
        .skip(1)
        .find(|(_, c)| c.is_alphabetic())
        .map(|(i, _)| i)?;
    let (num, unit) = s.split_at(idx);
    (is_number(num) && UNITS.contains(&unit)).then_some((num, unit))
}

#[derive(Debug)]
struct Sentence {
    start: usize,
    end: usize,
    list_item: bool,
    content: BTreeSet<String>,
    has_condition: bool,
    /// Ends with ':' or mentions `berikut`: introduces items that inherit its
    /// condition (Doc 07 §3 "pengantar daftar").
    carrier: bool,
}

#[derive(Debug)]
struct Marker {
    class: &'static Class,
    start: usize,
    end: usize,
    sent: usize,
}

#[derive(Debug)]
struct Value {
    /// Normalised `value unit` used for multiset comparison.
    key: String,
    number: String,
    unit: Option<String>,
    start: usize,
    end: usize,
    sent: usize,
}

struct Side<'a> {
    text: &'a str,
    sentences: Vec<Sentence>,
    markers: Vec<Marker>,
    values: Vec<Value>,
}

impl<'a> Side<'a> {
    fn analyze(text: &'a str) -> Side<'a> {
        let toks = tokenize(text);
        let list_toks: HashSet<usize> = (0..toks.len()).filter(|&k| is_list_marker(text, &toks[k])).collect();
        let list_dots: HashSet<usize> = list_toks.iter().map(|&k| toks[k].end).collect();
        let spans = split_sentences(text, &list_dots);
        let sent_at = |pos: usize| spans.iter().rposition(|&(s, _)| s <= pos).unwrap_or(0);
        let lower = |t: &Tok| text[t.start..t.end].to_lowercase();
        let whitespace_gap = |a: &Tok, b: &Tok| text[a.end..b.start].chars().all(char::is_whitespace);

        let mut markers = Vec::new();
        let mut values = Vec::new();
        let mut k = 0;
        while k < toks.len() {
            let t = toks[k];
            match t.kind {
                TokKind::Word | TokKind::Sym => {
                    let mut matched = None;
                    // Comparison phrases are operators only next to a number:
                    // `atau lebih`/`atau kurang` follow a value, every other
                    // operator precedes one (`di atas meja` is a location).
                    let numeric_context = |class: &Class, len: usize| {
                        if class.cat != Cat::Comparison {
                            return true;
                        }
                        if matches!(class.key, "ge" | "le") && lower(&t).starts_with("atau") {
                            let prev = k.checked_sub(1).map(|p| toks[p]);
                            return prev.is_some_and(|p| {
                                whitespace_gap(&p, &t) && values.last().is_some_and(|v: &Value| v.end == p.end)
                            });
                        }
                        toks.get(k + len).is_some_and(|nt| {
                            let s = &text[nt.start..nt.end];
                            nt.kind == TokKind::Alnum
                                && whitespace_gap(&toks[k + len - 1], nt)
                                && (is_number(s) || split_attached_unit(s).is_some())
                        })
                    };
                    if t.kind == TokKind::Sym {
                        matched = lookup(&text[t.start..t.end])
                            .filter(|c| numeric_context(c, 1))
                            .map(|c| (c, 1));
                    } else {
                        let mut phrase = lower(&t);
                        let mut words = vec![phrase.clone()];
                        for w in 1..MAX_PHRASE_WORDS {
                            match toks.get(k + w) {
                                Some(nt) if nt.kind == TokKind::Word && whitespace_gap(&toks[k + w - 1], nt) => {
                                    phrase.push(' ');
                                    phrase.push_str(&lower(nt));
                                    words.push(phrase.clone());
                                }
                                _ => break,
                            }
                        }
                        // Longest match first; a rejected operator phrase
                        // falls back to shorter ones (`tidak lebih dari meja` → `tidak`).
                        for (len, p) in words.iter().enumerate().rev() {
                            if let Some(c) = lookup(p).filter(|c| numeric_context(c, len + 1)) {
                                matched = Some((c, len + 1));
                                break;
                            }
                        }
                    }
                    if let Some((class, len)) = matched {
                        let end = toks[k + len - 1].end;
                        markers.push(Marker {
                            class,
                            start: t.start,
                            end,
                            sent: sent_at(t.start),
                        });
                        k += len;
                    } else {
                        k += 1;
                    }
                }
                TokKind::Alnum => {
                    if list_toks.contains(&k) {
                        k += 1;
                        continue;
                    }
                    let s = &text[t.start..t.end];
                    let (number, mut unit) = if is_number(s) {
                        (normalize_number(s), None)
                    } else if let Some((num, u)) = split_attached_unit(s) {
                        (normalize_number(num), Some(normalize_unit(u)))
                    } else {
                        // Identifier (v1.2, 14:02, A320): compared literally (NUM-006).
                        (s.to_string(), None)
                    };
                    let mut end = t.end;
                    if unit.is_none()
                        && is_number(s)
                        && let Some(nt) = toks.get(k + 1)
                    {
                        let gap = &text[t.end..nt.start];
                        let sym = &text[nt.start..nt.end];
                        if !gap.contains('\n') && gap.chars().all(char::is_whitespace) && UNITS.contains(&sym) {
                            unit = Some(normalize_unit(sym));
                            end = nt.end;
                            k += 1;
                        }
                    }
                    let key = match &unit {
                        Some(u) => format!("{number} {u}"),
                        None => number.clone(),
                    };
                    values.push(Value {
                        key,
                        number,
                        unit,
                        start: t.start,
                        end,
                        sent: sent_at(t.start),
                    });
                    k += 1;
                }
            }
        }

        let sentences = spans
            .iter()
            .enumerate()
            .map(|(idx, &(start, end))| {
                let body = &text[start..end];
                let words: Vec<String> = toks
                    .iter()
                    .filter(|t| t.kind == TokKind::Word && t.start >= start && t.end <= end)
                    .map(lower)
                    .collect();
                let content = words
                    .iter()
                    .filter(|w| w.chars().count() >= 3 && !STOPWORDS.contains(&w.as_str()) && !is_single_word_marker(w))
                    .map(|w| match w.strip_suffix("nya") {
                        Some(stem) if stem.chars().count() >= 3 => stem.to_string(),
                        _ => w.clone(),
                    })
                    .collect();
                Sentence {
                    start,
                    end,
                    list_item: starts_list_item(body),
                    content,
                    has_condition: markers
                        .iter()
                        .any(|m| m.sent == idx && CONDITION_CLASSES.contains(&m.class.key)),
                    carrier: body.trim_end().ends_with(':') || words.iter().any(|w| w == "berikut"),
                }
            })
            .collect();
        Side {
            text,
            sentences,
            markers,
            values,
        }
    }

    fn slice(&self, start: usize, end: usize) -> &'a str {
        &self.text[start..end]
    }
}

fn overlap(a: &Sentence, b: &Sentence) -> usize {
    a.content.intersection(&b.content).count()
}

/// Best partner in `to` for each sentence of `from`, by shared content words;
/// ties go to the nearest index. Zero overlap aligns by index only when both
/// sides have the same number of sentences.
fn align(from: &[Sentence], to: &[Sentence], fallback_by_index: bool) -> Vec<Option<usize>> {
    from.iter()
        .enumerate()
        .map(|(i, s)| {
            let best = to
                .iter()
                .enumerate()
                .map(|(j, t)| (overlap(s, t), std::cmp::Reverse(i.abs_diff(j)), std::cmp::Reverse(j)))
                .max();
            match best {
                Some((o, _, std::cmp::Reverse(j))) if o > 0 => Some(j),
                _ if fallback_by_index && from.len() == to.len() => Some(i),
                _ => None,
            }
        })
        .collect()
}

/// Matches equal keys, preferring occurrences in aligned sentences; returns
/// the unmatched indices of each side.
fn diff_multiset<T>(
    src: &[T],
    cand: &[T],
    key: impl Fn(&T) -> &str,
    sent: impl Fn(&T) -> usize,
    aligned: &[Option<usize>],
) -> (Vec<usize>, Vec<usize>) {
    let mut src_used = vec![false; src.len()];
    let mut cand_used = vec![false; cand.len()];
    for prefer_aligned in [true, false] {
        for (si, s) in src.iter().enumerate() {
            if src_used[si] {
                continue;
            }
            let hit = cand.iter().enumerate().position(|(ci, c)| {
                !cand_used[ci]
                    && key(c) == key(s)
                    && (!prefer_aligned || aligned.get(sent(s)).copied().flatten() == Some(sent(c)))
            });
            if let Some(ci) = hit {
                src_used[si] = true;
                cand_used[ci] = true;
            }
        }
    }
    let unmatched = |used: Vec<bool>| used.iter().enumerate().filter(|(_, u)| !**u).map(|(i, _)| i).collect();
    (unmatched(src_used), unmatched(cand_used))
}

// ---------------------------------------------------------------------------
// Findings
// ---------------------------------------------------------------------------

struct Pending {
    rule: &'static str,
    category: &'static str,
    scope: String,
    range: Option<Range>,
    outcome: Outcome,
    certainty: Certainty,
    method: Method,
    message: String,
    reason: &'static str,
}

struct Ctx<'a> {
    src: Side<'a>,
    cand: Side<'a>,
    map: SourceMap<'a>,
    base_scope: String,
    /// Source sentence → aligned candidate sentence.
    aligned: Vec<Option<usize>>,
    pending: Vec<Pending>,
}

impl Ctx<'_> {
    fn sentence_range(&self, j: Option<usize>) -> Option<Range> {
        j.map(|j| {
            let s = &self.cand.sentences[j];
            self.map.range(s.start, s.end)
        })
    }

    fn scope(&self, j: Option<usize>) -> String {
        match j {
            Some(j) => format!("{}#s{j}", self.base_scope),
            None => self.base_scope.clone(),
        }
    }

    fn finding(
        &mut self,
        (rule, category): (&'static str, &'static str),
        cand_sentence: Option<usize>,
        range: Option<Range>,
        message: String,
        reason: &'static str,
    ) {
        self.pending.push(Pending {
            rule,
            category,
            scope: self.scope(cand_sentence),
            range,
            outcome: Outcome::PossibleViolation,
            certainty: Certainty::High,
            method: Method::Deterministic,
            message,
            reason,
        });
    }

    fn compare_markers(&mut self) {
        let (removed, added) = diff_multiset(
            &self.src.markers,
            &self.cand.markers,
            |m| m.class.key,
            |m| m.sent,
            &self.aligned,
        );
        let mut added_paired = vec![false; added.len()];
        for &ri in &removed {
            let r = &self.src.markers[ri];
            let target = self.aligned.get(r.sent).copied().flatten();
            let partner = added.iter().enumerate().position(|(k, &ai)| {
                let a = &self.cand.markers[ai];
                !added_paired[k] && a.class.cat == r.class.cat && Some(a.sent) == target
            });
            let cat = r.class.cat;
            let src_form = self.src.slice(r.start, r.end);
            let (removed_reason, _, changed_reason) = cat.reasons();
            if let Some(k) = partner {
                added_paired[k] = true;
                let a = &self.cand.markers[added[k]];
                let message = format!(
                    "Penanda {} «{}» pada sumber menjadi penanda {} «{}» pada calon redaksi; {}",
                    r.class.label,
                    src_form,
                    a.class.label,
                    self.cand.slice(a.start, a.end),
                    cat.changed_effect()
                );
                let range = Some(self.map.range(a.start, a.end));
                self.finding(
                    (cat.rule(), cat.category()),
                    Some(a.sent),
                    range,
                    message,
                    changed_reason,
                );
            } else {
                let message = format!(
                    "Penanda {} «{}» pada sumber tidak ada pada calon redaksi; {}. {}",
                    r.class.label,
                    src_form,
                    r.class.removed_effect,
                    cat.removed_action()
                );
                let range = self.sentence_range(target);
                self.finding((cat.rule(), cat.category()), target, range, message, removed_reason);
            }
        }
        for (k, &ai) in added.iter().enumerate() {
            if added_paired[k] {
                continue;
            }
            let a = &self.cand.markers[ai];
            let cat = a.class.cat;
            let message = format!(
                "Calon redaksi menambahkan penanda {} «{}» yang tidak ada pada sumber; {}. Telaah apakah penambahan ini didukung sumber.",
                a.class.label,
                self.cand.slice(a.start, a.end),
                cat.added_effect()
            );
            let range = Some(self.map.range(a.start, a.end));
            self.finding(
                (cat.rule(), cat.category()),
                Some(a.sent),
                range,
                message,
                cat.reasons().1,
            );
        }
    }

    fn compare_values(&mut self) {
        let (removed, added) = diff_multiset(
            &self.src.values,
            &self.cand.values,
            |v| v.key.as_str(),
            |v| v.sent,
            &self.aligned,
        );
        let mut added_paired = vec![false; added.len()];
        for &ri in &removed {
            let r = &self.src.values[ri];
            let target = self.aligned.get(r.sent).copied().flatten();
            // Same number with another unit first (F15), then same unit with
            // another number; aligned sentences before anywhere.
            let mut partner = None;
            'search: for same_number in [true, false] {
                for require_aligned in [true, false] {
                    partner = added.iter().enumerate().position(|(k, &ai)| {
                        let a = &self.cand.values[ai];
                        let related = if same_number {
                            a.number == r.number
                        } else {
                            a.unit.is_some() && a.unit == r.unit
                        };
                        !added_paired[k] && related && (!require_aligned || Some(a.sent) == target)
                    });
                    if partner.is_some() {
                        break 'search;
                    }
                }
            }
            let src_form = self.src.slice(r.start, r.end);
            if let Some(k) = partner {
                added_paired[k] = true;
                let a = &self.cand.values[added[k]];
                let (what, reason) = if a.number == r.number {
                    ("satuan berubah tanpa penyesuaian nilai", "UNIT_CHANGED")
                } else {
                    ("nilai berubah", "VALUE_CHANGED")
                };
                let message = format!(
                    "Nilai «{}» pada sumber menjadi «{}» pada calon redaksi; {what}. Pertahankan nilai dan satuan sumber; konversi hanya dengan sumber, rumus, dan pembulatan yang dapat ditelaah.",
                    src_form,
                    self.cand.slice(a.start, a.end)
                );
                let range = Some(self.map.range(a.start, a.end));
                self.finding(("PASTI-SEM-001", "meaning.value"), Some(a.sent), range, message, reason);
            } else {
                let message = format!(
                    "Nilai «{src_form}» pada sumber tidak ada pada calon redaksi; nilai atau satuan dapat hilang. Telaah dan pertahankan nilai sumber."
                );
                let range = self.sentence_range(target);
                self.finding(
                    ("PASTI-SEM-001", "meaning.value"),
                    target,
                    range,
                    message,
                    "VALUE_REMOVED",
                );
            }
        }
        for (k, &ai) in added.iter().enumerate() {
            if added_paired[k] {
                continue;
            }
            let a = &self.cand.values[ai];
            let message = format!(
                "Calon redaksi memuat nilai «{}» yang tidak ada pada sumber; jangan menambahkan nilai, ketelitian, atau waktu yang tidak diberikan sumber. Telaah asal nilai ini.",
                self.cand.slice(a.start, a.end)
            );
            let range = Some(self.map.range(a.start, a.end));
            self.finding(
                ("PASTI-NUM-006", "meaning.added_value"),
                Some(a.sent),
                range,
                message,
                "VALUE_ADDED",
            );
        }
    }

    /// PASTI-SEM-004 shared condition: when a conditioned source sentence is
    /// split over several candidate sentences, each of those must carry a
    /// condition marker or follow a list introduction that carries it.
    fn compare_condition_scope(&mut self) {
        let owner = align(&self.cand.sentences, &self.src.sentences, false);
        let mut flagged = Vec::new();
        for (i, s) in self.src.sentences.iter().enumerate() {
            if !s.has_condition {
                continue;
            }
            let group: Vec<usize> = (0..owner.len()).filter(|&j| owner[j] == Some(i)).collect();
            let (Some(&first), Some(&last)) = (group.first(), group.last()) else {
                continue;
            };
            if group.len() < 2 || !group.iter().any(|&j| self.cand.sentences[j].has_condition) {
                // A single sentence keeps its scope; a lost marker is reported
                // by the meaning.scope marker diff.
                continue;
            }
            let mut carry = false;
            let mut in_list = false;
            for (j, c) in self.cand.sentences.iter().enumerate().take(last + 1).skip(first) {
                if c.carrier && (c.has_condition || carry) {
                    carry = true;
                    in_list = false;
                    continue;
                }
                if carry {
                    if c.list_item {
                        in_list = true;
                    } else if in_list {
                        carry = false;
                    }
                }
                if owner[j] == Some(i) && !c.has_condition && !carry {
                    flagged.push((i, j));
                }
            }
        }
        for (i, j) in flagged {
            let s = &self.src.sentences[i];
            let c = &self.cand.sentences[j];
            let marker = self
                .src
                .markers
                .iter()
                .find(|m| m.sent == i && CONDITION_CLASSES.contains(&m.class.key))
                .map_or("", |m| self.src.slice(m.start, m.end));
            let message = format!(
                "Kalimat «{}» pada calon redaksi berasal dari kalimat sumber bersyarat «{}», tetapi tidak membawa syarat «{}»; tindakan dapat tampak tanpa syarat. Telaah apakah syarat masih berlaku dan nyatakan kembali atau gunakan pengantar daftar yang jelas cakupannya.",
                self.cand.slice(c.start, c.end),
                self.src.slice(s.start, s.end),
                marker
            );
            self.pending.push(Pending {
                rule: "PASTI-SEM-004",
                category: "meaning.condition_scope",
                scope: self.scope(Some(j)),
                range: Some(self.map.range(c.start, c.end)),
                outcome: Outcome::PossibleViolation,
                certainty: Certainty::Medium,
                method: Method::Heuristic,
                message,
                reason: "CONDITION_SCOPE_LOST",
            });
        }
    }

    fn coverage(&mut self) {
        let evaluated: [(&str, &str, &str, &str); 6] = [
            (
                "PASTI-SEM-002",
                "meaning.modality",
                "NO_MARKER_CHANGE",
                "Tidak ditemukan perubahan penanda modalitas",
            ),
            (
                "PASTI-SEM-003",
                "meaning.aspect",
                "NO_MARKER_CHANGE",
                "Tidak ditemukan perubahan penanda aspek dan waktu",
            ),
            (
                "PASTI-SEM-004",
                "meaning.scope",
                "NO_MARKER_CHANGE",
                "Tidak ditemukan perubahan penanda negasi, syarat, pengecualian, atau kuantor",
            ),
            (
                "PASTI-SEM-001",
                "meaning.value",
                "NO_VALUE_CHANGE",
                "Tidak ditemukan perubahan nilai atau satuan",
            ),
            (
                "PASTI-SEM-001",
                "meaning.comparison",
                "NO_OPERATOR_CHANGE",
                "Tidak ditemukan perubahan operator perbandingan",
            ),
            (
                "PASTI-NUM-006",
                "meaning.added_value",
                "NO_ADDED_VALUE",
                "Tidak ditemukan nilai baru",
            ),
        ];
        let has_finding = |pending: &[Pending], rule: &str, category: &str| {
            pending.iter().any(|p| p.rule == rule && p.category == category)
        };
        let mut extra = Vec::new();
        for (rule, category, reason, finding) in evaluated {
            if !has_finding(&self.pending, rule, category) {
                extra.push(self.coverage_entry(
                    rule,
                    category,
                    Outcome::Satisfied,
                    (Certainty::High, Method::Deterministic),
                    format!(
                        "{finding} antara sumber dan calon redaksi; hasil ini bukan bukti kesetaraan makna dan tidak menggantikan telaah."
                    ),
                    reason,
                ));
            }
        }
        if !has_finding(&self.pending, "PASTI-SEM-004", "meaning.condition_scope") {
            extra.push(if self.src.sentences.iter().any(|s| s.has_condition) {
                self.coverage_entry(
                    "PASTI-SEM-004",
                    "meaning.condition_scope",
                    Outcome::Satisfied,
                    (Certainty::Medium, Method::Heuristic),
                    "Tidak ditemukan tindakan yang kehilangan syarat bersama setelah pemecahan kalimat; hasil heuristik ini tetap memerlukan telaah cakupan.".to_string(),
                    "NO_CONDITION_SCOPE_LOSS",
                )
            } else {
                self.coverage_entry(
                    "PASTI-SEM-004",
                    "meaning.condition_scope",
                    Outcome::NotApplicable,
                    (Certainty::High, Method::Deterministic),
                    "Sumber tidak memuat penanda syarat (jika, apabila, bila, kecuali); pemeriksaan syarat bersama tidak berlaku.".to_string(),
                    "NO_SOURCE_CONDITION",
                )
            });
        }
        extra.push(self.coverage_entry(
            "PASTI-SEM-005",
            "meaning.reference",
            Outcome::NotEvaluated,
            (Certainty::High, Method::Manual),
            "Pelaku, rujukan, dan urutan tindakan tidak diperiksa otomatis oleh alat ini; telaah manual diperlukan.".to_string(),
            "NOT_IMPLEMENTED",
        ));
        self.pending.extend(extra);
    }

    fn coverage_entry(
        &self,
        rule: &'static str,
        category: &'static str,
        outcome: Outcome,
        (certainty, method): (Certainty, Method),
        message: String,
        reason: &'static str,
    ) -> Pending {
        Pending {
            rule,
            category,
            scope: self.base_scope.clone(),
            range: None,
            outcome,
            certainty,
            method,
            message,
            reason,
        }
    }
}

/// Compares `candidate` against `source` and reports meaning-preservation
/// candidates (Doc 07). Ranges refer to the candidate text. Every rule
/// category evaluated without a finding yields one `satisfied` (or
/// `not_applicable`) entry, and PASTI-SEM-005 is reported `not_evaluated`, so
/// coverage stays visible. Output is sorted by [`Diagnostic::sort_key`].
pub fn compare(source: &str, candidate: &str, document_id: &str, mode: Mode) -> Vec<Diagnostic> {
    let src = Side::analyze(source);
    let cand = Side::analyze(candidate);
    let aligned = align(&src.sentences, &cand.sentences, true);
    let mut ctx = Ctx {
        base_scope: format!("rewrite:{}", &sha256_hex(source.as_bytes())[..12]),
        map: SourceMap::new(candidate),
        src,
        cand,
        aligned,
        pending: Vec::new(),
    };
    ctx.compare_markers();
    ctx.compare_values();
    ctx.compare_condition_scope();
    ctx.coverage();

    let mut seen = HashSet::new();
    let mut out: Vec<Diagnostic> = Vec::with_capacity(ctx.pending.len());
    for p in ctx.pending {
        // Identical findings at the same place (e.g. two removed `mungkin`
        // reported on one sentence) get an ordinal so IDs stay unique.
        for n in 1.. {
            let scope_id = if n == 1 {
                p.scope.clone()
            } else {
                format!("{}~{n}", p.scope)
            };
            let d = Diagnostic::new(Finding {
                rule_id: p.rule,
                category: p.category,
                document_id,
                scope_id,
                mode,
                source_range: p.range,
                outcome: p.outcome,
                certainty: p.certainty,
                method: p.method,
                message: p.message.clone(),
                reason_code: p.reason,
                autofix: Autofix::Forbidden,
            });
            if seen.insert(d.diagnostic_id.clone()) {
                out.push(d);
                break;
            }
        }
    }
    out.sort_by_key(Diagnostic::sort_key);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str, cand: &str) -> Vec<Diagnostic> {
        compare(src, cand, "doc", Mode::PROC)
    }

    fn findings(src: &str, cand: &str) -> Vec<Diagnostic> {
        run(src, cand).into_iter().filter(|d| d.outcome.is_finding()).collect()
    }

    fn expect(src: &str, cand: &str, rule: &str, category: &str) -> Diagnostic {
        let all = findings(src, cand);
        let hit = all
            .iter()
            .find(|d| d.rule_id == rule && d.category == category)
            .cloned();
        let d = hit.unwrap_or_else(|| panic!("no {rule}/{category} for {src:?} → {cand:?}; got {all:#?}"));
        assert_eq!(d.outcome, Outcome::PossibleViolation);
        assert_eq!(d.autofix, Autofix::Forbidden);
        d
    }

    fn cp_span(d: &Diagnostic) -> (usize, usize) {
        let r = d.source_range.expect("range");
        (r.start.offset_cp, r.end.offset_cp)
    }

    #[test]
    fn f09_uncertainty_removed() {
        let d = expect(
            "Katup mungkin bocor.",
            "Katup bocor.",
            "PASTI-SEM-002",
            "meaning.modality",
        );
        assert_eq!(d.certainty, Certainty::High);
        assert_eq!(d.method, Method::Deterministic);
        assert_eq!(d.reason_code, "MARKER_REMOVED");
        assert!(d.message.contains("«mungkin»"), "{}", d.message);
        assert_eq!(cp_span(&d), (0, 12));
    }

    #[test]
    fn f10_advice_becomes_obligation() {
        let d = expect(
            "Sebaiknya operator memeriksa sambungan.",
            "Operator harus memeriksa sambungan.",
            "PASTI-SEM-002",
            "meaning.modality",
        );
        assert_eq!(d.reason_code, "MARKER_CHANGED");
        assert!(
            d.message.contains("«Sebaiknya»") && d.message.contains("«harus»"),
            "{}",
            d.message
        );
        assert_eq!(cp_span(&d), (9, 14));
    }

    #[test]
    fn f11_permission_becomes_obligation() {
        let d = expect(
            "Operator boleh menunda pengujian.",
            "Operator harus menunda pengujian.",
            "PASTI-SEM-002",
            "meaning.modality",
        );
        assert_eq!(d.reason_code, "MARKER_CHANGED");
    }

    #[test]
    fn f12_aspect_removed() {
        let d = expect(
            "Pompa sudah berhenti.",
            "Pompa berhenti.",
            "PASTI-SEM-003",
            "meaning.aspect",
        );
        assert!(d.message.contains("«sudah»"));
    }

    #[test]
    fn f13_negation_removed() {
        let d = expect("Jangan buka katup.", "Buka katup.", "PASTI-SEM-004", "meaning.scope");
        assert!(d.message.contains("«Jangan»"));
    }

    #[test]
    fn f14_strict_bound_becomes_inclusive() {
        let d = expect(
            "lebih dari 80 °C",
            "80 °C atau lebih",
            "PASTI-SEM-001",
            "meaning.comparison",
        );
        assert_eq!(d.reason_code, "OPERATOR_CHANGED");
        assert!(d.message.contains("«lebih dari»") && d.message.contains("«atau lebih»"));
        assert_eq!(cp_span(&d), (6, 16));
        // The value itself is unchanged.
        assert!(
            !findings("lebih dari 80 °C", "80 °C atau lebih")
                .iter()
                .any(|d| d.category == "meaning.value")
        );
    }

    #[test]
    fn f15_unit_changed() {
        let d = expect(
            "Tambahkan 1,5 mL.",
            "Tambahkan 1,5 L.",
            "PASTI-SEM-001",
            "meaning.value",
        );
        assert_eq!(d.reason_code, "UNIT_CHANGED");
        assert!(
            d.message.contains("«1,5 mL»") && d.message.contains("«1,5 L»"),
            "{}",
            d.message
        );
        assert_eq!(cp_span(&d), (10, 15));
        assert!(
            !findings("Tambahkan 1,5 mL.", "Tambahkan 1,5 L.")
                .iter()
                .any(|d| d.rule_id == "PASTI-NUM-006")
        );
    }

    #[test]
    fn f16_shared_condition_lost() {
        let src = "Jika pengujian lulus, simpan hasil dan kirim laporan.";
        let cand = "Jika pengujian lulus, simpan hasil. Kirim laporan.";
        let d = expect(src, cand, "PASTI-SEM-004", "meaning.condition_scope");
        assert_eq!(d.certainty, Certainty::Medium);
        assert_eq!(d.method, Method::Heuristic);
        assert_eq!(d.reason_code, "CONDITION_SCOPE_LOST");
        assert_eq!(cp_span(&d), (36, 50));
        assert!(d.scope_id.ends_with("#s1"));
        assert_eq!(findings(src, cand).len(), 1);
    }

    #[test]
    fn identical_text_has_no_findings_but_reports_coverage() {
        let src = "Jika pengujian lulus, simpan hasil dan kirim laporan.";
        let all = run(src, src);
        assert!(all.iter().all(|d| !d.outcome.is_finding()), "{all:#?}");
        let satisfied = all.iter().filter(|d| d.outcome == Outcome::Satisfied).count();
        assert_eq!(satisfied, 7);
        let sem5 = all.iter().find(|d| d.rule_id == "PASTI-SEM-005").unwrap();
        assert_eq!(sem5.outcome, Outcome::NotEvaluated);
        assert_eq!(sem5.reason_code, "NOT_IMPLEMENTED");
        assert!(all.iter().all(|d| d.source_range.is_none()));
    }

    #[test]
    fn doc07_acceptable_split_keeps_condition_scope() {
        let src = "Jika pengujian lulus, tinjau hasilnya lalu catat nomor versi.";
        for cand in [
            "Jika pengujian lulus, lakukan dua langkah berikut.\n1. Tinjau hasil pengujian.\n2. Setelah peninjauan selesai, catat nomor versi.",
            "Jika pengujian lulus, lakukan dua langkah berikut. 1. Tinjau hasil pengujian. 2. Setelah peninjauan selesai, catat nomor versi.",
        ] {
            let all = findings(src, cand);
            assert!(!all.iter().any(|d| d.category == "meaning.condition_scope"), "{all:#?}");
            // List numbers are not values.
            assert!(!all.iter().any(|d| d.rule_id == "PASTI-NUM-006"), "{all:#?}");
        }
    }

    #[test]
    fn reordering_without_marker_change_is_clean() {
        let all = findings(
            "Operator harus memeriksa sambungan sebelum pengujian pada suhu 25 °C.",
            "Sebelum pengujian pada suhu 25 °C, operator harus memeriksa sambungan.",
        );
        assert!(all.is_empty(), "{all:#?}");
    }

    #[test]
    fn no_obligation_becomes_prohibition() {
        let d = expect(
            "Pengguna tidak wajib memasang modul",
            "Pengguna tidak boleh memasang modul",
            "PASTI-SEM-002",
            "meaning.modality",
        );
        assert_eq!(d.reason_code, "MARKER_CHANGED");
        assert!(d.message.contains("«tidak wajib»") && d.message.contains("«tidak boleh»"));
        // Longest match: no separate negation finding.
        let all = findings(
            "Pengguna tidak wajib memasang modul",
            "Pengguna tidak boleh memasang modul",
        );
        assert_eq!(all.len(), 1, "{all:#?}");
    }

    #[test]
    fn added_value_is_num006() {
        let d = expect(
            "Tunggu sampai indikator padam.",
            "Tunggu 5 detik sampai indikator padam.",
            "PASTI-NUM-006",
            "meaning.added_value",
        );
        assert!(d.message.contains("«5 detik»"));
    }

    #[test]
    fn comparison_phrases_need_a_number() {
        let all = findings("Letakkan di atas meja.", "Letakkan pada meja.");
        assert!(!all.iter().any(|d| d.category == "meaning.comparison"), "{all:#?}");
        let all = findings("Pasang minimal satu kabel.", "Pasang satu kabel.");
        assert!(!all.iter().any(|d| d.category == "meaning.comparison"), "{all:#?}");
        // A rejected operator phrase falls back to its shorter markers.
        let d = expect(
            "Nilai tidak lebih dari batas.",
            "Nilai lebih dari batas.",
            "PASTI-SEM-004",
            "meaning.scope",
        );
        assert!(d.message.contains("«tidak»"));
        // Prefix and postfix operators with numbers still count.
        expect(
            "Jika suhu di atas -5 °C",
            "Jika suhu -5 °C atau kurang",
            "PASTI-SEM-001",
            "meaning.comparison",
        );
    }

    #[test]
    fn litre_symbol_case_is_equivalent() {
        let all = findings("Tambahkan 1,5 ml.", "Tambahkan 1,5 mL.");
        assert!(!all.iter().any(|d| d.category == "meaning.value"), "{all:#?}");
        assert!(findings("Isi 2 l.", "Isi 2 L.").is_empty());
        assert!(findings("Isi 2 kl.", "Isi 2 kL.").is_empty());
        expect(
            "Tambahkan 1,5 mL.",
            "Tambahkan 1,5 L.",
            "PASTI-SEM-001",
            "meaning.value",
        );
        expect("Atur 5 MPa.", "Atur 5 mPa.", "PASTI-SEM-001", "meaning.value");
    }

    #[test]
    fn same_class_synonym_is_not_flagged() {
        assert!(findings("Operator harus menutup katup.", "Operator wajib menutup katup.").is_empty());
    }

    #[test]
    fn numbers_and_sentences_follow_tok_002() {
        assert_eq!(normalize_number("2.5"), "2,5");
        assert_eq!(normalize_number("1.000"), "1.000");
        assert_eq!(normalize_number("−3,5"), "-3,5");
        let text = "Tambahkan 2,5 mL. Isi v1.2 lalu No. 12 dipakai.";
        let spans = split_sentences(text, &HashSet::new());
        assert_eq!(spans.len(), 2);
        assert!(
            findings("Atur ke -5 °C.", "Atur ke 5 °C.")
                .iter()
                .any(|d| d.reason_code == "VALUE_CHANGED")
        );
    }

    #[test]
    fn output_is_sorted_with_unique_ids() {
        let all = run("Katup mungkin bocor, mungkin rusak.", "Katup bocor, rusak.");
        let mut keys: Vec<_> = all.iter().map(Diagnostic::sort_key).collect();
        let sorted = {
            let mut k = keys.clone();
            k.sort();
            k
        };
        assert_eq!(keys, sorted);
        keys.dedup_by(|a, b| a.4 == b.4);
        assert_eq!(keys.len(), all.len());
        assert_eq!(all.iter().filter(|d| d.rule_id == "PASTI-SEM-002").count(), 2);
    }
}
