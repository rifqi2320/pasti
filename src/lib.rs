//! PASTI linter: checks for PASTI (Panduan Standardisasi Teks dan Istilah),
//! an independent draft specification for controlled technical Indonesian
//! (spec 0.1.0-draft.1). Not affiliated with Badan Bahasa or ASD.

pub mod checks;
pub mod compare;
pub mod config;
pub mod coords;
pub mod diagnostic;
pub mod document;
pub mod formal;
pub mod glossary;
pub mod lint;
pub mod patterns;
pub mod report;
pub mod reviews;
pub mod rules;
pub mod segment;
