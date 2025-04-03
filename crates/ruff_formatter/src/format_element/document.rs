use std::collections::HashMap;
use std::ops::Deref;

use rustc_hash::FxHashMap;

use crate::format_element::tag::{Condition, DedentMode};
use crate::prelude::tag::GroupMode;
use crate::prelude::*;
use crate::source_code::SourceCode;
use crate::{
    format, write, BufferExtensions, Format, FormatContext, FormatElement, FormatOptions,
    FormatResult, Formatter, IndentStyle, IndentWidth, LineWidth, PrinterOptions,
};

use super::tag::Tag;

/// A formatted document.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct Document {
    elements: Vec<FormatElement>,
}

impl Document {
    /// Sets [`expand`](tag::Group::expand) to [`GroupMode::Propagated`] if the group contains any of:
    /// - a group with [`expand`](tag::Group::expand) set to [`GroupMode::Propagated`] or [`GroupMode::Expand`].
    /// - a non-soft [line break](FormatElement::Line) with mode [`LineMode::Hard`], [`LineMode::Empty`], or [`LineMode::Literal`].
    /// - a [`FormatElement::ExpandParent`]
    ///
    /// [`BestFitting`] elements act as expand boundaries, meaning that the fact that a
    /// [`BestFitting`]'s content expands is not propagated past the [`BestFitting`] element.
    ///
    /// [`BestFitting`]: FormatElement::BestFitting
    pub(crate) fn propagate_expand(&mut self) {
        #[derive(Debug)]
        enum Enclosing<'a> {
            Group(&'a tag::Group),
            ConditionalGroup(&'a tag::ConditionalGroup),
            FitsExpanded {
                tag: &'a tag::FitsExpanded,
                expands_before: bool,
            },
            BestFitting,
            BestFitParenthesize {
                expanded: bool,
            },
        }

        fn expand_parent(enclosing: &[Enclosing]) {
            match enclosing.last() {
                Some(Enclosing::Group(group)) => group.propagate_expand(),
                Some(Enclosing::ConditionalGroup(group)) => group.propagate_expand(),
                Some(Enclosing::FitsExpanded { tag, .. }) => tag.propagate_expand(),
                _ => {}
            }
        }

        fn propagate_expands<'a>(
            elements: &'a [FormatElement],
            enclosing: &mut Vec<Enclosing<'a>>,
            checked_interned: &mut FxHashMap<&'a Interned, bool>,
        ) -> bool {
            let mut expands = false;
            for element in elements {
                let element_expands = match element {
                    FormatElement::Tag(Tag::StartGroup(group)) => {
                        enclosing.push(Enclosing::Group(group));
                        false
                    }
                    FormatElement::Tag(Tag::EndGroup) => match enclosing.pop() {
                        Some(Enclosing::Group(group)) => !group.mode().is_flat(),
                        _ => false,
                    },
                    FormatElement::Tag(Tag::StartBestFitParenthesize { .. }) => {
                        enclosing.push(Enclosing::BestFitParenthesize { expanded: expands });
                        expands = false;
                        continue;
                    }

                    FormatElement::Tag(Tag::EndBestFitParenthesize) => {
                        if let Some(Enclosing::BestFitParenthesize { expanded }) = enclosing.pop() {
                            expands = expanded;
                        }
                        continue;
                    }
                    FormatElement::Tag(Tag::StartConditionalGroup(group)) => {
                        enclosing.push(Enclosing::ConditionalGroup(group));
                        false
                    }
                    FormatElement::Tag(Tag::EndConditionalGroup) => match enclosing.pop() {
                        Some(Enclosing::ConditionalGroup(group)) => !group.mode().is_flat(),
                        _ => false,
                    },
                    FormatElement::Interned(interned) => {
                        if let Some(interned_expands) = checked_interned.get(interned) {
                            *interned_expands
                        } else {
                            let interned_expands =
                                propagate_expands(interned, enclosing, checked_interned);
                            checked_interned.insert(interned, interned_expands);
                            interned_expands
                        }
                    }
                    FormatElement::BestFitting { variants, mode: _ } => {
                        enclosing.push(Enclosing::BestFitting);

                        propagate_expands(variants, enclosing, checked_interned);
                        enclosing.pop();
                        continue;
                    }
                    FormatElement::Tag(Tag::StartFitsExpanded(fits_expanded)) => {
                        enclosing.push(Enclosing::FitsExpanded {
                            tag: fits_expanded,
                            expands_before: expands,
                        });
                        false
                    }
                    FormatElement::Tag(Tag::EndFitsExpanded) => {
                        if let Some(Enclosing::FitsExpanded { expands_before, .. }) =
                            enclosing.pop()
                        {
                            expands = expands_before;
                        }

                        continue;
                    }
                    FormatElement::Text {
                        text: _,
                        text_width,
                    } => text_width.is_multiline(),
                    FormatElement::SourceCodeSlice { text_width, .. } => text_width.is_multiline(),
                    FormatElement::ExpandParent
                    | FormatElement::Line(LineMode::Hard | LineMode::Empty) => true,
                    _ => false,
                };

                if element_expands {
                    expands = true;
                    expand_parent(enclosing);
                }
            }

            expands
        }

        let mut enclosing = Vec::with_capacity(if self.is_empty() {
            0
        } else {
            self.len().ilog2() as usize
        });
        let mut interned = FxHashMap::default();
        propagate_expands(self, &mut enclosing, &mut interned);
    }

    pub fn display<'a>(&'a self, source_code: SourceCode<'a>) -> DisplayDocument<'a> {
        DisplayDocument {
            elements: self.elements.as_slice(),
            source_code,
        }
    }
}

impl From<Vec<FormatElement>> for Document {
    fn from(elements: Vec<FormatElement>) -> Self {
        Self { elements }
    }
}

impl Deref for Document {
    type Target = [FormatElement];

    fn deref(&self) -> &Self::Target {
        self.elements.as_slice()
    }
}

pub struct DisplayDocument<'a> {
    elements: &'a [FormatElement],
    source_code: SourceCode<'a>,
}

impl std::fmt::Display for DisplayDocument<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let formatted = format!(IrFormatContext::new(self.source_code), [self.elements])
            .expect("Formatting not to throw any FormatErrors");

        f.write_str(
            formatted
                .print()
                .expect("Expected a valid document")
                .as_code(),
        )
    }
}

impl std::fmt::Debug for DisplayDocument<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

#[derive(Clone, Debug)]
struct IrFormatContext<'a> {
    /// The interned elements that have been printed to this point
    printed_interned_elements: HashMap<Interned, usize>,

    source_code: SourceCode<'a>,
}

impl<'a> IrFormatContext<'a> {
    fn new(source_code: SourceCode<'a>) -> Self {
        Self {
            source_code,
            printed_interned_elements: HashMap::new(),
        }
    }
}

impl FormatContext for IrFormatContext<'_> {
    type Options = IrFormatOptions;

    fn options(&self) -> &Self::Options {
        &IrFormatOptions
    }

    fn source_code(&self) -> SourceCode {
        self.source_code
    }
}

#[derive(Debug, Clone, Default)]
struct IrFormatOptions;

impl FormatOptions for IrFormatOptions {
    fn indent_style(&self) -> IndentStyle {
        IndentStyle::Space
    }

    fn indent_width(&self) -> IndentWidth {
        IndentWidth::default()
    }

    fn line_width(&self) -> LineWidth {
        LineWidth::try_from(80).unwrap()
    }

    fn as_print_options(&self) -> PrinterOptions {
        PrinterOptions {
            line_width: self.line_width(),
            indent_style: IndentStyle::Space,
            ..PrinterOptions::default()
        }
    }
}

impl Format<IrFormatContext<'_>> for &[FormatElement] {
    fn fmt(&self, f: &mut Formatter<IrFormatContext>) -> FormatResult<()> {
        #[allow(clippy::enum_glob_use)]
        use Tag::*;

        write!(f, [ContentArrayStart])?;

        let mut tag_stack = Vec::new();
        let mut first_element = true;
        let mut in_text = false;

        let mut iter = self.iter().peekable();

        while let Some(element) = iter.next() {
            if !first_element && !in_text && !element.is_end_tag() {
                // Write a separator between every two elements
                write!(f, [token(","), soft_line_break_or_space()])?;
            }

            first_element = false;

            match element {
                element @ (FormatElement::Space
                | FormatElement::Token { .. }
                | FormatElement::Text { .. }
                | FormatElement::SourceCodeSlice { .. }) => {
                    fn write_escaped(element: &FormatElement, f: &mut Formatter<IrFormatContext>) {
                        let (text, text_width) = match element {
                            #[allow(clippy::cast_possible_truncation)]
                            FormatElement::Token { text } => {
                                (*text, TextWidth::Width(Width::new(text.len() as u32)))
                            }
                            FormatElement::Text { text, text_width } => {
                                (text.as_ref(), *text_width)
                            }
                            FormatElement::SourceCodeSlice { slice, text_width } => {
                                (slice.text(f.context().source_code()), *text_width)
                            }
                            _ => unreachable!(),
                        };

                        if text.contains('"') {
                            f.write_element(FormatElement::Text {
                                text: text.replace('"', r#"\""#).into(),
                                text_width,
                            });
                        } else {
                            f.write_element(element.clone());
                        }
                    }

                    if !in_text {
                        write!(f, [token("\"")])?;
                    }

                    in_text = true;

                    match element {
                        FormatElement::Space => {
                            write!(f, [token(" ")])?;
                        }
                        element if element.is_text() => {
                            write_escaped(element, f);
                        }
                        _ => unreachable!(),
                    }

                    let is_next_text = iter.peek().is_some_and(|e| e.is_text() || e.is_space());

                    if !is_next_text {
                        write!(f, [token("\"")])?;
                        in_text = false;
                    }
                }

                FormatElement::Line(mode) => match mode {
                    LineMode::SoftOrSpace => {
                        write!(f, [token("soft_line_break_or_space")])?;
                    }
                    LineMode::Soft => {
                        write!(f, [token("soft_line_break")])?;
                    }
                    LineMode::Hard => {
                        write!(f, [token("hard_line_break")])?;
                    }
                    LineMode::Empty => {
                        write!(f, [token("empty_line")])?;
                    }
                },
                FormatElement::ExpandParent => {
                    write!(f, [token("expand_parent")])?;
                }

                FormatElement::SourcePosition(position) => {
                    write!(f, [text(&std::format!("source_position({position:?})"))])?;
                }

                FormatElement::LineSuffixBoundary => {
                    write!(f, [token("line_suffix_boundary")])?;
                }

                FormatElement::BestFitting { variants, mode } => {
                    write!(f, [token("best_fitting(")])?;

                    if *mode != BestFittingMode::FirstLine {
                        write!(f, [text(&std::format!("mode: {mode:?}, "))])?;
                    }

                    write!(f, [token("[")])?;
                    f.write_elements([
                        FormatElement::Tag(StartIndent),
                        FormatElement::Line(LineMode::Hard),
                    ]);

                    for variant in variants {
                        write!(f, [variant, hard_line_break()])?;
                    }

                    f.write_elements([
                        FormatElement::Tag(EndIndent),
                        FormatElement::Line(LineMode::Hard),
                    ]);

                    write!(f, [token("])")])?;
                }

                FormatElement::Interned(interned) => {
                    let interned_elements = &mut f.context_mut().printed_interned_elements;

                    match interned_elements.get(interned).copied() {
                        None => {
                            let index = interned_elements.len();
                            interned_elements.insert(interned.clone(), index);

                            write!(
                                f,
                                [
                                    text(&std::format!("<interned {index}>")),
                                    space(),
                                    &&**interned,
                                ]
                            )?;
                        }
                        Some(reference) => {
                            write!(f, [text(&std::format!("<ref interned *{reference}>"))])?;
                        }
                    }
                }

                FormatElement::Tag(tag) => {
                    if tag.is_start() {
                        first_element = true;
                        tag_stack.push(tag.kind());
                    }
                    // Handle documents with mismatching start/end or superfluous end tags
                    else {
                        match tag_stack.pop() {
                            None => {
                                // Only write the end tag without any indent to ensure the output document is valid.
                                write!(
                                    f,
                                    [
                                        token("<END_TAG_WITHOUT_START<"),
                                        text(&std::format!("{:?}", tag.kind())),
                                        token(">>"),
                                    ]
                                )?;
                                first_element = false;
                                continue;
                            }
                            Some(start_kind) if start_kind != tag.kind() => {
                                write!(
                                    f,
                                    [
                                        ContentArrayEnd,
                                        token(")"),
                                        soft_line_break_or_space(),
                                        token("ERROR<START_END_TAG_MISMATCH<start: "),
                                        text(&std::format!("{start_kind:?}")),
                                        token(", end: "),
                                        text(&std::format!("{:?}", tag.kind())),
                                        token(">>")
                                    ]
                                )?;
                                first_element = false;
                                continue;
                            }
                            _ => {
                                // all ok
                            }
                        }
                    }

                    match tag {
                        StartIndent => {
                            write!(f, [token("indent(")])?;
                        }

                        StartDedent(mode) => {
                            let label = match mode {
                                DedentMode::Level => "dedent",
                                DedentMode::Root => "dedentRoot",
                            };

                            write!(f, [token(label), token("(")])?;
                        }

                        StartAlign(tag::Align(count)) => {
                            write!(
                                f,
                                [
                                    token("align("),
                                    text(&count.to_string()),
                                    token(","),
                                    space(),
                                ]
                            )?;
                        }

                        StartLineSuffix { reserved_width } => {
                            write!(
                                f,
                                [
                                    token("line_suffix("),
                                    text(&std::format!("{reserved_width:?}")),
                                    token(","),
                                    space(),
                                ]
                            )?;
                        }

                        StartVerbatim(_) => {
                            write!(f, [token("verbatim(")])?;
                        }

                        StartGroup(group) => {
                            write!(f, [token("group(")])?;

                            if let Some(group_id) = group.id() {
                                write!(
                                    f,
                                    [text(&std::format!("\"{group_id:?}\"")), token(","), space(),]
                                )?;
                            }

                            match group.mode() {
                                GroupMode::Flat => {}
                                GroupMode::Expand => {
                                    write!(f, [token("expand: true,"), space()])?;
                                }
                                GroupMode::Propagated => {
                                    write!(f, [token("expand: propagated,"), space()])?;
                                }
                            }
                        }

                        StartBestFitParenthesize { id } => {
                            write!(f, [token("best_fit_parenthesize(")])?;

                            if let Some(group_id) = id {
                                write!(
                                    f,
                                    [text(&std::format!("\"{group_id:?}\"")), token(","), space(),]
                                )?;
                            }
                        }

                        StartConditionalGroup(group) => {
                            write!(
                                f,
                                [
                                    token("conditional_group(condition:"),
                                    space(),
                                    group.condition(),
                                    token(","),
                                    space()
                                ]
                            )?;

                            match group.mode() {
                                GroupMode::Flat => {}
                                GroupMode::Expand => {
                                    write!(f, [token("expand: true,"), space()])?;
                                }
                                GroupMode::Propagated => {
                                    write!(f, [token("expand: propagated,"), space()])?;
                                }
                            }
                        }

                        StartIndentIfGroupBreaks(id) => {
                            write!(
                                f,
                                [
                                    token("indent_if_group_breaks("),
                                    text(&std::format!("\"{id:?}\"")),
                                    token(","),
                                    space(),
                                ]
                            )?;
                        }

                        StartConditionalContent(condition) => {
                            match condition.mode {
                                PrintMode::Flat => {
                                    write!(f, [token("if_group_fits_on_line(")])?;
                                }
                                PrintMode::Expanded => {
                                    write!(f, [token("if_group_breaks(")])?;
                                }
                            }

                            if let Some(group_id) = condition.group_id {
                                write!(
                                    f,
                                    [text(&std::format!("\"{group_id:?}\"")), token(","), space()]
                                )?;
                            }
                        }

                        StartLabelled(label_id) => {
                            write!(
                                f,
                                [
                                    token("label("),
                                    text(&std::format!("\"{label_id:?}\"")),
                                    token(","),
                                    space(),
                                ]
                            )?;
                        }

                        StartFill => {
                            write!(f, [token("fill(")])?;
                        }

                        StartFitsExpanded(tag::FitsExpanded {
                            condition,
                            propagate_expand,
                        }) => {
                            write!(f, [token("fits_expanded(propagate_expand:"), space()])?;

                            if propagate_expand.get() {
                                write!(f, [token("true")])?;
                            } else {
                                write!(f, [token("false")])?;
                            }

                            write!(f, [token(","), space()])?;

                            if let Some(condition) = condition {
                                write!(
                                    f,
                                    [token("condition:"), space(), condition, token(","), space()]
                                )?;
                            }
                        }

                        StartEntry | StartBestFittingEntry => {
                            // handled after the match for all start tags
                        }
                        EndEntry | EndBestFittingEntry => write!(f, [ContentArrayEnd])?,

                        EndFill
                        | EndLabelled
                        | EndConditionalContent
                        | EndIndentIfGroupBreaks
                        | EndAlign
                        | EndIndent
                        | EndGroup
                        | EndConditionalGroup
                        | EndBestFitParenthesize
                        | EndLineSuffix
                        | EndDedent
                        | EndFitsExpanded
                        | EndVerbatim => {
                            write!(f, [ContentArrayEnd, token(")")])?;
                        }
                    }

                    if tag.is_start() {
                        write!(f, [ContentArrayStart])?;
                    }
                }
            }
        }

        while let Some(top) = tag_stack.pop() {
            write!(
                f,
                [
                    ContentArrayEnd,
                    token(")"),
                    soft_line_break_or_space(),
                    text(&std::format!("<START_WITHOUT_END<{top:?}>>")),
                ]
            )?;
        }

        write!(f, [ContentArrayEnd])
    }
}

struct ContentArrayStart;

impl Format<IrFormatContext<'_>> for ContentArrayStart {
    fn fmt(&self, f: &mut Formatter<IrFormatContext>) -> FormatResult<()> {
        use Tag::{StartGroup, StartIndent};

        write!(f, [token("[")])?;

        f.write_elements([
            FormatElement::Tag(StartGroup(tag::Group::new())),
            FormatElement::Tag(StartIndent),
            FormatElement::Line(LineMode::Soft),
        ]);

        Ok(())
    }
}

struct ContentArrayEnd;

impl Format<IrFormatContext<'_>> for ContentArrayEnd {
    fn fmt(&self, f: &mut Formatter<IrFormatContext>) -> FormatResult<()> {
        use Tag::{EndGroup, EndIndent};
        f.write_elements([
            FormatElement::Tag(EndIndent),
            FormatElement::Line(LineMode::Soft),
            FormatElement::Tag(EndGroup),
        ]);

        write!(f, [token("]")])
    }
}

impl FormatElements for [FormatElement] {
    fn will_break(&self) -> bool {
        let mut ignore_depth = 0usize;

        for element in self {
            match element {
                // Line suffix
                // Ignore if any of its content breaks
                FormatElement::Tag(
                    Tag::StartLineSuffix { reserved_width: _ } | Tag::StartFitsExpanded(_),
                ) => {
                    ignore_depth += 1;
                }
                FormatElement::Tag(Tag::EndLineSuffix | Tag::EndFitsExpanded) => {
                    ignore_depth = ignore_depth.saturating_sub(1);
                }
                FormatElement::Interned(interned) if ignore_depth == 0 => {
                    if interned.will_break() {
                        return true;
                    }
                }

                element if ignore_depth == 0 && element.will_break() => {
                    return true;
                }
                _ => continue,
            }
        }

        debug_assert_eq!(ignore_depth, 0, "Unclosed start container");

        false
    }

    fn has_label(&self, expected: LabelId) -> bool {
        self.first()
            .is_some_and(|element| element.has_label(expected))
    }

    fn start_tag(&self, kind: TagKind) -> Option<&Tag> {
        fn traverse_slice<'a>(
            slice: &'a [FormatElement],
            kind: TagKind,
            depth: &mut usize,
        ) -> Option<&'a Tag> {
            for element in slice.iter().rev() {
                match element {
                    FormatElement::Tag(tag) if tag.kind() == kind => {
                        if tag.is_start() {
                            if *depth == 0 {
                                // Invalid document
                                return None;
                            } else if *depth == 1 {
                                return Some(tag);
                            }
                            *depth -= 1;
                        } else {
                            *depth += 1;
                        }
                    }
                    FormatElement::Interned(interned) => {
                        match traverse_slice(interned, kind, depth) {
                            Some(start) => {
                                return Some(start);
                            }
                            // Reached end or invalid document
                            None if *depth == 0 => {
                                return None;
                            }
                            _ => {
                                // continue with other elements
                            }
                        }
                    }
                    _ => {}
                }
            }

            None
        }
        // Assert that the document ends at a tag with the specified kind;
        let _ = self.end_tag(kind)?;

        let mut depth = 0usize;

        traverse_slice(self, kind, &mut depth)
    }

    fn end_tag(&self, kind: TagKind) -> Option<&Tag> {
        self.last().and_then(|element| element.end_tag(kind))
    }
}

impl Format<IrFormatContext<'_>> for Condition {
    fn fmt(&self, f: &mut Formatter<IrFormatContext>) -> FormatResult<()> {
        match (self.mode, self.group_id) {
            (PrintMode::Flat, None) => write!(f, [token("if_fits_on_line")]),
            (PrintMode::Flat, Some(id)) => write!(
                f,
                [
                    token("if_group_fits_on_line("),
                    text(&std::format!("\"{id:?}\"")),
                    token(")")
                ]
            ),
            (PrintMode::Expanded, None) => write!(f, [token("if_breaks")]),
            (PrintMode::Expanded, Some(id)) => write!(
                f,
                [
                    token("if_group_breaks("),
                    text(&std::format!("\"{id:?}\"")),
                    token(")")
                ]
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use ruff_text_size::{TextRange, TextSize};

    use crate::prelude::*;
    use crate::{format, format_args, write};
    use crate::{SimpleFormatContext, SourceCode};

    #[test]
    fn display_elements() {
        let formatted = format!(
            SimpleFormatContext::default(),
            [format_with(|f| {
                write!(
                    f,
                    [group(&format_args![
                        token("("),
                        soft_block_indent(&format_args![
                            token("Some longer content"),
                            space(),
                            token("That should ultimately break"),
                        ])
                    ])]
                )
            })]
        )
        .unwrap();

        let document = formatted.into_document();

        assert_eq!(
            &std::format!("{}", document.display(SourceCode::default())),
            r#"[
  group([
    "(",
    indent([
      soft_line_break,
      "Some longer content That should ultimately break"
    ]),
    soft_line_break
  ])
]"#
        );
    }

    #[test]
    fn escapes_quotes() {
        let formatted = format!(
            SimpleFormatContext::default(),
            [token(r#""""Python docstring""""#)]
        )
        .unwrap();

        let document = formatted.into_document();

        assert_eq!(
            &std::format!("{}", document.display(SourceCode::default())),
            r#"["\"\"\"Python docstring\"\"\""]"#
        );
    }

    #[test]
    fn display_elements_with_source_text_slice() {
        let source_code = "Some longer content\nThat should ultimately break";
        let formatted = format!(
            SimpleFormatContext::default().with_source_code(source_code),
            [format_with(|f| {
                write!(
                    f,
                    [group(&format_args![
                        token("("),
                        soft_block_indent(&format_args![
                            source_text_slice(TextRange::at(TextSize::new(0), TextSize::new(19))),
                            space(),
                            source_text_slice(TextRange::at(TextSize::new(20), TextSize::new(28))),
                        ])
                    ])]
                )
            })]
        )
        .unwrap();

        let document = formatted.into_document();

        assert_eq!(
            &std::format!("{}", document.display(SourceCode::new(source_code))),
            r#"[
  group([
    "(",
    indent([
      soft_line_break,
      "Some longer content That should ultimately break"
    ]),
    soft_line_break
  ])
]"#
        );
    }

    #[test]
    fn display_invalid_document() {
        use Tag::*;

        let document = Document::from(vec![
            FormatElement::Token { text: "[" },
            FormatElement::Tag(StartGroup(tag::Group::new())),
            FormatElement::Tag(StartIndent),
            FormatElement::Line(LineMode::Soft),
            FormatElement::Token { text: "a" },
            // Close group instead of indent
            FormatElement::Tag(EndGroup),
            FormatElement::Line(LineMode::Soft),
            FormatElement::Tag(EndIndent),
            FormatElement::Token { text: "]" },
            // End tag without start
            FormatElement::Tag(EndIndent),
            // Start tag without an end
            FormatElement::Tag(StartIndent),
        ]);

        assert_eq!(
            &std::format!("{}", document.display(SourceCode::default())),
            r#"[
  "[",
  group([
    indent([soft_line_break, "a"])
    ERROR<START_END_TAG_MISMATCH<start: Indent, end: Group>>,
    soft_line_break
  ])
  ERROR<START_END_TAG_MISMATCH<start: Group, end: Indent>>,
  "]"<END_TAG_WITHOUT_START<Indent>>,
  indent([])
  <START_WITHOUT_END<Indent>>
]"#
        );
    }
}
