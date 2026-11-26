use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};

use pasti_lint::config::Config;
use pasti_lint::diagnostic::{Diagnostic, Mode, Outcome, SPEC_VERSION, Severity};
use pasti_lint::document::{self, ATOM};
use pasti_lint::lint::{self, Input, Options};
use pasti_lint::report::{Report, TOOL};
use pasti_lint::{compare, formal, rules, segment};

#[derive(Parser)]
#[command(
    name = "pasti",
    version,
    about = "Linter eksperimental untuk PASTI (Panduan Standardisasi Teks dan Istilah), spesifikasi draf 0.1.0-draft.1."
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Text,
    Json,
}

#[derive(Subcommand)]
enum Cmd {
    /// Periksa dokumen Markdown atau teks. Tidak pernah mengubah berkas.
    Lint {
        files: Vec<PathBuf>,
        /// Berkas konfigurasi YAML (bawaan: ./pasti.yaml bila ada).
        #[arg(long)]
        config: Option<PathBuf>,
        /// Jenis teks bawaan: PROC, DESC, REQ, WARN, UI.
        #[arg(long)]
        mode: Option<String>,
        /// Format masukan: markdown atau text (bawaan: dari ekstensi).
        #[arg(long)]
        input_format: Option<String>,
        /// Kamus JSON tambahan (boleh berulang).
        #[arg(long)]
        glossary: Vec<PathBuf>,
        /// Berkas rekaman telaah, pengecualian, deviasi, dan suppression.
        #[arg(long)]
        reviews: Option<PathBuf>,
        /// ID dokumen (hanya untuk satu berkas; bawaan: jalur berkas).
        #[arg(long)]
        document_id: Option<String>,
        /// Aktifkan gerbang penerimaan; gagal → kode keluar 1.
        #[arg(long)]
        gate: bool,
        /// Severity minimum yang menggagalkan gerbang: error, warning, info.
        #[arg(long)]
        fail_on: Option<String>,
        /// Gerbang gagal selama telaah atau pemeriksaan wajib belum selesai.
        #[arg(long)]
        require_complete: bool,
        /// Sertakan hasil satisfied dan not_applicable.
        #[arg(long)]
        all: bool,
        #[arg(long, value_enum, default_value = "text")]
        format: Format,
    },
    /// Bandingkan sumber dan calon redaksi: cari perubahan makna (Dok. 07).
    Compare {
        source: PathBuf,
        candidate: PathBuf,
        #[arg(long, default_value = "unknown")]
        mode: String,
        /// Kode keluar 1 bila ada kandidat perubahan makna.
        #[arg(long)]
        gate: bool,
        #[arg(long)]
        all: bool,
        #[arg(long, value_enum, default_value = "text")]
        format: Format,
    },
    /// Urai pernyataan profil formal eksperimental PASTI-FOR-EX01 (Dok. 10).
    Parse {
        /// Berkas berisi satu pernyataan (atau gunakan --text).
        file: Option<PathBuf>,
        #[arg(long)]
        text: Option<String>,
        #[arg(long)]
        registry: PathBuf,
        #[arg(long)]
        source_id: Option<String>,
        /// Kode keluar 1 bila pernyataan ditolak.
        #[arg(long)]
        gate: bool,
        #[arg(long, value_enum, default_value = "text")]
        format: Format,
    },
    /// Tampilkan kalimat dan unit terhitung agar hitungan dapat diperiksa.
    Count {
        file: PathBuf,
        #[arg(long)]
        input_format: Option<String>,
    },
    /// Daftar aturan dan apakah alat ini memeriksanya otomatis.
    Rules,
}

fn fail(msg: impl std::fmt::Display) -> ExitCode {
    eprintln!("pasti: {msg}");
    ExitCode::from(2)
}

fn read(p: &Path) -> Result<String, String> {
    std::fs::read_to_string(p).map_err(|e| e.to_string())
}

fn main() -> ExitCode {
    match Cli::parse().cmd {
        Cmd::Lint {
            files,
            config,
            mode,
            input_format,
            glossary,
            reviews,
            document_id,
            gate,
            fail_on,
            require_complete,
            all,
            format,
        } => {
            let mut cfg = Config::default();
            let config = config.or_else(|| Path::new("pasti.yaml").exists().then(|| PathBuf::from("pasti.yaml")));
            if let Some(p) = &config
                && let Err(e) = cfg.apply_file(p)
            {
                return fail(e);
            }
            // Command-line arguments are the last configuration layer.
            if let Some(m) = mode {
                match Mode::parse(&m) {
                    Some(m) => cfg.default_mode = m,
                    None => return fail(format!("mode tidak dikenal: {m}")),
                }
            }
            if let Some(f) = input_format {
                match f.as_str() {
                    "markdown" => cfg.input_format = Some(document::InputFormat::Markdown),
                    "text" => cfg.input_format = Some(document::InputFormat::Text),
                    _ => return fail(format!("format masukan tidak dikenal: {f}")),
                }
            }
            cfg.glossaries.extend(glossary);
            if reviews.is_some() {
                cfg.reviews = reviews;
            }
            if let Some(f) = fail_on {
                match Severity::parse(&f) {
                    Some(s) => cfg.gate.fail_on = s,
                    None => return fail(format!("severity tidak dikenal: {f}")),
                }
            }
            if require_complete {
                cfg.gate.mandatory_checks_complete = true;
            }
            if files.is_empty() {
                return fail("tidak ada berkas yang diperiksa");
            }
            if document_id.is_some() && files.len() > 1 {
                return fail("--document-id hanya untuk satu berkas");
            }
            let inputs: Vec<Input> = files
                .iter()
                .map(|p| Input {
                    path: p.display().to_string(),
                    document_id: document_id.clone().unwrap_or_else(|| p.display().to_string()),
                    source: read(p),
                })
                .collect();
            let opts = Options {
                gate,
                include_passing: all,
                config_source: config.map(|p| p.display().to_string()),
            };
            let report = match lint::lint(&inputs, cfg, &opts) {
                Ok(r) => r,
                Err(e) => return fail(e),
            };
            match format {
                Format::Json => println!("{}", serde_json::to_string_pretty(&report).unwrap()),
                Format::Text => print_lint(&report, all),
            }
            ExitCode::from(lint::exit_code(&report) as u8)
        }
        Cmd::Compare {
            source,
            candidate,
            mode,
            gate,
            all,
            format,
        } => {
            let (s, c) = match (read(&source), read(&candidate)) {
                (Ok(s), Ok(c)) => (s, c),
                (Err(e), _) => return fail(format!("{}: {e}", source.display())),
                (_, Err(e)) => return fail(format!("{}: {e}", candidate.display())),
            };
            let Some(mode) = Mode::parse(&mode) else {
                return fail(format!("mode tidak dikenal: {mode}"));
            };
            let mut diags = compare::compare(&s, &c, &candidate.display().to_string(), mode);
            let findings = diags.iter().filter(|d| d.outcome.is_finding()).count();
            if !all {
                diags.retain(|d| !matches!(d.outcome, Outcome::Satisfied | Outcome::NotApplicable));
            }
            match format {
                Format::Json => println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "report_version": 1, "spec_version": SPEC_VERSION, "tool": TOOL, "kind": "rewrite_relation",
                        "source": source.display().to_string(), "candidate": candidate.display().to_string(),
                        "note": "Temuan otomatis adalah kandidat telaah (possible_violation), bukan putusan akhir; kesetaraan makna tetap memerlukan penelaah.",
                        "diagnostics": diags,
                    }))
                    .unwrap()
                ),
                Format::Text => {
                    print_diags(&diags, &c, true);
                    println!("{findings} kandidat perubahan makna. Kandidat bukan putusan akhir; kesetaraan makna tetap ditelaah manusia.");
                }
            }
            ExitCode::from(if gate && findings > 0 { 1 } else { 0 })
        }
        Cmd::Parse {
            file,
            text,
            registry,
            source_id,
            gate,
            format,
        } => {
            let reg = match read(&registry).and_then(|r| formal::Registry::from_json(&r)) {
                Ok(r) => r,
                Err(e) => return fail(format!("{}: {e}", registry.display())),
            };
            let (input, sid) = match (file, text) {
                (Some(f), None) => match read(&f) {
                    Ok(s) => (s, f.display().to_string()),
                    Err(e) => return fail(format!("{}: {e}", f.display())),
                },
                (None, Some(t)) => (t, "stdin-text".to_string()),
                _ => return fail("berikan tepat satu: FILE atau --text"),
            };
            let sid = source_id.unwrap_or(sid);
            let out = formal::parse(&input, &reg, &sid);
            let rejected = out.ast.is_none();
            match format {
                Format::Json => println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "report_version": 1, "spec_version": SPEC_VERSION, "tool": TOOL, "profile_id": formal::PROFILE_ID,
                        "registry_version": reg.registry_version(), "source_sha256": pasti_lint::diagnostic::sha256_hex(input.as_bytes()),
                        "ast": out.ast, "diagnostics": out.diagnostics,
                    }))
                    .unwrap()
                ),
                Format::Text => {
                    print_diags(&out.diagnostics, &input, true);
                    if let Some(ast) = &out.ast {
                        println!("{}", serde_json::to_string_pretty(ast).unwrap());
                        println!("Diterima profil {}. AST hanya representasi; tidak ada tindakan yang dijalankan.", formal::PROFILE_ID);
                    } else {
                        println!("Ditolak profil {}; AST tidak diterbitkan.", formal::PROFILE_ID);
                    }
                }
            }
            ExitCode::from(if gate && rejected { 1 } else { 0 })
        }
        Cmd::Count { file, input_format } => {
            let src = match read(&file) {
                Ok(s) => s,
                Err(e) => return fail(format!("{}: {e}", file.display())),
            };
            let mut cfg = Config::default();
            if Path::new("pasti.yaml").exists()
                && let Err(e) = cfg.apply_file(Path::new("pasti.yaml"))
            {
                return fail(e);
            }
            let fmt = match input_format.as_deref() {
                Some("text") => document::InputFormat::Text,
                Some("markdown") => document::InputFormat::Markdown,
                Some(f) => return fail(format!("format masukan tidak dikenal: {f}")),
                None => lint::format_for(&file.display().to_string(), &cfg),
            };
            let doc = match document::parse(&src, fmt, cfg.default_mode) {
                Ok(d) => d,
                Err(e) => return fail(e.0),
            };
            let map = pasti_lint::coords::SourceMap::new(&src);
            for b in &doc.blocks {
                let p = map.position(b.src.start);
                if !b.kind.is_prose() {
                    println!(
                        "{}:{} [{:?}] terlindungi; tidak dihitung",
                        p.line,
                        p.column_cp + 1,
                        b.kind
                    );
                    continue;
                }
                for s in segment::sentences(&b.text, &cfg.abbreviations) {
                    let r = b.src_range(s.vis.clone());
                    let p = map.position(r.start);
                    let units: Vec<String> = s
                        .units
                        .iter()
                        .map(|u| {
                            let t = &b.text[u.clone()];
                            if t.contains(ATOM) {
                                format!("⟦{}⟧", &src[b.src_range(u.clone())])
                            } else {
                                t.to_string()
                            }
                        })
                        .collect();
                    println!(
                        "{}:{} [{:?} {}] {} unit: {}",
                        p.line,
                        p.column_cp + 1,
                        b.kind,
                        b.mode.as_str(),
                        units.len(),
                        units.join(" | ")
                    );
                }
            }
            ExitCode::SUCCESS
        }
        Cmd::Rules => {
            for r in rules::RULES {
                let force = serde_json::to_value(r.force).unwrap();
                println!(
                    "{:<15} {:<12} {:<9} {:<10} {}",
                    r.id,
                    force.as_str().unwrap(),
                    format!("{:?}", r.target).to_lowercase(),
                    if r.automated { "otomatis" } else { "manual" },
                    r.title
                );
            }
            ExitCode::SUCCESS
        }
    }
}

fn outcome_str(o: Outcome) -> String {
    serde_json::to_value(o).unwrap().as_str().unwrap().to_string()
}

fn sev_str(s: Severity) -> String {
    serde_json::to_value(s).unwrap().as_str().unwrap().to_string()
}

fn print_diags(diags: &[Diagnostic], prefix_doc: &str, show_doc: bool) {
    let _ = (prefix_doc, show_doc);
    for d in diags {
        let loc = match d.source_range {
            Some(r) => format!("{}:{}:{}", d.document_id, r.start.line, r.start.column_cp + 1),
            None => d.document_id.clone(),
        };
        let mut flags = format!(
            "{}, {}",
            outcome_str(d.outcome),
            serde_json::to_value(d.certainty).unwrap().as_str().unwrap()
        );
        if d.suppressed {
            flags.push_str(", suppressed");
        }
        if d.review_status == pasti_lint::diagnostic::ReviewStatus::Pending {
            flags.push_str(", telaah tertunda");
        }
        println!(
            "{loc}: {} {} [{}] {} ({flags})",
            sev_str(d.severity),
            d.rule_id,
            d.category,
            d.message
        );
    }
}

fn print_lint(r: &Report, all: bool) {
    let shown: Vec<Diagnostic> = r
        .diagnostics
        .iter()
        .filter(|d| all || d.severity >= Severity::Warning || d.outcome.is_finding())
        .cloned()
        .collect();
    print_diags(&shown, "", true);
    for e in &r.errors {
        println!("galat: {e}");
    }
    let count = |o: Outcome| r.diagnostics.iter().filter(|d| d.outcome == o).count();
    let hidden = r.diagnostics.len() - shown.len();
    println!();
    println!(
        "{} dokumen; {} pelanggaran, {} kandidat telaah, {} belum dinilai{}.",
        r.documents.len(),
        count(Outcome::Violation),
        count(Outcome::PossibleViolation),
        count(Outcome::NotEvaluated),
        if hidden > 0 {
            format!(" ({hidden} info tersembunyi; gunakan --all)")
        } else {
            String::new()
        }
    );
    let c = &r.coverage;
    println!(
        "Telaah wajib tertunda: {}. Pemeriksaan wajib belum dinilai: {}. Aturan wajib yang perlu telaah manusia: {}.",
        c.mandatory_reviews_pending,
        c.mandatory_checks_not_evaluated,
        c.mandatory_rules_requiring_manual_review.len()
    );
    if c.suppressed_diagnostics > 0 {
        println!(
            "Disembunyikan (suppressed): {} — tetap dihitung dalam cakupan dan gerbang.",
            c.suppressed_diagnostics
        );
    }
    println!("Status: {} — {}", r.status, pasti_lint::report::STATUS_NOTE);
    if let Some(p) = r.gate.passed {
        println!(
            "Gerbang: {}{}",
            if p { "lulus" } else { "gagal: " },
            r.gate.reasons.join("; ")
        );
    }
}
