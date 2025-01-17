//! Adapted from [styled_buffer]
//!
//! [styled_buffer]: https://github.com/rust-lang/rust/blob/894f7a4ba6554d3797404bbf550d9919df060b97/compiler/rustc_errors/src/styled_buffer.rs

use crate::renderer::stylesheet::Stylesheet;
use anstyle::Style;
use std::fmt;
use std::fmt::Write;

#[derive(Debug)]
pub(crate) struct StyledBuffer {
    lines: Vec<Vec<StyledChar>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StyledChar {
    ch: char,
    style: Style,
}

impl StyledChar {
    pub(crate) const SPACE: Self = StyledChar::new(' ', Style::new());

    pub(crate) const fn new(ch: char, style: Style) -> StyledChar {
        StyledChar { ch, style }
    }
}

impl StyledBuffer {
    pub(crate) fn new() -> StyledBuffer {
        StyledBuffer { lines: vec![] }
    }

    fn ensure_lines(&mut self, line: usize) {
        if line >= self.lines.len() {
            self.lines.resize(line + 1, Vec::new());
        }
    }

    pub(crate) fn render(&self, stylesheet: &Stylesheet) -> Result<String, fmt::Error> {
        let mut str = String::new();
        for (i, line) in self.lines.iter().enumerate() {
            let mut current_style = stylesheet.none;
            for ch in line {
                if ch.style != current_style {
                    if !line.is_empty() {
                        write!(str, "{}", current_style.render_reset())?;
                    }
                    current_style = ch.style;
                    write!(str, "{}", current_style.render())?;
                }
                write!(str, "{}", ch.ch)?;
            }
            write!(str, "{}", current_style.render_reset())?;
            if i != self.lines.len() - 1 {
                writeln!(str)?;
            }
        }
        Ok(str)
    }

    /// Sets `chr` with `style` for given `line`, `col`.
    /// If `line` does not exist in our buffer, adds empty lines up to the given
    /// and fills the last line with unstyled whitespace.
    pub(crate) fn putc(&mut self, line: usize, col: usize, chr: char, style: Style) {
        self.ensure_lines(line);
        if col >= self.lines[line].len() {
            self.lines[line].resize(col + 1, StyledChar::SPACE);
        }
        self.lines[line][col] = StyledChar::new(chr, style);
    }

    /// Sets `string` with `style` for given `line`, starting from `col`.
    /// If `line` does not exist in our buffer, adds empty lines up to the given
    /// and fills the last line with unstyled whitespace.
    pub(crate) fn puts(&mut self, line: usize, col: usize, string: &str, style: Style) {
        let mut n = col;
        for c in string.chars() {
            self.putc(line, n, c, style);
            n += 1;
        }
    }
    /// For given `line` inserts `string` with `style` after old content of that line,
    /// adding lines if needed
    pub(crate) fn append(&mut self, line: usize, string: &str, style: Style) {
        if line >= self.lines.len() {
            self.puts(line, 0, string, style);
        } else {
            let col = self.lines[line].len();
            self.puts(line, col, string, style);
        }
    }

    pub(crate) fn num_lines(&self) -> usize {
        self.lines.len()
    }
}
