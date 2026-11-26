//! Runs the automated text rules over one document.
//!
//! Every automated rule yields a result for every scope it applies to,
//! including `satisfied`, `not_applicable` and `not_evaluated`, so coverage is
//! computed from evidence rather than from silence (PASTI-CORE-007).

use std::ops::Range;

use crate::config::Config;
use crate::coords::SourceMap;
use crate::diagnostic::{Autofix, Certainty, Diagnostic, Finding, Method, Mode, Outcome, Severity, sha256_hex};
use crate::document::{Block, BlockKind, Document};
use crate::glossary::{Glossaries, Layer, Status};
use crate::patterns::{self, Hit};
use crate::segment::{self, Sentence};

/// Rules evaluated per sentence by pattern detectors, with their categories.
const PATTERN_RULES: &[(&str, &str)] = &[
    ("PASTI-NUM-001", "number.format"),
    ("PASTI-NUM-002", "number.unit"),
    ("PASTI-NUM-003", "number.comparison"),
    ("PASTI-NUM-004", "number.range"),
    ("PASTI-GRA-001", "spelling.affix"),
    ("PASTI-GRA-011", "spelling.reduplication"),
];

pub struct Ctx<'a> {
    pub document_id: &'a str,
    pub src: &'a str,
    pub doc: &'a Document,
    pub cfg: &'a Config,
    pub glossaries: &'a Glossaries,
}

struct Out<'a> {
    ctx: &'a Ctx<'a>,
    map: SourceMap<'a>,
    diags: Vec<Diagnostic>,
}

impl Out<'_> {
    fn enabled(&self, rule: &str) -> bool {
        !self.ctx.cfg.disabled_rules.iter().any(|r| r == rule)
    }

    #[allow(clippy::too_many_arguments)]
    fn push(
        &mut self,
        rule: &str,
        category: &str,
        scope_id: &str,
        mode: Mode,
        src: Option<Range<usize>>,
        outcome: Outcome,
        certainty: Certainty,
        method: Method,
        message: String,
        reason: &str,
        autofix: Autofix,
    ) -> &mut Diagnostic {
        self.diags.push(Diagnostic::new(Finding {
            rule_id: rule,
            category,
            document_id: self.ctx.document_id,
            scope_id: scope_id.to_string(),
            mode,
            source_range: src.map(|r| self.map.range(r.start, r.end)),
            outcome,
            certainty,
            method,
            message,
            reason_code: reason,
            autofix,
        }));
        self.diags.last_mut().unwrap()
    }

    #[allow(clippy::too_many_arguments)]
    fn hit(&mut self, rule: &str, category: &str, scope: &str, mode: Mode, block: &Block, base: usize, h: Hit) {
        let r = block.src_range(base + h.range.start..base + h.range.end);
        self.push(
            rule,
            category,
            scope,
            mode,
            Some(r),
            h.outcome,
            h.certainty,
            h.method,
            h.message,
            h.reason,
            h.autofix,
        );
    }

    fn satisfied(&mut self, rule: &str, category: &str, scope: &str, mode: Mode, r: Range<usize>, method: Method) {
        self.push(rule, category, scope, mode, Some(r), Outcome::Satisfied, Certainty::High, method, "Pemeriksaan otomatis aturan ini tidak menemukan masalah pada lingkup ini; hasil ini tidak menilai aturan lain.".into(), "CHECK_PASSED", Autofix::Forbidden);
    }
}

fn scope_id(prefix: &str, text: &str) -> String {
    format!("{prefix}:{}", &sha256_hex(text.as_bytes())[..16])
}

pub fn run(ctx: &Ctx<'_>) -> Vec<Diagnostic> {
    let mut out = Out {
        ctx,
        map: SourceMap::new(ctx.src),
        diags: Vec::new(),
    };
    let cfg = ctx.cfg;

    for rule in &cfg.disabled_rules {
        out.push(
            rule,
            "config.disabled",
            "doc",
            cfg.default_mode,
            None,
            Outcome::NotEvaluated,
            Certainty::High,
            Method::Deterministic,
            format!(
                "{rule} dinonaktifkan oleh konfigurasi proyek; aturan tidak diperiksa dan tetap terlihat dalam cakupan."
            ),
            "DISABLED_BY_CONFIG",
            Autofix::Forbidden,
        );
    }
    let glossary_on = !ctx.glossaries.is_empty();
    if !glossary_on {
        for rule in ["PASTI-BAS-003", "PASTI-LEX-005"] {
            if out.enabled(rule) {
                out.push(
                    rule,
                    "term.glossary",
                    "doc",
                    cfg.default_mode,
                    None,
                    Outcome::NotEvaluated,
                    Certainty::High,
                    Method::Deterministic,
                    "Tidak ada kamus yang dimuat; pemeriksaan istilah belum dilakukan.".into(),
                    "NO_GLOSSARY",
                    Autofix::Forbidden,
                );
            }
        }
    }
    for c in &ctx.glossaries.conflicts {
        if out.enabled("PASTI-LEX-007") {
            out.push(
                "PASTI-LEX-007",
                "term.conflict",
                "glossaries",
                cfg.default_mode,
                None,
                Outcome::Violation,
                Certainty::High,
                Method::Deterministic,
                c.clone(),
                "LEX_GLOSSARY_CONFLICT",
                Autofix::Forbidden,
            );
        }
    }

    for block in &ctx.doc.blocks {
        let block_text = &ctx.src[block.src.clone()];
        let bscope = scope_id(&format!("b{}", block.index), block_text);
        if !block.kind.is_prose() {
            protected_block(&mut out, block, &bscope);
            continue;
        }
        let exact = exact_ranges(ctx.glossaries, &block.text);
        let sentences = segment::sentences(&block.text, &cfg.abbreviations);
        let mode_unknown_reported = &mut false;
        for (si, s) in sentences.iter().enumerate() {
            let src_r = block.src_range(s.vis.clone());
            let scope = scope_id(&format!("b{}.s{}", block.index, si), &ctx.src[src_r.clone()]);
            let text = &block.text[s.vis.clone()];

            if out.enabled("PASTI-TOK-005") {
                length(
                    &mut out,
                    block,
                    s,
                    &scope,
                    src_r.clone(),
                    mode_unknown_reported,
                    &bscope,
                );
            }
            if out.enabled("PASTI-TOK-002") {
                if s.ambiguous_abbrevs.is_empty() {
                    out.satisfied(
                        "PASTI-TOK-002",
                        "segmentation.boundary",
                        &scope,
                        block.mode,
                        src_r.clone(),
                        Method::Deterministic,
                    );
                }
                for a in &s.ambiguous_abbrevs {
                    let r = block.src_range(a.clone());
                    let msg = format!(
                        "«{}» dapat mengakhiri kalimat atau tidak; batas kalimat tidak ditebak. Hitungan panjang memakai kalimat yang tidak dipecah; telaah batasnya.",
                        &ctx.src[r.clone()]
                    );
                    out.push(
                        "PASTI-TOK-002",
                        "segmentation.boundary",
                        &scope,
                        block.mode,
                        Some(r),
                        Outcome::PossibleViolation,
                        Certainty::Low,
                        Method::Deterministic,
                        msg,
                        "SEGMENT_AMBIGUOUS_ABBREVIATION",
                        Autofix::Forbidden,
                    );
                }
            }
            for &(rule, category) in PATTERN_RULES {
                if !out.enabled(rule) {
                    continue;
                }
                let extra: Vec<(String, String)> = cfg
                    .spelling_pairs
                    .iter()
                    .map(|p| (p.wrong.clone(), p.right.clone()))
                    .collect();
                let hits: Vec<Hit> = match rule {
                    "PASTI-NUM-001" => patterns::number_format(text),
                    "PASTI-NUM-002" => patterns::unit_symbols(text),
                    "PASTI-NUM-003" => patterns::comparisons(text, &cfg.defined_comparisons),
                    "PASTI-NUM-004" => patterns::ranges(text, block.kind == BlockKind::TableCell),
                    "PASTI-GRA-001" => patterns::spelling(text, &extra),
                    "PASTI-GRA-011" => patterns::reduplication(text),
                    _ => unreachable!(),
                };
                // Exact-source forms (product labels, identifiers) are never re-spelled.
                let hits: Vec<Hit> = hits
                    .into_iter()
                    .filter(|h| !overlaps(&exact, s.vis.start + h.range.start..s.vis.start + h.range.end))
                    .collect();
                if hits.is_empty() {
                    out.satisfied(rule, category, &scope, block.mode, src_r.clone(), Method::Deterministic);
                }
                for h in hits {
                    out.hit(rule, category, &scope, block.mode, block, s.vis.start, h);
                }
            }
            if glossary_on {
                terms(&mut out, block, s, &scope, src_r.clone());
            }
        }
    }
    out.diags
}

/// Protected or unsupported blocks: kept verbatim, counted in coverage.
fn protected_block(out: &mut Out<'_>, block: &Block, bscope: &str) {
    let r = Some(block.src.clone());
    match block.kind {
        BlockKind::Html => {
            if out.enabled("PASTI-TOK-001") {
                out.push(
                    "PASTI-TOK-001",
                    "input.unsupported",
                    bscope,
                    block.mode,
                    r,
                    Outcome::NotEvaluated,
                    Certainty::High,
                    Method::Deterministic,
                    "Blok HTML mentah tidak diurai sebagai prosa; isinya belum diperiksa.".into(),
                    "UNSUPPORTED_BLOCK",
                    Autofix::Forbidden,
                );
            }
        }
        BlockKind::Metadata => {}
        _ => {
            if out.enabled("PASTI-TOK-005") {
                let what = match block.kind {
                    BlockKind::CodeBlock => "Blok kode",
                    BlockKind::ExactQuote => "Kutipan sumber persis",
                    _ => "Persamaan mandiri",
                };
                out.push("PASTI-TOK-005", "length.protected", bscope, block.mode, r, Outcome::NotApplicable, Certainty::High,
                    Method::Deterministic, format!("{what} terlindungi; ambang panjang prosa tidak berlaku. Bagian ini tercatat dalam cakupan, bukan dinilai benar."),
                    "PROTECTED_BLOCK", Autofix::Forbidden);
            }
        }
    }
}

fn length(
    out: &mut Out<'_>,
    block: &Block,
    s: &Sentence,
    scope: &str,
    src_r: Range<usize>,
    mode_unknown_reported: &mut bool,
    bscope: &str,
) {
    let mode = block.mode;
    let n = s.units.len();
    let cat = format!("length.{}", mode.as_str().to_lowercase());
    if block.kind == BlockKind::Heading {
        out.push(
            "PASTI-TOK-005",
            "length.heading",
            scope,
            mode,
            Some(src_r),
            Outcome::NotApplicable,
            Certainty::High,
            Method::Deterministic,
            format!("Judul ({n} unit) bukan kalimat; ambang panjang tidak berlaku."),
            "HEADING_NOT_SENTENCE",
            Autofix::Forbidden,
        );
        return;
    }
    if mode == Mode::Unknown {
        // Reported once per block; never pick a threshold silently.
        if !*mode_unknown_reported {
            *mode_unknown_reported = true;
            out.push("PASTI-TOK-005", "length.unknown", bscope, mode, Some(block.src.clone()), Outcome::NotEvaluated, Certainty::High,
                Method::Deterministic,
                "Jenis teks blok ini belum ditetapkan; ambang panjang tidak dipilih secara diam-diam. Tetapkan default_mode atau direktif <!-- pasti: mode=… -->.".into(),
                "MODE_UNKNOWN", Autofix::Forbidden);
        }
        return;
    }
    let Some(limit) = out.ctx.cfg.length_review.get(&mode).copied().flatten() else {
        let d = out.push(
            "PASTI-TOK-005",
            &cat,
            scope,
            mode,
            Some(src_r),
            Outcome::NotEvaluated,
            Certainty::High,
            Method::Deterministic,
            format!(
                "Ambang panjang untuk {} belum ditetapkan profil; pemeriksaan panjang tidak dinilai.",
                mode.as_str()
            ),
            "THRESHOLD_UNSET",
            Autofix::Forbidden,
        );
        d.severity = Severity::Info;
        return;
    };
    if n > limit {
        let fragment = if s.terminated {
            ""
        } else {
            " (fragmen tanpa tanda akhir)"
        };
        out.push("PASTI-TOK-005", &cat, scope, mode, Some(src_r), Outcome::PossibleViolation, Certainty::High, Method::Deterministic,
            format!("{n} kata{fragment}; ambang telaah {} adalah {limit}. Telaah apakah kalimat dapat dipecah tanpa mengubah syarat, urutan, atau nama komponen; panjang saja bukan pelanggaran.", mode.as_str()),
            "LENGTH_OVER_THRESHOLD", Autofix::Forbidden);
    } else {
        out.push(
            "PASTI-TOK-005",
            &cat,
            scope,
            mode,
            Some(src_r),
            Outcome::Satisfied,
            Certainty::High,
            Method::Deterministic,
            format!("{n} kata; tidak melewati ambang telaah {} ({limit}).", mode.as_str()),
            "LENGTH_WITHIN_THRESHOLD",
            Autofix::Forbidden,
        );
    }
}

/// Glossary-based term checks: PASTI-BAS-003 (status) and PASTI-LEX-005 (polysemy).
fn terms(out: &mut Out<'_>, block: &Block, s: &Sentence, scope: &str, src_r: Range<usize>) {
    let text = &block.text[s.vis.clone()];
    let g = out.ctx.glossaries;
    let mut any = (false, false);
    for (r, idxs) in g.find(text) {
        let entries: Vec<_> = idxs.iter().map(|&i| &g.entries[i].entry).collect();
        if entries.iter().any(|e| e.layer == Layer::ExactSource) {
            continue;
        }
        let form = &text[r.clone()];
        let src = block.src_range(s.vis.start + r.start..s.vis.start + r.end);
        let mut concepts: Vec<&str> = entries.iter().filter_map(|e| e.concept_id.as_deref()).collect();
        concepts.sort();
        concepts.dedup();
        if concepts.len() > 1 {
            any.1 = true;
            if out.enabled("PASTI-LEX-005") {
                out.push("PASTI-LEX-005", "term.polysemy", scope, block.mode, Some(src), Outcome::PossibleViolation, Certainty::Medium,
                    Method::Heuristic,
                    format!("«{form}» mempunyai beberapa makna dalam kamus ({}); pastikan konteks menyatakan makna yang dimaksud.", concepts.join(", ")),
                    "LEX_POLYSEMOUS_FORM", Autofix::Forbidden);
            }
            continue;
        }
        if !out.enabled("PASTI-BAS-003") {
            continue;
        }
        let e = entries[0];
        let concept = e.concept_id.as_deref().unwrap_or("-");
        any.0 = true;
        let (outcome, certainty, reason, msg) = match e.status {
            Status::Approved => (
                Outcome::Satisfied,
                Certainty::High,
                "LEX_TERM_APPROVED",
                format!("«{form}» disetujui untuk {concept} dalam cakupan entrinya."),
            ),
            Status::Proposed => (
                Outcome::NotEvaluated,
                Certainty::High,
                "LEX_TERM_PROPOSED",
                format!("«{form}» ({concept}) masih berstatus proposed; persetujuan istilah belum dapat dinilai."),
            ),
            Status::Restricted => (
                Outcome::PossibleViolation,
                Certainty::Medium,
                "LEX_TERM_RESTRICTED",
                format!(
                    "«{form}» ({concept}) berstatus restricted: {}. Pastikan konteks memenuhi batas tersebut.",
                    e.constraints.join(" ")
                ),
            ),
            Status::Deprecated => (
                Outcome::Violation,
                Certainty::High,
                "LEX_TERM_DEPRECATED",
                format!(
                    "«{form}» ({concept}) berstatus deprecated: {} Gunakan istilah pengganti yang ditetapkan kecuali ini kutipan sumber persis.",
                    e.constraints.join(" ")
                ),
            ),
        };
        let d = out.push(
            "PASTI-BAS-003",
            "term.status",
            scope,
            block.mode,
            Some(src),
            outcome,
            certainty,
            Method::Deterministic,
            msg,
            reason,
            Autofix::Forbidden,
        );
        if outcome == Outcome::NotEvaluated {
            d.severity = Severity::Info;
        }
    }
    if !any.0 && out.enabled("PASTI-BAS-003") {
        out.push(
            "PASTI-BAS-003",
            "term.status",
            scope,
            block.mode,
            Some(src_r.clone()),
            Outcome::NotApplicable,
            Certainty::High,
            Method::Deterministic,
            "Tidak ada istilah kamus pada kalimat ini.".into(),
            "NO_GLOSSARY_TERM",
            Autofix::Forbidden,
        );
    }
    if !any.1 && out.enabled("PASTI-LEX-005") {
        out.satisfied(
            "PASTI-LEX-005",
            "term.polysemy",
            scope,
            block.mode,
            src_r,
            Method::Deterministic,
        );
    }
}

fn exact_ranges(g: &Glossaries, text: &str) -> Vec<Range<usize>> {
    g.find(text)
        .into_iter()
        .filter(|(_, idxs)| idxs.iter().any(|&i| g.entries[i].entry.layer == Layer::ExactSource))
        .map(|(r, _)| r)
        .collect()
}

fn overlaps(set: &[Range<usize>], r: Range<usize>) -> bool {
    set.iter().any(|x| x.start < r.end && r.start < x.end)
}
