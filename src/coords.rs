//! Source coordinates (PASTI-LINT-002).
//!
//! Canonical coordinates are Unicode code points on the original source:
//! `offset_cp` and `column_cp` start at 0, `line` starts at 1, ranges are
//! half-open. CRLF counts as two code points in raw offsets but as a single
//! line break. Internally everything works on UTF-8 byte offsets and converts
//! here, so no caller ever treats a byte offset as a code-point offset.

use crate::diagnostic::{Position, Range};

#[derive(Debug, Clone)]
pub struct SourceMap<'a> {
    src: &'a str,
    /// Byte offset of the first byte of every line.
    line_starts: Vec<usize>,
    /// Code-point offset of every line start (parallel to `line_starts`).
    line_start_cps: Vec<usize>,
}

impl<'a> SourceMap<'a> {
    pub fn new(src: &'a str) -> Self {
        let mut line_starts = vec![0];
        let mut line_start_cps = vec![0];
        let bytes = src.as_bytes();
        let mut cp = 0usize;
        let mut i = 0usize;
        while i < bytes.len() {
            let b = bytes[i];
            // Count code points by skipping UTF-8 continuation bytes.
            if b & 0xC0 != 0x80 {
                cp += 1;
            }
            match b {
                b'\n' => {
                    line_starts.push(i + 1);
                    line_start_cps.push(cp);
                }
                b'\r' => {
                    if bytes.get(i + 1) == Some(&b'\n') {
                        // CRLF: one line break, two code points.
                        cp += 1;
                        i += 1;
                    }
                    line_starts.push(i + 1);
                    line_start_cps.push(cp);
                }
                _ => {}
            }
            i += 1;
        }
        Self {
            src,
            line_starts,
            line_start_cps,
        }
    }

    pub fn source(&self) -> &'a str {
        self.src
    }

    /// Converts a UTF-8 byte offset (must lie on a char boundary, may equal
    /// `src.len()`) into a canonical position.
    pub fn position(&self, byte: usize) -> Position {
        let byte = byte.min(self.src.len());
        debug_assert!(self.src.is_char_boundary(byte), "offset {byte} not on a char boundary");
        let line_idx = match self.line_starts.binary_search(&byte) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        let line_start = self.line_starts[line_idx];
        let column_cp = self.src[line_start..byte].chars().count();
        Position {
            line: line_idx + 1,
            column_cp,
            offset_cp: self.line_start_cps[line_idx] + column_cp,
        }
    }

    pub fn range(&self, start_byte: usize, end_byte: usize) -> Range {
        Range {
            start: self.position(start_byte),
            end: self.position(end_byte.max(start_byte)),
        }
    }
}

/// Converts a code-point offset into a UTF-16 code-unit offset for editor
/// adapters: one unit up to U+FFFF, two above it.
pub fn cp_to_utf16(src: &str, offset_cp: usize) -> usize {
    src.chars().take(offset_cp).map(char::len_utf16).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emoji_offsets_match_f22() {
        // F22: in "A😀B" the letter B spans code points [2,3) and UTF-16 units [3,4).
        let src = "A😀B";
        let map = SourceMap::new(src);
        let b = src.find('B').unwrap();
        let r = map.range(b, b + 1);
        assert_eq!((r.start.offset_cp, r.end.offset_cp), (2, 3));
        assert_eq!((r.start.line, r.start.column_cp), (1, 2));
        assert_eq!((cp_to_utf16(src, 2), cp_to_utf16(src, 3)), (3, 4));
    }

    #[test]
    fn crlf_is_two_code_points_but_one_line_break() {
        // F22 is also tested with CRLF before the target line.
        let src = "x\r\nA😀B";
        let map = SourceMap::new(src);
        let b = src.find('B').unwrap();
        let p = map.position(b);
        assert_eq!(p.line, 2);
        assert_eq!(p.column_cp, 2);
        assert_eq!(p.offset_cp, 5);
    }
}
