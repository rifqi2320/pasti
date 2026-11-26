//! Diagnostic contract (PASTI-LINT-003, Doc 08 §4).
//!
//! `force`, `outcome`, `severity` and `certainty` are independent fields and
//! never substitute for one another. Certainty is a reasoned category, not a
//! calibrated probability.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::rules::{self, Rule};

pub const SPEC_VERSION: &str = "0.1.0-draft.1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Mode {
    PROC,
    DESC,
    REQ,
    WARN,
    UI,
    #[serde(rename = "unknown")]
    Unknown,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Mode> {
        Some(match s.trim().to_ascii_uppercase().as_str() {
            "PROC" => Mode::PROC,
            "DESC" => Mode::DESC,
            "REQ" => Mode::REQ,
            "WARN" => Mode::WARN,
            "UI" => Mode::UI,
            "UNKNOWN" => Mode::Unknown,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Mode::PROC => "PROC",
            Mode::DESC => "DESC",
            Mode::REQ => "REQ",
            Mode::WARN => "WARN",
            Mode::UI => "UI",
            Mode::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Force {
    #[serde(rename = "HARUS")]
    Harus,
    #[serde(rename = "TIDAK BOLEH")]
    TidakBoleh,
    #[serde(rename = "SEBAIKNYA")]
    Sebaiknya,
    #[serde(rename = "BOLEH")]
    Boleh,
}

impl Force {
    /// HARUS and TIDAK BOLEH are mandatory; the others are not.
    pub fn is_mandatory(self) -> bool {
        matches!(self, Force::Harus | Force::TidakBoleh)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Violation,
    PossibleViolation,
    Satisfied,
    NotEvaluated,
    NotApplicable,
}

impl Outcome {
    pub fn is_finding(self) -> bool {
        matches!(self, Outcome::Violation | Outcome::PossibleViolation)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl Severity {
    pub fn parse(s: &str) -> Option<Severity> {
        Some(match s {
            "info" => Severity::Info,
            "warning" => Severity::Warning,
            "error" => Severity::Error,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Certainty {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    Deterministic,
    Heuristic,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Autofix {
    Forbidden,
    ReviewRequired,
    Safe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    NotRequired,
    Pending,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    /// 1-based line.
    pub line: usize,
    /// 0-based code-point column.
    pub column_cp: usize,
    /// 0-based code-point offset from the start of the source.
    pub offset_cp: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

/// One result entry, field-for-field the Doc 08 JSON Schema
/// (`additionalProperties: false`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    pub diagnostic_id: String,
    pub spec_version: String,
    pub rule_id: String,
    pub rule_revision: String,
    pub category: String,
    pub document_id: String,
    pub scope_id: String,
    pub mode: Mode,
    pub source_range: Option<Range>,
    pub force: Force,
    pub outcome: Outcome,
    pub severity: Severity,
    pub certainty: Certainty,
    pub method: Method,
    pub message: String,
    pub reason_code: String,
    pub autofix: Autofix,
    pub suppressed: bool,
    pub review_status: ReviewStatus,
    pub review_record_id: Option<String>,
}

/// Severity policy used across all checks, so priority is never chosen ad hoc:
/// a definite violation is `error`; a candidate needing review, or a mandatory
/// check that could not run, is `warning`; everything else is `info`.
pub fn default_severity(outcome: Outcome, certainty: Certainty, force: Force) -> Severity {
    match outcome {
        Outcome::Violation if certainty == Certainty::High => Severity::Error,
        Outcome::Violation | Outcome::PossibleViolation => Severity::Warning,
        Outcome::NotEvaluated if force.is_mandatory() => Severity::Warning,
        _ => Severity::Info,
    }
}

/// Inputs for [`Diagnostic::new`]; keeps call sites explicit about every
/// contract field that is not derived from the rule table.
pub struct Finding<'a> {
    pub rule_id: &'a str,
    pub category: &'a str,
    pub document_id: &'a str,
    pub scope_id: String,
    pub mode: Mode,
    pub source_range: Option<Range>,
    pub outcome: Outcome,
    pub certainty: Certainty,
    pub method: Method,
    pub message: String,
    pub reason_code: &'a str,
    pub autofix: Autofix,
}

impl Diagnostic {
    /// Builds a diagnostic with force/revision from the rule table, severity
    /// from [`default_severity`], and review status `pending` for candidates
    /// (`possible_violation` always needs a human decision).
    ///
    /// Panics if `rule_id` is not registered: an unregistered rule ID is a
    /// programming error, never user input.
    pub fn new(f: Finding<'_>) -> Diagnostic {
        let rule: &Rule = rules::get(f.rule_id).unwrap_or_else(|| panic!("unregistered rule {}", f.rule_id));
        let severity = default_severity(f.outcome, f.certainty, rule.force);
        let review_status = if f.outcome == Outcome::PossibleViolation {
            ReviewStatus::Pending
        } else {
            ReviewStatus::NotRequired
        };
        let mut d = Diagnostic {
            diagnostic_id: String::new(),
            spec_version: SPEC_VERSION.to_string(),
            rule_id: rule.id.to_string(),
            rule_revision: rule.revision.to_string(),
            category: f.category.to_string(),
            document_id: f.document_id.to_string(),
            scope_id: f.scope_id,
            mode: f.mode,
            source_range: f.source_range,
            force: rule.force,
            outcome: f.outcome,
            severity,
            certainty: f.certainty,
            method: f.method,
            message: f.message,
            reason_code: f.reason_code.to_string(),
            autofix: f.autofix,
            suppressed: false,
            review_status,
            review_record_id: None,
        };
        d.diagnostic_id = d.stable_id();
        d
    }

    /// Deterministic ID: identical source, config and components yield the
    /// same ID (PASTI-LINT-004 repeatability).
    fn stable_id(&self) -> String {
        let (s, e) = self
            .source_range
            .map(|r| (r.start.offset_cp, r.end.offset_cp))
            .unwrap_or((0, 0));
        let key = format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
            self.document_id, self.rule_id, self.category, self.scope_id, s, e, self.reason_code
        );
        format!("D-{}", &sha256_hex(key.as_bytes())[..16])
    }

    /// Canonical ordering: document, start offset, rule ID, category
    /// (Doc 08 §5). Document-level results (no range) sort first.
    pub fn sort_key(&self) -> (String, Option<usize>, String, String, String) {
        (
            self.document_id.clone(),
            self.source_range.map(|r| r.start.offset_cp),
            self.rule_id.clone(),
            self.category.clone(),
            self.diagnostic_id.clone(),
        )
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest.iter() {
        out.push_str(&format!("{b:02x}"));
    }
    out
}
