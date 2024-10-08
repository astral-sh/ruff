use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use std::io::Write;

use annotate_snippets::display_list::{DisplayList, FormatOptions};
use annotate_snippets::snippet::{Annotation, AnnotationType, Slice, Snippet, SourceAnnotation};
use bitflags::bitflags;
use colored::Colorize;

use ruff_notebook::NotebookIndex;
use ruff_source_file::{OneIndexed, SourceLocation};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::fs::relativize_path;
use crate::line_width::{IndentWidth, LineWidthBuilder};
use crate::message::diff::Diff;
use crate::message::{Emitter, EmitterContext, Message};
use crate::settings::types::UnsafeFixes;
use crate::text_helpers::ShowNonprinting;

bitflags! {
    #[derive(Default)]
    struct EmitterFlags: u8 {
        /// Whether to show the fix status of a diagnostic.
        const SHOW_FIX_STATUS    = 0b0000_0001;
        /// Whether to show the diff of a fix, for diagnostics that have a fix.
        const SHOW_FIX_DIFF      = 0b0000_0010;
        /// Whether to show the source code of a diagnostic.
        const SHOW_SOURCE        = 0b0000_0100;
    }
}

#[derive(Default)]
pub struct TextEmitter {
    flags: EmitterFlags,
    unsafe_fixes: UnsafeFixes,
}

impl TextEmitter {
    #[must_use]
    pub fn with_show_fix_status(mut self, show_fix_status: bool) -> Self {
        self.flags
            .set(EmitterFlags::SHOW_FIX_STATUS, show_fix_status);
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
        self.unsafe_fixes = unsafe_fixes;
        self
    }
}

impl Emitter for TextEmitter {
    fn emit(
        &mut self,
        writer: &mut dyn Write,
        messages: &[Message],
        context: &EmitterContext,
    ) -> anyhow::Result<()> {
        for message in messages {
            write!(
                writer,
                "{path}{sep}",
                path = relativize_path(message.filename()).bold(),
                sep = ":".cyan(),
            )?;

            let start_location = message.compute_start_location();
            let notebook_index = context.notebook_index(message.filename());

            // Check if we're working on a jupyter notebook and translate positions with cell accordingly
            let diagnostic_location = if let Some(notebook_index) = notebook_index {
                write!(
                    writer,
                    "cell {cell}{sep}",
                    cell = notebook_index
                        .cell(start_location.row)
                        .unwrap_or(OneIndexed::MIN),
                    sep = ":".cyan(),
                )?;

                SourceLocation {
                    row: notebook_index
                        .cell_row(start_location.row)
                        .unwrap_or(OneIndexed::MIN),
                    column: start_location.column,
                }
            } else {
                start_location
            };

            writeln!(
                writer,
                "{row}{sep}{col}{sep} {code_and_body}",
                row = diagnostic_location.row,
                col = diagnostic_location.column,
                sep = ":".cyan(),
                code_and_body = RuleCodeAndBody {
                    message,
                    show_fix_status: self.flags.intersects(EmitterFlags::SHOW_FIX_STATUS),
                    unsafe_fixes: self.unsafe_fixes,
                }
            )?;

            if self.flags.intersects(EmitterFlags::SHOW_SOURCE) {
                // The `0..0` range is used to highlight file-level diagnostics.
                if message.range() != TextRange::default() {
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
    pub(crate) message: &'a Message,
    pub(crate) show_fix_status: bool,
    pub(crate) unsafe_fixes: UnsafeFixes,
}

impl Display for RuleCodeAndBody<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.show_fix_status {
            if let Some(fix) = self.message.fix() {
                // Do not display an indicator for inapplicable fixes
                if fix.applies(self.unsafe_fixes.required_applicability()) {
                    if let Some(rule) = self.message.rule() {
                        write!(f, "{} ", rule.noqa_code().to_string().red().bold())?;
                    }
                    return write!(
                        f,
                        "{fix}{body}",
                        fix = format_args!("[{}] ", "*".cyan()),
                        body = self.message.body(),
                    );
                }
            }
        };

        if let Some(rule) = self.message.rule() {
            write!(
                f,
                "{code} {body}",
                code = rule.noqa_code().to_string().red().bold(),
                body = self.message.body(),
            )
        } else {
            f.write_str(self.message.body())
        }
    }
}

pub(super) struct MessageCodeFrame<'a> {
    pub(crate) message: &'a Message,
    pub(crate) notebook_index: Option<&'a NotebookIndex>,
}

impl Display for MessageCodeFrame<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let suggestion = self.message.suggestion();
        let footer = if suggestion.is_some() {
            vec![Annotation {
                id: None,
                label: suggestion,
                annotation_type: AnnotationType::Help,
            }]
        } else {
            Vec::new()
        };

        let source_code = self.message.source_file().to_source_code();

        let content_start_index = source_code.line_index(self.message.start());
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

        let content_end_index = source_code.line_index(self.message.end());
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

        let source = replace_whitespace(
            source_code.slice(TextRange::new(start_offset, end_offset)),
            self.message.range() - start_offset,
        );

        let source_text = source.text.show_nonprinting();

        let start_char = source.text[TextRange::up_to(source.annotation_range.start())]
            .chars()
            .count();

        let char_length = source.text[source.annotation_range].chars().count();

        let label = self
            .message
            .rule()
            .map_or_else(String::new, |rule| rule.noqa_code().to_string());

        let snippet = Snippet {
            title: None,
            slices: vec![Slice {
                source: &source_text,
                line_start: self.notebook_index.map_or_else(
                    || start_index.get(),
                    |notebook_index| {
                        notebook_index
                            .cell_row(start_index)
                            .unwrap_or(OneIndexed::MIN)
                            .get()
                    },
                ),
                annotations: vec![SourceAnnotation {
                    label: &label,
                    annotation_type: AnnotationType::Error,
                    range: (start_char, start_char + char_length),
                }],
                // The origin (file name, line number, and column number) is already encoded
                // in the `label`.
                origin: None,
                fold: false,
            }],
            footer,
            opt: FormatOptions {
                #[cfg(test)]
                color: false,
                #[cfg(not(test))]
                color: colored::control::SHOULD_COLORIZE.should_colorize(),
                ..FormatOptions::default()
            },
        };

        writeln!(f, "{message}", message = DisplayList::from(snippet))
    }
}

fn replace_whitespace(source: &str, annotation_range: TextRange) -> SourceCode {
    let mut result = String::new();
    let mut last_end = 0;
    let mut range = annotation_range;
    let mut line_width = LineWidthBuilder::new(IndentWidth::default());

    for (index, c) in source.char_indices() {
        let old_width = line_width.get();
        line_width = line_width.add_char(c);

        if matches!(c, '\t') {
            // SAFETY: The difference is a value in the range [1..TAB_SIZE] which is guaranteed to be less than `u32`.
            #[allow(clippy::cast_possible_truncation)]
            let tab_width = (line_width.get() - old_width) as u32;

            if index < usize::from(annotation_range.start()) {
                range += TextSize::new(tab_width - 1);
            } else if index < usize::from(annotation_range.end()) {
                range = range.add_end(TextSize::new(tab_width - 1));
            }

            result.push_str(&source[last_end..index]);

            for _ in 0..tab_width {
                result.push(' ');
            }

            last_end = index + 1;
        }
    }

    // No tabs
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

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use crate::message::tests::{
        capture_emitter_notebook_output, capture_emitter_output, create_messages,
        create_notebook_messages, create_syntax_error_messages,
    };
    use crate::message::TextEmitter;
    use crate::settings::types::UnsafeFixes;

    #[test]
    fn default() {
        let mut emitter = TextEmitter::default().with_show_source(true);
        let content = capture_emitter_output(&mut emitter, &create_messages());

        assert_snapshot!(content);
    }

    #[test]
    fn fix_status() {
        let mut emitter = TextEmitter::default()
            .with_show_fix_status(true)
            .with_show_source(true);
        let content = capture_emitter_output(&mut emitter, &create_messages());

        assert_snapshot!(content);
    }

    #[test]
    fn fix_status_unsafe() {
        let mut emitter = TextEmitter::default()
            .with_show_fix_status(true)
            .with_show_source(true)
            .with_unsafe_fixes(UnsafeFixes::Enabled);
        let content = capture_emitter_output(&mut emitter, &create_messages());

        assert_snapshot!(content);
    }

    #[test]
    fn notebook_output() {
        let mut emitter = TextEmitter::default()
            .with_show_fix_status(true)
            .with_show_source(true)
            .with_unsafe_fixes(UnsafeFixes::Enabled);
        let (messages, notebook_indexes) = create_notebook_messages();
        let content = capture_emitter_notebook_output(&mut emitter, &messages, &notebook_indexes);

        assert_snapshot!(content);
    }

    #[test]
    fn syntax_errors() {
        let mut emitter = TextEmitter::default().with_show_source(true);
        let content = capture_emitter_output(&mut emitter, &create_syntax_error_messages());

        assert_snapshot!(content);
    }
}
