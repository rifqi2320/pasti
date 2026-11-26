//! Doc 09 fixtures for the lint contract (F01–F08, F17, F20–F22, F27).
//! Rewrite fixtures F09–F16 live in `src/compare.rs`; formal-profile
//! fixtures F23–F26 live in `src/formal.rs`.

use pasti_lint::config::Config;
use pasti_lint::diagnostic::{Certainty, Diagnostic, Mode, Outcome, ReviewStatus, Severity};
use pasti_lint::lint::{self, Input, Options};
use pasti_lint::report::Report;

fn run(src: &str, mode: Mode, edit: impl FnOnce(&mut Config), gate: bool) -> Report {
    let mut cfg = Config {
        default_mode: mode,
        ..Config::default()
    };
    edit(&mut cfg);
    let input = Input {
        path: "fixture.md".into(),
        document_id: "fixture".into(),
        source: Ok(src.to_string()),
    };
    lint::lint(
        &[input],
        cfg,
        &Options {
            gate,
            include_passing: true,
            config_source: None,
        },
    )
    .unwrap()
}

fn of<'a>(r: &'a Report, rule: &str) -> Vec<&'a Diagnostic> {
    r.diagnostics.iter().filter(|d| d.rule_id == rule).collect()
}

const F01: &str = "Periksa setiap baut pada penutup pompa utama dengan kunci yang sesuai sebelum teknisi lain memasang kembali pelindung pada rangka mesin.";
const F03: &str = "Sensor tekanan mengirim nilai pengukuran ke pengendali setiap detik melalui sambungan kabel yang terlindungi dari gangguan listrik selama sistem bekerja dalam kondisi operasi yang ditetapkan.";

#[test]
fn f01_f02_proc_threshold() {
    let r = run(F01, Mode::PROC, |_| {}, false);
    let d = of(&r, "PASTI-TOK-005");
    assert_eq!(
        (d.len(), d[0].outcome, d[0].category.as_str()),
        (1, Outcome::Satisfied, "length.proc")
    );
    assert!(d[0].message.starts_with("20 kata"));

    let r = run(&F01.replace("mesin.", "mesin itu."), Mode::PROC, |_| {}, false);
    let d = of(&r, "PASTI-TOK-005")[0];
    assert_eq!(
        (d.outcome, d.severity, d.certainty),
        (Outcome::PossibleViolation, Severity::Warning, Certainty::High)
    );
    assert_eq!(d.review_status, ReviewStatus::Pending);
    assert!(d.message.starts_with("21 kata"));
}

#[test]
fn f03_f04_desc_threshold() {
    assert_eq!(
        of(&run(F03, Mode::DESC, |_| {}, false), "PASTI-TOK-005")[0].outcome,
        Outcome::Satisfied
    );
    let r = run(
        &F03.replace("pengendali", "pengendali utama"),
        Mode::DESC,
        |_| {},
        false,
    );
    let d = of(&r, "PASTI-TOK-005")[0];
    assert_eq!(
        (d.outcome, d.category.as_str()),
        (Outcome::PossibleViolation, "length.desc")
    );
    assert!(d.message.starts_with("26 kata"));
}

#[test]
fn f05_f06_f07_counts_and_exact_code() {
    let units = |s: &str| run(s, Mode::PROC, |_| {}, false).documents[0].counted_units;
    assert_eq!(units("Atur suhu ke 80 °C."), 5);
    assert_eq!(units("Periksa baut-bautnya."), 2);
    let src = "Jalankan `tool check --all`.";
    assert_eq!(units(src), 2);
    // Code content is untouched: no rule reports inside the code span.
    let r = run(src, Mode::PROC, |_| {}, false);
    assert!(r.diagnostics.iter().all(|d| !d.outcome.is_finding()));
}

#[test]
fn f08_code_block_is_protected_not_measured() {
    let words = vec!["kata"; 30].join(" ");
    let r = run(&format!("```\n{words}.\n```\n"), Mode::PROC, |_| {}, false);
    let d = of(&r, "PASTI-TOK-005");
    assert_eq!(
        (d.len(), d[0].outcome, d[0].reason_code.as_str()),
        (1, Outcome::NotApplicable, "PROTECTED_BLOCK")
    );
    assert_eq!(r.documents[0].protected.len(), 1);
    assert_eq!(r.documents[0].sentences, 0);
}

#[test]
fn unknown_mode_is_not_evaluated_not_guessed() {
    let r = run(&F01.replace("mesin.", "mesin itu."), Mode::Unknown, |_| {}, false);
    let d = of(&r, "PASTI-TOK-005");
    assert_eq!(
        (d.len(), d[0].outcome, d[0].reason_code.as_str()),
        (1, Outcome::NotEvaluated, "MODE_UNKNOWN")
    );
    let cov = r.coverage.rules.iter().find(|c| c.rule_id == "PASTI-TOK-005").unwrap();
    assert_eq!(cov.scopes_applicability_unknown, 1);
}

#[test]
fn f17_preposition_and_directory_not_flagged() {
    let r = run("Simpan berkas di direktori kerja.", Mode::PROC, |_| {}, false);
    assert!(of(&r, "PASTI-GRA-001").iter().all(|d| d.outcome == Outcome::Satisfied));
}

#[test]
fn f20_unknown_term_is_not_a_violation() {
    let gl = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/glossary-seed.json");
    let r = run("Periksa fluksor.", Mode::PROC, |c| c.glossaries.push(gl.into()), false);
    assert!(r.diagnostics.iter().all(|d| d.outcome != Outcome::Violation));
    // `periksa` is a proposed seed entry: approval cannot be evaluated yet.
    let d = of(&r, "PASTI-BAS-003");
    assert_eq!(
        (d[0].outcome, d[0].severity, d[0].reason_code.as_str()),
        (Outcome::NotEvaluated, Severity::Info, "LEX_TERM_PROPOSED")
    );
}

fn reviews_file(name: &str, body: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("pasti-fixtures-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(name);
    std::fs::write(&p, body).unwrap();
    p
}

fn scope_hash(d: &Diagnostic) -> String {
    d.scope_id.rsplit_once(':').unwrap().1.to_string()
}

#[test]
fn f21_suppressed_violation_still_fails_gate() {
    let src = "Pasang sensor disini.";
    let r = run(src, Mode::PROC, |_| {}, true);
    let hash = scope_hash(of(&r, "PASTI-GRA-001")[0]);
    let p = reviews_file(
        "f21.yaml",
        &format!(
            "records:\n  - record_id: S-1\n    kind: suppression\n    state: accepted\n    rule_id: PASTI-GRA-001\n    scope_hash: {hash}\n    reason: sementara\n    reviewed_by: [penulis]\n"
        ),
    );
    let r = run(src, Mode::PROC, |c| c.reviews = Some(p), true);
    let d = of(&r, "PASTI-GRA-001")[0];
    assert!(d.suppressed);
    assert_eq!(d.outcome, Outcome::Violation);
    assert_eq!(r.gate.passed, Some(false));
    assert_eq!(lint::exit_code(&r), 1);
    assert_eq!(r.coverage.suppressed_diagnostics, 1);
}

#[test]
fn f22_ranges_are_code_points_after_emoji_and_crlf() {
    let src = "x\r\nA😀B 5kg.";
    let r = run(src, Mode::DESC, |_| {}, false);
    let d = of(&r, "PASTI-NUM-002")
        .into_iter()
        .find(|d| d.outcome.is_finding())
        .unwrap();
    let range = d.source_range.unwrap();
    // `5kg` starts after "x\r\n" (3 cp) + "A😀B " (4 cp).
    assert_eq!((range.start.offset_cp, range.end.offset_cp), (7, 10));
    assert_eq!((range.start.line, range.start.column_cp), (2, 4));
}

#[test]
fn f27_pending_review_fails_complete_gate_until_reviewed() {
    let src = F01.replace("mesin.", "mesin itu.");
    let strict = |c: &mut Config| {
        c.gate.fail_on = Severity::Error;
        c.gate.mandatory_checks_complete = true;
    };
    let r = run(&src, Mode::PROC, strict, true);
    let d = of(&r, "PASTI-TOK-005")[0];
    assert_eq!(
        (d.outcome, d.severity, d.review_status),
        (Outcome::PossibleViolation, Severity::Warning, ReviewStatus::Pending)
    );
    assert_eq!(r.gate.passed, Some(false));
    assert_eq!(lint::exit_code(&r), 1);
    let cov = r.coverage.rules.iter().find(|c| c.rule_id == "PASTI-TOK-005").unwrap();
    assert_eq!(cov.detection_coverage.value, Some(1.0));
    assert_eq!(cov.decision_completion.value, Some(0.0));

    // A recorded human review keeps the 21-word sentence: satisfied, gate passes.
    let p = reviews_file(
        "f27.yaml",
        &format!(
            "records:\n  - record_id: R-1\n    kind: review\n    state: accepted\n    rule_id: PASTI-TOK-005\n    scope_hash: {}\n    decision: satisfied\n    reason: pemecahan memisahkan syarat\n    reviewed_by: [penelaah bahasa]\n",
            scope_hash(d)
        ),
    );
    let r = run(
        &src,
        Mode::PROC,
        |c| {
            strict(c);
            c.reviews = Some(p.clone());
        },
        true,
    );
    let d = of(&r, "PASTI-TOK-005")[0];
    assert_eq!(
        (d.outcome, d.review_status, d.review_record_id.as_deref()),
        (Outcome::Satisfied, ReviewStatus::Completed, Some("R-1"))
    );
    assert_eq!(r.gate.passed, Some(true), "{:?}", r.gate.reasons);

    // Editing the sentence makes the record stale instead of silently applying it.
    let r = run(
        &src.replace("itu.", "ini."),
        Mode::PROC,
        |c| {
            strict(c);
            c.reviews = Some(p);
        },
        true,
    );
    assert_eq!(r.review_records.stale, vec!["R-1".to_string()]);
    assert_eq!(r.gate.passed, Some(false));
}

#[test]
fn output_is_deterministic() {
    let src = "Pasang sensor disini. Atur 2.5 bar dan 5kg.\n\nJaga suhu 10–20 °C.";
    let a = serde_json::to_string(&run(src, Mode::PROC, |_| {}, false).diagnostics).unwrap();
    let b = serde_json::to_string(&run(src, Mode::PROC, |_| {}, false).diagnostics).unwrap();
    assert_eq!(a, b);
}

#[test]
fn read_failure_marks_partial_and_exit_2() {
    let inputs = [Input {
        path: "missing.md".into(),
        document_id: "missing".into(),
        source: Err("tidak ada".into()),
    }];
    let r = lint::lint(
        &inputs,
        Config::default(),
        &Options {
            gate: false,
            include_passing: false,
            config_source: None,
        },
    )
    .unwrap();
    assert!(r.partial);
    assert_eq!(lint::exit_code(&r), 2);
}
