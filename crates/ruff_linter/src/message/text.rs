use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use std::io::Write;

use bitflags::bitflags;
use colored::Colorize;
use ruff_annotate_snippets::{Level, Renderer, Snippet};

use ruff_db::diagnostic::{Diagnostic, DiagnosticFormat, DisplayDiagnosticConfig, SecondaryCode};
use ruff_notebook::NotebookIndex;
use ruff_source_file::OneIndexed;
use ruff_text_size::{TextLen, TextRange, TextSize};

use crate::Locator;
use crate::line_width::{IndentWidth, LineWidthBuilder};
use crate::message::diff::Diff;
use crate::message::{Emitter, EmitterContext};
use crate::settings::types::UnsafeFixes;

bitflags! {
    #[derive(Default)]
    struct EmitterFlags: u8 {
        /// Whether to show the diff of a fix, for diagnostics that have a fix.
        const SHOW_FIX_DIFF     = 1 << 1;
        /// Whether to show the source code of a diagnostic.
        const SHOW_SOURCE       = 1 << 2;
    }
}

pub struct TextEmitter {
    flags: EmitterFlags,
    config: DisplayDiagnosticConfig,
}

impl Default for TextEmitter {
    fn default() -> Self {
        Self {
            flags: EmitterFlags::default(),
            config: DisplayDiagnosticConfig::default()
                .format(DiagnosticFormat::Concise)
                .hide_severity(true)
                .color(!cfg!(test) && colored::control::SHOULD_COLORIZE.should_colorize()),
        }
    }
}

impl TextEmitter {
    #[must_use]
    pub fn with_show_fix_status(mut self, show_fix_status: bool) -> Self {
        self.config = self.config.show_fix_status(show_fix_status);
        self
    }

    #[must_use]
    pub fn with_show_fix_diff(mut self, show_fix_diff: bool) -> Self {
        self.flags.set(EmitterFlags::SHOW_FIX_DIFF, show_fix_diff);
        self
    }

    #[must_use]
    pub fn with_show_source(mut self, show_source: bool) -> Self {
        self.flags.set(EmitterFlags::SHOW_SOURCE, show_source);
        self
    }

    #[must_use]
    pub fn with_unsafe_fixes(mut self, unsafe_fixes: UnsafeFixes) -> Self {
        self.config = self
            .config
            .fix_applicability(unsafe_fixes.required_applicability());
        self
    }

    #[must_use]
    pub fn with_preview(mut self, preview: bool) -> Self {
        self.config = self.config.preview(preview);
        self
    }
}

impl Emitter for TextEmitter {
    fn emit(
        &mut self,
        writer: &mut dyn Write,
        diagnostics: &[Diagnostic],
        context: &EmitterContext,
    ) -> anyhow::Result<()> {
        for message in diagnostics {
            write!(writer, "{}", message.display(context, &self.config))?;

            let filename = message.expect_ruff_filename();
            let notebook_index = context.notebook_index(&filename);
            if self.flags.intersects(EmitterFlags::SHOW_SOURCE) {
                // The `0..0` range is used to highlight file-level diagnostics.
                if message.expect_range() != TextRange::default() {
                    writeln!(
                        writer,
                        "{}",
                        MessageCodeFrame {
                            message,
                            notebook_index
                        }
                    )?;
                }
            }

            if self.flags.intersects(EmitterFlags::SHOW_FIX_DIFF) {
                if let Some(diff) = Diff::from_message(message) {
                    writeln!(writer, "{diff}")?;
                }
            }
        }

        Ok(())
    }
}

pub(super) struct RuleCodeAndBody<'a> {
    pub(crate) message: &'a Diagnostic,
    pub(crate) show_fix_status: bool,
    pub(crate) unsafe_fixes: UnsafeFixes,
}

impl Display for RuleCodeAndBody<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.show_fix_status {
            if let Some(fix) = self.message.fix() {
                // Do not display an indicator for inapplicable fixes
                if fix.applies(self.unsafe_fixes.required_applicability()) {
                    if let Some(code) = self.message.secondary_code() {
                        write!(f, "{} ", code.red().bold())?;
                    }
                    return write!(
                        f,
                        "{fix}{body}",
                        fix = format_args!("[{}] ", "*".cyan()),
                        body = self.message.body(),
                    );
                }
            }
        }

        if let Some(code) = self.message.secondary_code() {
            write!(
                f,
                "{code} {body}",
                code = code.red().bold(),
                body = self.message.body(),
            )
        } else {
            f.write_str(self.message.body())
        }
    }
}

pub(super) struct MessageCodeFrame<'a> {
    pub(crate) message: &'a Diagnostic,
    pub(crate) notebook_index: Option<&'a NotebookIndex>,
}

impl Display for MessageCodeFrame<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let suggestion = self.message.first_help_text();
        let footers = if let Some(suggestion) = suggestion {
            vec![Level::Help.title(suggestion)]
        } else {
            Vec::new()
        };

        let source_file = self.message.expect_ruff_source_file();
        let source_code = source_file.to_source_code();

        let content_start_index = source_code.line_index(self.message.expect_range().start());
        let mut start_index = content_start_index.saturating_sub(2);

        // If we're working with a Jupyter Notebook, skip the lines which are
        // outside of the cell containing the diagnostic.
        if let Some(index) = self.notebook_index {
            let content_start_cell = index.cell(content_start_index).unwrap_or(OneIndexed::MIN);
            while start_index < content_start_index {
                if index.cell(start_index).unwrap_or(OneIndexed::MIN) == content_start_cell {
                    break;
                }
                start_index = start_index.saturating_add(1);
            }
        }

        // Trim leading empty lines.
        while start_index < content_start_index {
            if !source_code.line_text(start_index).trim().is_empty() {
                break;
            }
            start_index = start_index.saturating_add(1);
        }

        let content_end_index = source_code.line_index(self.message.expect_range().end());
        let mut end_index = content_end_index
            .saturating_add(2)
            .min(OneIndexed::from_zero_indexed(source_code.line_count()));

        // If we're working with a Jupyter Notebook, skip the lines which are
        // outside of the cell containing the diagnostic.
        if let Some(index) = self.notebook_index {
            let content_end_cell = index.cell(content_end_index).unwrap_or(OneIndexed::MIN);
            while end_index > content_end_index {
                if index.cell(end_index).unwrap_or(OneIndexed::MIN) == content_end_cell {
                    break;
                }
                end_index = end_index.saturating_sub(1);
            }
        }

        // Trim trailing empty lines.
        while end_index > content_end_index {
            if !source_code.line_text(end_index).trim().is_empty() {
                break;
            }

            end_index = end_index.saturating_sub(1);
        }

        let start_offset = source_code.line_start(start_index);
        let end_offset = source_code.line_end(end_index);

        let source = replace_whitespace_and_unprintable(
            source_code.slice(TextRange::new(start_offset, end_offset)),
            self.message.expect_range() - start_offset,
        )
        .fix_up_empty_spans_after_line_terminator();

        let label = self
            .message
            .secondary_code()
            .map(SecondaryCode::as_str)
            .unwrap_or_default();

        let line_start = self.notebook_index.map_or_else(
            || start_index.get(),
            |notebook_index| {
                notebook_index
                    .cell_row(start_index)
                    .unwrap_or(OneIndexed::MIN)
                    .get()
            },
        );

        let span = usize::from(source.annotation_range.start())
            ..usize::from(source.annotation_range.end());
        let annotation = Level::Error.span(span).label(label);
        let snippet = Snippet::source(&source.text)
            .line_start(line_start)
            .annotation(annotation)
            .fold(false);
        let message = Level::None.title("").snippet(snippet).footers(footers);

        let renderer = if !cfg!(test) && colored::control::SHOULD_COLORIZE.should_colorize() {
            Renderer::styled()
        } else {
            Renderer::plain()
        }
        .cut_indicator("…");
        let rendered = renderer.render(message);
        writeln!(f, "{rendered}")
    }
}

/// Given some source code and an annotation range, this routine replaces
/// tabs with ASCII whitespace, and unprintable characters with printable
/// representations of them.
///
/// The source code returned has an annotation that is updated to reflect
/// changes made to the source code (if any).
fn replace_whitespace_and_unprintable(source: &str, annotation_range: TextRange) -> SourceCode {
    let mut result = String::new();
    let mut last_end = 0;
    let mut range = annotation_range;
    let mut line_width = LineWidthBuilder::new(IndentWidth::default());

    // Updates the range given by the caller whenever a single byte (at
    // `index` in `source`) is replaced with `len` bytes.
    //
    // When the index occurs before the start of the range, the range is
    // offset by `len`. When the range occurs after or at the start but before
    // the end, then the end of the range only is offset by `len`.
    let mut update_range = |index, len| {
        if index < usize::from(annotation_range.start()) {
            range += TextSize::new(len - 1);
        } else if index < usize::from(annotation_range.end()) {
            range = range.add_end(TextSize::new(len - 1));
        }
    };

    // If `c` is an unprintable character, then this returns a printable
    // representation of it (using a fancier Unicode codepoint).
    let unprintable_replacement = |c: char| -> Option<char> {
        match c {
            '\x07' => Some('␇'),
            '\x08' => Some('␈'),
            '\x1b' => Some('␛'),
            '\x7f' => Some('␡'),
            _ => None,
        }
    };

    for (index, c) in source.char_indices() {
        let old_width = line_width.get();
        line_width = line_width.add_char(c);

        if matches!(c, '\t') {
            let tab_width = u32::try_from(line_width.get() - old_width)
                .expect("small width because of tab size");
            result.push_str(&source[last_end..index]);
            for _ in 0..tab_width {
                result.push(' ');
            }
            last_end = index + 1;
            update_range(index, tab_width);
        } else if let Some(printable) = unprintable_replacement(c) {
            result.push_str(&source[last_end..index]);
            result.push(printable);
            last_end = index + 1;

            let len = printable.text_len().to_u32();
            update_range(index, len);
        }
    }

    // No tabs or unprintable chars
    if result.is_empty() {
        SourceCode {
            annotation_range,
            text: Cow::Borrowed(source),
        }
    } else {
        result.push_str(&source[last_end..]);
        SourceCode {
            annotation_range: range,
            text: Cow::Owned(result),
        }
    }
}

struct SourceCode<'a> {
    text: Cow<'a, str>,
    annotation_range: TextRange,
}

impl<'a> SourceCode<'a> {
    /// This attempts to "fix up" the span on `SourceCode` in the case where
    /// it's an empty span immediately following a line terminator.
    ///
    /// At present, `annotate-snippets` (both upstream and our vendored copy)
    /// will render annotations of such spans to point to the space immediately
    /// following the previous line. But ideally, this should point to the space
    /// immediately preceding the next line.
    ///
    /// After attempting to fix `annotate-snippets` and giving up after a couple
    /// hours, this routine takes a different tact: it adjusts the span to be
    /// non-empty and it will cover the first codepoint of the following line.
    /// This forces `annotate-snippets` to point to the right place.
    ///
    /// See also: <https://github.com/astral-sh/ruff/issues/15509>
    fn fix_up_empty_spans_after_line_terminator(self) -> SourceCode<'a> {
        if !self.annotation_range.is_empty()
            || self.annotation_range.start() == TextSize::from(0)
            || self.annotation_range.start() >= self.text.text_len()
        {
            return self;
        }
        if self.text.as_bytes()[self.annotation_range.start().to_usize() - 1] != b'\n' {
            return self;
        }
        let locator = Locator::new(&self.text);
        let start = self.annotation_range.start();
        let end = locator.ceil_char_boundary(start + TextSize::from(1));
        SourceCode {
            annotation_range: TextRange::new(start, end),
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use insta::assert_snapshot;
    use rustc_hash::FxHashMap;

    use ruff_db::diagnostic::{DiagnosticFormat, DisplayDiagnosticConfig, DisplayDiagnostics};

    use crate::codes::Rule;
    use crate::message::tests::{
        capture_emitter_notebook_output, capture_emitter_output, create_diagnostics,
        create_notebook_diagnostics, create_syntax_error_diagnostics,
    };
    use crate::message::{EmitterContext, TextEmitter};
    use crate::settings::LinterSettings;
    use crate::settings::types::UnsafeFixes;
    use crate::test::test_path;

    #[test]
    fn default() {
        let mut emitter = TextEmitter::default().with_show_source(true);
        let content = capture_emitter_output(&mut emitter, &create_diagnostics());

        assert_snapshot!(content);
    }

    #[test]
    fn fix_status() {
        let mut emitter = TextEmitter::default()
            .with_show_fix_status(true)
            .with_show_source(true);
        let content = capture_emitter_output(&mut emitter, &create_diagnostics());

        assert_snapshot!(content);
    }

    #[test]
    fn fix_status_unsafe() {
        let mut emitter = TextEmitter::default()
            .with_show_fix_status(true)
            .with_show_source(true)
            .with_unsafe_fixes(UnsafeFixes::Enabled);
        let content = capture_emitter_output(&mut emitter, &create_diagnostics());

        assert_snapshot!(content);
    }

    #[test]
    fn notebook_output() {
        let mut emitter = TextEmitter::default()
            .with_show_fix_status(true)
            .with_show_source(true)
            .with_unsafe_fixes(UnsafeFixes::Enabled);
        let (messages, notebook_indexes) = create_notebook_diagnostics();
        let content = capture_emitter_notebook_output(&mut emitter, &messages, &notebook_indexes);

        assert_snapshot!(content);
    }

    #[test]
    fn syntax_errors() {
        let mut emitter = TextEmitter::default().with_show_source(true);
        let content = capture_emitter_output(&mut emitter, &create_syntax_error_diagnostics());

        assert_snapshot!(content);
    }

    /// Check that the new `full` rendering code in `ruff_db` handles cases fixed by commit c9b99e4.
    ///
    /// For example, without the fix, we get diagnostics like this:
    ///
    /// ```
    ///  0 │+error[no-indented-block]: Expected an indented block
    ///  1 │+  --> E11.py:9:1
    ///  2 │+   |
    ///  3 │+ 7 | #: E112
    ///  4 │+ 8 | if False:
    ///  5 │+   |          ^
    ///  6 │+ 9 | print()
    ///  7 │+10 | #: E113
    ///  8 │+11 | print()
    ///  9 │+   |
    ///  ```
    ///
    /// where the caret points to the end of the previous line instead of the start of the next.
    #[test]
    fn empty_span_after_line_terminator() -> anyhow::Result<()> {
        let path = Path::new("pycodestyle").join("E11.py");
        let settings = LinterSettings::for_rule(Rule::NoIndentedBlock);
        let diagnostics = test_path(path, &settings)?;
        let config = DisplayDiagnosticConfig::default().format(DiagnosticFormat::Full);
        let notebook_indexes = FxHashMap::default();
        let context = EmitterContext::new(&notebook_indexes);
        let value = DisplayDiagnostics::new(&context, &config, &diagnostics);
        insta::assert_snapshot!(value.to_string(), @r"
        error[no-indented-block]: Expected an indented block
          --> E11.py:9:1
           |
         7 | #: E112
         8 | if False:
           |          ^
         9 | print()
        10 | #: E113
        11 | print()
           |

        error[invalid-syntax]: SyntaxError: Expected an indented block after `if` statement
          --> E11.py:9:1
           |
         7 | #: E112
         8 | if False:
         9 | print()
           | ^^^^^
        10 | #: E113
        11 | print()
           |

        error[invalid-syntax]: SyntaxError: Unexpected indentation
          --> E11.py:12:1
           |
        10 | #: E113
        11 | print()
        12 |     print()
           | ^^^^
        13 | #: E114 E116
        14 | mimetype = 'application/x-directory'
           |

        error[invalid-syntax]: SyntaxError: Expected a statement
          --> E11.py:14:1
           |
        12 |     print()
        13 | #: E114 E116
           |             ^
        14 | mimetype = 'application/x-directory'
        15 |      # 'httpd/unix-directory'
        16 | create_date = False
           |

        error[no-indented-block]: Expected an indented block
          --> E11.py:45:1
           |
        43 | #: E112
        44 | if False:  #
           |             ^
        45 | print()
        46 | #:
        47 | if False:
           |

        error[invalid-syntax]: SyntaxError: Expected an indented block after `if` statement
          --> E11.py:45:1
           |
        43 | #: E112
        44 | if False:  #
        45 | print()
           | ^^^^^
        46 | #:
        47 | if False:
           |
        ");
        Ok(())
    }
}
