//! Report aggregation, coverage and gate (PASTI-LINT-004/005, PASTI-CORE-006/007).
//!
//! Coverage is computed from per-scope results. A clean output never upgrades
//! the document status: the strongest status this tool can claim is
//! `diperiksa_otomatis` (Doc 01 §6).

use std::collections::BTreeMap;

use serde::Serialize;

use crate::config::Config;
use crate::diagnostic::{Diagnostic, Force, Outcome, Range, ReviewStatus};
use crate::glossary::GlossaryInfo;
use crate::reviews;
use crate::rules::{self, RULES, Target};

pub const TOOL_NAME: &str = "pasti-lint";
pub const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Serialize)]
pub struct Tool {
    pub name: &'static str,
    pub version: &'static str,
}

pub const TOOL: Tool = Tool {
    name: TOOL_NAME,
    version: TOOL_VERSION,
};

#[derive(Debug, Serialize)]
pub struct ProtectedPart {
    pub kind: crate::document::BlockKind,
    pub source_range: Range,
}

#[derive(Debug, Serialize)]
pub struct DocumentInfo {
    pub document_id: String,
    pub path: String,
    pub input_format: crate::document::InputFormat,
    pub adapter: &'static str,
    pub source_sha256: String,
    pub prose_blocks: usize,
    pub sentences: usize,
    pub counted_units: usize,
    pub protected: Vec<ProtectedPart>,
}

/// A ratio reported with its numerator and denominator; `value` is null when
/// the denominator is zero (never 0 % or 100 %).
#[derive(Debug, Serialize, PartialEq)]
pub struct Ratio {
    pub numerator: usize,
    pub denominator: usize,
    pub value: Option<f64>,
}

impl Ratio {
    fn new(numerator: usize, denominator: usize) -> Ratio {
        Ratio {
            numerator,
            denominator,
            value: (denominator > 0).then(|| numerator as f64 / denominator as f64),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RuleCoverage {
    pub rule_id: &'static str,
    pub title: &'static str,
    pub force: Force,
    pub automated: bool,
    pub in_gate_scope: bool,
    pub scopes_known_applicable: usize,
    pub scopes_evaluated: usize,
    pub scopes_not_evaluated: usize,
    pub scopes_applicability_unknown: usize,
    pub findings: usize,
    pub reviews_pending: usize,
    pub suppressed: usize,
    pub detection_coverage: Ratio,
    pub decision_completion: Ratio,
}

#[derive(Debug, Serialize)]
pub struct Coverage {
    /// Text and glossary rules: automated ones with counts, manual ones listed
    /// as not evaluated by this tool.
    pub rules: Vec<RuleCoverage>,
    pub mandatory_reviews_pending: usize,
    pub mandatory_checks_not_evaluated: usize,
    /// Mandatory text rules this tool does not check; they need human review.
    pub mandatory_rules_requiring_manual_review: Vec<&'static str>,
    pub suppressed_diagnostics: usize,
    pub accepted_deviations: usize,
}

#[derive(Debug, Serialize)]
pub struct GateResult {
    pub enabled: bool,
    pub passed: Option<bool>,
    /// Rules the gate judges (declared in config, or the default below).
    pub scope: Vec<&'static str>,
    pub reasons: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub report_version: u32,
    pub spec_version: &'static str,
    pub tool: Tool,
    /// Strongest status an automated run can support (Doc 01 §6).
    pub status: &'static str,
    pub status_note: &'static str,
    pub partial: bool,
    pub errors: Vec<String>,
    pub config: Config,
    pub config_source: Option<String>,
    pub documents: Vec<DocumentInfo>,
    pub glossaries: Vec<GlossaryInfo>,
    pub review_records: reviews::Summary,
    pub coverage: Coverage,
    pub gate: GateResult,
    pub diagnostics: Vec<Diagnostic>,
}

pub const STATUS_NOTE: &str = "Status ini hanya menyatakan bahwa pemeriksaan otomatis tertentu telah dijalankan. Tidak adanya diagnostik bukan bukti kesesuaian; aturan yang tidak dievaluasi tercantum pada coverage.";

/// Default scope: automated text rules, plus glossary rules only when a
/// glossary is configured (they cannot run without one). The scope is always
/// printed in the report, so nothing is excluded silently.
pub fn gate_scope(cfg: &Config) -> Vec<&'static str> {
    match &cfg.gate.scope {
        Some(ids) => RULES
            .iter()
            .filter(|r| ids.iter().any(|i| i == r.id))
            .map(|r| r.id)
            .collect(),
        None => RULES
            .iter()
            .filter(|r| r.automated && matches!(r.target, Target::Text | Target::Glossary))
            .filter(|r| {
                !cfg.glossaries.is_empty() || !matches!(r.id, "PASTI-BAS-003" | "PASTI-LEX-005" | "PASTI-LEX-007")
            })
            .map(|r| r.id)
            .collect(),
    }
}

/// Builds coverage per rule from all results (including passing ones).
pub fn coverage(diags: &[Diagnostic], cfg: &Config, records: &[reviews::Record]) -> Coverage {
    let scope = gate_scope(cfg);
    let mut by_rule: BTreeMap<&str, BTreeMap<(&str, &str), Vec<&Diagnostic>>> = BTreeMap::new();
    for d in diags {
        by_rule
            .entry(d.rule_id.as_str())
            .or_default()
            .entry((d.document_id.as_str(), d.scope_id.as_str()))
            .or_default()
            .push(d);
    }
    let mut rules_out = Vec::new();
    let (mut pending_total, mut not_eval_total) = (0, 0);
    for r in RULES
        .iter()
        .filter(|r| matches!(r.target, Target::Text | Target::Glossary))
    {
        let in_scope = scope.contains(&r.id);
        let scopes = by_rule.get(r.id);
        let (mut applicable, mut evaluated, mut not_eval, mut unknown, mut decided) = (0, 0, 0, 0, 0);
        let (mut findings, mut pending, mut suppressed) = (0, 0, 0);
        for ds in scopes.into_iter().flat_map(|m| m.values()) {
            findings += ds.iter().filter(|d| d.outcome.is_finding()).count();
            pending += ds.iter().filter(|d| d.review_status == ReviewStatus::Pending).count();
            suppressed += ds.iter().filter(|d| d.suppressed).count();
            if ds.iter().all(|d| d.outcome == Outcome::NotApplicable) {
                continue;
            }
            if ds.iter().any(|d| d.reason_code == "MODE_UNKNOWN") {
                unknown += 1;
                continue;
            }
            applicable += 1;
            if ds.iter().any(|d| d.outcome == Outcome::NotEvaluated) {
                not_eval += 1;
                continue;
            }
            evaluated += 1;
            if ds.iter().all(|d| {
                matches!(
                    d.outcome,
                    Outcome::Satisfied | Outcome::Violation | Outcome::NotApplicable
                ) && d.review_status != ReviewStatus::Pending
            }) {
                decided += 1;
            }
        }
        if in_scope && r.force.is_mandatory() {
            pending_total += pending;
            not_eval_total += not_eval + unknown;
        }
        rules_out.push(RuleCoverage {
            rule_id: r.id,
            title: r.title,
            force: r.force,
            automated: r.automated,
            in_gate_scope: in_scope,
            scopes_known_applicable: applicable,
            scopes_evaluated: evaluated,
            scopes_not_evaluated: not_eval,
            scopes_applicability_unknown: unknown,
            findings,
            reviews_pending: pending,
            suppressed,
            detection_coverage: Ratio::new(evaluated, applicable),
            decision_completion: Ratio::new(decided, applicable),
        });
    }
    // Manual-only mandatory rules in the gate scope can never be completed by this tool.
    for id in &scope {
        let r = rules::get(id).unwrap();
        if !r.automated && r.force.is_mandatory() {
            not_eval_total += 1;
        }
    }
    Coverage {
        rules: rules_out,
        mandatory_reviews_pending: pending_total,
        mandatory_checks_not_evaluated: not_eval_total,
        mandatory_rules_requiring_manual_review: RULES
            .iter()
            .filter(|r| r.target == Target::Text && !r.automated && r.force.is_mandatory())
            .map(|r| r.id)
            .collect(),
        suppressed_diagnostics: diags.iter().filter(|d| d.suppressed).count(),
        accepted_deviations: diags.iter().filter(|d| reviews::is_deviation(records, d)).count(),
    }
}

/// Gate decision (Doc 08 §6). Suppression never helps a result pass;
/// only accepted deviation records exempt a violation from `fail_on`.
pub fn gate(
    enabled: bool,
    diags: &[Diagnostic],
    cov: &Coverage,
    cfg: &Config,
    records: &[reviews::Record],
) -> GateResult {
    if !enabled {
        return GateResult {
            enabled,
            passed: None,
            scope: Vec::new(),
            reasons: Vec::new(),
        };
    }
    let scope = gate_scope(cfg);
    let mut reasons = Vec::new();
    let failing = diags
        .iter()
        .filter(|d| scope.contains(&d.rule_id.as_str()))
        .filter(|d| d.outcome.is_finding() && d.severity >= cfg.gate.fail_on && !reviews::is_deviation(records, d))
        .count();
    if failing > 0 {
        reasons.push(
            format!(
                "{failing} temuan dengan severity ≥ {:?} dalam lingkup gerbang",
                cfg.gate.fail_on
            )
            .to_lowercase(),
        );
    }
    if cfg.gate.mandatory_checks_complete {
        if cov.mandatory_reviews_pending > 0 {
            reasons.push(format!("{} telaah wajib masih tertunda", cov.mandatory_reviews_pending));
        }
        if cov.mandatory_checks_not_evaluated > 0 {
            reasons.push(format!(
                "{} pemeriksaan wajib belum dinilai",
                cov.mandatory_checks_not_evaluated
            ));
        }
    }
    GateResult {
        enabled,
        passed: Some(reasons.is_empty()),
        scope,
        reasons,
    }
}
