//! Library entry point for `pasti lint`.

use std::path::Path;

use crate::checks::{self, Ctx};
use crate::config::Config;
use crate::diagnostic::{Outcome, sha256_hex};
use crate::document::{self, InputFormat};
use crate::glossary::Glossaries;
use crate::report::{self, DocumentInfo, ProtectedPart, Report};
use crate::reviews;
use crate::segment;

pub struct Input {
    pub path: String,
    pub document_id: String,
    /// File contents, or the read error.
    pub source: Result<String, String>,
}

pub struct Options {
    pub gate: bool,
    /// Keep `satisfied` and `not_applicable` entries in `diagnostics`.
    pub include_passing: bool,
    pub config_source: Option<String>,
}

pub fn format_for(path: &str, cfg: &Config) -> InputFormat {
    cfg.input_format
        .unwrap_or_else(|| match Path::new(path).extension().and_then(|e| e.to_str()) {
            Some("md" | "markdown" | "mdown" | "mkd") => InputFormat::Markdown,
            Some("txt" | "text") => InputFormat::Text,
            _ => InputFormat::Markdown,
        })
}

/// Lints every input. `Err` is a configuration failure (exit code 2); read or
/// parse failures of individual inputs mark the report `partial`.
pub fn lint(inputs: &[Input], cfg: Config, opts: &Options) -> Result<Report, String> {
    cfg.validate()?;
    let glossaries = Glossaries::load(&cfg.glossaries)?;
    let records = match &cfg.reviews {
        Some(p) => reviews::load(p)?,
        None => Vec::new(),
    };
    let mut errors = Vec::new();
    let mut documents = Vec::new();
    let mut diags = Vec::new();
    for input in inputs {
        let src = match &input.source {
            Ok(s) => s,
            Err(e) => {
                errors.push(format!("{}: {e}", input.path));
                continue;
            }
        };
        let fmt = format_for(&input.path, &cfg);
        let doc = match document::parse(src, fmt, cfg.default_mode) {
            Ok(d) => d,
            Err(e) => {
                errors.push(format!("{}: {}", input.path, e.0));
                continue;
            }
        };
        let map = crate::coords::SourceMap::new(src);
        let prose: Vec<_> = doc.blocks.iter().filter(|b| b.kind.is_prose()).collect();
        let sentences: Vec<_> = prose
            .iter()
            .flat_map(|b| segment::sentences(&b.text, &cfg.abbreviations))
            .collect();
        documents.push(DocumentInfo {
            document_id: input.document_id.clone(),
            path: input.path.clone(),
            input_format: fmt,
            adapter: doc.adapter(),
            source_sha256: sha256_hex(src.as_bytes()),
            prose_blocks: prose.len(),
            sentences: sentences.len(),
            counted_units: sentences.iter().map(|s| s.units.len()).sum(),
            protected: doc
                .blocks
                .iter()
                .filter(|b| !b.kind.is_prose())
                .map(|b| ProtectedPart {
                    kind: b.kind,
                    source_range: map.range(b.src.start, b.src.end),
                })
                .collect(),
        });
        diags.extend(checks::run(&Ctx {
            document_id: &input.document_id,
            src,
            doc: &doc,
            cfg: &cfg,
            glossaries: &glossaries,
        }));
    }
    let review_records = reviews::apply(&records, &mut diags);
    diags.sort_by_key(|d| d.sort_key());
    let coverage = report::coverage(&diags, &cfg, &records);
    let gate = report::gate(opts.gate, &diags, &coverage, &cfg, &records);
    if !opts.include_passing {
        diags.retain(|d| !matches!(d.outcome, Outcome::Satisfied | Outcome::NotApplicable));
    }
    Ok(Report {
        report_version: 1,
        spec_version: crate::diagnostic::SPEC_VERSION,
        tool: report::TOOL,
        status: "diperiksa_otomatis",
        status_note: report::STATUS_NOTE,
        partial: !errors.is_empty(),
        errors,
        config: cfg,
        config_source: opts.config_source.clone(),
        documents,
        glossaries: glossaries.infos,
        review_records,
        coverage,
        gate,
        diagnostics: diags,
    })
}

/// Exit code contract (Doc 08 §6): 2 for failed input or operation, 1 for a
/// failed gate, otherwise 0. Without `--gate`, findings never change the code.
pub fn exit_code(r: &Report) -> i32 {
    if r.partial {
        2
    } else if r.gate.passed == Some(false) {
        1
    } else {
        0
    }
}
