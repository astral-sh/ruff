use std::fmt::{Display, Formatter};
use std::num::NonZeroUsize;

use owo_colors::{OwoColorize, Style};

use ruff_text_size::{Ranged, TextRange, TextSize};
use similar::{ChangeTag, TextDiff};

use ruff_diagnostics::{Applicability, Fix};
use ruff_source_file::{OneIndexed, SourceFile};

use crate::message::Message;

/// Renders a diff that shows the code fixes.
///
/// The implementation isn't fully fledged out and only used by tests. Before using in production, try
/// * Improve layout
/// * Replace tabs with spaces for a consistent experience across terminals
/// * Replace zero-width whitespaces
/// * Print a simpler diff if only a single line has changed
/// * Compute the diff from the [`Edit`] because diff calculation is expensive.
pub(super) struct Diff<'a> {
    fix: &'a Fix,
    source_code: &'a SourceFile,
}

impl<'a> Diff<'a> {
    pub(crate) fn from_message(message: &'a Message) -> Option<Diff> {
        message.fix.as_ref().map(|fix| Diff {
            source_code: &message.file,
            fix,
        })
    }
}

impl Display for Diff<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // TODO(dhruvmanila): Add support for Notebook cells once it's user-facing
        let mut output = String::with_capacity(self.source_code.source_text().len());
        let mut last_end = TextSize::default();

        for edit in self.fix.edits() {
            output.push_str(
                self.source_code
                    .slice(TextRange::new(last_end, edit.start())),
            );
            output.push_str(edit.content().unwrap_or_default());
            last_end = edit.end();
        }

        output.push_str(&self.source_code.source_text()[usize::from(last_end)..]);

        let diff = TextDiff::from_lines(self.source_code.source_text(), &output);

        let message = match self.fix.applicability() {
            // TODO(zanieb): Adjust this messaging once it's user-facing
            Applicability::Safe => "Safe fix",
            Applicability::Unsafe => "Unsafe fix",
            Applicability::DisplayOnly => "Display-only fix",
        };
        writeln!(f, "ℹ {}", message.blue())?;

        let (largest_old, largest_new) = diff
            .ops()
            .last()
            .map(|op| (op.old_range().start, op.new_range().start))
            .unwrap_or_default();

        let digit_with =
            calculate_print_width(OneIndexed::from_zero_indexed(largest_new.max(largest_old)));

        for (idx, group) in diff.grouped_ops(3).iter().enumerate() {
            if idx > 0 {
                writeln!(f, "{:-^1$}", "-", 80)?;
            }
            for op in group {
                for change in diff.iter_inline_changes(op) {
                    let sign = match change.tag() {
                        ChangeTag::Delete => "-",
                        ChangeTag::Insert => "+",
                        ChangeTag::Equal => " ",
                    };

                    let line_style = diff_line_style(change.tag());

                    let old_index = change.old_index().map(OneIndexed::from_zero_indexed);
                    let new_index = change.new_index().map(OneIndexed::from_zero_indexed);

                    write!(
                        f,
                        "{} {} |{}",
                        Line {
                            index: old_index,
                            width: digit_with
                        },
                        Line {
                            index: new_index,
                            width: digit_with
                        },
                        sign.style(line_style).bold()
                    )?;

                    for (emphasized, value) in change.iter_strings_lossy() {
                        if emphasized {
                            write!(f, "{}", value.style(line_style).underline().on_black())?;
                        } else {
                            write!(f, "{}", value.style(line_style))?;
                        }
                    }
                    if change.missing_newline() {
                        writeln!(f)?;
                    }
                }
            }
        }

        Ok(())
    }
}

fn diff_line_style(change_tag: ChangeTag) -> Style {
    match change_tag {
        ChangeTag::Equal => Style::new().dimmed(),
        ChangeTag::Delete => Style::new().red(),
        ChangeTag::Insert => Style::new().green(),
    }
}

struct Line {
    index: Option<OneIndexed>,
    width: NonZeroUsize,
}

impl Display for Line {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self.index {
            None => {
                for _ in 0..self.width.get() {
                    f.write_str(" ")?;
                }
                Ok(())
            }
            Some(idx) => write!(f, "{:<width$}", idx, width = self.width.get()),
        }
    }
}

/// Calculate the length of the string representation of `value`
pub(super) fn calculate_print_width(mut value: OneIndexed) -> NonZeroUsize {
    const TEN: OneIndexed = OneIndexed::from_zero_indexed(9);

    let mut width = OneIndexed::ONE;

    while value >= TEN {
        value = OneIndexed::new(value.get() / 10).unwrap_or(OneIndexed::MIN);
        width = width.checked_add(1).unwrap();
    }

    width
}
