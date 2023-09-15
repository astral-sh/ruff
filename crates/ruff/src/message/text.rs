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
use crate::line_width::{LineWidthBuilder, TabSize};
use crate::message::diff::Diff;
use crate::message::{Emitter, EmitterContext, Message};
use crate::registry::AsRule;

bitflags! {
    #[derive(Default)]
    struct EmitterFlags: u8 {
        /// Whether to show the fix status of a diagnostic.
        const SHOW_FIX_STATUS = 0b0000_0001;
        /// Whether to show the diff of a fix, for diagnostics that have a fix.
        const SHOW_FIX_DIFF   = 0b0000_0010;
        /// Whether to show the source code of a diagnostic.
        const SHOW_SOURCE     = 0b0000_0100;
    }
}

#[derive(Default)]
pub struct TextEmitter {
    flags: EmitterFlags,
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
                        .cell(start_location.row.get())
                        .unwrap_or_default(),
                    sep = ":".cyan(),
                )?;

                SourceLocation {
                    row: OneIndexed::new(
                        notebook_index
                            .cell_row(start_location.row.get())
                            .unwrap_or(1) as usize,
                    )
                    .unwrap(),
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
                    show_fix_status: self.flags.intersects(EmitterFlags::SHOW_FIX_STATUS)
                }
            )?;

            if self.flags.intersects(EmitterFlags::SHOW_SOURCE) {
                writeln!(
                    writer,
                    "{}",
                    MessageCodeFrame {
                        message,
                        notebook_index
                    }
                )?;
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
}

impl Display for RuleCodeAndBody<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let kind = &self.message.kind;

        if self.show_fix_status && self.message.fix.is_some() {
            write!(
                f,
                "{code} {autofix}{body}",
                code = kind.rule().noqa_code().to_string().red().bold(),
                autofix = format_args!("[{}] ", "*".cyan()),
                body = kind.body,
            )
        } else {
            write!(
                f,
                "{code} {body}",
                code = kind.rule().noqa_code().to_string().red().bold(),
                body = kind.body,
            )
        }
    }
}

pub(super) struct MessageCodeFrame<'a> {
    pub(crate) message: &'a Message,
    pub(crate) notebook_index: Option<&'a NotebookIndex>,
}

impl Display for MessageCodeFrame<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Message {
            kind, file, range, ..
        } = self.message;

        let suggestion = kind.suggestion.as_deref();
        let footer = if suggestion.is_some() {
            vec![Annotation {
                id: None,
                label: suggestion,
                annotation_type: AnnotationType::Help,
            }]
        } else {
            Vec::new()
        };

        let source_code = file.to_source_code();

        let content_start_index = source_code.line_index(range.start());
        let mut start_index = content_start_index.saturating_sub(2);

        // If we're working with a Jupyter Notebook, skip the lines which are
        // outside of the cell containing the diagnostic.
        if let Some(index) = self.notebook_index {
            let content_start_cell = index.cell(content_start_index.get()).unwrap_or_default();
            while start_index < content_start_index {
                if index.cell(start_index.get()).unwrap_or_default() == content_start_cell {
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

        let content_end_index = source_code.line_index(range.end());
        let mut end_index = content_end_index
            .saturating_add(2)
            .min(OneIndexed::from_zero_indexed(source_code.line_count()));

        // If we're working with a Jupyter Notebook, skip the lines which are
        // outside of the cell containing the diagnostic.
        if let Some(index) = self.notebook_index {
            let content_end_cell = index.cell(content_end_index.get()).unwrap_or_default();
            while end_index > content_end_index {
                if index.cell(end_index.get()).unwrap_or_default() == content_end_cell {
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
            range - start_offset,
        );

        let start_char = source.text[TextRange::up_to(source.annotation_range.start())]
            .chars()
            .count();

        let char_length = source.text[source.annotation_range].chars().count();

        let label = kind.rule().noqa_code().to_string();

        let snippet = Snippet {
            title: None,
            slices: vec![Slice {
                source: &source.text,
                line_start: self.notebook_index.map_or_else(
                    || start_index.get(),
                    |notebook_index| {
                        notebook_index
                            .cell_row(start_index.get())
                            .unwrap_or_default() as usize
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
    let mut line_width = LineWidthBuilder::new(TabSize::default());

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

    use crate::message::tests::{capture_emitter_output, create_messages};
    use crate::message::TextEmitter;

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
}
