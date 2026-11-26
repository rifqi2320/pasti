//! Rule registry for PASTI spec 0.1.0-draft.1.
//!
//! Every rule ID in Docs 01–11 is listed so coverage can show rules this tool
//! does not evaluate (PASTI-CORE-007: silence is not evidence). `automated`
//! records what this implementation actually checks; everything else is
//! reported as `not_evaluated`.

use crate::diagnostic::{Force, Mode};

pub const RULE_REVISION: &str = "0.1.0-draft.1";

/// What a rule is applied to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// Prose in a linted document.
    Text,
    /// Glossary data (Doc 05).
    Glossary,
    /// A source/candidate rewrite pair (Doc 07).
    Rewrite,
    /// The experimental formal profile EX01 (Doc 10).
    Formal,
    /// Process, governance, tooling or evaluation obligations; never checked
    /// against a document.
    Process,
}

#[derive(Debug)]
pub struct Rule {
    pub id: &'static str,
    pub revision: &'static str,
    pub title: &'static str,
    pub force: Force,
    pub target: Target,
    /// Modes the rule applies to; empty means every mode.
    pub modes: &'static [Mode],
    /// True when this tool runs an automated check for (part of) the rule.
    pub automated: bool,
}

impl Rule {
    pub fn applies_to(&self, mode: Mode) -> bool {
        self.modes.is_empty() || self.modes.contains(&mode)
    }
}

use Force::{Harus as H, Sebaiknya as S, TidakBoleh as TB};
use Mode::{DESC, PROC, REQ, UI, WARN};
use Target::{Formal as F, Glossary as G, Process as P, Rewrite as RW, Text as T};

macro_rules! rule {
    ($id:literal, $title:literal, $force:expr, $target:expr, $modes:expr, $auto:literal) => {
        Rule {
            id: $id,
            revision: RULE_REVISION,
            title: $title,
            force: $force,
            target: $target,
            modes: $modes,
            automated: $auto,
        }
    };
}

const ALL: &[Mode] = &[];
const PW: &[Mode] = &[PROC, WARN];
const PWDR: &[Mode] = &[PROC, WARN, DESC, REQ];

pub static RULES: &[Rule] = &[
    // Doc 01 — foundations and conformance model.
    rule!("PASTI-CORE-001", "Nyatakan konteks penilaian", H, P, ALL, false),
    rule!(
        "PASTI-CORE-002",
        "Jangan memperluas klaim di luar cakupan",
        TB,
        P,
        ALL,
        false
    ),
    rule!(
        "PASTI-CORE-003",
        "Bedakan aturan dan informasi pendukung",
        H,
        P,
        ALL,
        false
    ),
    rule!(
        "PASTI-CORE-004",
        "Pisahkan kekuatan, tingkat masalah, dan kepastian",
        H,
        P,
        ALL,
        false
    ),
    rule!(
        "PASTI-CORE-005",
        "Tetapkan jenis teks pada lingkup kecil",
        H,
        P,
        ALL,
        false
    ),
    rule!("PASTI-CORE-006", "Gunakan status yang sesuai bukti", H, P, ALL, false),
    rule!(
        "PASTI-CORE-007",
        "Jangan menafsirkan diamnya alat sebagai bukti",
        TB,
        P,
        ALL,
        false
    ),
    rule!(
        "PASTI-CORE-008",
        "Pertahankan isi teknis saat benturan gaya",
        H,
        P,
        ALL,
        false
    ),
    rule!("PASTI-CORE-009", "Nyatakan asal setiap ketentuan", H, P, ALL, false),
    rule!(
        "PASTI-CORE-010",
        "Pisahkan versi spesifikasi, skema, profil, kamus, alat",
        H,
        P,
        ALL,
        false
    ),
    // Doc 02 — basic writing rules.
    rule!(
        "PASTI-BAS-001",
        "Pertahankan isi dan kekuatan pernyataan",
        H,
        T,
        ALL,
        false
    ),
    rule!(
        "PASTI-BAS-002",
        "Jelaskan pelaku, tindakan, dan sasaran",
        H,
        T,
        ALL,
        false
    ),
    rule!(
        "PASTI-BAS-003",
        "Gunakan istilah sesuai konsep dan konteks",
        H,
        T,
        ALL,
        true
    ),
    rule!("PASTI-BAS-004", "Pisahkan tindakan mandiri", H, T, PW, false),
    rule!(
        "PASTI-BAS-005",
        "Telaah kalimat yang melewati ambang panjang",
        H,
        T,
        PWDR,
        false
    ),
    rule!("PASTI-BAS-006", "Tempatkan prasyarat sebelum tindakan", H, T, PW, false),
    rule!("PASTI-BAS-007", "Buat rujukan dapat ditentukan", H, T, ALL, false),
    rule!(
        "PASTI-BAS-008",
        "Bedakan tindakan, hasil, dan pengamatan",
        H,
        T,
        ALL,
        false
    ),
    rule!(
        "PASTI-BAS-009",
        "Sediakan informasi sebelum diperlukan",
        S,
        T,
        ALL,
        false
    ),
    rule!(
        "PASTI-BAS-010",
        "Tentukan kriteria operasional bila tersedia",
        H,
        T,
        ALL,
        false
    ),
    rule!(
        "PASTI-BAS-011",
        "Pertahankan bentuk yang harus sama persis",
        H,
        T,
        ALL,
        false
    ),
    rule!(
        "PASTI-BAS-012",
        "Pertahankan tata bahasa dan ejaan yang benar",
        H,
        T,
        ALL,
        false
    ),
    // Doc 03 — rules by text type.
    rule!("PASTI-PROC-001", "Nyatakan tindakan langsung", H, T, &[PROC], false),
    rule!(
        "PASTI-PROC-002",
        "Pertahankan urutan dan cakupan langkah",
        H,
        T,
        &[PROC],
        false
    ),
    rule!(
        "PASTI-PROC-003",
        "Pisahkan persiapan dan hasil yang diharapkan",
        H,
        T,
        &[PROC],
        false
    ),
    rule!(
        "PASTI-PROC-004",
        "Buat percabangan dan pengulangan dapat diikuti",
        H,
        T,
        &[PROC],
        false
    ),
    rule!(
        "PASTI-DESC-001",
        "Bedakan keadaan, kejadian, dan pelaku",
        H,
        T,
        &[DESC],
        false
    ),
    rule!(
        "PASTI-DESC-002",
        "Pertahankan hubungan sebab dan kepastian",
        H,
        T,
        &[DESC],
        false
    ),
    rule!(
        "PASTI-DESC-003",
        "Kelompokkan uraian menurut topik",
        S,
        T,
        &[DESC],
        false
    ),
    rule!(
        "PASTI-REQ-001",
        "Bedakan kewajiban, izin, dan kemampuan",
        H,
        T,
        &[REQ],
        false
    ),
    rule!(
        "PASTI-REQ-002",
        "Nyatakan objek penilaian dan kriteria",
        H,
        T,
        &[REQ],
        false
    ),
    rule!("PASTI-REQ-003", "Pertahankan cakupan logika", H, T, &[REQ], false),
    rule!(
        "PASTI-WARN-001",
        "Hubungkan bahaya, akibat, dan pencegahan",
        H,
        T,
        &[WARN],
        false
    ),
    rule!(
        "PASTI-WARN-002",
        "Letakkan peringatan sebelum paparan",
        H,
        T,
        &[WARN],
        false
    ),
    rule!(
        "PASTI-WARN-003",
        "Pertahankan label dan larangan yang disahkan",
        H,
        T,
        &[WARN],
        false
    ),
    rule!(
        "PASTI-UI-001",
        "Tulis sesuai fungsi dan batas ruang",
        H,
        T,
        &[UI],
        false
    ),
    rule!(
        "PASTI-UI-002",
        "Cocokkan pesan dengan keadaan sistem",
        H,
        T,
        &[UI],
        false
    ),
    rule!(
        "PASTI-UI-003",
        "Pertahankan elemen dinamis dan rujukan layar",
        H,
        T,
        &[UI],
        false
    ),
    // Doc 04 — Indonesian grammar and edge cases.
    rule!("PASTI-GRA-001", "Bedakan imbuhan dan kata depan", H, T, ALL, true),
    rule!("PASTI-GRA-002", "Nilai turunan kata menurut perannya", H, T, ALL, false),
    rule!("PASTI-GRA-003", "Gunakan bentuk imperatif yang benar", H, T, PW, false),
    rule!(
        "PASTI-GRA-004",
        "Bedakan pasif, keadaan, dan perintah",
        H,
        T,
        ALL,
        false
    ),
    rule!("PASTI-GRA-005", "Periksa fungsi bentuk ter-", H, T, ALL, false),
    rule!(
        "PASTI-GRA-006",
        "Pertahankan aspek dan hubungan waktu",
        H,
        T,
        ALL,
        false
    ),
    rule!("PASTI-GRA-007", "Pertahankan modalitas", H, T, ALL, false),
    rule!(
        "PASTI-GRA-008",
        "Pertahankan cakupan negasi dan syarat",
        H,
        T,
        ALL,
        false
    ),
    rule!(
        "PASTI-GRA-009",
        "Periksa fungsi -nya dan kata penunjuk",
        H,
        T,
        ALL,
        false
    ),
    rule!(
        "PASTI-GRA-010",
        "Jelaskan hubungan dalam frasa nomina",
        H,
        T,
        ALL,
        false
    ),
    rule!("PASTI-GRA-011", "Pertahankan makna bentuk ulang", H, T, ALL, true),
    rule!(
        "PASTI-GRA-012",
        "Bedakan serapan, istilah asing, dan pengenal",
        H,
        T,
        ALL,
        false
    ),
    rule!(
        "PASTI-GRA-013",
        "Nilai kelengkapan menurut konstruksi",
        H,
        T,
        ALL,
        false
    ),
    // Doc 05 — controlled vocabulary.
    rule!("PASTI-LEX-001", "Identitas konsep", H, G, ALL, true),
    rule!("PASTI-LEX-002", "Status entri", H, G, ALL, true),
    rule!("PASTI-LEX-003", "Pemilihan istilah menurut konsep", H, T, ALL, false),
    rule!("PASTI-LEX-004", "Bentuk dan peran gramatikal", H, G, ALL, false),
    rule!("PASTI-LEX-005", "Polisemi dan batas makna", H, T, ALL, true),
    rule!(
        "PASTI-LEX-006",
        "Istilah baru dan istilah tidak dikenal",
        H,
        T,
        ALL,
        false
    ),
    rule!("PASTI-LEX-007", "Konflik antarkamus", TB, G, ALL, true),
    rule!("PASTI-LEX-008", "Pembaruan dan pelestarian sumber", H, G, ALL, false),
    // Doc 06 — numbers, units, segmentation.
    rule!("PASTI-TOK-001", "Pemilihan teks dan cakupan", H, T, ALL, true),
    rule!("PASTI-TOK-002", "Batas kalimat", H, T, ALL, true),
    // TOK-003/004 define how this tool counts; they are tool behaviour, not
    // findings on text.
    rule!("PASTI-TOK-003", "Unit hitung ortografis", H, P, ALL, false),
    rule!("PASTI-TOK-004", "Rentang atomik dan istilah majemuk", H, P, ALL, false),
    // UI has no universal threshold; it is evaluated only when the profile sets one.
    rule!("PASTI-TOK-005", "Ambang penelaahan", H, T, ALL, true),
    rule!("PASTI-NUM-001", "Penafsiran angka", H, T, ALL, true),
    rule!("PASTI-NUM-002", "Lambang satuan", H, T, ALL, true),
    rule!("PASTI-NUM-003", "Batas perbandingan", H, T, ALL, true),
    rule!("PASTI-NUM-004", "Rentang dan pencantuman satuan", H, T, ALL, true),
    rule!("PASTI-NUM-005", "Toleransi dan ketelitian", H, T, ALL, false),
    rule!("PASTI-NUM-006", "Nilai sumber dan konteks waktu", TB, RW, ALL, true),
    // Doc 07 — meaning preservation, exceptions.
    rule!("PASTI-SEM-001", "Unsur makna yang dilindungi", H, RW, ALL, true),
    rule!("PASTI-SEM-002", "Modalitas dan kepastian", TB, RW, ALL, true),
    rule!("PASTI-SEM-003", "Aspek, waktu, dan keadaan", H, RW, ALL, true),
    rule!("PASTI-SEM-004", "Negasi, syarat, dan pengecualian", H, RW, ALL, true),
    rule!("PASTI-SEM-005", "Pelaku, rujukan, dan urutan", H, RW, ALL, false),
    rule!("PASTI-EXC-001", "Pengecualian yang diizinkan aturan", H, P, ALL, false),
    rule!("PASTI-EXC-002", "Deviasi terhadap aturan wajib", H, P, ALL, false),
    rule!("PASTI-EXC-003", "Penyembunyian diagnostik", TB, P, ALL, false),
    rule!("PASTI-EXC-004", "Ruang berlaku dan peninjauan ulang", H, P, ALL, false),
    // Doc 08 — linter contract.
    rule!("PASTI-LINT-001", "Sumber dan bagian terlindungi", H, P, ALL, false),
    rule!("PASTI-LINT-002", "Rentang yang dapat dilacak", H, P, ALL, false),
    rule!(
        "PASTI-LINT-003",
        "Pisahkan kewajiban, hasil, dan kepastian",
        H,
        P,
        ALL,
        false
    ),
    rule!("PASTI-LINT-004", "Konfigurasi efektif yang eksplisit", H, P, ALL, false),
    rule!(
        "PASTI-LINT-005",
        "Jangan menyamakan diam dengan sesuai",
        H,
        P,
        ALL,
        false
    ),
    rule!("PASTI-LINT-006", "Perubahan harus dapat ditinjau", TB, P, ALL, false),
    // Doc 09 — evaluation.
    rule!("PASTI-TEST-001", "Korpus harus dapat ditelusuri", H, P, ALL, false),
    rule!("PASTI-TEST-002", "Putusan acuan harus independen", H, P, ALL, false),
    rule!(
        "PASTI-TEST-003",
        "Uji batas dan contoh yang tidak boleh ditandai",
        H,
        P,
        ALL,
        false
    ),
    rule!("PASTI-TEST-004", "Ukur kemampuan per aturan", H, P, ALL, false),
    rule!(
        "PASTI-TEST-005",
        "Rilis memerlukan bukti sesuai lingkup",
        H,
        P,
        ALL,
        false
    ),
    rule!(
        "PASTI-TEST-006",
        "Pisahkan keterbacaan dari kepatuhan alat",
        H,
        P,
        ALL,
        false
    ),
    // Doc 10 — formal profile EX01.
    rule!(
        "PASTI-FOR-001",
        "Terima hanya bentuk yang didefinisikan",
        H,
        F,
        ALL,
        true
    ),
    rule!("PASTI-FOR-002", "Ikat setiap acuan ke registry", H, F, ALL, true),
    rule!("PASTI-FOR-003", "Perbandingan harus bertipe", H, F, ALL, true),
    rule!("PASTI-FOR-004", "Satu tafsir yang terikat", H, F, ALL, true),
    rule!(
        "PASTI-FOR-005",
        "Jangan menambahkan perilaku yang tidak dinyatakan",
        TB,
        F,
        ALL,
        true
    ),
    rule!(
        "PASTI-FOR-006",
        "AST adalah representasi, bukan tindakan",
        TB,
        F,
        ALL,
        true
    ),
    // Doc 11 — governance.
    rule!("PASTI-GOV-001", "Sebut penanggung jawab keputusan", H, P, ALL, false),
    rule!(
        "PASTI-GOV-002",
        "Mulai perubahan dari masalah yang dapat ditunjukkan",
        H,
        P,
        ALL,
        false
    ),
    rule!(
        "PASTI-GOV-003",
        "Perbarui aturan, contoh, dan uji secara konsisten",
        H,
        P,
        ALL,
        false
    ),
    rule!("PASTI-GOV-004", "Terbitkan manifest rilis", H, P, ALL, false),
    rule!(
        "PASTI-GOV-005",
        "Jangan mengubah hasil historis tanpa jejak",
        TB,
        P,
        ALL,
        false
    ),
    rule!("PASTI-GOV-006", "Catat asal dan izin bahan", H, P, ALL, false),
    rule!(
        "PASTI-GOV-007",
        "Komunikasikan status independen secara tepat",
        H,
        P,
        ALL,
        false
    ),
    rule!(
        "PASTI-GOV-008",
        "Jangan mengubah sasaran menjadi hasil yang diklaim",
        TB,
        P,
        ALL,
        false
    ),
];

pub fn get(id: &str) -> Option<&'static Rule> {
    RULES.iter().find(|r| r.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_unique_and_match_schema_pattern() {
        let mut seen = HashSet::new();
        for r in RULES {
            assert!(seen.insert(r.id), "duplicate {}", r.id);
            let rest = r.id.strip_prefix("PASTI-").unwrap();
            let (group, num) = rest.split_once('-').unwrap();
            assert!(group.chars().all(|c| c.is_ascii_uppercase()), "{}", r.id);
            assert!(num.len() == 3 && num.chars().all(|c| c.is_ascii_digit()), "{}", r.id);
        }
    }
}
