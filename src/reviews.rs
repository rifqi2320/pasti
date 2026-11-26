//! Human decision records (Doc 07 §4–5, PASTI-EXC-001..004, PASTI-LINT-003).
//!
//! Records are written by people, never by this tool. A record binds to a
//! rule and to the exact source text of a scope through `scope_hash` (the
//! hash embedded in every diagnostic's `scope_id`). Editing that text makes
//! the record stale; stale records are listed, never silently applied.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::diagnostic::{Diagnostic, Outcome, ReviewStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Review decision on a finding (e.g. a long sentence kept after review).
    Review,
    /// Exception permitted by the rule text; the rule is satisfied.
    Exception,
    /// Accepted deviation from a mandatory rule; the finding stays a violation.
    Deviation,
    /// Display suppression only; changes nothing about the result.
    Suppression,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Proposed,
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Satisfied,
    Violation,
    NotApplicable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub record_id: String,
    pub kind: Kind,
    pub state: State,
    pub rule_id: String,
    /// Hash part of the diagnostic `scope_id` (after `:`), or a prefix of it
    /// with at least 12 hex digits.
    pub scope_hash: String,
    #[serde(default)]
    pub document_id: Option<String>,
    /// Required for `review`.
    #[serde(default)]
    pub decision: Option<Decision>,
    /// Rule clause that permits an exception (required for `exception`).
    #[serde(default)]
    pub basis: Option<String>,
    pub reason: String,
    #[serde(default)]
    pub reviewed_by: Vec<String>,
    #[serde(default)]
    pub reviewed_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    records: Vec<Record>,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct Summary {
    pub applied: Vec<String>,
    /// Records that match no current scope (source changed or wrong hash).
    pub stale: Vec<String>,
    /// Proposed or rejected records, which have no effect.
    pub not_applied: Vec<String>,
}

pub fn load(path: &Path) -> Result<Vec<Record>, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let f: File = serde_yaml::from_str(&raw).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut ids = std::collections::BTreeSet::new();
    for r in &f.records {
        let ctx = |m: &str| format!("{}: rekaman {}: {m}", path.display(), r.record_id);
        if !ids.insert(r.record_id.clone()) {
            return Err(ctx("record_id ganda"));
        }
        if crate::rules::get(&r.rule_id).is_none() {
            return Err(ctx("rule_id tidak terdaftar"));
        }
        if r.scope_hash.len() < 12 || !r.scope_hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(ctx("scope_hash harus sekurang-kurangnya 12 digit heksadesimal"));
        }
        if r.reason.trim().is_empty() {
            return Err(ctx("reason wajib diisi"));
        }
        if r.state == State::Accepted && r.reviewed_by.iter().all(|s| s.trim().is_empty()) {
            // PASTI-GOV-001: every acceptance names who is responsible.
            return Err(ctx("rekaman accepted memerlukan reviewed_by"));
        }
        match r.kind {
            Kind::Review if r.decision.is_none() => return Err(ctx("rekaman review memerlukan decision")),
            Kind::Exception if r.basis.as_deref().is_none_or(|b| b.trim().is_empty()) => {
                return Err(ctx("rekaman exception memerlukan basis (klausul yang mengizinkan)"));
            }
            _ => {}
        }
    }
    Ok(f.records)
}

fn scope_hash(scope_id: &str) -> Option<&str> {
    scope_id.rsplit_once(':').map(|(_, h)| h)
}

/// Applies accepted records to matching diagnostics in place.
pub fn apply(records: &[Record], diags: &mut [Diagnostic]) -> Summary {
    let mut s = Summary::default();
    for r in records {
        if r.state != State::Accepted {
            s.not_applied.push(r.record_id.clone());
            continue;
        }
        let mut hit = false;
        for d in diags.iter_mut() {
            let same_doc = r.document_id.as_deref().is_none_or(|id| id == d.document_id);
            let same_scope =
                scope_hash(&d.scope_id).is_some_and(|h| h.starts_with(&r.scope_hash) || r.scope_hash.starts_with(h));
            if d.rule_id != r.rule_id || !same_doc || !same_scope {
                continue;
            }
            hit = true;
            match r.kind {
                Kind::Suppression => d.suppressed = true,
                Kind::Review | Kind::Exception | Kind::Deviation if d.outcome.is_finding() => {
                    d.review_status = ReviewStatus::Completed;
                    d.review_record_id = Some(r.record_id.clone());
                    d.outcome = match (r.kind, r.decision) {
                        (Kind::Exception, _) => Outcome::Satisfied,
                        (Kind::Deviation, _) => Outcome::Violation,
                        (_, Some(Decision::Satisfied)) => Outcome::Satisfied,
                        (_, Some(Decision::NotApplicable)) => Outcome::NotApplicable,
                        _ => Outcome::Violation,
                    };
                    if !d.outcome.is_finding() {
                        d.severity = crate::diagnostic::Severity::Info;
                    }
                }
                _ => {}
            }
        }
        if hit {
            s.applied.push(r.record_id.clone())
        } else {
            s.stale.push(r.record_id.clone())
        }
    }
    s
}

/// Deviation records accepted for a diagnostic (report status input).
pub fn is_deviation(records: &[Record], d: &Diagnostic) -> bool {
    d.review_record_id
        .as_ref()
        .is_some_and(|id| records.iter().any(|r| &r.record_id == id && r.kind == Kind::Deviation))
}
