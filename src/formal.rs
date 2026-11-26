//! Experimental formal profile PASTI-FOR-EX01 (Doc 10).
//!
//! A closed input language: one unconditional command, or one numeric
//! comparison restricting one command. Input is parsed by a hand-written
//! scanner over the exact Doc 10 §2 grammar (unambiguous, so at most one
//! tree), bound against a validated registry (FOR-002), typed without any
//! unit conversion (FOR-003), and checked for measurement ownership
//! (FOR-004). An AST is emitted only when no violation exists.
//!
//! Nothing here executes the operations in an AST (FOR-006): the AST and the
//! canonical formatter are representations only.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::coords::SourceMap;
use crate::diagnostic::{Autofix, Certainty, Diagnostic, Finding, Method, Mode, Outcome, Range, Severity};

pub const AST_VERSION: &str = "0.1.0-draft.1";
pub const PROFILE_ID: &str = "PASTI-FOR-EX01";

/// Implementation resource limit. Inputs above it are reported as
/// `not_evaluated` (FOR_PARSER_FAILURE), never as a language violation.
pub const MAX_INPUT_BYTES: usize = 64 * 1024;

// ---------------------------------------------------------------------------
// Vocabulary shared by registry, AST, parser and formatter.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Unit {
    #[serde(rename = "°C")]
    Celsius,
    #[serde(rename = "kPa")]
    Kilopascal,
    #[serde(rename = "bar")]
    Bar,
    #[serde(rename = "V")]
    Volt,
}

const UNITS: &[(&str, Unit)] = &[
    ("°C", Unit::Celsius),
    ("kPa", Unit::Kilopascal),
    ("bar", Unit::Bar),
    ("V", Unit::Volt),
];

impl Unit {
    pub fn as_str(self) -> &'static str {
        match self {
            Unit::Celsius => "°C",
            Unit::Kilopascal => "kPa",
            Unit::Bar => "bar",
            Unit::Volt => "V",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Power,
    Position,
}

impl Capability {
    pub fn as_str(self) -> &'static str {
        match self {
            Capability::Power => "power",
            Capability::Position => "position",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operator {
    Gt,
    Lt,
    Eq,
    Ge,
    Le,
}

const COMPARISONS: &[(&str, Operator)] = &[
    ("melebihi", Operator::Gt),
    ("kurang dari", Operator::Lt),
    ("sama dengan", Operator::Eq),
    ("sekurang-kurangnya", Operator::Ge),
    ("paling banyak", Operator::Le),
];

impl Operator {
    /// Input phrase of the operator (Doc 10 §4 table).
    pub fn phrase(self) -> &'static str {
        match self {
            Operator::Gt => "melebihi",
            Operator::Lt => "kurang dari",
            Operator::Eq => "sama dengan",
            Operator::Ge => "sekurang-kurangnya",
            Operator::Le => "paling banyak",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    PowerOff,
    PowerOn,
    Open,
    Close,
}

const ACTIONS_LOWER: &[(&str, Operation)] = &[
    ("matikan", Operation::PowerOff),
    ("nyalakan", Operation::PowerOn),
    ("buka", Operation::Open),
    ("tutup", Operation::Close),
];

impl Operation {
    /// Lower-case verb used after a condition.
    pub fn verb(self) -> &'static str {
        match self {
            Operation::PowerOff => "matikan",
            Operation::PowerOn => "nyalakan",
            Operation::Open => "buka",
            Operation::Close => "tutup",
        }
    }

    /// Capitalized verb used in an unconditional instruction.
    pub fn verb_upper(self) -> &'static str {
        match self {
            Operation::PowerOff => "Matikan",
            Operation::PowerOn => "Nyalakan",
            Operation::Open => "Buka",
            Operation::Close => "Tutup",
        }
    }

    /// Capability the target entity must have (Doc 10 §5 table).
    pub fn required_capability(self) -> Capability {
        match self {
            Operation::PowerOff | Operation::PowerOn => Capability::Power,
            Operation::Open | Operation::Close => Capability::Position,
        }
    }
}

/// `identifier = lower, { lower | digit | "_" }`.
fn is_identifier(s: &str) -> bool {
    let mut bytes = s.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z')) && bytes.all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_'))
}

// ---------------------------------------------------------------------------
// Registry (FOR-002).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entity {
    pub id: String,
    pub label: String,
    pub concept_id: String,
    pub entity_type: String,
    pub capabilities: Vec<Capability>,
}

impl Entity {
    pub fn has(&self, capability: Capability) -> bool {
        self.capabilities.contains(&capability)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Measurement {
    pub id: String,
    /// ID of the owning entity.
    pub owner: String,
    pub quantity_kind: String,
    pub dimension: String,
    pub unit: Unit,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RegistryFile {
    registry_version: String,
    entities: Vec<Entity>,
    measurements: Vec<Measurement>,
}

/// A validated, version-pinned registry. Only obtainable through
/// [`Registry::from_json`], so every instance satisfies FOR-002.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registry {
    version: String,
    entities: BTreeMap<String, Entity>,
    measurements: BTreeMap<String, Measurement>,
}

impl Registry {
    /// Parses and validates a registry. `Err` is a configuration failure
    /// (caller exits 2), never a finding about the text being checked.
    pub fn from_json(s: &str) -> Result<Registry, String> {
        let file: RegistryFile = serde_json::from_str(s).map_err(|e| format!("registry tidak sah: {e}"))?;
        if file.registry_version.trim().is_empty() {
            return Err("registry tidak sah: registry_version kosong; registry harus dipatok versinya".into());
        }
        let mut seen = BTreeSet::new();
        let ids = file
            .entities
            .iter()
            .map(|e| &e.id)
            .chain(file.measurements.iter().map(|m| &m.id));
        for id in ids {
            if !is_identifier(id) {
                return Err(format!(
                    "registry tidak sah: ID «{id}» tidak memenuhi grammar identifier (huruf kecil a–z, angka, _, diawali huruf kecil)"
                ));
            }
            if !seen.insert(id.as_str()) {
                return Err(format!(
                    "registry tidak sah: ID «{id}» muncul lebih dari satu kali (entitas dan pengukuran tidak boleh berbagi ID)"
                ));
            }
        }
        for e in &file.entities {
            for (name, value) in [
                ("label", &e.label),
                ("concept_id", &e.concept_id),
                ("entity_type", &e.entity_type),
            ] {
                if value.trim().is_empty() {
                    return Err(format!("registry tidak sah: entitas «{}» memiliki {name} kosong", e.id));
                }
            }
            let caps: BTreeSet<_> = e.capabilities.iter().collect();
            if caps.len() != e.capabilities.len() {
                return Err(format!(
                    "registry tidak sah: entitas «{}» mencantumkan kemampuan ganda",
                    e.id
                ));
            }
        }
        for m in &file.measurements {
            for (name, value) in [("quantity_kind", &m.quantity_kind), ("dimension", &m.dimension)] {
                if value.trim().is_empty() {
                    return Err(format!(
                        "registry tidak sah: pengukuran «{}» memiliki {name} kosong",
                        m.id
                    ));
                }
            }
            if !file.entities.iter().any(|e| e.id == m.owner) {
                return Err(format!(
                    "registry tidak sah: pemilik «{}» dari pengukuran «{}» bukan entitas terdaftar",
                    m.owner, m.id
                ));
            }
        }
        Ok(Registry {
            version: file.registry_version,
            entities: file.entities.into_iter().map(|e| (e.id.clone(), e)).collect(),
            measurements: file.measurements.into_iter().map(|m| (m.id.clone(), m)).collect(),
        })
    }

    pub fn registry_version(&self) -> &str {
        &self.version
    }

    pub fn entity(&self, id: &str) -> Option<&Entity> {
        self.entities.get(id)
    }

    pub fn measurement(&self, id: &str) -> Option<&Measurement> {
        self.measurements.get(id)
    }
}

// ---------------------------------------------------------------------------
// AST (Doc 10 §5).
// ---------------------------------------------------------------------------

/// Typed AST. `condition: None` is an `instruction`, `Some` a
/// `conditional_instruction`; the constant `ast_version`/`profile_id` and the
/// `kind` discriminator exist only in the JSON form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "AstRepr", into = "AstRepr")]
pub struct Ast {
    pub registry_version: String,
    pub source_id: String,
    pub condition: Option<Condition>,
    pub action: Action,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AstKind {
    Instruction,
    ConditionalInstruction,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AstRepr {
    ast_version: String,
    profile_id: String,
    registry_version: String,
    source_id: String,
    kind: AstKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    condition: Option<Condition>,
    action: Action,
}

impl From<Ast> for AstRepr {
    fn from(a: Ast) -> Self {
        AstRepr {
            ast_version: AST_VERSION.to_string(),
            profile_id: PROFILE_ID.to_string(),
            registry_version: a.registry_version,
            source_id: a.source_id,
            kind: if a.condition.is_some() {
                AstKind::ConditionalInstruction
            } else {
                AstKind::Instruction
            },
            condition: a.condition,
            action: a.action,
        }
    }
}

#[derive(Debug)]
struct AstShapeError(String);

impl fmt::Display for AstShapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<AstRepr> for Ast {
    type Error = AstShapeError;

    fn try_from(r: AstRepr) -> Result<Self, Self::Error> {
        if r.ast_version != AST_VERSION {
            return Err(AstShapeError(format!("ast_version «{}» tidak didukung", r.ast_version)));
        }
        if r.profile_id != PROFILE_ID {
            return Err(AstShapeError(format!("profile_id «{}» tidak didukung", r.profile_id)));
        }
        match (r.kind, &r.condition) {
            (AstKind::Instruction, None) | (AstKind::ConditionalInstruction, Some(_)) => {}
            (AstKind::Instruction, Some(_)) => {
                return Err(AstShapeError("instruction tidak boleh memuat condition".into()));
            }
            (AstKind::ConditionalInstruction, None) => {
                return Err(AstShapeError("conditional_instruction wajib memuat condition".into()));
            }
        }
        Ok(Ast {
            registry_version: r.registry_version,
            source_id: r.source_id,
            condition: r.condition,
            action: r.action,
        })
    }
}

/// Condition node; EX01 only has `comparison`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Condition {
    Comparison(Comparison),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Comparison {
    pub left: Left,
    pub operator: Operator,
    pub right: Right,
}

/// Left operand; EX01 only has `measurement_ref`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Left {
    MeasurementRef(MeasurementRef),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeasurementRef {
    pub id: String,
    pub entity_id: String,
    pub quantity_kind: String,
    pub dimension: String,
    pub unit: Unit,
}

/// Right operand; EX01 only has `decimal_quantity`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Right {
    DecimalQuantity(DecimalQuantity),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecimalQuantity {
    /// Exact normalized decimal with `.` as separator (never a float).
    pub value: String,
    pub quantity_kind: String,
    pub dimension: String,
    pub unit: Unit,
}

/// Action node; EX01 only has `command`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    Command(Command),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub operation: Operation,
    pub target: Target,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub id: String,
    pub entity_type: String,
}

// ---------------------------------------------------------------------------
// Scanner / parser (FOR-001).
// ---------------------------------------------------------------------------

/// Half-open UTF-8 byte span; converted to code points only via SourceMap.
#[derive(Debug, Clone, Copy)]
struct Span {
    start: usize,
    end: usize,
}

struct SyntaxError {
    span: Span,
    expected: String,
    found: String,
}

struct ParsedCondition<'a> {
    measurement: &'a str,
    measurement_span: Span,
    operator: Operator,
    value: String,
    unit: Unit,
    unit_span: Span,
}

struct Parsed<'a> {
    condition: Option<ParsedCondition<'a>>,
    operation: Operation,
    operation_span: Span,
    entity: &'a str,
    entity_span: Span,
    /// Byte offset just after the final '.' (before the optional eol).
    statement_end: usize,
}

#[derive(Clone, Copy)]
enum Start {
    If,
    Act(Operation),
}

const STARTS: &[(&str, Start)] = &[
    ("Jika", Start::If),
    ("Matikan", Start::Act(Operation::PowerOff)),
    ("Nyalakan", Start::Act(Operation::PowerOn)),
    ("Buka", Start::Act(Operation::Open)),
    ("Tutup", Start::Act(Operation::Close)),
];

/// Word characters: a keyword must not be directly followed by one.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

struct Scanner<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Scanner<'a> {
    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    /// Error located at `pos`, spanning the offending character or token.
    fn error_at(&self, pos: usize, expected: impl Into<String>) -> SyntaxError {
        let rest = &self.src[pos..];
        let Some(c) = rest.chars().next() else {
            return SyntaxError {
                span: Span { start: pos, end: pos },
                expected: expected.into(),
                found: "akhir masukan".into(),
            };
        };
        let one = Span {
            start: pos,
            end: pos + c.len_utf8(),
        };
        let (span, found) = match c {
            ' ' if pos == 0 => (one, "spasi awal".to_string()),
            ' ' if self.src[..pos].ends_with(' ') => (one, "spasi ganda".to_string()),
            ' ' => match token(&rest[1..]) {
                "" => (one, "spasi".to_string()),
                next => (one, format!("spasi sebelum «{next}»")),
            },
            '\t' => (one, "tab".to_string()),
            '\n' | '\r' => (one, "akhir baris".to_string()),
            '\u{feff}' => (one, "BOM (U+FEFF)".to_string()),
            _ => {
                let t = token(rest);
                (
                    Span {
                        start: pos,
                        end: pos + t.len(),
                    },
                    format!("«{t}»"),
                )
            }
        };
        SyntaxError {
            span,
            expected: expected.into(),
            found,
        }
    }

    fn fail<T>(&self, expected: impl Into<String>) -> Result<T, SyntaxError> {
        Err(self.error_at(self.pos, expected))
    }

    fn literal(&mut self, lit: &str, expected: &str) -> Result<Span, SyntaxError> {
        if self.rest().starts_with(lit) {
            let start = self.pos;
            self.pos += lit.len();
            Ok(Span { start, end: self.pos })
        } else {
            self.fail(expected)
        }
    }

    fn space(&mut self) -> Result<(), SyntaxError> {
        self.literal(" ", "satu spasi U+0020").map(|_| ())
    }

    /// Matches one whole keyword from `table` (not a prefix of a longer word).
    fn keyword<T: Copy>(&mut self, table: &[(&str, T)], expected: &str) -> Result<(T, Span), SyntaxError> {
        let rest = self.rest();
        for &(kw, value) in table {
            if rest.starts_with(kw) && !rest[kw.len()..].chars().next().is_some_and(is_word_char) {
                let start = self.pos;
                self.pos += kw.len();
                return Ok((value, Span { start, end: self.pos }));
            }
        }
        self.fail(expected)
    }

    fn identifier(&mut self, expected: &str) -> Result<(&'a str, Span), SyntaxError> {
        let rest = self.rest();
        let len = rest
            .find(|c: char| c.is_whitespace() || c == ',' || c == '.')
            .unwrap_or(rest.len());
        let word = &rest[..len];
        if word.is_empty() {
            return self.fail(expected);
        }
        let span = Span {
            start: self.pos,
            end: self.pos + len,
        };
        if !is_identifier(word) {
            return Err(SyntaxError {
                span,
                expected: format!("{expected} (huruf kecil a–z, angka, atau _, diawali huruf kecil)"),
                found: format!("«{word}»"),
            });
        }
        self.pos += len;
        Ok((word, span))
    }

    fn digits(&mut self) -> &'a str {
        let rest = self.rest();
        let len = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
        self.pos += len;
        &rest[..len]
    }

    /// `decimal = [ "-" ], integer, [ ",", digit, { digit } ]`, returned
    /// normalized (Doc 10 §4).
    fn decimal(&mut self) -> Result<String, SyntaxError> {
        let negative = self.rest().starts_with('-');
        if negative {
            self.pos += 1;
        }
        let int_start = self.pos;
        let int = self.digits();
        if int.is_empty() {
            return self.fail("angka desimal (mis. 80, -5, atau 1,5)");
        }
        if int.len() > 1 && int.starts_with('0') {
            return Err(SyntaxError {
                span: Span {
                    start: int_start,
                    end: self.pos,
                },
                expected: "bilangan bulat tanpa nol awal".into(),
                found: format!("«{int}»"),
            });
        }
        let mut frac = "";
        if self.rest().starts_with(',') {
            self.pos += 1;
            frac = self.digits();
            if frac.is_empty() {
                return self.fail("digit setelah koma desimal");
            }
        }
        Ok(normalize_decimal(negative, int, frac))
    }

    /// `[ eol ]` then end of input.
    fn end(&mut self) -> Result<(), SyntaxError> {
        let rest = self.rest();
        if matches!(rest, "" | "\n" | "\r\n") {
            return Ok(());
        }
        let skip = if rest.starts_with("\r\n") {
            2
        } else if rest.starts_with('\n') {
            1
        } else {
            0
        };
        Err(self.error_at(
            self.pos + skip,
            "akhir masukan setelah \".\" (boleh diikuti satu akhir baris)",
        ))
    }
}

/// Token shown in error messages: up to whitespace, or up to ',' / '.' after
/// the first character.
fn token(s: &str) -> &str {
    let end = s
        .char_indices()
        .find(|&(i, c)| c.is_whitespace() || (i > 0 && (c == ',' || c == '.')))
        .map_or(s.len(), |(i, _)| i);
    &s[..end]
}

/// Comma→dot, trailing fractional zeros stripped (and the dot if empty),
/// negative zero → "0". Kept as a string: no binary floating point.
fn normalize_decimal(negative: bool, int: &str, frac: &str) -> String {
    let frac = frac.trim_end_matches('0');
    let zero = int == "0" && frac.is_empty();
    let mut out = String::with_capacity(int.len() + frac.len() + 2);
    if negative && !zero {
        out.push('-');
    }
    out.push_str(int);
    if !frac.is_empty() {
        out.push('.');
        out.push_str(frac);
    }
    out
}

fn parse_syntax(src: &str) -> Result<Parsed<'_>, SyntaxError> {
    let mut s = Scanner { src, pos: 0 };
    let (start, start_span) = s.keyword(
        STARTS,
        "\"Jika\" atau perintah berhuruf awal kapital (Matikan, Nyalakan, Buka, Tutup)",
    )?;
    let (condition, operation, operation_span) = match start {
        Start::Act(op) => (None, op, start_span),
        Start::If => {
            s.space()?;
            let (measurement, measurement_span) = s.identifier("ID pengukuran")?;
            s.space()?;
            let (operator, _) = s.keyword(
                COMPARISONS,
                "perbandingan (melebihi, kurang dari, sama dengan, sekurang-kurangnya, paling banyak)",
            )?;
            s.space()?;
            let value = s.decimal()?;
            s.space()?;
            let (unit, unit_span) = s.keyword(UNITS, "satuan (°C, kPa, bar, atau V) setelah satu spasi")?;
            s.literal(",", "\",\" setelah satuan")?;
            s.space()?;
            let (op, op_span) = s.keyword(
                ACTIONS_LOWER,
                "perintah berhuruf kecil (matikan, nyalakan, buka, tutup)",
            )?;
            let cond = ParsedCondition {
                measurement,
                measurement_span,
                operator,
                value,
                unit,
                unit_span,
            };
            (Some(cond), op, op_span)
        }
    };
    s.space()?;
    let (entity, entity_span) = s.identifier("ID entitas")?;
    s.literal(".", "\".\" di akhir pernyataan")?;
    let statement_end = s.pos;
    s.end()?;
    Ok(Parsed {
        condition,
        operation,
        operation_span,
        entity,
        entity_span,
        statement_end,
    })
}

// ---------------------------------------------------------------------------
// Binding, typing, diagnostics (FOR-002..004).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FormalOutput {
    /// Present only when the input is accepted (no violation at all).
    pub ast: Option<Ast>,
    pub diagnostics: Vec<Diagnostic>,
}

struct Ctx<'a> {
    source_id: &'a str,
    map: SourceMap<'a>,
}

impl Ctx<'_> {
    fn diag(
        &self,
        rule_id: &str,
        category: &str,
        reason_code: &str,
        outcome: Outcome,
        range: Option<Range>,
        message: String,
    ) -> Diagnostic {
        Diagnostic::new(Finding {
            rule_id,
            category,
            document_id: self.source_id,
            scope_id: self.source_id.to_string(),
            mode: Mode::PROC,
            source_range: range,
            outcome,
            certainty: Certainty::High,
            method: Method::Deterministic,
            message,
            reason_code,
            autofix: Autofix::Forbidden,
        })
    }

    fn violation(&self, rule_id: &str, category: &str, reason_code: &str, span: Span, message: String) -> Diagnostic {
        let range = self.map.range(span.start, span.end);
        self.diag(rule_id, category, reason_code, Outcome::Violation, Some(range), message)
    }
}

/// Parses one EX01 statement, binds it to `registry` and returns either the
/// AST plus one `satisfied` diagnostic, or the violations without any AST.
pub fn parse(input: &str, registry: &Registry, source_id: &str) -> FormalOutput {
    let ctx = Ctx {
        source_id,
        map: SourceMap::new(input),
    };
    if input.len() > MAX_INPUT_BYTES {
        let mut d = ctx.diag(
            "PASTI-FOR-004",
            "formal.parser",
            "FOR_PARSER_FAILURE",
            Outcome::NotEvaluated,
            None,
            format!(
                "masukan {} bita melampaui batas implementasi {MAX_INPUT_BYTES} bita; profil formal tidak dinilai, kirim pernyataan sebagai rekaman terpisah",
                input.len()
            ),
        );
        // Doc 10 §6 fixes parser failure as error/high.
        d.severity = Severity::Error;
        return FormalOutput {
            ast: None,
            diagnostics: vec![d],
        };
    }

    let parsed = match parse_syntax(input) {
        Ok(p) => p,
        Err(e) => {
            let message = format!(
                "bentuk di luar profil {PROFILE_ID}: diharapkan {}, ditemukan {}; tulis ulang dalam bentuk yang didefinisikan grammar EX01 atau gunakan profil lain",
                e.expected, e.found
            );
            let d = ctx.violation("PASTI-FOR-001", "formal.syntax", "FOR_SYNTAX", e.span, message);
            return FormalOutput {
                ast: None,
                diagnostics: vec![d],
            };
        }
    };

    let mut diagnostics = Vec::new();
    let version = registry.registry_version();

    let measurement = parsed.condition.as_ref().and_then(|c| {
        let m = registry.measurement(c.measurement);
        if m.is_none() {
            let kind = if registry.entity(c.measurement).is_some() { "terdaftar sebagai entitas, bukan pengukuran" } else { "tidak terdaftar sebagai pengukuran" };
            diagnostics.push(ctx.violation(
                "PASTI-FOR-002",
                "formal.reference",
                "FOR_UNKNOWN_REFERENCE",
                c.measurement_span,
                format!(
                    "identifier «{}» {kind} pada registry {version}; gunakan ID pengukuran terdaftar atau minta pengelola registry menambahkannya",
                    c.measurement
                ),
            ));
        }
        m
    });

    if let (Some(c), Some(m)) = (&parsed.condition, measurement)
        && c.unit != m.unit
    {
        diagnostics.push(ctx.violation(
            "PASTI-FOR-003",
            "formal.unit",
            "FOR_UNIT_MISMATCH",
            c.unit_span,
            format!(
                "satuan «{}» berbeda dari satuan registry «{}» untuk {}; profil tidak mengonversi satuan, nyatakan batas dalam {}",
                c.unit.as_str(),
                m.unit.as_str(),
                m.id,
                m.unit.as_str()
            ),
        ));
    }

    let entity = registry.entity(parsed.entity);
    match entity {
        None => {
            let kind = if registry.measurement(parsed.entity).is_some() {
                "terdaftar sebagai pengukuran, bukan entitas"
            } else {
                "tidak terdaftar sebagai entitas"
            };
            diagnostics.push(ctx.violation(
                "PASTI-FOR-002",
                "formal.reference",
                "FOR_UNKNOWN_REFERENCE",
                parsed.entity_span,
                format!(
                    "identifier «{}» {kind} pada registry {version}; gunakan ID entitas terdaftar atau minta pengelola registry menambahkannya",
                    parsed.entity
                ),
            ));
        }
        Some(e) => {
            let needed = parsed.operation.required_capability();
            if !e.has(needed) {
                let available: Vec<_> = e.capabilities.iter().map(|c| c.as_str()).collect();
                let available = if available.is_empty() {
                    "tidak ada".to_string()
                } else {
                    available.join(", ")
                };
                diagnostics.push(ctx.violation(
                    "PASTI-FOR-002",
                    "formal.reference",
                    "FOR_CAPABILITY_MISMATCH",
                    parsed.operation_span,
                    format!(
                        "perintah «{}» memerlukan kemampuan {}, sedangkan {} hanya memiliki: {available}; pilih perintah yang sesuai sasaran atau ajukan perubahan registry kepada pengelola",
                        parsed.operation.verb(),
                        needed.as_str(),
                        e.id
                    ),
                ));
            }
            if let (Some(c), Some(m)) = (&parsed.condition, measurement)
                && m.owner != e.id
            {
                diagnostics.push(ctx.violation(
                    "PASTI-FOR-004",
                    "formal.owner",
                    "FOR_OWNER_MISMATCH",
                    c.measurement_span,
                    format!(
                        "pengukuran {} dimiliki {}, bukan sasaran perintah {}; kondisi harus memakai pengukuran milik sasaran, hubungan antarkomponen di luar EX01",
                        m.id, m.owner, e.id
                    ),
                ));
            }
        }
    }

    let (Some(entity), true) = (entity, diagnostics.is_empty()) else {
        return FormalOutput { ast: None, diagnostics };
    };

    let condition = parsed.condition.as_ref().zip(measurement).map(|(c, m)| {
        Condition::Comparison(Comparison {
            left: Left::MeasurementRef(MeasurementRef {
                id: m.id.clone(),
                entity_id: m.owner.clone(),
                quantity_kind: m.quantity_kind.clone(),
                dimension: m.dimension.clone(),
                unit: m.unit,
            }),
            operator: c.operator,
            right: Right::DecimalQuantity(DecimalQuantity {
                value: c.value.clone(),
                quantity_kind: m.quantity_kind.clone(),
                dimension: m.dimension.clone(),
                unit: c.unit,
            }),
        })
    });
    let ast = Ast {
        registry_version: version.to_string(),
        source_id: source_id.to_string(),
        condition,
        action: Action::Command(Command {
            operation: parsed.operation,
            target: Target {
                id: entity.id.clone(),
                entity_type: entity.entity_type.clone(),
            },
        }),
    };
    let accepted = ctx.diag(
        "PASTI-FOR-001",
        "formal.accept",
        "FOR_ACCEPTED",
        Outcome::Satisfied,
        Some(ctx.map.range(0, parsed.statement_end)),
        format!(
            "pernyataan diterima profil {PROFILE_ID} dengan registry {version}: sintaks, pengikatan, kemampuan, dan tipe sesuai profil; penerimaan tidak membuktikan ketepatan teknik atau kelengkapan prosedur, telaah isi tetap diperlukan"
        ),
    );
    FormalOutput {
        ast: Some(ast),
        diagnostics: vec![accepted],
    }
}

// ---------------------------------------------------------------------------
// Canonical formatter and AST validation (FOR-006).
// ---------------------------------------------------------------------------

/// Canonical text for an AST: capitalized verb when unconditional, comma
/// decimal, single spaces, final '.', no trailing newline.
pub fn format(ast: &Ast) -> String {
    let Action::Command(cmd) = &ast.action;
    match &ast.condition {
        None => format!("{} {}.", cmd.operation.verb_upper(), cmd.target.id),
        Some(Condition::Comparison(c)) => {
            let Left::MeasurementRef(m) = &c.left;
            let Right::DecimalQuantity(q) = &c.right;
            format!(
                "Jika {} {} {} {}, {} {}.",
                m.id,
                c.operator.phrase(),
                q.value.replace('.', ","),
                q.unit.as_str(),
                cmd.operation.verb(),
                cmd.target.id
            )
        }
    }
}

/// Checks that every derived field of a (deserialized) AST agrees with
/// `registry`: the AST is re-derived from its canonical text and compared.
/// Unknown properties and malformed nodes are already rejected by serde.
pub fn validate_ast(ast: &Ast, registry: &Registry) -> Result<(), String> {
    if ast.registry_version != registry.registry_version() {
        return Err(format!(
            "AST memakai registry {}, sedangkan registry yang dimuat {}",
            ast.registry_version,
            registry.registry_version()
        ));
    }
    let text = format(ast);
    let out = parse(&text, registry, &ast.source_id);
    match out.ast {
        Some(derived) if derived == *ast => Ok(()),
        Some(_) => Err("informasi turunan AST tidak sesuai dengan registry atau nilai belum dinormalkan".into()),
        None => {
            let reasons: Vec<_> = out.diagnostics.iter().map(|d| d.message.as_str()).collect();
            Err(format!(
                "AST tidak sah untuk profil {PROFILE_ID}: {}",
                reasons.join("; ")
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const DEMO: &str = include_str!("../examples/registry-demo.json");

    fn demo() -> Registry {
        Registry::from_json(DEMO).expect("demo registry loads")
    }

    fn run(input: &str) -> FormalOutput {
        parse(input, &demo(), "test")
    }

    fn reasons(out: &FormalOutput) -> Vec<&str> {
        out.diagnostics.iter().map(|d| d.reason_code.as_str()).collect()
    }

    fn assert_accepted(input: &str) -> Ast {
        let out = run(input);
        assert_eq!(reasons(&out), ["FOR_ACCEPTED"], "{input:?}: {:#?}", out.diagnostics);
        let d = &out.diagnostics[0];
        assert_eq!(d.outcome, Outcome::Satisfied);
        assert_eq!((d.severity, d.certainty), (Severity::Info, Certainty::High));
        assert_eq!(d.rule_id, "PASTI-FOR-001");
        out.ast.expect("AST on acceptance")
    }

    /// Asserts exactly one violation with `reason` and returns its code-point span.
    fn assert_rejected(input: &str, reason: &str) -> (usize, usize) {
        let out = run(input);
        assert!(out.ast.is_none(), "{input:?} must not produce an AST");
        assert_eq!(reasons(&out), [reason], "{input:?}: {:#?}", out.diagnostics);
        let d = &out.diagnostics[0];
        assert_eq!(d.outcome, Outcome::Violation);
        assert_eq!(
            (d.severity, d.certainty, d.method, d.autofix),
            (
                Severity::Error,
                Certainty::High,
                Method::Deterministic,
                Autofix::Forbidden
            )
        );
        assert_eq!(d.mode, Mode::PROC);
        assert_eq!((d.document_id.as_str(), d.scope_id.as_str()), ("test", "test"));
        let r = d.source_range.expect("range");
        (r.start.offset_cp, r.end.offset_cp)
    }

    fn value_of(ast: &Ast) -> &str {
        let Some(Condition::Comparison(c)) = &ast.condition else {
            panic!("no condition")
        };
        let Right::DecimalQuantity(q) = &c.right;
        &q.value
    }

    #[test]
    fn f23_exact_ast() {
        let out = parse(
            "Jika suhu_pompa_utama melebihi 80 °C, matikan pompa_utama.",
            &demo(),
            "fixture-F23",
        );
        let expected = json!({
            "ast_version": "0.1.0-draft.1",
            "profile_id": "PASTI-FOR-EX01",
            "registry_version": "demo-0.1.0-draft.1",
            "source_id": "fixture-F23",
            "kind": "conditional_instruction",
            "condition": {
                "kind": "comparison",
                "left": {
                    "kind": "measurement_ref",
                    "id": "suhu_pompa_utama",
                    "entity_id": "pompa_utama",
                    "quantity_kind": "temperature",
                    "dimension": "temperature",
                    "unit": "°C"
                },
                "operator": "gt",
                "right": {
                    "kind": "decimal_quantity",
                    "value": "80",
                    "quantity_kind": "temperature",
                    "dimension": "temperature",
                    "unit": "°C"
                }
            },
            "action": {
                "kind": "command",
                "operation": "power_off",
                "target": { "id": "pompa_utama", "entity_type": "pump" }
            }
        });
        let ast = out.ast.clone().expect("F23 accepted");
        assert_eq!(serde_json::to_value(&ast).unwrap(), expected);
        let back: Ast = serde_json::from_value(expected).unwrap();
        assert_eq!(back, ast);
        assert_eq!(reasons(&out), ["FOR_ACCEPTED"]);
        assert_eq!(out.diagnostics[0].outcome, Outcome::Satisfied);
        let r = out.diagnostics[0].source_range.unwrap();
        assert_eq!((r.start.offset_cp, r.end.offset_cp), (0, 58));
    }

    #[test]
    fn unconditional_instruction_json() {
        let ast = assert_accepted("Matikan pompa_utama.");
        assert_eq!(
            serde_json::to_value(&ast).unwrap(),
            json!({
                "ast_version": "0.1.0-draft.1",
                "profile_id": "PASTI-FOR-EX01",
                "registry_version": "demo-0.1.0-draft.1",
                "source_id": "test",
                "kind": "instruction",
                "action": {
                    "kind": "command",
                    "operation": "power_off",
                    "target": { "id": "pompa_utama", "entity_type": "pump" }
                }
            })
        );
    }

    #[test]
    fn f24_unknown_entity() {
        assert_eq!(
            assert_rejected("Matikan pompa_cadangan.", "FOR_UNKNOWN_REFERENCE"),
            (8, 22)
        );
        let out = run("Matikan pompa_cadangan.");
        assert_eq!(out.diagnostics[0].rule_id, "PASTI-FOR-002");
    }

    #[test]
    fn unknown_measurement_and_namespace_confusion() {
        assert_rejected(
            "Jika suhu_katup melebihi 80 °C, matikan pompa_utama.",
            "FOR_UNKNOWN_REFERENCE",
        );
        // An entity ID is not a measurement and vice versa.
        assert_rejected(
            "Jika pompa_utama melebihi 80 °C, matikan pompa_utama.",
            "FOR_UNKNOWN_REFERENCE",
        );
        assert_rejected("Matikan suhu_pompa_utama.", "FOR_UNKNOWN_REFERENCE");
    }

    #[test]
    fn f25_unit_mismatch_without_conversion() {
        let span = assert_rejected(
            "Jika tekanan_katup_masuk melebihi 2 bar, tutup katup_masuk.",
            "FOR_UNIT_MISMATCH",
        );
        assert_eq!(span, (36, 39));
        let out = run("Jika tekanan_katup_masuk melebihi 2 bar, tutup katup_masuk.");
        assert_eq!(out.diagnostics[0].rule_id, "PASTI-FOR-003");
        // Different dimension is rejected the same way.
        assert_rejected(
            "Jika suhu_pompa_utama melebihi 80 V, matikan pompa_utama.",
            "FOR_UNIT_MISMATCH",
        );
        assert_accepted("Jika tekanan_katup_masuk melebihi 200 kPa, tutup katup_masuk.");
    }

    #[test]
    fn f26_compound_condition_is_syntax_error() {
        let input = "Jika suhu_pompa_utama melebihi 80 °C atau tekanan naik, matikan pompa_utama.";
        assert_eq!(assert_rejected(input, "FOR_SYNTAX"), (36, 37));
        let out = run(input);
        assert_eq!(out.diagnostics[0].rule_id, "PASTI-FOR-001");
        assert!(out.diagnostics[0].message.contains("atau"));
    }

    #[test]
    fn every_operator() {
        for (phrase, op) in COMPARISONS {
            let ast = assert_accepted(&format!("Jika suhu_pompa_utama {phrase} 80 °C, matikan pompa_utama."));
            let Some(Condition::Comparison(c)) = &ast.condition else {
                panic!()
            };
            assert_eq!(c.operator, *op);
        }
        let ops: Vec<_> = COMPARISONS
            .iter()
            .map(|(_, op)| serde_json::to_value(op).unwrap())
            .collect();
        assert_eq!(ops, [json!("gt"), json!("lt"), json!("eq"), json!("ge"), json!("le")]);
        assert_rejected(
            "Jika suhu_pompa_utama lebih dari 80 °C, matikan pompa_utama.",
            "FOR_SYNTAX",
        );
        assert_rejected(
            "Jika suhu_pompa_utama sekitar 80 °C, matikan pompa_utama.",
            "FOR_SYNTAX",
        );
    }

    #[test]
    fn actions_against_capabilities() {
        let ok = [
            ("Matikan pompa_utama.", Operation::PowerOff),
            ("Nyalakan pompa_utama.", Operation::PowerOn),
            ("Buka katup_masuk.", Operation::Open),
            ("Tutup katup_masuk.", Operation::Close),
        ];
        for (input, op) in ok {
            let Action::Command(c) = assert_accepted(input).action;
            assert_eq!(c.operation, op);
        }
        for input in [
            "Matikan katup_masuk.",
            "Nyalakan katup_masuk.",
            "Buka pompa_utama.",
            "Tutup pompa_utama.",
        ] {
            let span = assert_rejected(input, "FOR_CAPABILITY_MISMATCH");
            assert_eq!(span.0, 0);
        }
        assert_eq!(
            assert_rejected(
                "Jika tekanan_katup_masuk melebihi 200 kPa, matikan katup_masuk.",
                "FOR_CAPABILITY_MISMATCH"
            ),
            (43, 50)
        );
        assert_accepted("Jika tegangan_pompa_utama kurang dari 200 V, nyalakan pompa_utama.");
        assert_accepted("Jika tekanan_katup_masuk paling banyak 50 kPa, buka katup_masuk.");
    }

    #[test]
    fn owner_mismatch() {
        let span = assert_rejected(
            "Jika suhu_pompa_utama melebihi 80 °C, tutup katup_masuk.",
            "FOR_OWNER_MISMATCH",
        );
        assert_eq!(span, (5, 21));
        let out = run("Jika suhu_pompa_utama melebihi 80 °C, tutup katup_masuk.");
        assert_eq!(out.diagnostics[0].rule_id, "PASTI-FOR-004");
    }

    #[test]
    fn independent_binding_errors_all_reported() {
        let out = run("Jika tekanan_katup_masuk melebihi 2 bar, matikan pompa_cadangan.");
        assert!(out.ast.is_none());
        assert_eq!(reasons(&out), ["FOR_UNIT_MISMATCH", "FOR_UNKNOWN_REFERENCE"]);
        let out = run("Jika tekanan_katup_masuk melebihi 2 °C, buka pompa_utama.");
        assert_eq!(
            reasons(&out),
            ["FOR_UNIT_MISMATCH", "FOR_CAPABILITY_MISMATCH", "FOR_OWNER_MISMATCH"]
        );
    }

    #[test]
    fn decimals() {
        let v = |n: &str| {
            value_of(&assert_accepted(&format!(
                "Jika suhu_pompa_utama melebihi {n} °C, matikan pompa_utama."
            )))
            .to_string()
        };
        assert_eq!(v("1,5"), "1.5");
        assert_eq!(v("-0,50"), "-0.5");
        assert_eq!(v("-0"), "0");
        assert_eq!(v("-0,00"), "0");
        assert_eq!(v("0"), "0");
        assert_eq!(v("80,0"), "80");
        assert_eq!(v("100,250"), "100.25");
        assert_eq!(v("-12"), "-12");
        assert_eq!(v("0,05"), "0.05");
        for bad in ["080", "00", "-080", "80.5", "+80", "1.000", "80,", "1e3", "-", "80,5,1"] {
            assert_rejected(
                &format!("Jika suhu_pompa_utama melebihi {bad} °C, matikan pompa_utama."),
                "FOR_SYNTAX",
            );
        }
        assert_eq!(
            assert_rejected(
                "Jika suhu_pompa_utama melebihi 080 °C, matikan pompa_utama.",
                "FOR_SYNTAX"
            ),
            (31, 34)
        );
    }

    #[test]
    fn unit_must_be_separated_by_one_space() {
        let span = assert_rejected(
            "Jika suhu_pompa_utama melebihi 80°C, matikan pompa_utama.",
            "FOR_SYNTAX",
        );
        assert_eq!(span, (33, 35));
        assert_rejected(
            "Jika suhu_pompa_utama melebihi 80  °C, matikan pompa_utama.",
            "FOR_SYNTAX",
        );
        assert_rejected(
            "Jika tekanan_katup_masuk melebihi 2 kpa, tutup katup_masuk.",
            "FOR_SYNTAX",
        );
        assert_rejected(
            "Jika tekanan_katup_masuk melebihi 2 kPa tutup katup_masuk.",
            "FOR_SYNTAX",
        );
    }

    #[test]
    fn whitespace_and_line_endings() {
        assert_eq!(assert_rejected("Matikan  pompa_utama.", "FOR_SYNTAX"), (8, 9));
        assert_eq!(assert_rejected("Matikan\tpompa_utama.", "FOR_SYNTAX"), (7, 8));
        assert_eq!(assert_rejected(" Matikan pompa_utama.", "FOR_SYNTAX"), (0, 1));
        assert_rejected("Matikan pompa_utama. ", "FOR_SYNTAX");
        assert_rejected("Matikan pompa_utama", "FOR_SYNTAX");
        assert_rejected("", "FOR_SYNTAX");
        assert_rejected("\u{feff}Matikan pompa_utama.", "FOR_SYNTAX");
        assert_accepted("Matikan pompa_utama.\n");
        assert_accepted("Matikan pompa_utama.\r\n");
        assert_eq!(assert_rejected("Matikan pompa_utama.\n\n", "FOR_SYNTAX"), (21, 22));
        assert_rejected("Matikan pompa_utama.\r\n\r\n", "FOR_SYNTAX");
        assert_rejected("Matikan pompa_utama.\r", "FOR_SYNTAX");
        assert_rejected("Matikan pompa_utama.\nMatikan pompa_utama.", "FOR_SYNTAX");
    }

    #[test]
    fn letters_and_forms() {
        assert_rejected("matikan pompa_utama.", "FOR_SYNTAX");
        assert_rejected("MATIKAN pompa_utama.", "FOR_SYNTAX");
        assert_rejected("Matikan Pompa_utama.", "FOR_SYNTAX");
        assert_rejected("Matikanlah pompa_utama.", "FOR_SYNTAX");
        assert_rejected("Jangan matikan pompa_utama.", "FOR_SYNTAX");
        assert_rejected("Matikan pompa_utama dan katup_masuk.", "FOR_SYNTAX");
        assert_rejected(
            "jika suhu_pompa_utama melebihi 80 °C, matikan pompa_utama.",
            "FOR_SYNTAX",
        );
        assert_rejected(
            "Jika suhu_pompa_utama melebihi 80 °C, Matikan pompa_utama.",
            "FOR_SYNTAX",
        );
        assert_rejected("Jika suhu berbahaya, matikan pompa.", "FOR_SYNTAX");
    }

    #[test]
    fn code_point_ranges_after_non_ascii() {
        // '°' is two UTF-8 bytes but one code point.
        let input = "Jika suhu_pompa_utama melebihi 80 °C, matikan pompa_cadangan.";
        assert_eq!(assert_rejected(input, "FOR_UNKNOWN_REFERENCE"), (46, 60));
    }

    #[test]
    fn oversized_input_is_not_evaluated() {
        let input = "a".repeat(MAX_INPUT_BYTES + 1);
        let out = run(&input);
        assert!(out.ast.is_none());
        assert_eq!(reasons(&out), ["FOR_PARSER_FAILURE"]);
        assert_eq!(out.diagnostics[0].outcome, Outcome::NotEvaluated);
        assert_eq!(
            (out.diagnostics[0].severity, out.diagnostics[0].certainty),
            (Severity::Error, Certainty::High)
        );
    }

    #[test]
    fn formatter_round_trip() {
        let inputs = [
            "Jika suhu_pompa_utama melebihi 80 °C, matikan pompa_utama.",
            "Jika suhu_pompa_utama sekurang-kurangnya -0,50 °C, nyalakan pompa_utama.\r\n",
            "Jika tekanan_katup_masuk sama dengan 1,250 kPa, buka katup_masuk.",
            "Jika tegangan_pompa_utama paling banyak -0 V, matikan pompa_utama.",
            "Tutup katup_masuk.\n",
            "Nyalakan pompa_utama.",
        ];
        for input in inputs {
            let ast = assert_accepted(input);
            let text = format(&ast);
            assert!(!text.ends_with('\n'));
            let again = parse(&text, &demo(), "other").ast.expect("canonical text accepted");
            assert_eq!(
                Ast {
                    source_id: ast.source_id.clone(),
                    ..again
                },
                ast,
                "{text}"
            );
            assert_eq!(format(&ast), text);
            assert!(validate_ast(&ast, &demo()).is_ok());
        }
        assert_eq!(
            format(&assert_accepted(
                "Jika suhu_pompa_utama kurang dari 1,50 °C, matikan pompa_utama."
            )),
            "Jika suhu_pompa_utama kurang dari 1,5 °C, matikan pompa_utama."
        );
        assert_eq!(format(&assert_accepted("Buka katup_masuk.\r\n")), "Buka katup_masuk.");
    }

    #[test]
    fn ast_validation_rejects_tampering() {
        let ast = assert_accepted("Jika suhu_pompa_utama melebihi 80 °C, matikan pompa_utama.");
        let mut v = serde_json::to_value(&ast).unwrap();
        v["action"]["target"]["entity_type"] = json!("valve");
        let tampered: Ast = serde_json::from_value(v.clone()).unwrap();
        assert!(validate_ast(&tampered, &demo()).is_err());

        let mut v = serde_json::to_value(&ast).unwrap();
        v["condition"]["right"]["value"] = json!("80.0");
        let unnormalized: Ast = serde_json::from_value(v).unwrap();
        assert!(validate_ast(&unnormalized, &demo()).is_err());

        let base = serde_json::to_value(&ast).unwrap();
        let mut extra = base.clone();
        extra["condition"]["left"]["note"] = json!("x");
        assert!(serde_json::from_value::<Ast>(extra).is_err());
        let mut extra_top = base.clone();
        extra_top["note"] = json!("x");
        assert!(serde_json::from_value::<Ast>(extra_top).is_err());
        let mut bad_op = base.clone();
        bad_op["condition"]["operator"] = json!("ne");
        assert!(serde_json::from_value::<Ast>(bad_op).is_err());
        let mut bad_kind = base.clone();
        bad_kind["kind"] = json!("instruction");
        assert!(serde_json::from_value::<Ast>(bad_kind).is_err());
        let mut bad_node = base;
        bad_node["condition"]["kind"] = json!("conjunction");
        assert!(serde_json::from_value::<Ast>(bad_node).is_err());
    }

    #[test]
    fn registry_demo_contents() {
        let r = demo();
        assert_eq!(r.registry_version(), "demo-0.1.0-draft.1");
        assert_eq!(r.entity("katup_masuk").unwrap().capabilities, [Capability::Position]);
        assert_eq!(r.measurement("tekanan_katup_masuk").unwrap().unit, Unit::Kilopascal);
        assert_eq!(r.measurement("tegangan_pompa_utama").unwrap().owner, "pompa_utama");
    }

    fn registry_with(entities: serde_json::Value, measurements: serde_json::Value) -> Result<Registry, String> {
        let doc = json!({ "registry_version": "t-1", "entities": entities, "measurements": measurements });
        Registry::from_json(&doc.to_string())
    }

    fn pump(id: &str) -> serde_json::Value {
        json!({ "id": id, "label": "l", "concept_id": "c", "entity_type": "pump", "capabilities": ["power"] })
    }

    fn temp(id: &str, owner: &str) -> serde_json::Value {
        json!({ "id": id, "owner": owner, "quantity_kind": "temperature", "dimension": "temperature", "unit": "°C" })
    }

    #[test]
    fn registry_validation_failures() {
        assert!(registry_with(json!([pump("p")]), json!([temp("t", "p")])).is_ok());
        // Duplicate across namespaces.
        let e = registry_with(json!([pump("p")]), json!([temp("p", "p")])).unwrap_err();
        assert!(e.contains("lebih dari satu kali"), "{e}");
        // Duplicate within a namespace.
        assert!(registry_with(json!([pump("p"), pump("p")]), json!([])).is_err());
        // Unknown owner.
        let e = registry_with(json!([pump("p")]), json!([temp("t", "q")])).unwrap_err();
        assert!(e.contains("pemilik"), "{e}");
        // Identifier grammar.
        assert!(registry_with(json!([pump("Pompa")]), json!([])).is_err());
        assert!(registry_with(json!([pump("1pompa")]), json!([])).is_err());
        // Unit outside the grammar set.
        let mut m = temp("t", "p");
        m["unit"] = json!("K");
        assert!(registry_with(json!([pump("p")]), json!([m])).is_err());
        // Capability outside {power, position}.
        let mut e = pump("p");
        e["capabilities"] = json!(["speed"]);
        assert!(registry_with(json!([e]), json!([])).is_err());
        // Unknown field.
        let mut e = pump("p");
        e["alias"] = json!("pompa");
        assert!(registry_with(json!([e]), json!([])).is_err());
        assert!(Registry::from_json(r#"{"registry_version":"v","entities":[],"measurements":[],"x":1}"#).is_err());
        // Missing / empty version.
        assert!(Registry::from_json(r#"{"registry_version":"","entities":[],"measurements":[]}"#).is_err());
        assert!(Registry::from_json(r#"{"entities":[],"measurements":[]}"#).is_err());
        assert!(Registry::from_json("not json").is_err());
    }
}
