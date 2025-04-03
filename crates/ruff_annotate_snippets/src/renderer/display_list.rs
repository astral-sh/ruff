//! `display_list` module stores the output model for the snippet.
//!
//! `DisplayList` is a central structure in the crate, which contains
//! the structured list of lines to be displayed.
//!
//! It is made of two types of lines: `Source` and `Raw`. All `Source` lines
//! are structured using four columns:
//!
//! ```text
//!  /------------ (1) Line number column.
//!  |  /--------- (2) Line number column delimiter.
//!  |  | /------- (3) Inline marks column.
//!  |  | |   /--- (4) Content column with the source and annotations for slices.
//!  |  | |   |
//! =============================================================================
//! error[E0308]: mismatched types
//!    --> src/format.rs:51:5
//!     |
//! 151 | /   fn test() -> String {
//! 152 | |       return "test";
//! 153 | |   }
//!     | |___^ error: expected `String`, for `&str`.
//!     |
//! ```
//!
//! The first two lines of the example above are `Raw` lines, while the rest
//! are `Source` lines.
//!
//! `DisplayList` does not store column alignment information, and those are
//! only calculated by the implementation of `std::fmt::Display` using information such as
//! styling.
//!
//! The above snippet has been built out of the following structure:
use crate::snippet;
use std::cmp::{max, min, Reverse};
use std::collections::HashMap;
use std::fmt::Display;
use std::ops::Range;
use std::{cmp, fmt};

use unicode_width::UnicodeWidthStr;

use crate::renderer::styled_buffer::StyledBuffer;
use crate::renderer::{stylesheet::Stylesheet, Margin, Style, DEFAULT_TERM_WIDTH};

const ANONYMIZED_LINE_NUM: &str = "LL";
const ERROR_TXT: &str = "error";
const HELP_TXT: &str = "help";
const INFO_TXT: &str = "info";
const NOTE_TXT: &str = "note";
const WARNING_TXT: &str = "warning";

/// List of lines to be displayed.
pub(crate) struct DisplayList<'a> {
    pub(crate) body: Vec<DisplaySet<'a>>,
    pub(crate) stylesheet: &'a Stylesheet,
    pub(crate) anonymized_line_numbers: bool,
    pub(crate) cut_indicator: &'static str,
}

impl PartialEq for DisplayList<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.body == other.body && self.anonymized_line_numbers == other.anonymized_line_numbers
    }
}

impl fmt::Debug for DisplayList<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DisplayList")
            .field("body", &self.body)
            .field("anonymized_line_numbers", &self.anonymized_line_numbers)
            .finish()
    }
}

impl Display for DisplayList<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let lineno_width = self.body.iter().fold(0, |max, set| {
            set.display_lines.iter().fold(max, |max, line| match line {
                DisplayLine::Source { lineno, .. } => cmp::max(lineno.unwrap_or(0), max),
                _ => max,
            })
        });
        let lineno_width = if lineno_width == 0 {
            lineno_width
        } else if self.anonymized_line_numbers {
            ANONYMIZED_LINE_NUM.len()
        } else {
            ((lineno_width as f64).log10().floor() as usize) + 1
        };

        let multiline_depth = self.body.iter().fold(0, |max, set| {
            set.display_lines.iter().fold(max, |max2, line| match line {
                DisplayLine::Source { annotations, .. } => cmp::max(
                    annotations.iter().fold(max2, |max3, line| {
                        cmp::max(
                            match line.annotation_part {
                                DisplayAnnotationPart::Standalone => 0,
                                DisplayAnnotationPart::LabelContinuation => 0,
                                DisplayAnnotationPart::MultilineStart(depth) => depth + 1,
                                DisplayAnnotationPart::MultilineEnd(depth) => depth + 1,
                            },
                            max3,
                        )
                    }),
                    max,
                ),
                _ => max2,
            })
        });
        let mut buffer = StyledBuffer::new();
        for set in self.body.iter() {
            self.format_set(set, lineno_width, multiline_depth, &mut buffer)?;
        }
        write!(f, "{}", buffer.render(self.stylesheet)?)
    }
}

impl<'a> DisplayList<'a> {
    pub(crate) fn new(
        message: snippet::Message<'a>,
        stylesheet: &'a Stylesheet,
        anonymized_line_numbers: bool,
        term_width: usize,
        cut_indicator: &'static str,
    ) -> DisplayList<'a> {
        let body = format_message(
            message,
            term_width,
            anonymized_line_numbers,
            cut_indicator,
            true,
        );

        Self {
            body,
            stylesheet,
            anonymized_line_numbers,
            cut_indicator,
        }
    }

    fn format_set(
        &self,
        set: &DisplaySet<'_>,
        lineno_width: usize,
        multiline_depth: usize,
        buffer: &mut StyledBuffer,
    ) -> fmt::Result {
        for line in &set.display_lines {
            set.format_line(
                line,
                lineno_width,
                multiline_depth,
                self.stylesheet,
                self.anonymized_line_numbers,
                self.cut_indicator,
                buffer,
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct DisplaySet<'a> {
    pub(crate) display_lines: Vec<DisplayLine<'a>>,
    pub(crate) margin: Margin,
}

impl DisplaySet<'_> {
    fn format_label(
        &self,
        line_offset: usize,
        label: &[DisplayTextFragment<'_>],
        stylesheet: &Stylesheet,
        buffer: &mut StyledBuffer,
    ) -> fmt::Result {
        for fragment in label {
            let style = match fragment.style {
                DisplayTextStyle::Regular => stylesheet.none(),
                DisplayTextStyle::Emphasis => stylesheet.emphasis(),
            };
            buffer.append(line_offset, fragment.content, *style);
        }
        Ok(())
    }
    fn format_annotation(
        &self,
        line_offset: usize,
        annotation: &Annotation<'_>,
        continuation: bool,
        stylesheet: &Stylesheet,
        buffer: &mut StyledBuffer,
    ) -> fmt::Result {
        let color = get_annotation_style(&annotation.annotation_type, stylesheet);
        let formatted_len = if let Some(id) = &annotation.id {
            2 + id.len() + annotation_type_len(&annotation.annotation_type)
        } else {
            annotation_type_len(&annotation.annotation_type)
        };

        if continuation {
            for _ in 0..formatted_len + 2 {
                buffer.append(line_offset, " ", Style::new());
            }
            return self.format_label(line_offset, &annotation.label, stylesheet, buffer);
        }
        if formatted_len == 0 {
            self.format_label(line_offset, &annotation.label, stylesheet, buffer)
        } else {
            let id = match &annotation.id {
                Some(id) => format!("[{id}]"),
                None => String::new(),
            };
            buffer.append(
                line_offset,
                &format!("{}{}", annotation_type_str(&annotation.annotation_type), id),
                *color,
            );

            if !is_annotation_empty(annotation) {
                buffer.append(line_offset, ": ", stylesheet.none);
                self.format_label(line_offset, &annotation.label, stylesheet, buffer)?;
            }
            Ok(())
        }
    }

    #[inline]
    fn format_raw_line(
        &self,
        line_offset: usize,
        line: &DisplayRawLine<'_>,
        lineno_width: usize,
        stylesheet: &Stylesheet,
        buffer: &mut StyledBuffer,
    ) -> fmt::Result {
        match line {
            DisplayRawLine::Origin {
                path,
                pos,
                header_type,
            } => {
                let header_sigil = match header_type {
                    DisplayHeaderType::Initial => "-->",
                    DisplayHeaderType::Continuation => ":::",
                };
                let lineno_color = stylesheet.line_no();
                buffer.puts(line_offset, lineno_width, header_sigil, *lineno_color);
                buffer.puts(line_offset, lineno_width + 4, path, stylesheet.none);
                if let Some((col, row)) = pos {
                    buffer.append(line_offset, ":", stylesheet.none);
                    buffer.append(line_offset, col.to_string().as_str(), stylesheet.none);
                    buffer.append(line_offset, ":", stylesheet.none);
                    buffer.append(line_offset, row.to_string().as_str(), stylesheet.none);
                }
                Ok(())
            }
            DisplayRawLine::Annotation {
                annotation,
                source_aligned,
                continuation,
            } => {
                if *source_aligned {
                    if *continuation {
                        for _ in 0..lineno_width + 3 {
                            buffer.append(line_offset, " ", stylesheet.none);
                        }
                    } else {
                        let lineno_color = stylesheet.line_no();
                        for _ in 0..lineno_width + 1 {
                            buffer.append(line_offset, " ", stylesheet.none);
                        }
                        buffer.append(line_offset, "=", *lineno_color);
                        buffer.append(line_offset, " ", *lineno_color);
                    }
                }
                self.format_annotation(line_offset, annotation, *continuation, stylesheet, buffer)
            }
        }
    }

    // Adapted from https://github.com/rust-lang/rust/blob/d371d17496f2ce3a56da76aa083f4ef157572c20/compiler/rustc_errors/src/emitter.rs#L706-L1211
    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn format_line(
        &self,
        dl: &DisplayLine<'_>,
        lineno_width: usize,
        multiline_depth: usize,
        stylesheet: &Stylesheet,
        anonymized_line_numbers: bool,
        cut_indicator: &'static str,
        buffer: &mut StyledBuffer,
    ) -> fmt::Result {
        let line_offset = buffer.num_lines();
        match dl {
            DisplayLine::Source {
                lineno,
                inline_marks,
                line,
                annotations,
            } => {
                let lineno_color = stylesheet.line_no();
                if anonymized_line_numbers && lineno.is_some() {
                    let num = format!("{ANONYMIZED_LINE_NUM:>lineno_width$} |");
                    buffer.puts(line_offset, 0, &num, *lineno_color);
                } else {
                    match lineno {
                        Some(n) => {
                            let num = format!("{n:>lineno_width$} |");
                            buffer.puts(line_offset, 0, &num, *lineno_color);
                        }
                        None => {
                            buffer.putc(line_offset, lineno_width + 1, '|', *lineno_color);
                        }
                    }
                }
                if let DisplaySourceLine::Content { text, .. } = line {
                    // The width of the line number, a space, pipe, and a space
                    // `123 | ` is `lineno_width + 3`.
                    let width_offset = lineno_width + 3;
                    let code_offset = if multiline_depth == 0 {
                        width_offset
                    } else {
                        width_offset + multiline_depth + 1
                    };

                    // Add any inline marks to the code line
                    if !inline_marks.is_empty() || 0 < multiline_depth {
                        format_inline_marks(
                            line_offset,
                            inline_marks,
                            lineno_width,
                            stylesheet,
                            buffer,
                        )?;
                    }

                    let text = normalize_whitespace(text);
                    let line_len = text.as_bytes().len();
                    let left = self.margin.left(line_len);
                    let right = self.margin.right(line_len);

                    // On long lines, we strip the source line, accounting for unicode.
                    let mut taken = 0;
                    let mut was_cut_right = false;
                    let mut code = String::new();
                    for ch in text.chars().skip(left) {
                        // Make sure that the trimming on the right will fall within the terminal width.
                        // FIXME: `unicode_width` sometimes disagrees with terminals on how wide a `char`
                        // is. For now, just accept that sometimes the code line will be longer than
                        // desired.
                        let next = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);
                        if taken + next > right - left {
                            was_cut_right = true;
                            break;
                        }
                        taken += next;
                        code.push(ch);
                    }
                    buffer.puts(line_offset, code_offset, &code, Style::new());
                    if self.margin.was_cut_left() {
                        // We have stripped some code/whitespace from the beginning, make it clear.
                        buffer.puts(line_offset, code_offset, cut_indicator, *lineno_color);
                    }
                    if was_cut_right {
                        buffer.puts(
                            line_offset,
                            code_offset + taken - cut_indicator.width(),
                            cut_indicator,
                            *lineno_color,
                        );
                    }

                    let left: usize = text
                        .chars()
                        .take(left)
                        .map(|ch| unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1))
                        .sum();

                    let mut annotations = annotations.clone();
                    annotations.sort_by_key(|a| Reverse(a.range.0));

                    let mut annotations_positions = vec![];
                    let mut line_len: usize = 0;
                    let mut p = 0;
                    for (i, annotation) in annotations.iter().enumerate() {
                        for (j, next) in annotations.iter().enumerate() {
                            // This label overlaps with another one and both take space (
                            // they have text and are not multiline lines).
                            if overlaps(next, annotation, 0)
                                && annotation.has_label()
                                && j > i
                                && p == 0
                            // We're currently on the first line, move the label one line down
                            {
                                // If we're overlapping with an un-labelled annotation with the same span
                                // we can just merge them in the output
                                if next.range.0 == annotation.range.0
                                    && next.range.1 == annotation.range.1
                                    && !next.has_label()
                                {
                                    continue;
                                }

                                // This annotation needs a new line in the output.
                                p += 1;
                                break;
                            }
                        }
                        annotations_positions.push((p, annotation));
                        for (j, next) in annotations.iter().enumerate() {
                            if j > i {
                                let l = next
                                    .annotation
                                    .label
                                    .iter()
                                    .map(|label| label.content)
                                    .collect::<Vec<_>>()
                                    .join("")
                                    .len()
                                    + 2;
                                // Do not allow two labels to be in the same line if they
                                // overlap including padding, to avoid situations like:
                                //
                                // fn foo(x: u32) {
                                // -------^------
                                // |      |
                                // fn_spanx_span
                                //
                                // Both labels must have some text, otherwise they are not
                                // overlapping. Do not add a new line if this annotation or
                                // the next are vertical line placeholders. If either this
                                // or the next annotation is multiline start/end, move it
                                // to a new line so as not to overlap the horizontal lines.
                                if (overlaps(next, annotation, l)
                                    && annotation.has_label()
                                    && next.has_label())
                                    || (annotation.takes_space() && next.has_label())
                                    || (annotation.has_label() && next.takes_space())
                                    || (annotation.takes_space() && next.takes_space())
                                    || (overlaps(next, annotation, l)
                                        && next.range.1 <= annotation.range.1
                                        && next.has_label()
                                        && p == 0)
                                // Avoid #42595.
                                {
                                    // This annotation needs a new line in the output.
                                    p += 1;
                                    break;
                                }
                            }
                        }
                        line_len = max(line_len, p);
                    }

                    if line_len != 0 {
                        line_len += 1;
                    }

                    if annotations_positions.iter().all(|(_, ann)| {
                        matches!(
                            ann.annotation_part,
                            DisplayAnnotationPart::MultilineStart(_)
                        )
                    }) {
                        if let Some(max_pos) =
                            annotations_positions.iter().map(|(pos, _)| *pos).max()
                        {
                            // Special case the following, so that we minimize overlapping multiline spans.
                            //
                            // 3 │       X0 Y0 Z0
                            //   │ ┏━━━━━┛  │  │     < We are writing these lines
                            //   │ ┃┌───────┘  │     < by reverting the "depth" of
                            //   │ ┃│┌─────────┘     < their multiline spans.
                            // 4 │ ┃││   X1 Y1 Z1
                            // 5 │ ┃││   X2 Y2 Z2
                            //   │ ┃│└────╿──│──┘ `Z` label
                            //   │ ┃└─────│──┤
                            //   │ ┗━━━━━━┥  `Y` is a good letter too
                            //   ╰╴       `X` is a good letter
                            for (pos, _) in &mut annotations_positions {
                                *pos = max_pos - *pos;
                            }
                            // We know then that we don't need an additional line for the span label, saving us
                            // one line of vertical space.
                            line_len = line_len.saturating_sub(1);
                        }
                    }

                    // This is a special case where we have a multiline
                    // annotation that is at the start of the line disregarding
                    // any leading whitespace, and no other multiline
                    // annotations overlap it. In this case, we want to draw
                    //
                    // 2 |   fn foo() {
                    //   |  _^
                    // 3 | |
                    // 4 | | }
                    //   | |_^ test
                    //
                    // we simplify the output to:
                    //
                    // 2 | / fn foo() {
                    // 3 | |
                    // 4 | | }
                    //   | |_^ test
                    if multiline_depth == 1
                        && annotations_positions.len() == 1
                        && annotations_positions
                            .first()
                            .map_or(false, |(_, annotation)| {
                                matches!(
                                    annotation.annotation_part,
                                    DisplayAnnotationPart::MultilineStart(_)
                                ) && text
                                    .chars()
                                    .take(annotation.range.0)
                                    .all(|c| c.is_whitespace())
                            })
                    {
                        let (_, ann) = annotations_positions.remove(0);
                        let style = get_annotation_style(&ann.annotation_type, stylesheet);
                        buffer.putc(line_offset, 3 + lineno_width, '/', *style);
                    }

                    // Draw the column separator for any extra lines that were
                    // created
                    //
                    // After this we will have:
                    //
                    // 2 |   fn foo() {
                    //   |
                    //   |
                    //   |
                    // 3 |
                    // 4 |   }
                    //   |
                    if !annotations_positions.is_empty() {
                        for pos in 0..=line_len {
                            buffer.putc(
                                line_offset + pos + 1,
                                lineno_width + 1,
                                '|',
                                stylesheet.line_no,
                            );
                        }
                    }

                    // Write the horizontal lines for multiline annotations
                    // (only the first and last lines need this).
                    //
                    // After this we will have:
                    //
                    // 2 |   fn foo() {
                    //   |  __________
                    //   |
                    //   |
                    // 3 |
                    // 4 |   }
                    //   |  _
                    for &(pos, annotation) in &annotations_positions {
                        let style = get_annotation_style(&annotation.annotation_type, stylesheet);
                        let pos = pos + 1;
                        match annotation.annotation_part {
                            DisplayAnnotationPart::MultilineStart(depth)
                            | DisplayAnnotationPart::MultilineEnd(depth) => {
                                for col in width_offset + depth
                                    ..(code_offset + annotation.range.0).saturating_sub(left)
                                {
                                    buffer.putc(line_offset + pos, col + 1, '_', *style);
                                }
                            }
                            _ => {}
                        }
                    }

                    // Write the vertical lines for labels that are on a different line as the underline.
                    //
                    // After this we will have:
                    //
                    // 2 |   fn foo() {
                    //   |  __________
                    //   | |    |
                    //   | |
                    // 3 | |
                    // 4 | | }
                    //   | |_
                    for &(pos, annotation) in &annotations_positions {
                        let style = get_annotation_style(&annotation.annotation_type, stylesheet);
                        let pos = pos + 1;
                        if pos > 1 && (annotation.has_label() || annotation.takes_space()) {
                            for p in line_offset + 2..=line_offset + pos {
                                buffer.putc(
                                    p,
                                    (code_offset + annotation.range.0).saturating_sub(left),
                                    '|',
                                    *style,
                                );
                            }
                        }
                        match annotation.annotation_part {
                            DisplayAnnotationPart::MultilineStart(depth) => {
                                for p in line_offset + pos + 1..line_offset + line_len + 2 {
                                    buffer.putc(p, width_offset + depth, '|', *style);
                                }
                            }
                            DisplayAnnotationPart::MultilineEnd(depth) => {
                                for p in line_offset..=line_offset + pos {
                                    buffer.putc(p, width_offset + depth, '|', *style);
                                }
                            }
                            _ => {}
                        }
                    }

                    // Add in any inline marks for any extra lines that have
                    // been created. Output should look like above.
                    for inline_mark in inline_marks {
                        let DisplayMarkType::AnnotationThrough(depth) = inline_mark.mark_type;
                        let style = get_annotation_style(&inline_mark.annotation_type, stylesheet);
                        if annotations_positions.is_empty() {
                            buffer.putc(line_offset, width_offset + depth, '|', *style);
                        } else {
                            for p in line_offset..=line_offset + line_len + 1 {
                                buffer.putc(p, width_offset + depth, '|', *style);
                            }
                        }
                    }

                    // Write the labels on the annotations that actually have a label.
                    //
                    // After this we will have:
                    //
                    // 2 |   fn foo() {
                    //   |  __________
                    //   |      |
                    //   |      something about `foo`
                    // 3 |
                    // 4 |   }
                    //   |  _  test
                    for &(pos, annotation) in &annotations_positions {
                        if !is_annotation_empty(&annotation.annotation) {
                            let style =
                                get_annotation_style(&annotation.annotation_type, stylesheet);
                            let mut formatted_len = if let Some(id) = &annotation.annotation.id {
                                2 + id.len()
                                    + annotation_type_len(&annotation.annotation.annotation_type)
                            } else {
                                annotation_type_len(&annotation.annotation.annotation_type)
                            };
                            let (pos, col) = if pos == 0 {
                                (pos + 1, (annotation.range.1 + 1).saturating_sub(left))
                            } else {
                                (pos + 2, annotation.range.0.saturating_sub(left))
                            };
                            if annotation.annotation_part
                                == DisplayAnnotationPart::LabelContinuation
                            {
                                formatted_len = 0;
                            } else if formatted_len != 0 {
                                formatted_len += 2;
                                let id = match &annotation.annotation.id {
                                    Some(id) => format!("[{id}]"),
                                    None => String::new(),
                                };
                                buffer.puts(
                                    line_offset + pos,
                                    col + code_offset,
                                    &format!(
                                        "{}{}: ",
                                        annotation_type_str(&annotation.annotation_type),
                                        id
                                    ),
                                    *style,
                                );
                            } else {
                                formatted_len = 0;
                            }
                            let mut before = 0;
                            for fragment in &annotation.annotation.label {
                                let inner_col = before + formatted_len + col + code_offset;
                                buffer.puts(line_offset + pos, inner_col, fragment.content, *style);
                                before += fragment.content.len();
                            }
                        }
                    }

                    // Sort from biggest span to smallest span so that smaller spans are
                    // represented in the output:
                    //
                    // x | fn foo()
                    //   | ^^^---^^
                    //   | |  |
                    //   | |  something about `foo`
                    //   | something about `fn foo()`
                    annotations_positions.sort_by_key(|(_, ann)| {
                        // Decreasing order. When annotations share the same length, prefer `Primary`.
                        Reverse(ann.len())
                    });

                    // Write the underlines.
                    //
                    // After this we will have:
                    //
                    // 2 |   fn foo() {
                    //   |  ____-_____^
                    //   |      |
                    //   |      something about `foo`
                    // 3 |
                    // 4 |   }
                    //   |  _^  test
                    for &(_, annotation) in &annotations_positions {
                        let mark = match annotation.annotation_type {
                            DisplayAnnotationType::Error => '^',
                            DisplayAnnotationType::Warning => '-',
                            DisplayAnnotationType::Info => '-',
                            DisplayAnnotationType::Note => '-',
                            DisplayAnnotationType::Help => '-',
                            DisplayAnnotationType::None => ' ',
                        };
                        let style = get_annotation_style(&annotation.annotation_type, stylesheet);
                        for p in annotation.range.0..annotation.range.1 {
                            buffer.putc(
                                line_offset + 1,
                                (code_offset + p).saturating_sub(left),
                                mark,
                                *style,
                            );
                        }
                    }
                } else if !inline_marks.is_empty() {
                    format_inline_marks(
                        line_offset,
                        inline_marks,
                        lineno_width,
                        stylesheet,
                        buffer,
                    )?;
                }
                Ok(())
            }
            DisplayLine::Fold { inline_marks } => {
                buffer.puts(line_offset, 0, cut_indicator, *stylesheet.line_no());
                if !inline_marks.is_empty() || 0 < multiline_depth {
                    format_inline_marks(
                        line_offset,
                        inline_marks,
                        lineno_width,
                        stylesheet,
                        buffer,
                    )?;
                }
                Ok(())
            }
            DisplayLine::Raw(line) => {
                self.format_raw_line(line_offset, line, lineno_width, stylesheet, buffer)
            }
        }
    }
}

/// Inline annotation which can be used in either Raw or Source line.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Annotation<'a> {
    pub(crate) annotation_type: DisplayAnnotationType,
    pub(crate) id: Option<&'a str>,
    pub(crate) label: Vec<DisplayTextFragment<'a>>,
}

/// A single line used in `DisplayList`.
#[derive(Debug, PartialEq)]
pub(crate) enum DisplayLine<'a> {
    /// A line with `lineno` portion of the slice.
    Source {
        lineno: Option<usize>,
        inline_marks: Vec<DisplayMark>,
        line: DisplaySourceLine<'a>,
        annotations: Vec<DisplaySourceAnnotation<'a>>,
    },

    /// A line indicating a folded part of the slice.
    Fold { inline_marks: Vec<DisplayMark> },

    /// A line which is displayed outside of slices.
    Raw(DisplayRawLine<'a>),
}

/// A source line.
#[derive(Debug, PartialEq)]
pub(crate) enum DisplaySourceLine<'a> {
    /// A line with the content of the Snippet.
    Content {
        text: &'a str,
        range: (usize, usize), // meta information for annotation placement.
        end_line: EndLine,
    },
    /// An empty source line.
    Empty,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplaySourceAnnotation<'a> {
    pub(crate) annotation: Annotation<'a>,
    pub(crate) range: (usize, usize),
    pub(crate) annotation_type: DisplayAnnotationType,
    pub(crate) annotation_part: DisplayAnnotationPart,
}

impl DisplaySourceAnnotation<'_> {
    fn has_label(&self) -> bool {
        !self
            .annotation
            .label
            .iter()
            .all(|label| label.content.is_empty())
    }

    // Length of this annotation as displayed in the stderr output
    fn len(&self) -> usize {
        // Account for usize underflows
        if self.range.1 > self.range.0 {
            self.range.1 - self.range.0
        } else {
            self.range.0 - self.range.1
        }
    }

    fn takes_space(&self) -> bool {
        // Multiline annotations always have to keep vertical space.
        matches!(
            self.annotation_part,
            DisplayAnnotationPart::MultilineStart(_) | DisplayAnnotationPart::MultilineEnd(_)
        )
    }
}

/// Raw line - a line which does not have the `lineno` part and is not considered
/// a part of the snippet.
#[derive(Debug, PartialEq)]
pub(crate) enum DisplayRawLine<'a> {
    /// A line which provides information about the location of the given
    /// slice in the project structure.
    Origin {
        path: &'a str,
        pos: Option<(usize, usize)>,
        header_type: DisplayHeaderType,
    },

    /// An annotation line which is not part of any snippet.
    Annotation {
        annotation: Annotation<'a>,

        /// If set to `true`, the annotation will be aligned to the
        /// lineno delimiter of the snippet.
        source_aligned: bool,
        /// If set to `true`, only the label of the `Annotation` will be
        /// displayed. It allows for a multiline annotation to be aligned
        /// without displaying the meta information (`type` and `id`) to be
        /// displayed on each line.
        continuation: bool,
    },
}

/// An inline text fragment which any label is composed of.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplayTextFragment<'a> {
    pub(crate) content: &'a str,
    pub(crate) style: DisplayTextStyle,
}

/// A style for the `DisplayTextFragment` which can be visually formatted.
///
/// This information may be used to emphasis parts of the label.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum DisplayTextStyle {
    Regular,
    Emphasis,
}

/// An indicator of what part of the annotation a given `Annotation` is.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DisplayAnnotationPart {
    /// A standalone, single-line annotation.
    Standalone,
    /// A continuation of a multi-line label of an annotation.
    LabelContinuation,
    /// A line starting a multiline annotation.
    MultilineStart(usize),
    /// A line ending a multiline annotation.
    MultilineEnd(usize),
}

/// A visual mark used in `inline_marks` field of the `DisplaySourceLine`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DisplayMark {
    pub(crate) mark_type: DisplayMarkType,
    pub(crate) annotation_type: DisplayAnnotationType,
}

/// A type of the `DisplayMark`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DisplayMarkType {
    /// A mark indicating a multiline annotation going through the current line.
    AnnotationThrough(usize),
}

/// A type of the `Annotation` which may impact the sigils, style or text displayed.
///
/// There are several ways to uses this information when formatting the `DisplayList`:
///
/// * An annotation may display the name of the type like `error` or `info`.
/// * An underline for `Error` may be `^^^` while for `Warning` it could be `---`.
/// * `ColorStylesheet` may use different colors for different annotations.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DisplayAnnotationType {
    None,
    Error,
    Warning,
    Info,
    Note,
    Help,
}

impl From<snippet::Level> for DisplayAnnotationType {
    fn from(at: snippet::Level) -> Self {
        match at {
            snippet::Level::None => DisplayAnnotationType::None,
            snippet::Level::Error => DisplayAnnotationType::Error,
            snippet::Level::Warning => DisplayAnnotationType::Warning,
            snippet::Level::Info => DisplayAnnotationType::Info,
            snippet::Level::Note => DisplayAnnotationType::Note,
            snippet::Level::Help => DisplayAnnotationType::Help,
        }
    }
}

/// Information whether the header is the initial one or a consecutive one
/// for multi-slice cases.
// TODO: private
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DisplayHeaderType {
    /// Initial header is the first header in the snippet.
    Initial,

    /// Continuation marks all headers of following slices in the snippet.
    Continuation,
}

struct CursorLines<'a>(&'a str);

impl CursorLines<'_> {
    fn new(src: &str) -> CursorLines<'_> {
        CursorLines(src)
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum EndLine {
    Eof,
    Lf,
    Crlf,
}

impl EndLine {
    /// The number of characters this line ending occupies in bytes.
    pub(crate) fn len(self) -> usize {
        match self {
            EndLine::Eof => 0,
            EndLine::Lf => 1,
            EndLine::Crlf => 2,
        }
    }
}

impl<'a> Iterator for CursorLines<'a> {
    type Item = (&'a str, EndLine);

    fn next(&mut self) -> Option<Self::Item> {
        if self.0.is_empty() {
            None
        } else {
            self.0
                .find('\n')
                .map(|x| {
                    let ret = if 0 < x {
                        if self.0.as_bytes()[x - 1] == b'\r' {
                            (&self.0[..x - 1], EndLine::Crlf)
                        } else {
                            (&self.0[..x], EndLine::Lf)
                        }
                    } else {
                        ("", EndLine::Lf)
                    };
                    self.0 = &self.0[x + 1..];
                    ret
                })
                .or_else(|| {
                    let ret = Some((self.0, EndLine::Eof));
                    self.0 = "";
                    ret
                })
        }
    }
}

fn format_message<'m>(
    message: snippet::Message<'m>,
    term_width: usize,
    anonymized_line_numbers: bool,
    cut_indicator: &'static str,
    primary: bool,
) -> Vec<DisplaySet<'m>> {
    let snippet::Message {
        level,
        id,
        title,
        footer,
        snippets,
    } = message;

    let mut sets = vec![];
    let body = if !snippets.is_empty() || primary {
        vec![format_title(level, id, title)]
    } else {
        format_footer(level, id, title)
    };

    for (idx, snippet) in snippets.into_iter().enumerate() {
        let snippet = fold_prefix_suffix(snippet);
        sets.push(format_snippet(
            snippet,
            idx == 0,
            !footer.is_empty(),
            term_width,
            anonymized_line_numbers,
            cut_indicator,
        ));
    }

    if let Some(first) = sets.first_mut() {
        for line in body {
            first.display_lines.insert(0, line);
        }
    } else {
        sets.push(DisplaySet {
            display_lines: body,
            margin: Margin::new(0, 0, 0, 0, DEFAULT_TERM_WIDTH, 0),
        });
    }

    for annotation in footer {
        sets.extend(format_message(
            annotation,
            term_width,
            anonymized_line_numbers,
            cut_indicator,
            false,
        ));
    }

    sets
}

fn format_title<'a>(level: crate::Level, id: Option<&'a str>, label: &'a str) -> DisplayLine<'a> {
    DisplayLine::Raw(DisplayRawLine::Annotation {
        annotation: Annotation {
            annotation_type: DisplayAnnotationType::from(level),
            id,
            label: format_label(Some(label), Some(DisplayTextStyle::Emphasis)),
        },
        source_aligned: false,
        continuation: false,
    })
}

fn format_footer<'a>(
    level: crate::Level,
    id: Option<&'a str>,
    label: &'a str,
) -> Vec<DisplayLine<'a>> {
    let mut result = vec![];
    for (i, line) in label.lines().enumerate() {
        result.push(DisplayLine::Raw(DisplayRawLine::Annotation {
            annotation: Annotation {
                annotation_type: DisplayAnnotationType::from(level),
                id,
                label: format_label(Some(line), None),
            },
            source_aligned: true,
            continuation: i != 0,
        }));
    }
    result
}

fn format_label(
    label: Option<&str>,
    style: Option<DisplayTextStyle>,
) -> Vec<DisplayTextFragment<'_>> {
    let mut result = vec![];
    if let Some(label) = label {
        let element_style = style.unwrap_or(DisplayTextStyle::Regular);
        result.push(DisplayTextFragment {
            content: label,
            style: element_style,
        });
    }
    result
}

fn format_snippet<'m>(
    snippet: snippet::Snippet<'m>,
    is_first: bool,
    has_footer: bool,
    term_width: usize,
    anonymized_line_numbers: bool,
    cut_indicator: &'static str,
) -> DisplaySet<'m> {
    let main_range = snippet.annotations.first().map(|x| x.range.start);
    let origin = snippet.origin;
    let need_empty_header = origin.is_some() || is_first;
    let mut body = format_body(
        snippet,
        need_empty_header,
        has_footer,
        term_width,
        anonymized_line_numbers,
        cut_indicator,
    );
    let header = format_header(origin, main_range, &body.display_lines, is_first);

    if let Some(header) = header {
        body.display_lines.insert(0, header);
    }

    body
}

#[inline]
// TODO: option_zip
fn zip_opt<A, B>(a: Option<A>, b: Option<B>) -> Option<(A, B)> {
    a.and_then(|a| b.map(|b| (a, b)))
}

fn format_header<'a>(
    origin: Option<&'a str>,
    main_range: Option<usize>,
    body: &[DisplayLine<'_>],
    is_first: bool,
) -> Option<DisplayLine<'a>> {
    let display_header = if is_first {
        DisplayHeaderType::Initial
    } else {
        DisplayHeaderType::Continuation
    };

    if let Some((main_range, path)) = zip_opt(main_range, origin) {
        let mut col = 1;
        let mut line_offset = 1;

        for item in body {
            if let DisplayLine::Source {
                line:
                    DisplaySourceLine::Content {
                        text,
                        range,
                        end_line,
                    },
                lineno,
                ..
            } = item
            {
                if main_range >= range.0 && main_range < range.1 + max(*end_line as usize, 1) {
                    let char_column = text[0..(main_range - range.0).min(text.len())]
                        .chars()
                        .count();
                    col = char_column + 1;
                    line_offset = lineno.unwrap_or(1);
                    break;
                }
            }
        }

        return Some(DisplayLine::Raw(DisplayRawLine::Origin {
            path,
            pos: Some((line_offset, col)),
            header_type: display_header,
        }));
    }

    if let Some(path) = origin {
        return Some(DisplayLine::Raw(DisplayRawLine::Origin {
            path,
            pos: None,
            header_type: display_header,
        }));
    }

    None
}

fn fold_prefix_suffix(mut snippet: snippet::Snippet<'_>) -> snippet::Snippet<'_> {
    if !snippet.fold {
        return snippet;
    }

    let ann_start = snippet
        .annotations
        .iter()
        .map(|ann| ann.range.start)
        .min()
        .unwrap_or(0);
    if let Some(before_new_start) = snippet.source[0..ann_start].rfind('\n') {
        let new_start = before_new_start + 1;

        let line_offset = newline_count(&snippet.source[..new_start]);
        snippet.line_start += line_offset;

        snippet.source = &snippet.source[new_start..];

        for ann in &mut snippet.annotations {
            let range_start = ann.range.start - new_start;
            let range_end = ann.range.end - new_start;
            ann.range = range_start..range_end;
        }
    }

    let ann_end = snippet
        .annotations
        .iter()
        .map(|ann| ann.range.end)
        .max()
        .unwrap_or(snippet.source.len());
    if let Some(end_offset) = snippet.source[ann_end..].find('\n') {
        let new_end = ann_end + end_offset;
        snippet.source = &snippet.source[..new_end];
    }

    snippet
}

fn newline_count(body: &str) -> usize {
    memchr::memchr_iter(b'\n', body.as_bytes()).count()
}

fn fold_body(body: Vec<DisplayLine<'_>>) -> Vec<DisplayLine<'_>> {
    const INNER_CONTEXT: usize = 1;
    const INNER_UNFOLD_SIZE: usize = INNER_CONTEXT * 2 + 1;

    let mut lines = vec![];
    let mut unhighlighted_lines = vec![];
    for line in body {
        match &line {
            DisplayLine::Source { annotations, .. } => {
                if annotations.is_empty() {
                    unhighlighted_lines.push(line);
                } else {
                    if lines.is_empty() {
                        // Ignore leading unhighlighted lines
                        unhighlighted_lines.clear();
                    }
                    match unhighlighted_lines.len() {
                        0 => {}
                        n if n <= INNER_UNFOLD_SIZE => {
                            // Rather than render our cut indicator, don't fold
                            lines.append(&mut unhighlighted_lines);
                        }
                        _ => {
                            lines.extend(unhighlighted_lines.drain(..INNER_CONTEXT));
                            let inline_marks = lines
                                .last()
                                .and_then(|line| {
                                    if let DisplayLine::Source {
                                        ref inline_marks, ..
                                    } = line
                                    {
                                        let inline_marks = inline_marks.clone();
                                        Some(inline_marks)
                                    } else {
                                        None
                                    }
                                })
                                .unwrap_or_default();
                            lines.push(DisplayLine::Fold {
                                inline_marks: inline_marks.clone(),
                            });
                            unhighlighted_lines
                                .drain(..unhighlighted_lines.len().saturating_sub(INNER_CONTEXT));
                            lines.append(&mut unhighlighted_lines);
                        }
                    }
                    lines.push(line);
                }
            }
            _ => {
                unhighlighted_lines.push(line);
            }
        }
    }

    lines
}

fn format_body<'m>(
    snippet: snippet::Snippet<'m>,
    need_empty_header: bool,
    has_footer: bool,
    term_width: usize,
    anonymized_line_numbers: bool,
    cut_indicator: &'static str,
) -> DisplaySet<'m> {
    let source_len = snippet.source.len();
    if let Some(bigger) = snippet.annotations.iter().find_map(|x| {
        // Allow highlighting one past the last character in the source.
        if source_len + 1 < x.range.end {
            Some(&x.range)
        } else {
            None
        }
    }) {
        panic!("SourceAnnotation range `{bigger:?}` is beyond the end of buffer `{source_len}`")
    }

    let mut body = vec![];
    let mut current_line = snippet.line_start;
    let mut current_index = 0;

    let mut whitespace_margin = usize::MAX;
    let mut span_left_margin = usize::MAX;
    let mut span_right_margin = 0;
    let mut label_right_margin = 0;
    let mut max_line_len = 0;

    let mut depth_map: HashMap<usize, usize> = HashMap::new();
    let mut current_depth = 0;
    let mut annotations = snippet.annotations;
    let ranges = annotations
        .iter()
        .map(|a| a.range.clone())
        .collect::<Vec<_>>();
    // We want to merge multiline annotations that have the same range into one
    // multiline annotation to save space. This is done by making any duplicate
    // multiline annotations into a single-line annotation pointing at the end
    // of the range.
    //
    // 3 |       X0 Y0 Z0
    //   |  _____^
    //   | | ____|
    //   | || ___|
    //   | |||
    // 4 | |||   X1 Y1 Z1
    // 5 | |||   X2 Y2 Z2
    //   | |||    ^
    //   | |||____|
    //   |  ||____`X` is a good letter
    //   |   |____`Y` is a good letter too
    //   |        `Z` label
    // Should be
    // error: foo
    //  --> test.rs:3:3
    //   |
    // 3 | /   X0 Y0 Z0
    // 4 | |   X1 Y1 Z1
    // 5 | |   X2 Y2 Z2
    //   | |    ^
    //   | |____|
    //   |      `X` is a good letter
    //   |      `Y` is a good letter too
    //   |      `Z` label
    //   |
    ranges.iter().enumerate().for_each(|(r_idx, range)| {
        annotations
            .iter_mut()
            .enumerate()
            .skip(r_idx + 1)
            .for_each(|(ann_idx, ann)| {
                // Skip if the annotation's index matches the range index
                if ann_idx != r_idx
                    // We only want to merge multiline annotations
                    && snippet.source[ann.range.clone()].lines().count() > 1
                    // We only want to merge annotations that have the same range
                    && ann.range.start == range.start
                    && ann.range.end == range.end
                {
                    ann.range.start = ann.range.end.saturating_sub(1);
                }
            });
    });
    annotations.sort_by_key(|a| a.range.start);
    let mut annotations = annotations.into_iter().enumerate().collect::<Vec<_>>();

    for (idx, (line, end_line)) in CursorLines::new(snippet.source).enumerate() {
        let line_length: usize = line.len();
        let line_range = (current_index, current_index + line_length);
        let end_line_size = end_line.len();
        body.push(DisplayLine::Source {
            lineno: Some(current_line),
            inline_marks: vec![],
            line: DisplaySourceLine::Content {
                text: line,
                range: line_range,
                end_line,
            },
            annotations: vec![],
        });

        let leading_whitespace = line
            .chars()
            .take_while(|c| c.is_whitespace())
            .map(|c| {
                match c {
                    // Tabs are displayed as 4 spaces
                    '\t' => 4,
                    _ => 1,
                }
            })
            .sum();
        whitespace_margin = min(whitespace_margin, leading_whitespace);
        max_line_len = max(max_line_len, line_length);

        let line_start_index = line_range.0;
        let line_end_index = line_range.1;
        current_line += 1;
        current_index += line_length + end_line_size;

        // It would be nice to use filter_drain here once it's stable.
        annotations.retain(|(key, annotation)| {
            let body_idx = idx;
            let annotation_type = match annotation.level {
                snippet::Level::Error => DisplayAnnotationType::None,
                snippet::Level::Warning => DisplayAnnotationType::None,
                _ => DisplayAnnotationType::from(annotation.level),
            };
            let label_right = annotation.label.map_or(0, |label| label.len() + 1);
            match annotation.range {
                // This handles if the annotation is on the next line. We add
                // the `end_line_size` to account for annotating the line end.
                Range { start, .. } if start > line_end_index + end_line_size => true,
                // This handles the case where an annotation is contained
                // within the current line including any line-end characters.
                Range { start, end }
                    if start >= line_start_index
                        // We add at least one to `line_end_index` to allow
                        // highlighting the end of a file
                        && end <= line_end_index + max(end_line_size, 1) =>
                {
                    if let DisplayLine::Source {
                        ref mut annotations,
                        ..
                    } = body[body_idx]
                    {
                        let annotation_start_col = line
                            [0..(start - line_start_index).min(line_length)]
                            .chars()
                            .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0))
                            .sum::<usize>();
                        let mut annotation_end_col = line
                            [0..(end - line_start_index).min(line_length)]
                            .chars()
                            .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0))
                            .sum::<usize>();
                        if annotation_start_col == annotation_end_col {
                            // At least highlight something
                            annotation_end_col += 1;
                        }

                        span_left_margin = min(span_left_margin, annotation_start_col);
                        span_right_margin = max(span_right_margin, annotation_end_col);
                        label_right_margin =
                            max(label_right_margin, annotation_end_col + label_right);

                        let range = (annotation_start_col, annotation_end_col);
                        annotations.push(DisplaySourceAnnotation {
                            annotation: Annotation {
                                annotation_type,
                                id: None,
                                label: format_label(annotation.label, None),
                            },
                            range,
                            annotation_type: DisplayAnnotationType::from(annotation.level),
                            annotation_part: DisplayAnnotationPart::Standalone,
                        });
                    }
                    false
                }
                // This handles the case where a multiline annotation starts
                // somewhere on the current line, including any line-end chars
                Range { start, end }
                    if start >= line_start_index
                        // The annotation can start on a line ending
                        && start <= line_end_index + end_line_size.saturating_sub(1)
                        && end > line_end_index =>
                {
                    if let DisplayLine::Source {
                        ref mut annotations,
                        ..
                    } = body[body_idx]
                    {
                        let annotation_start_col = line
                            [0..(start - line_start_index).min(line_length)]
                            .chars()
                            .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0))
                            .sum::<usize>();
                        let annotation_end_col = annotation_start_col + 1;

                        span_left_margin = min(span_left_margin, annotation_start_col);
                        span_right_margin = max(span_right_margin, annotation_end_col);
                        label_right_margin =
                            max(label_right_margin, annotation_end_col + label_right);

                        let range = (annotation_start_col, annotation_end_col);
                        annotations.push(DisplaySourceAnnotation {
                            annotation: Annotation {
                                annotation_type,
                                id: None,
                                label: vec![],
                            },
                            range,
                            annotation_type: DisplayAnnotationType::from(annotation.level),
                            annotation_part: DisplayAnnotationPart::MultilineStart(current_depth),
                        });
                        depth_map.insert(*key, current_depth);
                        current_depth += 1;
                    }
                    true
                }
                // This handles the case where a multiline annotation starts
                // somewhere before this line and ends after it as well
                Range { start, end }
                    if start < line_start_index && end > line_end_index + max(end_line_size, 1) =>
                {
                    if let DisplayLine::Source {
                        ref mut inline_marks,
                        ..
                    } = body[body_idx]
                    {
                        let depth = depth_map.get(key).cloned().unwrap_or_default();
                        inline_marks.push(DisplayMark {
                            mark_type: DisplayMarkType::AnnotationThrough(depth),
                            annotation_type: DisplayAnnotationType::from(annotation.level),
                        });
                    }
                    true
                }
                // This handles the case where a multiline annotation ends
                // somewhere on the current line, including any line-end chars
                Range { start, end }
                    if start < line_start_index
                        && end >= line_start_index
                        // We add at least one to `line_end_index` to allow
                        // highlighting the end of a file
                        && end <= line_end_index + max(end_line_size, 1) =>
                {
                    if let DisplayLine::Source {
                        ref mut annotations,
                        ..
                    } = body[body_idx]
                    {
                        let end_mark = line[0..(end - line_start_index).min(line_length)]
                            .chars()
                            .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0))
                            .sum::<usize>()
                            .saturating_sub(1);
                        // If the annotation ends on a line-end character, we
                        // need to annotate one past the end of the line
                        let (end_mark, end_plus_one) = if end > line_end_index
                            // Special case for highlighting the end of a file
                            || (end == line_end_index + 1 && end_line_size == 0)
                        {
                            (end_mark + 1, end_mark + 2)
                        } else {
                            (end_mark, end_mark + 1)
                        };

                        span_left_margin = min(span_left_margin, end_mark);
                        span_right_margin = max(span_right_margin, end_plus_one);
                        label_right_margin = max(label_right_margin, end_plus_one + label_right);

                        let range = (end_mark, end_plus_one);
                        let depth = depth_map.remove(key).unwrap_or(0);
                        annotations.push(DisplaySourceAnnotation {
                            annotation: Annotation {
                                annotation_type,
                                id: None,
                                label: format_label(annotation.label, None),
                            },
                            range,
                            annotation_type: DisplayAnnotationType::from(annotation.level),
                            annotation_part: DisplayAnnotationPart::MultilineEnd(depth),
                        });
                    }
                    false
                }
                _ => true,
            }
        });
        // Reset the depth counter, but only after we've processed all
        // annotations for a given line.
        let max = depth_map.len();
        if current_depth > max {
            current_depth = max;
        }
    }

    if snippet.fold {
        body = fold_body(body);
    }

    if need_empty_header {
        body.insert(
            0,
            DisplayLine::Source {
                lineno: None,
                inline_marks: vec![],
                line: DisplaySourceLine::Empty,
                annotations: vec![],
            },
        );
    }

    if has_footer {
        body.push(DisplayLine::Source {
            lineno: None,
            inline_marks: vec![],
            line: DisplaySourceLine::Empty,
            annotations: vec![],
        });
    } else if let Some(DisplayLine::Source { .. }) = body.last() {
        body.push(DisplayLine::Source {
            lineno: None,
            inline_marks: vec![],
            line: DisplaySourceLine::Empty,
            annotations: vec![],
        });
    }
    let max_line_num_len = if anonymized_line_numbers {
        ANONYMIZED_LINE_NUM.len()
    } else {
        current_line.to_string().len()
    };

    let width_offset = cut_indicator.len() + max_line_num_len;

    if span_left_margin == usize::MAX {
        span_left_margin = 0;
    }

    let margin = Margin::new(
        whitespace_margin,
        span_left_margin,
        span_right_margin,
        label_right_margin,
        term_width.saturating_sub(width_offset),
        max_line_len,
    );

    DisplaySet {
        display_lines: body,
        margin,
    }
}

#[inline]
fn annotation_type_str(annotation_type: &DisplayAnnotationType) -> &'static str {
    match annotation_type {
        DisplayAnnotationType::Error => ERROR_TXT,
        DisplayAnnotationType::Help => HELP_TXT,
        DisplayAnnotationType::Info => INFO_TXT,
        DisplayAnnotationType::Note => NOTE_TXT,
        DisplayAnnotationType::Warning => WARNING_TXT,
        DisplayAnnotationType::None => "",
    }
}

fn annotation_type_len(annotation_type: &DisplayAnnotationType) -> usize {
    match annotation_type {
        DisplayAnnotationType::Error => ERROR_TXT.len(),
        DisplayAnnotationType::Help => HELP_TXT.len(),
        DisplayAnnotationType::Info => INFO_TXT.len(),
        DisplayAnnotationType::Note => NOTE_TXT.len(),
        DisplayAnnotationType::Warning => WARNING_TXT.len(),
        DisplayAnnotationType::None => 0,
    }
}

fn get_annotation_style<'a>(
    annotation_type: &DisplayAnnotationType,
    stylesheet: &'a Stylesheet,
) -> &'a Style {
    match annotation_type {
        DisplayAnnotationType::Error => stylesheet.error(),
        DisplayAnnotationType::Warning => stylesheet.warning(),
        DisplayAnnotationType::Info => stylesheet.info(),
        DisplayAnnotationType::Note => stylesheet.note(),
        DisplayAnnotationType::Help => stylesheet.help(),
        DisplayAnnotationType::None => stylesheet.none(),
    }
}

#[inline]
fn is_annotation_empty(annotation: &Annotation<'_>) -> bool {
    annotation
        .label
        .iter()
        .all(|fragment| fragment.content.is_empty())
}

// We replace some characters so the CLI output is always consistent and underlines aligned.
const OUTPUT_REPLACEMENTS: &[(char, &str)] = &[
    ('\t', "    "),   // We do our own tab replacement
    ('\u{200D}', ""), // Replace ZWJ with nothing for consistent terminal output of grapheme clusters.
    ('\u{202A}', ""), // The following unicode text flow control characters are inconsistently
    ('\u{202B}', ""), // supported across CLIs and can cause confusion due to the bytes on disk
    ('\u{202D}', ""), // not corresponding to the visible source code, so we replace them always.
    ('\u{202E}', ""),
    ('\u{2066}', ""),
    ('\u{2067}', ""),
    ('\u{2068}', ""),
    ('\u{202C}', ""),
    ('\u{2069}', ""),
];

fn normalize_whitespace(str: &str) -> String {
    let mut s = str.to_owned();
    for (c, replacement) in OUTPUT_REPLACEMENTS {
        s = s.replace(*c, replacement);
    }
    s
}

fn overlaps(
    a1: &DisplaySourceAnnotation<'_>,
    a2: &DisplaySourceAnnotation<'_>,
    padding: usize,
) -> bool {
    (a2.range.0..a2.range.1).contains(&a1.range.0)
        || (a1.range.0..a1.range.1 + padding).contains(&a2.range.0)
}

fn format_inline_marks(
    line: usize,
    inline_marks: &[DisplayMark],
    lineno_width: usize,
    stylesheet: &Stylesheet,
    buf: &mut StyledBuffer,
) -> fmt::Result {
    for mark in inline_marks.iter() {
        let annotation_style = get_annotation_style(&mark.annotation_type, stylesheet);
        match mark.mark_type {
            DisplayMarkType::AnnotationThrough(depth) => {
                buf.putc(line, 3 + lineno_width + depth, '|', *annotation_style);
            }
        }
    }
    Ok(())
}
