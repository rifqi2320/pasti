//! Document model and input adapters (PASTI-LINT-001, PASTI-TOK-001).
//!
//! Adapters keep the original source untouched and emit blocks whose visible
//! text maps back to source byte offsets. Inline code, inline math, URLs and
//! images become *atoms*: one placeholder character in the visible text that
//! maps to the whole source span, so sentence splitting never looks inside
//! them and they count as exactly one unit (PASTI-TOK-004).

use std::ops::Range as ByteRange;

use pulldown_cmark::{Event, LinkType, Options, Parser, Tag, TagEnd};
use regex::Regex;
use std::sync::LazyLock;

use crate::diagnostic::Mode;

/// Placeholder for an atomic span in visible text (Unicode private use).
pub const ATOM: char = '\u{E000}';

pub const MARKDOWN_ADAPTER: &str = "commonmark-0.31.2+tables+math (pulldown-cmark 0.13)";
pub const TEXT_ADAPTER: &str = "plain-text-1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputFormat {
    Markdown,
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockKind {
    Heading,
    Paragraph,
    ListItem,
    TableCell,
    /// Fenced or indented code block (protected).
    CodeBlock,
    /// Block marked `<!-- pasti: exact -->` (protected exact-source quote).
    ExactQuote,
    /// Raw HTML block (unsupported, not evaluated).
    Html,
    /// Paragraph consisting only of display math (protected equation).
    Equation,
    /// YAML front matter (metadata, not prose).
    Metadata,
}

impl BlockKind {
    pub fn is_prose(self) -> bool {
        matches!(
            self,
            BlockKind::Heading | BlockKind::Paragraph | BlockKind::ListItem | BlockKind::TableCell
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AtomKind {
    InlineCode,
    InlineMath,
    DisplayMath,
    Url,
    Image,
}

#[derive(Debug, Clone)]
pub struct Atom {
    pub kind: AtomKind,
    /// Byte offset of the placeholder in visible text.
    pub vis: usize,
    pub src: ByteRange<usize>,
}

/// A run of visible text mapped to a source span. `exact` pieces map
/// byte-for-byte; inexact ones (entities, soft breaks, atoms) map to their
/// whole span.
#[derive(Debug, Clone)]
struct Piece {
    vis: ByteRange<usize>,
    src: ByteRange<usize>,
    exact: bool,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub index: usize,
    pub kind: BlockKind,
    pub mode: Mode,
    pub src: ByteRange<usize>,
    pub text: String,
    pub atoms: Vec<Atom>,
    pieces: Vec<Piece>,
}

impl Block {
    fn new(index: usize, kind: BlockKind, mode: Mode, start: usize) -> Block {
        Block {
            index,
            kind,
            mode,
            src: start..start,
            text: String::new(),
            atoms: Vec::new(),
            pieces: Vec::new(),
        }
    }

    fn push_text(&mut self, text: &str, src: ByteRange<usize>) {
        if text.is_empty() {
            return;
        }
        let exact = src.len() == text.len();
        let start = self.text.len();
        self.text.push_str(text);
        self.pieces.push(Piece {
            vis: start..self.text.len(),
            src,
            exact,
        });
    }

    fn push_atom(&mut self, kind: AtomKind, src: ByteRange<usize>) {
        let vis = self.text.len();
        self.text.push(ATOM);
        self.pieces.push(Piece {
            vis: vis..self.text.len(),
            src: src.clone(),
            exact: false,
        });
        self.atoms.push(Atom { kind, vis, src });
    }

    /// Visible text with bare URLs inside text pieces turned into atoms.
    fn push_text_with_urls(&mut self, text: &str, src: ByteRange<usize>) {
        let exact = src.len() == text.len();
        let mut last = 0;
        if exact {
            for m in URL_RE.find_iter(text) {
                // Trailing sentence punctuation is not part of the URL.
                let end = m.start() + m.as_str().trim_end_matches(['.', ',', ';', ':', '!', '?', ')']).len();
                self.push_text(&text[last..m.start()], src.start + last..src.start + m.start());
                self.push_atom(AtomKind::Url, src.start + m.start()..src.start + end);
                last = end;
            }
        }
        self.push_text(&text[last..], src.start + last..src.end);
    }

    /// Maps a visible byte offset used as a range start to a source byte offset.
    pub fn src_start(&self, vis: usize) -> usize {
        for p in &self.pieces {
            if p.vis.start <= vis && vis < p.vis.end {
                return if p.exact {
                    p.src.start + (vis - p.vis.start)
                } else {
                    p.src.start
                };
            }
        }
        self.pieces.last().map_or(self.src.start, |p| p.src.end)
    }

    /// Maps a visible byte offset used as a range end to a source byte offset.
    pub fn src_end(&self, vis: usize) -> usize {
        for p in &self.pieces {
            if p.vis.start < vis && vis <= p.vis.end {
                return if p.exact {
                    p.src.start + (vis - p.vis.start)
                } else {
                    p.src.end
                };
            }
        }
        self.pieces.first().map_or(self.src.start, |p| p.src.start)
    }

    pub fn src_range(&self, vis: ByteRange<usize>) -> ByteRange<usize> {
        let s = self.src_start(vis.start);
        s..self.src_end(vis.end).max(s)
    }

    /// The atom whose placeholder sits at visible offset `vis`.
    pub fn atom_at(&self, vis: usize) -> Option<&Atom> {
        self.atoms.iter().find(|a| a.vis == vis)
    }
}

#[derive(Debug, Clone)]
pub struct Document {
    pub format: InputFormat,
    pub blocks: Vec<Block>,
}

impl Document {
    pub fn adapter(&self) -> &'static str {
        match self.format {
            InputFormat::Markdown => MARKDOWN_ADAPTER,
            InputFormat::Text => TEXT_ADAPTER,
        }
    }
}

static URL_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b(?:https?|ftp)://[^\s<>()]+").unwrap());
static DIRECTIVE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)^\s*<!--\s*pasti:\s*(.*?)\s*-->\s*$").unwrap());

#[derive(Debug, PartialEq)]
pub struct InputError(pub String);

/// Parses a `pasti:` directive body. Supported: `mode=PROC|DESC|REQ|WARN|UI|unknown`
/// (applies to following blocks until the next mode directive) and `exact`
/// (the next top-level block is a protected exact-source quote).
enum Directive {
    Mode(Mode),
    Exact,
}

fn parse_directive(body: &str) -> Result<Directive, String> {
    let body = body.trim();
    if body == "exact" {
        return Ok(Directive::Exact);
    }
    if let Some(m) = body.strip_prefix("mode=") {
        return Mode::parse(m)
            .map(Directive::Mode)
            .ok_or_else(|| format!("mode tidak dikenal: {m:?}"));
    }
    Err(format!(
        "direktif pasti tidak dikenal: {body:?} (gunakan `mode=PROC|DESC|REQ|WARN|UI` atau `exact`)"
    ))
}

pub fn parse(src: &str, format: InputFormat, default_mode: Mode) -> Result<Document, InputError> {
    match format {
        InputFormat::Markdown => parse_markdown(src, default_mode),
        InputFormat::Text => Ok(parse_text(src, default_mode)),
    }
}

/// Plain text: blocks are runs of non-blank lines; line breaks inside a block
/// read as spaces.
fn parse_text(src: &str, mode: Mode) -> Document {
    let mut blocks = Vec::new();
    let mut cur: Option<Block> = None;
    let mut pos = 0;
    for line in src.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        let start = pos;
        pos += line.len();
        if content.trim().is_empty() {
            if let Some(b) = cur.take() {
                blocks.push(b);
            }
            continue;
        }
        let b = cur.get_or_insert_with(|| Block::new(blocks.len(), BlockKind::Paragraph, mode, start));
        if !b.text.is_empty() {
            let prev_end = b.src.end;
            b.push_text(" ", prev_end..start);
        }
        b.push_text_with_urls(content, start..start + content.len());
        b.src.end = start + content.len();
    }
    if let Some(b) = cur {
        blocks.push(b);
    }
    Document {
        format: InputFormat::Text,
        blocks,
    }
}

fn parse_markdown(src: &str, default_mode: Mode) -> Result<Document, InputError> {
    let opts = Options::ENABLE_TABLES | Options::ENABLE_MATH | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;
    let mut blocks: Vec<Block> = Vec::new();
    let mut mode = default_mode;
    // Current inline container being filled.
    let mut cur: Option<Block> = None;
    // Container nesting depth; 0 means top level.
    let mut depth = 0usize;
    let mut item_depth = 0usize;
    // `exact` directive waiting for the next top-level block.
    let mut exact_pending = false;
    // Source range of the top-level block currently protected as exact.
    let mut exact_until: Option<usize> = None;
    let mut html_block: Option<ByteRange<usize>> = None;
    let mut link_autolink = false;
    let mut image_depth = 0usize;
    // Inside a code or metadata block, whose text was already captured verbatim.
    let mut in_raw = false;

    fn flush(cur: &mut Option<Block>, blocks: &mut Vec<Block>) {
        if let Some(mut b) = cur.take() {
            if b.kind.is_prose()
                && b.text.trim() == ATOM.to_string()
                && b.atoms.iter().all(|a| a.kind == AtomKind::DisplayMath)
            {
                b.kind = BlockKind::Equation;
            }
            if !b.text.trim().is_empty() {
                b.index = blocks.len();
                blocks.push(b);
            }
        }
    }

    for (event, range) in Parser::new_ext(src, opts).into_offset_iter() {
        let in_exact = exact_until.is_some_and(|end| range.start < end);
        if !in_exact {
            exact_until = None;
        }
        match event {
            Event::Start(tag) => {
                let top_level = depth == 0;
                depth += 1;
                if top_level && exact_pending && !matches!(tag, Tag::HtmlBlock) {
                    exact_pending = false;
                    exact_until = Some(range.end);
                    flush(&mut cur, &mut blocks);
                    let mut b = Block::new(0, BlockKind::ExactQuote, mode, range.start);
                    b.src = range.clone();
                    b.push_text(&src[range.clone()], range.clone());
                    blocks.push(Block {
                        index: blocks.len(),
                        ..b
                    });
                    continue;
                }
                if exact_until.is_some() {
                    continue;
                }
                match tag {
                    Tag::Paragraph | Tag::Heading { .. } | Tag::TableCell => {
                        flush(&mut cur, &mut blocks);
                        let kind = match tag {
                            Tag::Heading { .. } => BlockKind::Heading,
                            Tag::TableCell => BlockKind::TableCell,
                            _ if item_depth > 0 => BlockKind::ListItem,
                            _ => BlockKind::Paragraph,
                        };
                        let mut b = Block::new(0, kind, mode, range.start);
                        b.src = range.clone();
                        cur = Some(b);
                    }
                    Tag::Item => {
                        flush(&mut cur, &mut blocks);
                        item_depth += 1;
                    }
                    Tag::List(_) | Tag::BlockQuote(_) | Tag::Table(_) => flush(&mut cur, &mut blocks),
                    Tag::CodeBlock(_) => {
                        in_raw = true;
                        flush(&mut cur, &mut blocks);
                        let mut b = Block::new(blocks.len(), BlockKind::CodeBlock, mode, range.start);
                        b.src = range.clone();
                        b.push_text(&src[range.clone()], range.clone());
                        blocks.push(b);
                    }
                    Tag::MetadataBlock(_) => {
                        in_raw = true;
                        flush(&mut cur, &mut blocks);
                        let mut b = Block::new(blocks.len(), BlockKind::Metadata, mode, range.start);
                        b.src = range.clone();
                        b.push_text(&src[range.clone()], range.clone());
                        blocks.push(b);
                    }
                    Tag::HtmlBlock => {
                        flush(&mut cur, &mut blocks);
                        html_block = Some(range.clone());
                    }
                    Tag::Link { link_type, .. } => {
                        if matches!(link_type, LinkType::Autolink | LinkType::Email) {
                            link_autolink = true;
                            open_inline(&mut cur, mode, item_depth, range.start)
                                .push_atom(AtomKind::Url, range.clone());
                        }
                    }
                    Tag::Image { .. } => {
                        if image_depth == 0 {
                            open_inline(&mut cur, mode, item_depth, range.start)
                                .push_atom(AtomKind::Image, range.clone());
                        }
                        image_depth += 1;
                    }
                    _ => {}
                }
            }
            Event::End(tag) => {
                depth = depth.saturating_sub(1);
                if exact_until.is_some() {
                    continue;
                }
                match tag {
                    TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::TableCell => flush(&mut cur, &mut blocks),
                    TagEnd::Item => {
                        flush(&mut cur, &mut blocks);
                        item_depth = item_depth.saturating_sub(1);
                    }
                    TagEnd::HtmlBlock => {
                        let r = html_block.take().unwrap_or(range.clone());
                        let raw = &src[r.clone()];
                        if let Some(c) = DIRECTIVE_RE.captures(raw) {
                            match parse_directive(&c[1]).map_err(|e| {
                                let line = src[..r.start].matches('\n').count() + 1;
                                InputError(format!("baris {line}: {e}"))
                            })? {
                                Directive::Mode(m) => mode = m,
                                Directive::Exact => exact_pending = true,
                            }
                        } else {
                            let mut b = Block::new(blocks.len(), BlockKind::Html, mode, r.start);
                            b.src = r.clone();
                            b.push_text(raw, r);
                            blocks.push(b);
                        }
                    }
                    TagEnd::Link => link_autolink = false,
                    TagEnd::CodeBlock | TagEnd::MetadataBlock(_) => in_raw = false,
                    TagEnd::Image => image_depth = image_depth.saturating_sub(1),
                    _ => {}
                }
            }
            _ if exact_until.is_some() || link_autolink || image_depth > 0 || in_raw => {}
            Event::Text(text) => {
                if html_block.is_some() {
                    continue;
                }
                open_inline(&mut cur, mode, item_depth, range.start).push_text_with_urls(&text, range.clone());
            }
            Event::Code(_) => {
                open_inline(&mut cur, mode, item_depth, range.start).push_atom(AtomKind::InlineCode, range.clone())
            }
            Event::InlineMath(_) => {
                open_inline(&mut cur, mode, item_depth, range.start).push_atom(AtomKind::InlineMath, range.clone())
            }
            Event::DisplayMath(_) => {
                open_inline(&mut cur, mode, item_depth, range.start).push_atom(AtomKind::DisplayMath, range.clone())
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some(b) = cur.as_mut() {
                    b.push_text(" ", range.clone());
                }
            }
            _ => {}
        }
        if let Some(b) = cur.as_mut() {
            b.src.end = b.src.end.max(range.end);
        }
    }
    flush(&mut cur, &mut blocks);
    if exact_pending {
        return Err(InputError("direktif `pasti: exact` tidak diikuti blok apa pun".into()));
    }
    Ok(Document {
        format: InputFormat::Markdown,
        blocks,
    })
}

/// Inline content outside a paragraph (tight list items) opens an implicit block.
fn open_inline(cur: &mut Option<Block>, mode: Mode, item_depth: usize, start: usize) -> &mut Block {
    cur.get_or_insert_with(|| {
        let kind = if item_depth > 0 {
            BlockKind::ListItem
        } else {
            BlockKind::Paragraph
        };
        Block::new(0, kind, mode, start)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn md(src: &str) -> Document {
        parse(src, InputFormat::Markdown, Mode::PROC).unwrap()
    }

    #[test]
    fn inline_code_is_one_atom_with_exact_source() {
        let src = "Jalankan `tool check --all`.";
        let d = md(src);
        assert_eq!(d.blocks.len(), 1);
        let b = &d.blocks[0];
        assert_eq!(b.atoms.len(), 1);
        assert_eq!(&src[b.atoms[0].src.clone()], "`tool check --all`");
        let r = b.src_range(0..b.text.len());
        assert_eq!(&src[r], src);
    }

    #[test]
    fn code_block_is_protected_and_kept_verbatim() {
        let src = "Teks.\n\n```\nsatu dua tiga.\n```\n";
        let d = md(src);
        assert_eq!(d.blocks[1].kind, BlockKind::CodeBlock);
        assert_eq!(&src[d.blocks[1].src.clone()], "```\nsatu dua tiga.\n```");
    }

    #[test]
    fn directives_set_mode_and_exact() {
        let src =
            "<!-- pasti: mode=WARN -->\n\nJangan sentuh.\n\n<!-- pasti: exact -->\n\n> Klik Restart.\n\nLanjut.\n";
        let d = md(src);
        let kinds: Vec<_> = d.blocks.iter().map(|b| (b.kind, b.mode)).collect();
        assert_eq!(
            kinds,
            vec![
                (BlockKind::Paragraph, Mode::WARN),
                (BlockKind::ExactQuote, Mode::WARN),
                (BlockKind::Paragraph, Mode::WARN)
            ]
        );
    }

    #[test]
    fn unknown_directive_is_input_error() {
        assert!(parse("<!-- pasti: bogus -->\n", InputFormat::Markdown, Mode::PROC).is_err());
    }

    #[test]
    fn tight_list_items_and_table_cells_are_separate_blocks() {
        let src = "- Lepas penutup.\n- Periksa segel.\n\n| Suhu (°C) | Nilai |\n|---|---|\n| 80 | ok |\n";
        let d = md(src);
        let kinds: Vec<_> = d.blocks.iter().map(|b| b.kind).collect();
        assert_eq!(kinds[..2], [BlockKind::ListItem, BlockKind::ListItem]);
        assert_eq!(kinds[2..].iter().filter(|k| **k == BlockKind::TableCell).count(), 4);
        assert_eq!(d.blocks[0].text, "Lepas penutup.");
    }

    #[test]
    fn soft_break_maps_and_bare_url_becomes_atom() {
        let src = "Buka\nhttps://contoh.id/a.b sekarang.";
        let d = parse(src, InputFormat::Text, Mode::PROC).unwrap();
        let b = &d.blocks[0];
        assert_eq!(b.atoms.len(), 1);
        assert_eq!(&src[b.atoms[0].src.clone()], "https://contoh.id/a.b");
    }
}
