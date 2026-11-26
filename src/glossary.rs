//! Controlled vocabulary (Doc 05): loading, validation, conflict detection,
//! and form matching.
//!
//! A glossary file is JSON: `{"glossary_id", "glossary_version", "entries": [...]}`
//! where each entry follows the Doc 05 §4 schema. Schema validity only proves
//! data shape, never that a definition is right or was really approved.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::diagnostic::{SPEC_VERSION, sha256_hex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Proposed,
    Approved,
    Restricted,
    Deprecated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layer {
    Core,
    Domain,
    ExactSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pos {
    Verb,
    Noun,
    Adjective,
    Adverb,
    Connector,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Form {
    pub text: String,
    pub pos: Pos,
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub author: String,
    pub revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_record_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub spec_version: String,
    pub concept_id: Option<String>,
    pub preferred: String,
    pub status: Status,
    pub layer: Layer,
    pub domain_id: String,
    pub sense: String,
    pub forms: Vec<Form>,
    pub frames: Vec<String>,
    pub constraints: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split_from: Option<Vec<String>>,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct GlossaryFile {
    glossary_id: String,
    glossary_version: String,
    entries: Vec<Entry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlossaryInfo {
    pub glossary_id: String,
    pub glossary_version: String,
    pub path: String,
    pub sha256: String,
    pub entries: usize,
}

/// A form occurrence key: lowercase words.
type FormKey = Vec<String>;

#[derive(Debug, Clone)]
pub struct EntryRef {
    pub glossary: usize,
    pub entry: Entry,
}

#[derive(Debug, Default)]
pub struct Glossaries {
    pub infos: Vec<GlossaryInfo>,
    pub entries: Vec<EntryRef>,
    /// Lowercased form words → indices into `entries`.
    forms: BTreeMap<FormKey, BTreeSet<usize>>,
    max_words: usize,
    /// Cross-glossary conflicts (PASTI-LEX-007), as messages.
    pub conflicts: Vec<String>,
}

impl Glossaries {
    pub fn is_empty(&self) -> bool {
        self.infos.is_empty()
    }

    pub fn load(paths: &[impl AsRef<Path>]) -> Result<Glossaries, String> {
        let mut g = Glossaries::default();
        for p in paths {
            let p = p.as_ref();
            let raw = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
            let file: GlossaryFile = serde_json::from_slice(&raw).map_err(|e| format!("{}: {e}", p.display()))?;
            let gi = g.infos.len();
            for (i, e) in file.entries.iter().enumerate() {
                validate_entry(e).map_err(|m| format!("{} entri #{i} ({}): {m}", p.display(), e.preferred))?;
            }
            g.infos.push(GlossaryInfo {
                glossary_id: file.glossary_id,
                glossary_version: file.glossary_version,
                path: p.display().to_string(),
                sha256: sha256_hex(&raw),
                entries: file.entries.len(),
            });
            for entry in file.entries {
                g.entries.push(EntryRef { glossary: gi, entry });
            }
        }
        g.index();
        g.find_conflicts();
        Ok(g)
    }

    fn index(&mut self) {
        for (i, e) in self.entries.iter().enumerate() {
            let texts =
                std::iter::once(e.entry.preferred.as_str()).chain(e.entry.forms.iter().map(|f| f.text.as_str()));
            for t in texts {
                let key = form_key(t);
                if key.is_empty() {
                    continue;
                }
                self.max_words = self.max_words.max(key.len());
                self.forms.entry(key).or_default().insert(i);
            }
        }
    }

    /// LEX-007: a form or concept defined differently by different glossary
    /// files is a conflict; it is never resolved by "last file wins".
    fn find_conflicts(&mut self) {
        let mut by_concept: BTreeMap<&str, Vec<&EntryRef>> = BTreeMap::new();
        for e in &self.entries {
            if let Some(c) = e.entry.concept_id.as_deref() {
                by_concept.entry(c).or_default().push(e);
            }
        }
        for (concept, es) in &by_concept {
            for (a, b) in pairs(es) {
                if a.glossary != b.glossary && a.entry != b.entry {
                    self.conflicts.push(format!(
                        "Konsep {concept} didefinisikan berbeda oleh kamus {} («{}») dan {} («{}»); selesaikan konflik secara eksplisit.",
                        self.infos[a.glossary].glossary_id, a.entry.preferred, self.infos[b.glossary].glossary_id, b.entry.preferred
                    ));
                }
            }
        }
        for (key, idxs) in &self.forms {
            let es: Vec<&EntryRef> = idxs.iter().map(|&i| &self.entries[i]).collect();
            for (a, b) in pairs(&es) {
                let same_domain = a.entry.domain_id == b.entry.domain_id;
                if a.glossary != b.glossary && same_domain && a.entry.concept_id != b.entry.concept_id {
                    self.conflicts.push(format!(
                        "Bentuk «{}» dipetakan ke konsep berbeda ({} di {}, {} di {}) dalam domain {}.",
                        key.join(" "),
                        a.entry.concept_id.as_deref().unwrap_or("null"),
                        self.infos[a.glossary].glossary_id,
                        b.entry.concept_id.as_deref().unwrap_or("null"),
                        self.infos[b.glossary].glossary_id,
                        a.entry.domain_id
                    ));
                }
            }
        }
        self.conflicts.sort();
        self.conflicts.dedup();
    }

    /// Longest-first, case-insensitive, whole-word matches in `text`.
    /// Returns (byte range, entry indices).
    pub fn find(&self, text: &str) -> Vec<(std::ops::Range<usize>, Vec<usize>)> {
        let words = words(text);
        let mut out = Vec::new();
        let mut i = 0;
        while i < words.len() {
            let mut matched = false;
            for n in (1..=self.max_words.min(words.len() - i)).rev() {
                let key: FormKey = words[i..i + n].iter().map(|w| text[w.clone()].to_lowercase()).collect();
                // Multiword forms must be separated by plain spaces only.
                let contiguous = words[i..i + n]
                    .windows(2)
                    .all(|w| text[w[0].end..w[1].start].chars().all(char::is_whitespace));
                if !contiguous {
                    continue;
                }
                if let Some(idxs) = self.forms.get(&key) {
                    out.push((words[i].start..words[i + n - 1].end, idxs.iter().copied().collect()));
                    i += n;
                    matched = true;
                    break;
                }
            }
            if !matched {
                i += 1;
            }
        }
        out
    }
}

fn pairs<'a, T>(xs: &'a [&'a T]) -> impl Iterator<Item = (&'a T, &'a T)> {
    xs.iter()
        .enumerate()
        .flat_map(move |(i, a)| xs[i + 1..].iter().map(move |b| (*a, *b)))
}

fn form_key(t: &str) -> FormKey {
    words(t).into_iter().map(|r| t[r].to_lowercase()).collect()
}

/// Word spans: runs of alphanumerics, hyphens and underscores (so `sensor-sensor`
/// and `restart_service` are one word).
pub fn words(text: &str) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices().chain(std::iter::once((text.len(), ' '))) {
        let w = c.is_alphanumeric() || c == '_' || (c == '-' && start.is_some());
        match (w, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                let tok = text[s..i].trim_end_matches('-');
                out.push(s..s + tok.len());
                start = None;
            }
            _ => {}
        }
    }
    out
}

fn nonempty(field: &str, v: &str) -> Result<(), String> {
    if v.is_empty() {
        Err(format!("{field} kosong"))
    } else {
        Ok(())
    }
}

/// Doc 05 §4 schema plus the additional validator checks the schema cannot
/// express (restricted/deprecated must say why).
fn validate_entry(e: &Entry) -> Result<(), String> {
    if e.spec_version != SPEC_VERSION {
        return Err(format!("spec_version harus {SPEC_VERSION}"));
    }
    nonempty("preferred", &e.preferred)?;
    nonempty("domain_id", &e.domain_id)?;
    nonempty("sense", &e.sense)?;
    nonempty("provenance.author", &e.provenance.author)?;
    nonempty("provenance.revision", &e.provenance.revision)?;
    if e.forms.is_empty() {
        return Err("forms minimal satu".into());
    }
    for f in &e.forms {
        nonempty("forms[].text", &f.text)?;
        nonempty("forms[].role", &f.role)?;
    }
    if let Some(c) = &e.concept_id {
        nonempty("concept_id", c)?;
    }
    match e.layer {
        Layer::Core | Layer::Domain if e.concept_id.is_none() => {
            return Err("entri core/domain memerlukan concept_id".into());
        }
        Layer::ExactSource if e.provenance.source_id.is_none() || e.provenance.source_revision.is_none() => {
            return Err("entri exact_source memerlukan provenance.source_id dan source_revision".into());
        }
        _ => {}
    }
    if e.status != Status::Proposed && e.provenance.review_record_id.as_deref().is_none_or(str::is_empty) {
        return Err("status selain proposed memerlukan provenance.review_record_id".into());
    }
    if matches!(e.status, Status::Restricted | Status::Deprecated) && e.constraints.iter().all(|c| c.trim().is_empty())
    {
        return Err("entri restricted/deprecated memerlukan constraints yang menjelaskan batas atau alasan".into());
    }
    if let Some(u) = &e.provenance.source_url
        && !u.contains("://")
    {
        return Err("provenance.source_url bukan URI".into());
    }
    if let Some(s) = &e.split_from {
        let set: BTreeSet<_> = s.iter().collect();
        if set.len() != s.len() || s.iter().any(String::is_empty) {
            return Err("split_from harus unik dan tidak kosong".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(name: &str, body: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pasti-gl-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        std::fs::write(&p, body).unwrap();
        p
    }

    fn entry(concept: &str, preferred: &str, status: &str, extra_prov: &str) -> String {
        format!(
            r#"{{"spec_version":"0.1.0-draft.1","concept_id":"{concept}","preferred":"{preferred}","status":"{status}","layer":"core","domain_id":"general","sense":"s","forms":[{{"text":"{preferred}","pos":"verb","role":"imperative"}}],"frames":[],"constraints":["alasan"],"provenance":{{"author":"a","revision":"r"{extra_prov}}}}}"#
        )
    }

    #[test]
    fn schema_rules_are_enforced() {
        let ok = write(
            "ok.json",
            &format!(
                r#"{{"glossary_id":"g","glossary_version":"1","entries":[{}]}}"#,
                entry("C.A", "periksa", "proposed", "")
            ),
        );
        assert!(Glossaries::load(&[ok]).is_ok());
        let no_review = write(
            "bad.json",
            &format!(
                r#"{{"glossary_id":"g","glossary_version":"1","entries":[{}]}}"#,
                entry("C.A", "periksa", "approved", "")
            ),
        );
        assert!(Glossaries::load(&[no_review]).unwrap_err().contains("review_record_id"));
    }

    #[test]
    fn cross_file_conflicts_are_reported_not_resolved() {
        let a = write(
            "a.json",
            &format!(
                r#"{{"glossary_id":"a","glossary_version":"1","entries":[{}]}}"#,
                entry("C.PORT", "port", "proposed", "")
            ),
        );
        let b = write(
            "b.json",
            &format!(
                r#"{{"glossary_id":"b","glossary_version":"1","entries":[{}]}}"#,
                entry("C.PORT2", "port", "proposed", "")
            ),
        );
        let g = Glossaries::load(&[a, b]).unwrap();
        assert_eq!(g.conflicts.len(), 1, "{:?}", g.conflicts);
    }

    #[test]
    fn longest_multiword_match_wins() {
        let body = format!(
            r#"{{"glossary_id":"g","glossary_version":"1","entries":[{},{}]}}"#,
            entry("C.T", "batas waktu", "proposed", ""),
            entry("C.B", "batas", "proposed", "")
        );
        let g = Glossaries::load(&[write("m.json", &body)]).unwrap();
        let text = "Atur Batas waktu dan batas.";
        let m = g.find(text);
        assert_eq!(m.len(), 2);
        assert_eq!(&text[m[0].0.clone()], "Batas waktu");
        assert_eq!(&text[m[1].0.clone()], "batas");
    }
}
