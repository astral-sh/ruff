use super::tag::Tag;
use crate::format_element::tag::{Condition, DedentMode};
use crate::prelude::tag::GroupMode;
use crate::prelude::*;
use crate::printer::LineEnding;
use crate::source_code::SourceCode;
use crate::{format, write};
use crate::{
    BufferExtensions, Format, FormatContext, FormatElement, FormatOptions, FormatResult, Formatter,
    IndentStyle, LineWidth, PrinterOptions,
};
use rustc_hash::FxHashMap;
use std::collections::HashMap;
use std::ops::Deref;

/// A formatted document.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct Document {
    elements: Vec<FormatElement>,
}

impl Document {
    /// Sets [`expand`](tag::Group::expand) to [`GroupMode::Propagated`] if the group contains any of:
    /// - a group with [`expand`](tag::Group::expand) set to [GroupMode::Propagated] or [GroupMode::Expand].
    /// - a non-soft [line break](FormatElement::Line) with mode [LineMode::Hard], [LineMode::Empty], or [LineMode::Literal].
    /// - a [FormatElement::ExpandParent]
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
            FitsExpanded(&'a tag::FitsExpanded),
            BestFitting,
        }

        fn expand_parent(enclosing: &[Enclosing]) {
            match enclosing.last() {
                Some(Enclosing::Group(group)) => group.propagate_expand(),
                Some(Enclosing::ConditionalGroup(group)) => group.propagate_expand(),
                Some(Enclosing::FitsExpanded(fits_expanded)) => fits_expanded.propagate_expand(),
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
                    FormatElement::Tag(Tag::StartConditionalGroup(group)) => {
                        enclosing.push(Enclosing::ConditionalGroup(group));
                        false
                    }
                    FormatElement::Tag(Tag::EndConditionalGroup) => match enclosing.pop() {
                        Some(Enclosing::ConditionalGroup(group)) => !group.mode().is_flat(),
                        _ => false,
                    },
                    FormatElement::Interned(interned) => match checked_interned.get(interned) {
                        Some(interned_expands) => *interned_expands,
                        None => {
                            let interned_expands =
                                propagate_expands(interned, enclosing, checked_interned);
                            checked_interned.insert(interned, interned_expands);
                            interned_expands
                        }
                    },
                    FormatElement::BestFitting { variants } => {
                        enclosing.push(Enclosing::BestFitting);

                        for variant in variants {
                            propagate_expands(variant, enclosing, checked_interned);
                        }

                        // Best fitting acts as a boundary
                        expands = false;
                        enclosing.pop();
                        continue;
                    }
                    FormatElement::Tag(Tag::StartFitsExpanded(fits_expanded)) => {
                        enclosing.push(Enclosing::FitsExpanded(fits_expanded));
                        false
                    }
                    FormatElement::Tag(Tag::EndFitsExpanded) => {
                        enclosing.pop();
                        // Fits expanded acts as a boundary
                        expands = false;
                        continue;
                    }
                    FormatElement::StaticText { text } => text.contains('\n'),
                    FormatElement::DynamicText { text, .. } => text.contains('\n'),
                    FormatElement::SourceCodeSlice {
                        contains_newlines, ..
                    } => *contains_newlines,
                    FormatElement::ExpandParent
                    | FormatElement::Line(LineMode::Hard | LineMode::Empty) => true,
                    _ => false,
                };

                if element_expands {
                    expands = true;
                    expand_parent(enclosing)
                }
            }

            expands
        }

        let mut enclosing: Vec<Enclosing> = Vec::new();
        let mut interned: FxHashMap<&Interned, bool> = FxHashMap::default();
        propagate_expands(self, &mut enclosing, &mut interned);
    }

    pub fn display<'a>(&'a self, source_code: SourceCode<'a>) -> DisplayDocument {
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
        IndentStyle::Space(2)
    }

    fn line_width(&self) -> LineWidth {
        LineWidth(80)
    }

    fn as_print_options(&self) -> PrinterOptions {
        PrinterOptions {
            tab_width: 2,
            print_width: self.line_width().into(),
            line_ending: LineEnding::LineFeed,
            indent_style: IndentStyle::Space(2),
        }
    }
}

impl Format<IrFormatContext<'_>> for &[FormatElement] {
    fn fmt(&self, f: &mut Formatter<IrFormatContext>) -> FormatResult<()> {
        use Tag::*;

        write!(f, [ContentArrayStart])?;

        let mut tag_stack = Vec::new();
        let mut first_element = true;
        let mut in_text = false;

        let mut iter = self.iter().peekable();

        while let Some(element) = iter.next() {
            if !first_element && !in_text && !element.is_end_tag() {
                // Write a separator between every two elements
                write!(f, [text(","), soft_line_break_or_space()])?;
            }

            first_element = false;

            match element {
                element @ FormatElement::Space
                | element @ FormatElement::StaticText { .. }
                | element @ FormatElement::DynamicText { .. }
                | element @ FormatElement::SourceCodeSlice { .. } => {
                    if !in_text {
                        write!(f, [text("\"")])?;
                    }

                    in_text = true;

                    fn write_escaped(
                        element: &FormatElement,
                        f: &mut Formatter<IrFormatContext>,
                    ) -> FormatResult<()> {
                        let text = match element {
                            FormatElement::StaticText { text } => text,
                            FormatElement::DynamicText { text } => text.as_ref(),
                            FormatElement::SourceCodeSlice { slice, .. } => {
                                slice.text(f.context().source_code())
                            }
                            _ => unreachable!(),
                        };

                        if text.contains('"') {
                            f.write_element(FormatElement::DynamicText {
                                text: text.replace('"', r#"\""#).into(),
                            })
                        } else {
                            f.write_element(element.clone())
                        }
                    }

                    match element {
                        FormatElement::Space => {
                            write!(f, [text(" ")])?;
                        }
                        element if element.is_text() => {
                            write_escaped(element, f)?;
                        }
                        _ => unreachable!(),
                    }

                    let is_next_text = iter.peek().map_or(false, |e| e.is_text() || e.is_space());

                    if !is_next_text {
                        write!(f, [text("\"")])?;
                        in_text = false;
                    }
                }

                FormatElement::Line(mode) => match mode {
                    LineMode::SoftOrSpace => {
                        write!(f, [text("soft_line_break_or_space")])?;
                    }
                    LineMode::Soft => {
                        write!(f, [text("soft_line_break")])?;
                    }
                    LineMode::Hard => {
                        write!(f, [text("hard_line_break")])?;
                    }
                    LineMode::Empty => {
                        write!(f, [text("empty_line")])?;
                    }
                },
                FormatElement::ExpandParent => {
                    write!(f, [text("expand_parent")])?;
                }

                FormatElement::SourcePosition(position) => {
                    write!(
                        f,
                        [dynamic_text(
                            &std::format!("source_position({:?})", position),
                            None
                        )]
                    )?;
                }

                FormatElement::LineSuffixBoundary => {
                    write!(f, [text("line_suffix_boundary")])?;
                }

                FormatElement::BestFitting { variants } => {
                    write!(f, [text("best_fitting([")])?;
                    f.write_elements([
                        FormatElement::Tag(StartIndent),
                        FormatElement::Line(LineMode::Hard),
                    ])?;

                    for variant in variants {
                        write!(f, [variant.deref(), hard_line_break()])?;
                    }

                    f.write_elements([
                        FormatElement::Tag(EndIndent),
                        FormatElement::Line(LineMode::Hard),
                    ])?;

                    write!(f, [text("])")])?;
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
                                    dynamic_text(&std::format!("<interned {index}>"), None),
                                    space(),
                                    &interned.deref(),
                                ]
                            )?;
                        }
                        Some(reference) => {
                            write!(
                                f,
                                [dynamic_text(
                                    &std::format!("<ref interned *{reference}>"),
                                    None
                                )]
                            )?;
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
                                        text("<END_TAG_WITHOUT_START<"),
                                        dynamic_text(&std::format!("{:?}", tag.kind()), None),
                                        text(">>"),
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
                                        text(")"),
                                        soft_line_break_or_space(),
                                        text("ERROR<START_END_TAG_MISMATCH<start: "),
                                        dynamic_text(&std::format!("{start_kind:?}"), None),
                                        text(", end: "),
                                        dynamic_text(&std::format!("{:?}", tag.kind()), None),
                                        text(">>")
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
                            write!(f, [text("indent(")])?;
                        }

                        StartDedent(mode) => {
                            let label = match mode {
                                DedentMode::Level => "dedent",
                                DedentMode::Root => "dedentRoot",
                            };

                            write!(f, [text(label), text("(")])?;
                        }

                        StartAlign(tag::Align(count)) => {
                            write!(
                                f,
                                [
                                    text("align("),
                                    dynamic_text(&count.to_string(), None),
                                    text(","),
                                    space(),
                                ]
                            )?;
                        }

                        StartLineSuffix => {
                            write!(f, [text("line_suffix(")])?;
                        }

                        StartVerbatim(_) => {
                            write!(f, [text("verbatim(")])?;
                        }

                        StartGroup(group) => {
                            write!(f, [text("group(")])?;

                            if let Some(group_id) = group.id() {
                                write!(
                                    f,
                                    [
                                        dynamic_text(&std::format!("\"{group_id:?}\""), None),
                                        text(","),
                                        space(),
                                    ]
                                )?;
                            }

                            match group.mode() {
                                GroupMode::Flat => {}
                                GroupMode::Expand => {
                                    write!(f, [text("expand: true,"), space()])?;
                                }
                                GroupMode::Propagated => {
                                    write!(f, [text("expand: propagated,"), space()])?;
                                }
                            }
                        }

                        StartConditionalGroup(group) => {
                            write!(
                                f,
                                [
                                    text("conditional_group(condition:"),
                                    space(),
                                    group.condition(),
                                    text(","),
                                    space()
                                ]
                            )?;

                            match group.mode() {
                                GroupMode::Flat => {}
                                GroupMode::Expand => {
                                    write!(f, [text("expand: true,"), space()])?;
                                }
                                GroupMode::Propagated => {
                                    write!(f, [text("expand: propagated,"), space()])?;
                                }
                            }
                        }

                        StartIndentIfGroupBreaks(id) => {
                            write!(
                                f,
                                [
                                    text("indent_if_group_breaks("),
                                    dynamic_text(&std::format!("\"{id:?}\""), None),
                                    text(","),
                                    space(),
                                ]
                            )?;
                        }

                        StartConditionalContent(condition) => {
                            match condition.mode {
                                PrintMode::Flat => {
                                    write!(f, [text("if_group_fits_on_line(")])?;
                                }
                                PrintMode::Expanded => {
                                    write!(f, [text("if_group_breaks(")])?;
                                }
                            }

                            if let Some(group_id) = condition.group_id {
                                write!(
                                    f,
                                    [
                                        dynamic_text(&std::format!("\"{group_id:?}\""), None),
                                        text(","),
                                        space(),
                                    ]
                                )?;
                            }
                        }

                        StartLabelled(label_id) => {
                            write!(
                                f,
                                [
                                    text("label("),
                                    dynamic_text(&std::format!("\"{label_id:?}\""), None),
                                    text(","),
                                    space(),
                                ]
                            )?;
                        }

                        StartFill => {
                            write!(f, [text("fill(")])?;
                        }

                        StartFitsExpanded(tag::FitsExpanded {
                            condition,
                            propagate_expand,
                        }) => {
                            write!(f, [text("fits_expanded(propagate_expand:"), space()])?;

                            if propagate_expand.get() {
                                write!(f, [text("true")])?;
                            } else {
                                write!(f, [text("false")])?;
                            }

                            write!(f, [text(","), space()])?;

                            if let Some(condition) = condition {
                                write!(
                                    f,
                                    [text("condition:"), space(), condition, text(","), space()]
                                )?;
                            }
                        }

                        StartEntry => {
                            // handled after the match for all start tags
                        }
                        EndEntry => write!(f, [ContentArrayEnd])?,

                        EndFill
                        | EndLabelled
                        | EndConditionalContent
                        | EndIndentIfGroupBreaks
                        | EndAlign
                        | EndIndent
                        | EndGroup
                        | EndConditionalGroup
                        | EndLineSuffix
                        | EndDedent
                        | EndFitsExpanded
                        | EndVerbatim => {
                            write!(f, [ContentArrayEnd, text(")")])?;
                        }
                    };

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
                    text(")"),
                    soft_line_break_or_space(),
                    dynamic_text(&std::format!("<START_WITHOUT_END<{top:?}>>"), None),
                ]
            )?;
        }

        write!(f, [ContentArrayEnd])
    }
}

struct ContentArrayStart;

impl Format<IrFormatContext<'_>> for ContentArrayStart {
    fn fmt(&self, f: &mut Formatter<IrFormatContext>) -> FormatResult<()> {
        use Tag::*;

        write!(f, [text("[")])?;

        f.write_elements([
            FormatElement::Tag(StartGroup(tag::Group::new())),
            FormatElement::Tag(StartIndent),
            FormatElement::Line(LineMode::Soft),
        ])
    }
}

struct ContentArrayEnd;

impl Format<IrFormatContext<'_>> for ContentArrayEnd {
    fn fmt(&self, f: &mut Formatter<IrFormatContext>) -> FormatResult<()> {
        use Tag::*;
        f.write_elements([
            FormatElement::Tag(EndIndent),
            FormatElement::Line(LineMode::Soft),
            FormatElement::Tag(EndGroup),
        ])?;

        write!(f, [text("]")])
    }
}

impl FormatElements for [FormatElement] {
    fn will_break(&self) -> bool {
        use Tag::*;
        let mut ignore_depth = 0usize;

        for element in self {
            match element {
                // Line suffix
                // Ignore if any of its content breaks
                FormatElement::Tag(StartLineSuffix) => {
                    ignore_depth += 1;
                }
                FormatElement::Tag(EndLineSuffix) => {
                    ignore_depth -= 1;
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
            .map_or(false, |element| element.has_label(expected))
    }

    fn start_tag(&self, kind: TagKind) -> Option<&Tag> {
        // Assert that the document ends at a tag with the specified kind;
        let _ = self.end_tag(kind)?;

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
                            } else {
                                *depth -= 1;
                            }
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
            (PrintMode::Flat, None) => write!(f, [text("if_fits_on_line")]),
            (PrintMode::Flat, Some(id)) => write!(
                f,
                [
                    text("if_group_fits_on_line("),
                    dynamic_text(&std::format!("\"{id:?}\""), None),
                    text(")")
                ]
            ),
            (PrintMode::Expanded, None) => write!(f, [text("if_breaks")]),
            (PrintMode::Expanded, Some(id)) => write!(
                f,
                [
                    text("if_group_breaks("),
                    dynamic_text(&std::format!("\"{id:?}\""), None),
                    text(")")
                ]
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::prelude::*;
    use crate::{format, format_args, write};
    use crate::{SimpleFormatContext, SourceCode};
    use ruff_text_size::{TextRange, TextSize};

    #[test]
    fn display_elements() {
        let formatted = format!(
            SimpleFormatContext::default(),
            [format_with(|f| {
                write!(
                    f,
                    [group(&format_args![
                        text("("),
                        soft_block_indent(&format_args![
                            text("Some longer content"),
                            space(),
                            text("That should ultimately break"),
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
            [text(r#""""Python docstring""""#)]
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
                        text("("),
                        soft_block_indent(&format_args![
                            source_text_slice(
                                TextRange::at(TextSize::new(0), TextSize::new(19)),
                                ContainsNewlines::No
                            ),
                            space(),
                            source_text_slice(
                                TextRange::at(TextSize::new(20), TextSize::new(28)),
                                ContainsNewlines::No
                            ),
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
            FormatElement::StaticText { text: "[" },
            FormatElement::Tag(StartGroup(tag::Group::new())),
            FormatElement::Tag(StartIndent),
            FormatElement::Line(LineMode::Soft),
            FormatElement::StaticText { text: "a" },
            // Close group instead of indent
            FormatElement::Tag(EndGroup),
            FormatElement::Line(LineMode::Soft),
            FormatElement::Tag(EndIndent),
            FormatElement::StaticText { text: "]" },
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
