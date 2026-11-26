//! Effective configuration (PASTI-LINT-004).
//!
//! Merge order: profile defaults, then the project file, then command-line
//! arguments. Options select policy inputs (thresholds, lists, gate); none of
//! them changes the force of a normative rule.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::diagnostic::{Mode, SPEC_VERSION, Severity};
use crate::document::InputFormat;
use crate::segment::Abbreviations;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Config {
    pub config_version: u32,
    pub spec_version: String,
    pub profile_id: String,
    pub profile_version: String,
    pub default_mode: Mode,
    pub input_format: Option<InputFormat>,
    /// Review thresholds in counted units; `None` means not set (not evaluated).
    pub length_review: BTreeMap<Mode, Option<usize>>,
    pub gate: Gate,
    pub fix: Fix,
    pub glossaries: Vec<PathBuf>,
    pub reviews: Option<PathBuf>,
    pub abbreviations: Abbreviations,
    /// Project spelling pairs added to the built-in PASTI-GRA-001 list.
    pub spelling_pairs: Vec<SpellingPair>,
    /// Comparison phrases the project has defined; they are not reported as
    /// unmapped (PASTI-NUM-003).
    pub defined_comparisons: Vec<String>,
    /// Rules switched off by the project; reported as not_evaluated.
    pub disabled_rules: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Gate {
    pub fail_on: Severity,
    pub mandatory_checks_complete: bool,
    /// Rules within the gate's declared scope; `None` means every rule this
    /// tool evaluates automatically on text.
    #[serde(default)]
    pub scope: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Fix {
    pub automatic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SpellingPair {
    pub wrong: String,
    pub right: String,
}

impl Default for Config {
    /// Built-in profile `documentation`, mirroring the Doc 08 example.
    fn default() -> Self {
        Config {
            config_version: 1,
            spec_version: SPEC_VERSION.to_string(),
            profile_id: "documentation".into(),
            profile_version: SPEC_VERSION.to_string(),
            default_mode: Mode::Unknown,
            input_format: None,
            length_review: BTreeMap::from([
                (Mode::PROC, Some(20)),
                (Mode::WARN, Some(20)),
                (Mode::DESC, Some(25)),
                (Mode::REQ, Some(25)),
                (Mode::UI, None),
            ]),
            gate: Gate {
                fail_on: Severity::Error,
                mandatory_checks_complete: false,
                scope: None,
            },
            fix: Fix { automatic: false },
            glossaries: Vec::new(),
            reviews: None,
            abbreviations: Abbreviations::default(),
            spelling_pairs: Vec::new(),
            defined_comparisons: Vec::new(),
            disabled_rules: Vec::new(),
        }
    }
}

/// Project file layer: every field optional.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    config_version: Option<u32>,
    spec_version: Option<String>,
    profile_id: Option<String>,
    profile_version: Option<String>,
    default_mode: Option<String>,
    input_format: Option<InputFormat>,
    length_review: Option<BTreeMap<String, Option<usize>>>,
    gate: Option<FileGate>,
    fix: Option<Fix>,
    glossaries: Option<Vec<PathBuf>>,
    reviews: Option<PathBuf>,
    abbreviations: Option<Abbreviations>,
    spelling_pairs: Option<Vec<SpellingPair>>,
    defined_comparisons: Option<Vec<String>>,
    disabled_rules: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileGate {
    fail_on: Option<Severity>,
    mandatory_checks_complete: Option<bool>,
    scope: Option<Vec<String>>,
}

impl Config {
    /// Applies a project YAML file on top of `self`. Relative paths inside the
    /// file resolve against the file's directory.
    pub fn apply_file(&mut self, path: &Path) -> Result<(), String> {
        let raw = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let f: FileConfig = serde_yaml::from_str(&raw).map_err(|e| format!("{}: {e}", path.display()))?;
        let base = path.parent().unwrap_or(Path::new("."));
        let resolve = |p: PathBuf| if p.is_absolute() { p } else { base.join(p) };

        if let Some(v) = f.config_version {
            self.config_version = v;
        }
        if let Some(v) = f.spec_version {
            self.spec_version = v;
        }
        if let Some(v) = f.profile_id {
            self.profile_id = v;
        }
        if let Some(v) = f.profile_version {
            self.profile_version = v;
        }
        if let Some(v) = f.default_mode {
            self.default_mode = Mode::parse(&v).ok_or_else(|| format!("default_mode tidak dikenal: {v}"))?;
        }
        if f.input_format.is_some() {
            self.input_format = f.input_format;
        }
        if let Some(lr) = f.length_review {
            for (k, v) in lr {
                let m = Mode::parse(&k)
                    .filter(|m| *m != Mode::Unknown)
                    .ok_or_else(|| format!("length_review: mode tidak dikenal: {k}"))?;
                self.length_review.insert(m, v);
            }
        }
        if let Some(g) = f.gate {
            if let Some(v) = g.fail_on {
                self.gate.fail_on = v;
            }
            if let Some(v) = g.mandatory_checks_complete {
                self.gate.mandatory_checks_complete = v;
            }
            if g.scope.is_some() {
                self.gate.scope = g.scope;
            }
        }
        if let Some(v) = f.fix {
            self.fix = v;
        }
        if let Some(v) = f.glossaries {
            self.glossaries.extend(v.into_iter().map(resolve));
        }
        if let Some(v) = f.reviews {
            self.reviews = Some(resolve(v));
        }
        if let Some(v) = f.abbreviations {
            self.abbreviations = v;
        }
        if let Some(v) = f.spelling_pairs {
            self.spelling_pairs.extend(v);
        }
        if let Some(v) = f.defined_comparisons {
            self.defined_comparisons.extend(v);
        }
        if let Some(v) = f.disabled_rules {
            self.disabled_rules.extend(v);
        }
        Ok(())
    }

    /// Rejects configurations this implementation cannot honour.
    pub fn validate(&self) -> Result<(), String> {
        if self.config_version != 1 {
            return Err(format!(
                "config_version {} tidak didukung (hanya 1)",
                self.config_version
            ));
        }
        if self.spec_version != SPEC_VERSION {
            return Err(format!(
                "spec_version {} tidak didukung (alat ini: {SPEC_VERSION})",
                self.spec_version
            ));
        }
        if self.fix.automatic {
            return Err("fix.automatic: true belum didukung; alat ini tidak menerapkan perbaikan otomatis".into());
        }
        for id in self.disabled_rules.iter().chain(self.gate.scope.iter().flatten()) {
            if crate::rules::get(id).is_none() {
                return Err(format!("ID aturan tidak terdaftar: {id}"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doc08_example_config_loads() {
        let dir = std::env::temp_dir().join(format!("pasti-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("pasti.yaml");
        std::fs::write(
            &p,
            "config_version: 1\nspec_version: 0.1.0-draft.1\nprofile_id: documentation\nprofile_version: 0.1.0-draft.1\n\
             default_mode: DESC\nlength_review:\n  PROC: 20\n  WARN: 20\n  DESC: 25\n  REQ: 25\n  UI: null\n\
             gate:\n  fail_on: error\n  mandatory_checks_complete: false\nfix:\n  automatic: false\n",
        )
        .unwrap();
        let mut c = Config::default();
        c.apply_file(&p).unwrap();
        c.validate().unwrap();
        assert_eq!(c.default_mode, Mode::DESC);
        assert_eq!(c.length_review[&Mode::UI], None);
        std::fs::write(&p, "fix:\n  automatic: true\n").unwrap();
        let mut c = Config::default();
        c.apply_file(&p).unwrap();
        assert!(c.validate().is_err());
        std::fs::write(&p, "bogus: 1\n").unwrap();
        assert!(Config::default().apply_file(&p).is_err());
        std::fs::remove_dir_all(dir).ok();
    }
}
