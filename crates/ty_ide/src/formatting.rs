use ruff_db::files::File;
use ruff_python_ast::PySourceType;
use ruff_python_trivia::CommentRanges;

use crate::Db;
use ruff_db::source::source_text;
use ruff_formatter::{IndentStyle, IndentWidth};
use ruff_python_formatter::{PyFormatOptions, format_module_ast};
use ruff_python_parser::{ParseOptions, parse};

#[derive(Debug, Copy, Clone)]
pub struct FormattingPoint {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Copy, Clone)]
pub struct FormattingRange {
    start: FormattingPoint,
    end: FormattingPoint,
}

#[derive(Debug, Clone)]
pub struct FormatData {
    pub code: String,
    pub range: FormattingRange,
}

impl FormattingRange {
    pub fn start(&self) -> FormattingPoint {
        self.start
    }
    pub fn end(&self) -> FormattingPoint {
        self.end
    }
}

fn get_range(source: &str) -> Option<FormattingRange> {
    let lines: Vec<&str> = source.lines().collect();
    let lines_count = lines.len() as u32;
    let Some(final_line) = lines.last() else {
        return None;
    };
    let end = final_line.chars().count() as u32;
    Some(FormattingRange {
        start: FormattingPoint {
            line: 0,
            character: 0,
        },
        end: FormattingPoint {
            line: lines_count - 1,
            character: end,
        },
    })
}

#[derive(Debug, Clone, Copy)]
pub struct FormattingOptions {
    use_space: bool,
    indent_width: u32,
}

impl FormattingOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn prefer_space(mut self, prefer_space: bool) -> Self {
        self.use_space = prefer_space;
        self
    }
    pub fn with_indent_width(mut self, indent_width: u32) -> Self {
        self.indent_width = indent_width;
        self
    }
    fn indent_width(&self) -> IndentWidth {
        let indent_width: u8 = self.indent_width.try_into().unwrap_or(4);
        indent_width.try_into().unwrap_or(IndentWidth::default())
    }
    fn indent_style(&self) -> IndentStyle {
        if self.use_space {
            IndentStyle::Space
        } else {
            IndentStyle::Tab
        }
    }
}

impl Default for FormattingOptions {
    fn default() -> Self {
        Self {
            use_space: true,
            indent_width: 4,
        }
    }
}

/// This is format
pub fn formatting<'db>(
    db: &'db dyn Db,
    file: File,
    options: FormattingOptions,
) -> Option<FormatData> {
    let Some(pa) = file.path(db).as_system_path().map(|pa| pa.as_std_path()) else {
        return None;
    };
    let source_type = PySourceType::from(&pa);
    let source = source_text(db, file);
    let options = PyFormatOptions::from_extension(pa)
        .with_indent_style(options.indent_style())
        .with_indent_width(options.indent_width());
    // Parse the AST.
    let parsed = parse(source.as_str(), ParseOptions::from(source_type)).ok()?;
    let comment_ranges = CommentRanges::from(parsed.tokens());
    let formatted = format_module_ast(&parsed, &comment_ranges, source.as_str(), options).ok()?;
    let print = formatted.print().ok()?;
    Some(FormatData {
        range: get_range(source.as_str())?,
        code: print.into_code(),
    })
}
