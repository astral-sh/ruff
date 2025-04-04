use std::num::NonZeroU8;

use drop_bomb::DebugDropBomb;
use unicode_width::UnicodeWidthChar;

pub use printer_options::*;
use ruff_text_size::{TextLen, TextSize};

use crate::format_element::document::Document;
use crate::format_element::tag::{Condition, GroupMode};
use crate::format_element::{BestFittingMode, BestFittingVariants, LineMode, PrintMode};
use crate::prelude::tag::{DedentMode, Tag, TagKind, VerbatimKind};
use crate::prelude::{tag, TextWidth};
use crate::printer::call_stack::{
    CallStack, FitsCallStack, PrintCallStack, PrintElementArgs, StackFrame,
};
use crate::printer::line_suffixes::{LineSuffixEntry, LineSuffixes};
use crate::printer::queue::{
    AllPredicate, FitsEndPredicate, FitsQueue, PrintQueue, Queue, SingleEntryPredicate,
};
use crate::source_code::SourceCode;
use crate::{
    ActualStart, FormatElement, GroupId, IndentStyle, InvalidDocumentError, PrintError,
    PrintResult, Printed, SourceMarker, TextRange,
};

mod call_stack;
mod line_suffixes;
mod printer_options;
mod queue;
mod stack;

/// Prints the format elements into a string
#[derive(Debug, Default)]
pub struct Printer<'a> {
    options: PrinterOptions,
    source_code: SourceCode<'a>,
    state: PrinterState<'a>,
}

impl<'a> Printer<'a> {
    pub fn new(source_code: SourceCode<'a>, options: PrinterOptions) -> Self {
        Self {
            source_code,
            options,
            state: PrinterState::with_capacity(source_code.as_str().len()),
        }
    }

    /// Prints the passed in element as well as all its content
    pub fn print(self, document: &'a Document) -> PrintResult<Printed> {
        self.print_with_indent(document, 0)
    }

    /// Prints the passed in element as well as all its content,
    /// starting at the specified indentation level
    #[tracing::instrument(level = "debug", name = "Printer::print", skip_all)]
    pub fn print_with_indent(
        mut self,
        document: &'a Document,
        indent: u16,
    ) -> PrintResult<Printed> {
        let indentation = Indentation::Level(indent);
        self.state.pending_indent = indentation;

        let mut stack = PrintCallStack::new(PrintElementArgs::new(indentation));
        let mut queue: PrintQueue<'a> = PrintQueue::new(document.as_ref());

        loop {
            if let Some(element) = queue.pop() {
                self.print_element(&mut stack, &mut queue, element)?;
            } else {
                if !self.flush_line_suffixes(&mut queue, &mut stack, None) {
                    break;
                }
            }
        }

        // Push any pending marker
        self.push_marker();

        Ok(Printed::new(
            self.state.buffer,
            None,
            self.state.source_markers,
            self.state.verbatim_markers,
        ))
    }

    /// Prints a single element and push the following elements to queue
    fn print_element(
        &mut self,
        stack: &mut PrintCallStack,
        queue: &mut PrintQueue<'a>,
        element: &'a FormatElement,
    ) -> PrintResult<()> {
        #[allow(clippy::enum_glob_use)]
        use Tag::*;

        let args = stack.top();

        match element {
            FormatElement::Space => self.print_text(Text::Token(" ")),
            FormatElement::Token { text } => self.print_text(Text::Token(text)),
            FormatElement::Text { text, text_width } => self.print_text(Text::Text {
                text,
                text_width: *text_width,
            }),
            FormatElement::SourceCodeSlice { slice, text_width } => {
                let text = slice.text(self.source_code);
                self.print_text(Text::Text {
                    text,
                    text_width: *text_width,
                });
            }
            FormatElement::Line(line_mode) => {
                if args.mode().is_flat()
                    && matches!(line_mode, LineMode::Soft | LineMode::SoftOrSpace)
                {
                    if line_mode == &LineMode::SoftOrSpace {
                        self.print_text(Text::Token(" "));
                    }
                } else if self.state.line_suffixes.has_pending() {
                    self.flush_line_suffixes(queue, stack, Some(element));
                } else {
                    // Only print a newline if the current line isn't already empty
                    if !self.state.buffer[self.state.line_start..].is_empty() {
                        self.push_marker();
                        self.print_char('\n');
                    }

                    // Print a second line break if this is an empty line
                    if line_mode == &LineMode::Empty {
                        self.push_marker();
                        self.print_char('\n');
                    }

                    self.state.pending_indent = args.indentation();
                }
            }

            FormatElement::ExpandParent => {
                // Handled in `Document::propagate_expands()
            }

            FormatElement::SourcePosition(position) => {
                // The printer defers printing indents until the next text
                // is printed. Pushing the marker now would mean that the
                // mapped range includes the indent range, which we don't want.
                // Queue the source map position and emit it when printing the next character
                self.state.pending_source_position = Some(*position);
            }

            FormatElement::LineSuffixBoundary => {
                const HARD_BREAK: &FormatElement = &FormatElement::Line(LineMode::Hard);
                self.flush_line_suffixes(queue, stack, Some(HARD_BREAK));
            }

            FormatElement::BestFitting { variants, mode } => {
                self.print_best_fitting(variants, *mode, queue, stack)?;
            }

            FormatElement::Interned(content) => {
                queue.extend_back(content);
            }

            FormatElement::Tag(StartGroup(group)) => {
                let print_mode = match group.mode() {
                    GroupMode::Expand | GroupMode::Propagated => PrintMode::Expanded,
                    GroupMode::Flat => {
                        self.flat_group_print_mode(TagKind::Group, group.id(), args, queue, stack)?
                    }
                };

                if let Some(id) = group.id() {
                    self.state.group_modes.insert_print_mode(id, print_mode);
                }

                stack.push(TagKind::Group, args.with_print_mode(print_mode));
            }

            FormatElement::Tag(StartBestFitParenthesize { id }) => {
                const OPEN_PAREN: FormatElement = FormatElement::Token { text: "(" };
                const INDENT: FormatElement = FormatElement::Tag(Tag::StartIndent);
                const HARD_LINE_BREAK: FormatElement = FormatElement::Line(LineMode::Hard);

                let fits_flat = self.flat_group_print_mode(
                    TagKind::BestFitParenthesize,
                    *id,
                    args,
                    queue,
                    stack,
                )? == PrintMode::Flat;

                let print_mode = if fits_flat {
                    PrintMode::Flat
                } else {
                    // Test if the content fits in expanded mode. If not, prefer avoiding the parentheses
                    // over parenthesizing the expression.
                    if let Some(id) = id {
                        self.state
                            .group_modes
                            .insert_print_mode(*id, PrintMode::Expanded);
                    }

                    stack.push(
                        TagKind::BestFitParenthesize,
                        args.with_measure_mode(MeasureMode::AllLines),
                    );

                    queue.extend_back(&[OPEN_PAREN, INDENT, HARD_LINE_BREAK]);
                    let fits_expanded = self.fits(queue, stack)?;
                    queue.pop_slice();
                    stack.pop(TagKind::BestFitParenthesize)?;

                    if fits_expanded {
                        PrintMode::Expanded
                    } else {
                        PrintMode::Flat
                    }
                };

                if let Some(id) = id {
                    self.state.group_modes.insert_print_mode(*id, print_mode);
                }

                if print_mode.is_expanded() {
                    // Parenthesize the content. The `EndIndent` is handled inside of the `EndBestFitParenthesize`
                    queue.extend_back(&[OPEN_PAREN, INDENT, HARD_LINE_BREAK]);
                }

                stack.push(
                    TagKind::BestFitParenthesize,
                    args.with_print_mode(print_mode),
                );
            }

            FormatElement::Tag(EndBestFitParenthesize) => {
                if args.mode().is_expanded() {
                    const HARD_LINE_BREAK: FormatElement = FormatElement::Line(LineMode::Hard);
                    const CLOSE_PAREN: FormatElement = FormatElement::Token { text: ")" };

                    // Finish the indent and print the hardline break and closing parentheses.
                    stack.pop(TagKind::Indent)?;
                    queue.extend_back(&[HARD_LINE_BREAK, CLOSE_PAREN]);
                }

                stack.pop(TagKind::BestFitParenthesize)?;
            }

            FormatElement::Tag(StartConditionalGroup(group)) => {
                let condition = group.condition();
                let expected_mode = match condition.group_id {
                    None => args.mode(),
                    Some(id) => self.state.group_modes.get_print_mode(id)?,
                };

                if expected_mode == condition.mode {
                    let print_mode = match group.mode() {
                        GroupMode::Expand | GroupMode::Propagated => PrintMode::Expanded,
                        GroupMode::Flat => self.flat_group_print_mode(
                            TagKind::ConditionalGroup,
                            None,
                            args,
                            queue,
                            stack,
                        )?,
                    };

                    stack.push(TagKind::ConditionalGroup, args.with_print_mode(print_mode));
                } else {
                    // Condition isn't met, render as normal content
                    stack.push(TagKind::ConditionalGroup, args);
                }
            }

            FormatElement::Tag(StartFill) => {
                self.print_fill_entries(queue, stack)?;
            }

            FormatElement::Tag(StartIndent) => {
                stack.push(
                    TagKind::Indent,
                    args.increment_indent_level(self.options.indent_style()),
                );
            }

            FormatElement::Tag(StartDedent(mode)) => {
                let args = match mode {
                    DedentMode::Level => args.decrement_indent(),
                    DedentMode::Root => args.reset_indent(),
                };
                stack.push(TagKind::Dedent, args);
            }

            FormatElement::Tag(StartAlign(align)) => {
                stack.push(TagKind::Align, args.set_indent_align(align.count()));
            }

            FormatElement::Tag(StartConditionalContent(Condition { mode, group_id })) => {
                let group_mode = match group_id {
                    None => args.mode(),
                    Some(id) => self.state.group_modes.get_print_mode(*id)?,
                };

                if *mode == group_mode {
                    stack.push(TagKind::ConditionalContent, args);
                } else {
                    queue.skip_content(TagKind::ConditionalContent);
                }
            }

            FormatElement::Tag(StartIndentIfGroupBreaks(group_id)) => {
                let group_mode = self.state.group_modes.get_print_mode(*group_id)?;

                let args = match group_mode {
                    PrintMode::Flat => args,
                    PrintMode::Expanded => args.increment_indent_level(self.options.indent_style),
                };

                stack.push(TagKind::IndentIfGroupBreaks, args);
            }

            FormatElement::Tag(StartLineSuffix { reserved_width }) => {
                self.state.line_width += reserved_width;
                self.state
                    .line_suffixes
                    .extend(args, queue.iter_content(TagKind::LineSuffix));
            }

            FormatElement::Tag(StartVerbatim(kind)) => {
                if let VerbatimKind::Verbatim { length } = kind {
                    // SAFETY: Ruff only supports formatting files <= 4GB
                    #[allow(clippy::cast_possible_truncation)]
                    self.state.verbatim_markers.push(TextRange::at(
                        TextSize::from(self.state.buffer.len() as u32),
                        *length,
                    ));
                }

                stack.push(TagKind::Verbatim, args);
            }

            FormatElement::Tag(StartFitsExpanded(tag::FitsExpanded { condition, .. })) => {
                let condition_met = match condition {
                    Some(condition) => {
                        let group_mode = match condition.group_id {
                            Some(group_id) => self.state.group_modes.get_print_mode(group_id)?,
                            None => args.mode(),
                        };

                        condition.mode == group_mode
                    }
                    None => true,
                };

                if condition_met {
                    // We measured the inner groups all in expanded. It now is necessary to measure if the inner groups fit as well.
                    self.state.measured_group_fits = false;
                }

                stack.push(TagKind::FitsExpanded, args);
            }

            FormatElement::Tag(tag @ (StartLabelled(_) | StartEntry | StartBestFittingEntry)) => {
                stack.push(tag.kind(), args);
            }

            FormatElement::Tag(
                tag @ (EndLabelled
                | EndEntry
                | EndGroup
                | EndConditionalGroup
                | EndIndent
                | EndDedent
                | EndAlign
                | EndConditionalContent
                | EndIndentIfGroupBreaks
                | EndFitsExpanded
                | EndVerbatim
                | EndLineSuffix
                | EndBestFittingEntry
                | EndFill),
            ) => {
                stack.pop(tag.kind())?;
            }
        }

        Ok(())
    }

    fn fits(&mut self, queue: &PrintQueue<'a>, stack: &PrintCallStack) -> PrintResult<bool> {
        let mut measure = FitsMeasurer::new(queue, stack, self);
        let result = measure.fits(&mut AllPredicate);
        measure.finish();
        result
    }

    fn flat_group_print_mode(
        &mut self,
        kind: TagKind,
        id: Option<GroupId>,
        args: PrintElementArgs,
        queue: &PrintQueue<'a>,
        stack: &mut PrintCallStack,
    ) -> PrintResult<PrintMode> {
        let print_mode = match args.mode() {
            PrintMode::Flat if self.state.measured_group_fits => {
                // A parent group has already verified that this group fits on a single line
                // Thus, just continue in flat mode
                PrintMode::Flat
            }
            // The printer is either in expanded mode or it's necessary to re-measure if the group fits
            // because the printer printed a line break
            _ => {
                self.state.measured_group_fits = true;

                if let Some(id) = id {
                    self.state
                        .group_modes
                        .insert_print_mode(id, PrintMode::Flat);
                }

                // Measure to see if the group fits up on a single line. If that's the case,
                // print the group in "flat" mode, otherwise continue in expanded mode
                stack.push(kind, args.with_print_mode(PrintMode::Flat));
                let fits = self.fits(queue, stack)?;
                stack.pop(kind)?;

                if fits {
                    PrintMode::Flat
                } else {
                    PrintMode::Expanded
                }
            }
        };

        Ok(print_mode)
    }

    fn print_text(&mut self, text: Text) {
        if !self.state.pending_indent.is_empty() {
            let (indent_char, repeat_count) = match self.options.indent_style() {
                IndentStyle::Tab => ('\t', 1),
                IndentStyle::Space => (' ', self.options.indent_width()),
            };

            let indent = std::mem::take(&mut self.state.pending_indent);
            let total_indent_char_count = indent.level() as usize * repeat_count as usize;

            self.state
                .buffer
                .reserve(total_indent_char_count + indent.align() as usize);

            for _ in 0..total_indent_char_count {
                self.print_char(indent_char);
            }

            for _ in 0..indent.align() {
                self.print_char(' ');
            }
        }

        self.push_marker();

        match text {
            #[allow(clippy::cast_possible_truncation)]
            Text::Token(token) => {
                self.state.buffer.push_str(token);
                self.state.line_width += token.len() as u32;
            }
            Text::Text {
                text,
                text_width: width,
            } => {
                if let Some(width) = width.width() {
                    self.state.buffer.push_str(text);
                    self.state.line_width += width.value();
                } else {
                    for char in text.chars() {
                        self.print_char(char);
                    }
                }
            }
        }
    }

    fn push_marker(&mut self) {
        let Some(source_position) = self.state.pending_source_position.take() else {
            return;
        };

        let marker = SourceMarker {
            source: source_position,
            dest: self.state.buffer.text_len(),
        };

        if self.state.source_markers.last() != Some(&marker) {
            self.state.source_markers.push(marker);
        }
    }

    fn flush_line_suffixes(
        &mut self,
        queue: &mut PrintQueue<'a>,
        stack: &mut PrintCallStack,
        line_break: Option<&'a FormatElement>,
    ) -> bool {
        let suffixes = self.state.line_suffixes.take_pending();

        if suffixes.len() > 0 {
            // Print this line break element again once all the line suffixes have been flushed
            if let Some(line_break) = line_break {
                queue.push(line_break);
            }

            for entry in suffixes.rev() {
                match entry {
                    LineSuffixEntry::Suffix(suffix) => {
                        queue.push(suffix);
                    }
                    LineSuffixEntry::Args(args) => {
                        const LINE_SUFFIX_END: &FormatElement =
                            &FormatElement::Tag(Tag::EndLineSuffix);

                        stack.push(TagKind::LineSuffix, args);

                        queue.push(LINE_SUFFIX_END);
                    }
                }
            }

            true
        } else {
            false
        }
    }

    fn print_best_fitting(
        &mut self,
        variants: &'a BestFittingVariants,
        mode: BestFittingMode,
        queue: &mut PrintQueue<'a>,
        stack: &mut PrintCallStack,
    ) -> PrintResult<()> {
        let args = stack.top();

        if args.mode().is_flat() && self.state.measured_group_fits {
            queue.extend_back(variants.most_flat());
            self.print_entry(queue, stack, args, TagKind::BestFittingEntry)
        } else {
            self.state.measured_group_fits = true;
            let mut variants_iter = variants.into_iter();
            let mut current = variants_iter.next().unwrap();

            for next in variants_iter {
                // Test if this variant fits and if so, use it. Otherwise try the next
                // variant.

                // Try to fit only the first variant on a single line
                if !matches!(
                    current.first(),
                    Some(&FormatElement::Tag(Tag::StartBestFittingEntry))
                ) {
                    return invalid_start_tag(TagKind::BestFittingEntry, current.first());
                }

                // Skip the first element because we want to override the args for the entry and the
                // args must be popped from the stack as soon as it sees the matching end entry.
                let content = &current[1..];

                let entry_args = args
                    .with_print_mode(PrintMode::Flat)
                    .with_measure_mode(MeasureMode::from(mode));

                queue.extend_back(content);
                stack.push(TagKind::BestFittingEntry, entry_args);
                let variant_fits = self.fits(queue, stack)?;
                stack.pop(TagKind::BestFittingEntry)?;

                // Remove the content slice because printing needs the variant WITH the start entry
                let popped_slice = queue.pop_slice();
                debug_assert_eq!(popped_slice, Some(content));

                if variant_fits {
                    queue.extend_back(current);
                    return self.print_entry(
                        queue,
                        stack,
                        args.with_print_mode(PrintMode::Flat),
                        TagKind::BestFittingEntry,
                    );
                }

                current = next;
            }

            // At this stage current is the most expanded.

            // No variant fits, take the last (most expanded) as fallback
            queue.extend_back(current);
            self.print_entry(
                queue,
                stack,
                args.with_print_mode(PrintMode::Expanded),
                TagKind::BestFittingEntry,
            )
        }
    }

    /// Tries to fit as much content as possible on a single line.
    ///
    /// `Fill` is a sequence of *item*, *separator*, *item*, *separator*, *item*, ... entries.
    /// The goal is to fit as many items (with their separators) on a single line as possible and
    /// first expand the *separator* if the content exceeds the print width and only fallback to expanding
    /// the *item*s if the *item* or the *item* and the expanded *separator* don't fit on the line.
    ///
    /// The implementation handles the following 5 cases:
    ///
    /// - The *item*, *separator*, and the *next item* fit on the same line.
    ///   Print the *item* and *separator* in flat mode.
    /// - The *item* and *separator* fit on the line but there's not enough space for the *next item*.
    ///   Print the *item* in flat mode and the *separator* in expanded mode.
    /// - The *item* fits on the line but the *separator* does not in flat mode.
    ///   Print the *item* in flat mode and the *separator* in expanded mode.
    /// - The *item* fits on the line but the *separator* does not in flat **NOR** expanded mode.
    ///   Print the *item* and *separator* in expanded mode.
    /// - The *item* does not fit on the line.
    ///   Print the *item* and *separator* in expanded mode.
    fn print_fill_entries(
        &mut self,
        queue: &mut PrintQueue<'a>,
        stack: &mut PrintCallStack,
    ) -> PrintResult<()> {
        let args = stack.top();

        // It's already known that the content fit, print all items in flat mode.
        if self.state.measured_group_fits && args.mode().is_flat() {
            stack.push(TagKind::Fill, args.with_print_mode(PrintMode::Flat));
            return Ok(());
        }

        stack.push(TagKind::Fill, args);

        while matches!(queue.top(), Some(FormatElement::Tag(Tag::StartEntry))) {
            let mut measurer = FitsMeasurer::new_flat(queue, stack, self);

            // The number of item/separator pairs that fit on the same line.
            let mut flat_pairs = 0usize;
            let mut item_fits = measurer.fill_item_fits()?;

            let last_pair_layout = if item_fits {
                // Measure the remaining pairs until the first item or separator that does not fit (or the end of the fill element).
                // Optimisation to avoid re-measuring the next-item twice:
                // * Once when measuring if the *item*, *separator*, *next-item* fit
                // * A second time when measuring if *next-item*, *separator*, *next-next-item* fit.
                loop {
                    // Item that fits without a following separator.
                    if !matches!(
                        measurer.queue.top(),
                        Some(FormatElement::Tag(Tag::StartEntry))
                    ) {
                        break FillPairLayout::Flat;
                    }

                    let separator_fits = measurer.fill_separator_fits(PrintMode::Flat)?;

                    // Item fits but the flat separator does not.
                    if !separator_fits {
                        break FillPairLayout::ItemMaybeFlat;
                    }

                    // Last item/separator pair that both fit
                    if !matches!(
                        measurer.queue.top(),
                        Some(FormatElement::Tag(Tag::StartEntry))
                    ) {
                        break FillPairLayout::Flat;
                    }

                    item_fits = measurer.fill_item_fits()?;

                    if item_fits {
                        flat_pairs += 1;
                    } else {
                        // Item and separator both fit, but the next element doesn't.
                        // Print the separator in expanded mode and then re-measure if the item now
                        // fits in the next iteration of the outer loop.
                        break FillPairLayout::ItemFlatSeparatorExpanded;
                    }
                }
            } else {
                // Neither item nor separator fit, print both in expanded mode.
                FillPairLayout::Expanded
            };

            measurer.finish();

            self.state.measured_group_fits = true;

            // Print all pairs that fit in flat mode.
            for _ in 0..flat_pairs {
                self.print_fill_item(queue, stack, args.with_print_mode(PrintMode::Flat))?;
                self.print_fill_separator(queue, stack, args.with_print_mode(PrintMode::Flat))?;
            }

            let item_mode = match last_pair_layout {
                FillPairLayout::Flat | FillPairLayout::ItemFlatSeparatorExpanded => PrintMode::Flat,
                FillPairLayout::Expanded => PrintMode::Expanded,
                FillPairLayout::ItemMaybeFlat => {
                    let mut measurer = FitsMeasurer::new_flat(queue, stack, self);
                    // SAFETY: That the item fits is guaranteed by `ItemMaybeFlat`.
                    // Re-measuring is required to get the measurer in the correct state for measuring the separator.
                    assert!(measurer.fill_item_fits()?);
                    let separator_fits = measurer.fill_separator_fits(PrintMode::Expanded)?;
                    measurer.finish();

                    if separator_fits {
                        PrintMode::Flat
                    } else {
                        PrintMode::Expanded
                    }
                }
            };

            self.print_fill_item(queue, stack, args.with_print_mode(item_mode))?;

            if matches!(queue.top(), Some(FormatElement::Tag(Tag::StartEntry))) {
                let separator_mode = match last_pair_layout {
                    FillPairLayout::Flat => PrintMode::Flat,
                    FillPairLayout::ItemFlatSeparatorExpanded
                    | FillPairLayout::Expanded
                    | FillPairLayout::ItemMaybeFlat => PrintMode::Expanded,
                };

                // Push a new stack frame with print mode `Flat` for the case where the separator gets printed in expanded mode
                // but does contain a group to ensure that the group will measure "fits" with the "flat" versions of the next item/separator.
                stack.push(TagKind::Fill, args.with_print_mode(PrintMode::Flat));
                self.print_fill_separator(queue, stack, args.with_print_mode(separator_mode))?;
                stack.pop(TagKind::Fill)?;
            }
        }

        if queue.top() == Some(&FormatElement::Tag(Tag::EndFill)) {
            Ok(())
        } else {
            invalid_end_tag(TagKind::Fill, stack.top_kind())
        }
    }

    /// Semantic alias for [`Self::print_entry`] for fill items.
    fn print_fill_item(
        &mut self,
        queue: &mut PrintQueue<'a>,
        stack: &mut PrintCallStack,
        args: PrintElementArgs,
    ) -> PrintResult<()> {
        self.print_entry(queue, stack, args, TagKind::Entry)
    }

    /// Semantic alias for [`Self::print_entry`] for fill separators.
    fn print_fill_separator(
        &mut self,
        queue: &mut PrintQueue<'a>,
        stack: &mut PrintCallStack,
        args: PrintElementArgs,
    ) -> PrintResult<()> {
        self.print_entry(queue, stack, args, TagKind::Entry)
    }

    /// Fully print an element (print the element itself and all its descendants)
    ///
    /// Unlike [`print_element`], this function ensures the entire element has
    /// been printed when it returns and the queue is back to its original state
    fn print_entry(
        &mut self,
        queue: &mut PrintQueue<'a>,
        stack: &mut PrintCallStack,
        args: PrintElementArgs,
        kind: TagKind,
    ) -> PrintResult<()> {
        let start_entry = queue.top();

        if queue
            .pop()
            .is_some_and(|start| start.tag_kind() == Some(kind))
        {
            stack.push(kind, args);
        } else {
            return invalid_start_tag(kind, start_entry);
        }

        let mut depth = 1u32;

        while let Some(element) = queue.pop() {
            match element {
                FormatElement::Tag(Tag::StartEntry | Tag::StartBestFittingEntry) => {
                    depth += 1;
                }
                FormatElement::Tag(end_tag @ (Tag::EndEntry | Tag::EndBestFittingEntry)) => {
                    depth -= 1;
                    // Reached the end entry, pop the entry from the stack and return.
                    if depth == 0 {
                        stack.pop(end_tag.kind())?;
                        return Ok(());
                    }
                }
                _ => {
                    // Fall through
                }
            }

            self.print_element(stack, queue, element)?;
        }

        invalid_end_tag(kind, stack.top_kind())
    }

    fn print_char(&mut self, char: char) {
        if char == '\n' {
            self.state
                .buffer
                .push_str(self.options.line_ending.as_str());

            self.state.line_width = 0;
            self.state.line_start = self.state.buffer.len();

            // Fit's only tests if groups up to the first line break fit.
            // The next group must re-measure if it still fits.
            self.state.measured_group_fits = false;
        } else {
            self.state.buffer.push(char);

            #[allow(clippy::cast_possible_truncation)]
            let char_width = if char == '\t' {
                self.options.indent_width.value()
            } else {
                // SAFETY: A u32 is sufficient to represent the width of a file <= 4GB
                char.width().unwrap_or(0) as u32
            };

            self.state.line_width += char_width;
        }
    }
}

#[derive(Copy, Clone, Debug)]
enum FillPairLayout {
    /// The item, separator, and next item fit. Print the first item and the separator in flat mode.
    Flat,

    /// The item and separator fit but the next element does not. Print the item in flat mode and
    /// the separator in expanded mode.
    ItemFlatSeparatorExpanded,

    /// The item does not fit. Print the item and any potential separator in expanded mode.
    Expanded,

    /// The item fits but the separator does not in flat mode. If the separator fits in expanded mode then
    /// print the item in flat and the separator in expanded mode, otherwise print both in expanded mode.
    ItemMaybeFlat,
}

/// Printer state that is global to all elements.
/// Stores the result of the print operation (buffer and mappings) and at what
/// position the printer currently is.
#[derive(Default, Debug)]
struct PrinterState<'a> {
    /// The formatted output.
    buffer: String,

    /// The source markers that map source positions to formatted positions.
    source_markers: Vec<SourceMarker>,

    /// The next source position that should be flushed when writing the next text.
    pending_source_position: Option<TextSize>,

    /// The current indentation that should be written before the next text.
    pending_indent: Indentation,

    /// Caches if the code up to the next newline has been measured to fit on a single line.
    /// This is used to avoid re-measuring the same content multiple times.
    measured_group_fits: bool,

    /// The offset at which the current line in `buffer` starts.
    line_start: usize,

    /// The accumulated unicode-width of all characters on the current line.
    line_width: u32,

    /// The line suffixes that should be printed at the end of the line.
    line_suffixes: LineSuffixes<'a>,
    verbatim_markers: Vec<TextRange>,
    group_modes: GroupModes,
    // Reused queue to measure if a group fits. Optimisation to avoid re-allocating a new
    // vec every time a group gets measured
    fits_stack: Vec<StackFrame>,
    fits_queue: Vec<std::slice::Iter<'a, FormatElement>>,
}

impl PrinterState<'_> {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: String::with_capacity(capacity),
            ..Self::default()
        }
    }
}

/// Tracks the mode in which groups with ids are printed. Stores the groups at `group.id()` index.
/// This is based on the assumption that the group ids for a single document are dense.
#[derive(Debug, Default)]
struct GroupModes(Vec<Option<PrintMode>>);

impl GroupModes {
    fn insert_print_mode(&mut self, group_id: GroupId, mode: PrintMode) {
        let index = u32::from(group_id) as usize;

        if self.0.len() <= index {
            self.0.resize(index + 1, None);
        }

        self.0[index] = Some(mode);
    }

    fn get_print_mode(&self, group_id: GroupId) -> PrintResult<PrintMode> {
        let index = u32::from(group_id) as usize;

        match self.0.get(index) {
            Some(Some(print_mode)) => Ok(*print_mode),
            None | Some(None) => Err(PrintError::InvalidDocument(
                InvalidDocumentError::UnknownGroupId { group_id },
            )),
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum Indentation {
    /// Indent the content by `count` levels by using the indentation sequence specified by the printer options.
    Level(u16),

    /// Indent the content by n-`level`s using the indentation sequence specified by the printer options and `align` spaces.
    Align { level: u16, align: NonZeroU8 },
}

impl Indentation {
    const fn is_empty(self) -> bool {
        matches!(self, Indentation::Level(0))
    }

    /// Creates a new indentation level with a zero-indent.
    const fn new() -> Self {
        Indentation::Level(0)
    }

    /// Returns the indentation level
    fn level(self) -> u16 {
        match self {
            Indentation::Level(count) => count,
            Indentation::Align { level: indent, .. } => indent,
        }
    }

    /// Returns the number of trailing align spaces or 0 if none
    fn align(self) -> u8 {
        match self {
            Indentation::Level(_) => 0,
            Indentation::Align { align, .. } => align.into(),
        }
    }

    /// Increments the level by one.
    ///
    /// The behaviour depends on the [`indent_style`][IndentStyle] if this is an [`Indent::Align`]:
    /// - **Tabs**: `align` is converted into an indent. This results in `level` increasing by two: once for the align, once for the level increment
    /// - **Spaces**: Increments the `level` by one and keeps the `align` unchanged.
    ///   Keeps any  the current value is [`Indent::Align`] and increments the level by one.
    fn increment_level(self, indent_style: IndentStyle) -> Self {
        match self {
            Indentation::Level(count) => Indentation::Level(count + 1),
            // Increase the indent AND convert the align to an indent
            Indentation::Align { level, .. } if indent_style.is_tab() => {
                Indentation::Level(level + 2)
            }
            Indentation::Align {
                level: indent,
                align,
            } => Indentation::Align {
                level: indent + 1,
                align,
            },
        }
    }

    /// Decrements the indent by one by:
    /// - Reducing the level by one if this is [`Indent::Level`]
    /// - Removing the `align` if this is [`Indent::Align`]
    ///
    /// No-op if the level is already zero.
    fn decrement(self) -> Self {
        match self {
            Indentation::Level(level) => Indentation::Level(level.saturating_sub(1)),
            Indentation::Align { level, .. } => Indentation::Level(level),
        }
    }

    /// Adds an `align` of `count` spaces to the current indentation.
    ///
    /// It increments the `level` value if the current value is [`Indent::IndentAlign`].
    fn set_align(self, count: NonZeroU8) -> Self {
        match self {
            Indentation::Level(indent_count) => Indentation::Align {
                level: indent_count,
                align: count,
            },

            // Convert the existing align to an indent
            Indentation::Align { level: indent, .. } => Indentation::Align {
                level: indent + 1,
                align: count,
            },
        }
    }
}

impl Default for Indentation {
    fn default() -> Self {
        Indentation::new()
    }
}

#[must_use = "FitsMeasurer must be finished."]
struct FitsMeasurer<'a, 'print> {
    state: FitsState,
    queue: FitsQueue<'a, 'print>,
    stack: FitsCallStack<'print>,
    printer: &'print mut Printer<'a>,
    must_be_flat: bool,

    /// Bomb that enforces that finish is explicitly called to restore the `fits_stack` and `fits_queue` vectors.
    bomb: DebugDropBomb,
}

impl<'a, 'print> FitsMeasurer<'a, 'print> {
    fn new_flat(
        print_queue: &'print PrintQueue<'a>,
        print_stack: &'print PrintCallStack,
        printer: &'print mut Printer<'a>,
    ) -> Self {
        let mut measurer = Self::new(print_queue, print_stack, printer);
        measurer.must_be_flat = true;
        measurer
    }

    fn new(
        print_queue: &'print PrintQueue<'a>,
        print_stack: &'print PrintCallStack,
        printer: &'print mut Printer<'a>,
    ) -> Self {
        let saved_stack = std::mem::take(&mut printer.state.fits_stack);
        let saved_queue = std::mem::take(&mut printer.state.fits_queue);
        debug_assert!(saved_stack.is_empty());
        debug_assert!(saved_queue.is_empty());

        let fits_queue = FitsQueue::new(print_queue, saved_queue);
        let fits_stack = FitsCallStack::new(print_stack, saved_stack);

        let fits_state = FitsState {
            pending_indent: printer.state.pending_indent,
            line_width: printer.state.line_width,
            has_line_suffix: printer.state.line_suffixes.has_pending(),
        };

        Self {
            state: fits_state,
            queue: fits_queue,
            stack: fits_stack,
            must_be_flat: false,
            printer,
            bomb: DebugDropBomb::new(
                "MeasurerFits must be `finished` to restore the `fits_queue` and `fits_stack`.",
            ),
        }
    }

    /// Tests if it's possible to print the content of the queue up to the first hard line break
    /// or the end of the document on a single line without exceeding the line width.
    fn fits<P>(&mut self, predicate: &mut P) -> PrintResult<bool>
    where
        P: FitsEndPredicate,
    {
        while let Some(element) = self.queue.pop() {
            match self.fits_element(element)? {
                Fits::Yes => return Ok(true),
                Fits::No => {
                    return Ok(false);
                }
                Fits::Maybe => {
                    if predicate.is_end(element)? {
                        break;
                    }

                    continue;
                }
            }
        }

        Ok(true)
    }

    /// Tests if the content of a `Fill` item fits in [`PrintMode::Flat`].
    ///
    /// Returns `Err` if the top element of the queue is not a [`Tag::StartEntry`]
    /// or if the document has any mismatching start/end tags.
    fn fill_item_fits(&mut self) -> PrintResult<bool> {
        self.fill_entry_fits(PrintMode::Flat)
    }

    /// Tests if the content of a `Fill` separator fits with `mode`.
    ///
    /// Returns `Err` if the top element of the queue is not a [`Tag::StartEntry`]
    /// or if the document has any mismatching start/end tags.
    fn fill_separator_fits(&mut self, mode: PrintMode) -> PrintResult<bool> {
        self.fill_entry_fits(mode)
    }

    /// Tests if the elements between the [`Tag::StartEntry`] and [`Tag::EndEntry`]
    /// of a fill item or separator fits with `mode`.
    ///
    /// Returns `Err` if the queue isn't positioned at a [`Tag::StartEntry`] or if
    /// the matching [`Tag::EndEntry`] is missing.
    fn fill_entry_fits(&mut self, mode: PrintMode) -> PrintResult<bool> {
        let start_entry = self.queue.top();

        if !matches!(start_entry, Some(&FormatElement::Tag(Tag::StartEntry))) {
            return invalid_start_tag(TagKind::Entry, start_entry);
        }

        self.stack
            .push(TagKind::Fill, self.stack.top().with_print_mode(mode));
        let mut predicate = SingleEntryPredicate::default();
        let fits = self.fits(&mut predicate)?;

        if predicate.is_done() {
            self.stack.pop(TagKind::Fill)?;
        }

        Ok(fits)
    }

    /// Tests if the passed element fits on the current line or not.
    fn fits_element(&mut self, element: &'a FormatElement) -> PrintResult<Fits> {
        #[allow(clippy::enum_glob_use)]
        use Tag::*;

        let args = self.stack.top();

        match element {
            FormatElement::Space => return Ok(self.fits_text(Text::Token(" "), args)),

            FormatElement::Line(line_mode) => {
                match args.mode() {
                    PrintMode::Flat => match line_mode {
                        LineMode::SoftOrSpace => return Ok(self.fits_text(Text::Token(" "), args)),
                        LineMode::Soft => {}
                        LineMode::Hard | LineMode::Empty => {
                            return Ok(if self.must_be_flat {
                                Fits::No
                            } else {
                                Fits::Yes
                            });
                        }
                    },
                    PrintMode::Expanded => {
                        match args.measure_mode() {
                            MeasureMode::FirstLine => {
                                // Reachable if the restQueue contains an element with mode expanded because Expanded
                                // is what the mode's initialized to by default
                                // This means, the printer is outside of the current element at this point and any
                                // line break should be printed as regular line break
                                return Ok(Fits::Yes);
                            }
                            MeasureMode::AllLines | MeasureMode::AllLinesAllowTextOverflow => {
                                // Continue measuring on the next line
                                self.state.line_width = 0;
                                self.state.pending_indent = args.indentation();
                            }
                        }
                    }
                }
            }

            FormatElement::Token { text } => return Ok(self.fits_text(Text::Token(text), args)),
            FormatElement::Text { text, text_width } => {
                return Ok(self.fits_text(
                    Text::Text {
                        text,
                        text_width: *text_width,
                    },
                    args,
                ))
            }
            FormatElement::SourceCodeSlice { slice, text_width } => {
                let text = slice.text(self.printer.source_code);
                return Ok(self.fits_text(
                    Text::Text {
                        text,
                        text_width: *text_width,
                    },
                    args,
                ));
            }
            FormatElement::LineSuffixBoundary => {
                if self.state.has_line_suffix {
                    return Ok(Fits::No);
                }
            }

            FormatElement::ExpandParent => {
                if self.must_be_flat {
                    return Ok(Fits::No);
                }
            }

            FormatElement::SourcePosition(_) => {}

            FormatElement::BestFitting { variants, mode } => {
                let (slice, args) = match args.mode() {
                    PrintMode::Flat => (
                        variants.most_flat(),
                        args.with_measure_mode(MeasureMode::from(*mode)),
                    ),
                    PrintMode::Expanded => (variants.most_expanded(), args),
                };

                if !matches!(
                    slice.first(),
                    Some(FormatElement::Tag(Tag::StartBestFittingEntry))
                ) {
                    return invalid_start_tag(TagKind::BestFittingEntry, slice.first());
                }

                self.stack.push(TagKind::BestFittingEntry, args);
                self.queue.extend_back(&slice[1..]);
            }

            FormatElement::Interned(content) => self.queue.extend_back(content),

            FormatElement::Tag(StartIndent) => {
                self.stack.push(
                    TagKind::Indent,
                    args.increment_indent_level(self.options().indent_style()),
                );
            }

            FormatElement::Tag(StartDedent(mode)) => {
                let args = match mode {
                    DedentMode::Level => args.decrement_indent(),
                    DedentMode::Root => args.reset_indent(),
                };
                self.stack.push(TagKind::Dedent, args);
            }

            FormatElement::Tag(StartAlign(align)) => {
                self.stack
                    .push(TagKind::Align, args.set_indent_align(align.count()));
            }

            FormatElement::Tag(StartGroup(group)) => {
                return Ok(self.fits_group(TagKind::Group, group.mode(), group.id(), args));
            }

            FormatElement::Tag(StartBestFitParenthesize { id }) => {
                if let Some(id) = id {
                    self.printer
                        .state
                        .group_modes
                        .insert_print_mode(*id, args.mode());
                }

                // Don't use the parenthesized with indent layout even when measuring expanded mode similar to `BestFitting`.
                // This is to expand the left and not right after the `(` parentheses (it is okay to expand after the content that it wraps).
                self.stack.push(TagKind::BestFitParenthesize, args);
            }

            FormatElement::Tag(EndBestFitParenthesize) => {
                // If this is the end tag of the outer most parentheses for which we measure if it fits,
                // pop the indent.
                if args.mode().is_expanded() && self.stack.top_kind() == Some(TagKind::Indent) {
                    self.stack.pop(TagKind::Indent).unwrap();
                    let unindented = self.stack.pop(TagKind::BestFitParenthesize)?;

                    // There's a hard line break after the indent but don't return `Fits::Yes` here
                    // to ensure any trailing comments (that, unfortunately, are attached to the statement and not the expression)
                    // fit too.
                    self.state.line_width = 0;
                    self.state.pending_indent = unindented.indentation();

                    return Ok(self.fits_text(Text::Token(")"), unindented));
                }

                self.stack.pop(TagKind::BestFitParenthesize)?;
            }

            FormatElement::Tag(StartConditionalGroup(group)) => {
                let condition = group.condition();

                let print_mode = match condition.group_id {
                    None => args.mode(),
                    Some(group_id) => self.group_modes().get_print_mode(group_id)?,
                };

                if condition.mode == print_mode {
                    return Ok(self.fits_group(
                        TagKind::ConditionalGroup,
                        group.mode(),
                        None,
                        args,
                    ));
                }
                self.stack.push(TagKind::ConditionalGroup, args);
            }

            FormatElement::Tag(StartConditionalContent(condition)) => {
                let print_mode = match condition.group_id {
                    None => args.mode(),
                    Some(group_id) => self.group_modes().get_print_mode(group_id)?,
                };

                if condition.mode == print_mode {
                    self.stack.push(TagKind::ConditionalContent, args);
                } else {
                    self.queue.skip_content(TagKind::ConditionalContent);
                }
            }

            FormatElement::Tag(StartIndentIfGroupBreaks(id)) => {
                let print_mode = self.group_modes().get_print_mode(*id)?;

                match print_mode {
                    PrintMode::Flat => {
                        self.stack.push(TagKind::IndentIfGroupBreaks, args);
                    }
                    PrintMode::Expanded => {
                        self.stack.push(
                            TagKind::IndentIfGroupBreaks,
                            args.increment_indent_level(self.options().indent_style()),
                        );
                    }
                }
            }

            FormatElement::Tag(StartLineSuffix { reserved_width }) => {
                if *reserved_width > 0 {
                    self.state.line_width += reserved_width;
                    if self.state.line_width > self.options().line_width.into() {
                        return Ok(Fits::No);
                    }
                }
                self.queue.skip_content(TagKind::LineSuffix);
                self.state.has_line_suffix = true;
            }

            FormatElement::Tag(EndLineSuffix) => {
                return invalid_end_tag(TagKind::LineSuffix, self.stack.top_kind());
            }

            FormatElement::Tag(StartFitsExpanded(tag::FitsExpanded {
                condition,
                propagate_expand,
            })) => {
                match args.mode() {
                    PrintMode::Expanded => {
                        // As usual, nothing to measure
                        self.stack.push(TagKind::FitsExpanded, args);
                    }
                    PrintMode::Flat => {
                        let condition_met = match condition {
                            Some(condition) => {
                                let group_mode = match condition.group_id {
                                    Some(group_id) => {
                                        self.group_modes().get_print_mode(group_id)?
                                    }
                                    None => args.mode(),
                                };

                                condition.mode == group_mode
                            }
                            None => true,
                        };

                        if condition_met {
                            // Measure in fully expanded mode and allow overflows
                            self.stack.push(
                                TagKind::FitsExpanded,
                                args.with_measure_mode(MeasureMode::AllLinesAllowTextOverflow)
                                    .with_print_mode(PrintMode::Expanded),
                            );
                        } else {
                            if propagate_expand.get() {
                                return Ok(Fits::No);
                            }

                            // As usual
                            self.stack.push(TagKind::FitsExpanded, args);
                        }
                    }
                }
            }

            FormatElement::Tag(
                tag @ (StartFill
                | StartVerbatim(_)
                | StartLabelled(_)
                | StartEntry
                | StartBestFittingEntry),
            ) => {
                self.stack.push(tag.kind(), args);
            }

            FormatElement::Tag(
                tag @ (EndFill
                | EndVerbatim
                | EndLabelled
                | EndEntry
                | EndGroup
                | EndConditionalGroup
                | EndIndentIfGroupBreaks
                | EndConditionalContent
                | EndAlign
                | EndDedent
                | EndIndent
                | EndBestFittingEntry
                | EndFitsExpanded),
            ) => {
                self.stack.pop(tag.kind())?;
            }
        }

        Ok(Fits::Maybe)
    }

    fn fits_group(
        &mut self,
        kind: TagKind,
        group_mode: GroupMode,
        id: Option<GroupId>,
        args: PrintElementArgs,
    ) -> Fits {
        if self.must_be_flat && !group_mode.is_flat() {
            return Fits::No;
        }

        // Continue printing groups in expanded mode if measuring a `best_fitting` element where
        // a group expands.
        let print_mode = if group_mode.is_flat() {
            args.mode()
        } else {
            PrintMode::Expanded
        };

        self.stack.push(kind, args.with_print_mode(print_mode));

        if let Some(id) = id {
            self.group_modes_mut().insert_print_mode(id, print_mode);
        }

        Fits::Maybe
    }

    fn fits_text(&mut self, text: Text, args: PrintElementArgs) -> Fits {
        fn exceeds_width(fits: &FitsMeasurer, args: PrintElementArgs) -> bool {
            fits.state.line_width > fits.options().line_width.into()
                && !args.measure_mode().allows_text_overflow()
        }

        let indent = std::mem::take(&mut self.state.pending_indent);
        self.state.line_width +=
            u32::from(indent.level()) * self.options().indent_width() + u32::from(indent.align());

        match text {
            #[allow(clippy::cast_possible_truncation)]
            Text::Token(token) => {
                self.state.line_width += token.len() as u32;
            }
            Text::Text { text, text_width } => {
                if let Some(width) = text_width.width() {
                    self.state.line_width += width.value();
                } else {
                    for c in text.chars() {
                        let char_width = match c {
                            '\t' => self.options().indent_width.value(),
                            '\n' => {
                                if self.must_be_flat {
                                    return Fits::No;
                                }
                                match args.measure_mode() {
                                    MeasureMode::FirstLine => {
                                        return if exceeds_width(self, args) {
                                            Fits::No
                                        } else {
                                            Fits::Yes
                                        };
                                    }
                                    MeasureMode::AllLines
                                    | MeasureMode::AllLinesAllowTextOverflow => {
                                        self.state.line_width = 0;
                                        continue;
                                    }
                                }
                            }
                            // SAFETY: A u32 is sufficient to format files <= 4GB
                            #[allow(clippy::cast_possible_truncation)]
                            c => c.width().unwrap_or(0) as u32,
                        };
                        self.state.line_width += char_width;
                    }
                }
            }
        }

        if exceeds_width(self, args) {
            return Fits::No;
        }

        Fits::Maybe
    }

    fn finish(mut self) {
        self.bomb.defuse();

        let mut queue = self.queue.finish();
        queue.clear();
        self.printer.state.fits_queue = queue;

        let mut stack = self.stack.finish();
        stack.clear();
        self.printer.state.fits_stack = stack;
    }

    fn options(&self) -> &PrinterOptions {
        &self.printer.options
    }

    fn group_modes(&self) -> &GroupModes {
        &self.printer.state.group_modes
    }

    fn group_modes_mut(&mut self) -> &mut GroupModes {
        &mut self.printer.state.group_modes
    }
}

#[cold]
fn invalid_end_tag<R>(end_tag: TagKind, start_tag: Option<TagKind>) -> PrintResult<R> {
    Err(PrintError::InvalidDocument(match start_tag {
        None => InvalidDocumentError::StartTagMissing { kind: end_tag },
        Some(kind) => InvalidDocumentError::StartEndTagMismatch {
            start_kind: end_tag,
            end_kind: kind,
        },
    }))
}

#[cold]
fn invalid_start_tag<R>(expected: TagKind, actual: Option<&FormatElement>) -> PrintResult<R> {
    let start = match actual {
        None => ActualStart::EndOfDocument,
        Some(FormatElement::Tag(tag)) => {
            if tag.is_start() {
                ActualStart::Start(tag.kind())
            } else {
                ActualStart::End(tag.kind())
            }
        }
        Some(_) => ActualStart::Content,
    };

    Err(PrintError::InvalidDocument(
        InvalidDocumentError::ExpectedStart {
            actual: start,
            expected_start: expected,
        },
    ))
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum Fits {
    // Element fits
    Yes,
    // Element doesn't fit
    No,
    // Element may fit, depends on the elements following it
    Maybe,
}

impl From<bool> for Fits {
    fn from(value: bool) -> Self {
        if value {
            Fits::Yes
        } else {
            Fits::No
        }
    }
}

/// State used when measuring if a group fits on a single line
#[derive(Debug)]
struct FitsState {
    pending_indent: Indentation,
    has_line_suffix: bool,
    line_width: u32,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum MeasureMode {
    /// The content fits if a hard line break or soft line break in [`PrintMode::Expanded`] is seen
    /// before exceeding the configured print width.
    /// Returns
    FirstLine,

    /// The content only fits if none of the lines exceed the print width. Lines are terminated by either
    /// a hard line break or a soft line break in [`PrintMode::Expanded`].
    AllLines,

    /// Measures all lines and allows lines to exceed the configured line width. Useful when it only matters
    /// whether the content *before* and *after* fits.
    AllLinesAllowTextOverflow,
}

impl MeasureMode {
    /// Returns `true` if this mode allows text exceeding the configured line width.
    const fn allows_text_overflow(self) -> bool {
        matches!(self, MeasureMode::AllLinesAllowTextOverflow)
    }
}

impl From<BestFittingMode> for MeasureMode {
    fn from(value: BestFittingMode) -> Self {
        match value {
            BestFittingMode::FirstLine => Self::FirstLine,
            BestFittingMode::AllLines => Self::AllLines,
        }
    }
}

#[derive(Copy, Clone, Debug)]
enum Text<'a> {
    /// ASCII only text that contains no line breaks or tab characters.
    Token(&'a str),
    /// Arbitrary text. May contain `\n` line breaks, tab characters, or unicode characters.
    Text {
        text: &'a str,
        text_width: TextWidth,
    },
}

#[cfg(test)]
mod tests {
    use crate::prelude::*;
    use crate::printer::{LineEnding, Printer, PrinterOptions};
    use crate::source_code::SourceCode;
    use crate::{
        format_args, write, Document, FormatState, IndentStyle, IndentWidth, LineWidth, Printed,
        VecBuffer,
    };

    fn format(root: &dyn Format<SimpleFormatContext>) -> Printed {
        format_with_options(
            root,
            PrinterOptions {
                indent_style: IndentStyle::Space,
                ..PrinterOptions::default()
            },
        )
    }

    fn format_with_options(
        root: &dyn Format<SimpleFormatContext>,
        options: PrinterOptions,
    ) -> Printed {
        let formatted = crate::format!(SimpleFormatContext::default(), [root]).unwrap();

        Printer::new(SourceCode::default(), options)
            .print(formatted.document())
            .expect("Document to be valid")
    }

    #[test]
    fn it_prints_a_group_on_a_single_line_if_it_fits() {
        let result = format(&FormatArrayElements {
            items: vec![
                &token("\"a\""),
                &token("\"b\""),
                &token("\"c\""),
                &token("\"d\""),
            ],
        });

        assert_eq!(r#"["a", "b", "c", "d"]"#, result.as_code());
    }

    #[test]
    fn it_tracks_the_indent_for_each_token() {
        let formatted = format(&format_args!(
            token("a"),
            soft_block_indent(&format_args!(
                token("b"),
                soft_block_indent(&format_args!(
                    token("c"),
                    soft_block_indent(&format_args!(token("d"), soft_line_break(), token("d"))),
                    token("c"),
                )),
                token("b"),
            )),
            token("a")
        ));

        assert_eq!(
            "a
  b
    c
      d
      d
    c
  b
a",
            formatted.as_code()
        );
    }

    #[test]
    fn it_converts_line_endings() {
        let options = PrinterOptions {
            line_ending: LineEnding::CarriageReturnLineFeed,
            ..PrinterOptions::default()
        };

        let result = format_with_options(
            &format_args![
                token("function main() {"),
                block_indent(&text("let x = `This is a multiline\nstring`;")),
                token("}"),
                hard_line_break()
            ],
            options,
        );

        assert_eq!(
            "function main() {\r\n\tlet x = `This is a multiline\r\nstring`;\r\n}\r\n",
            result.as_code()
        );
    }

    #[test]
    fn it_breaks_a_group_if_a_string_contains_a_newline() {
        let result = format(&FormatArrayElements {
            items: vec![
                &text("`This is a string spanning\ntwo lines`"),
                &token("\"b\""),
            ],
        });

        assert_eq!(
            r#"[
  `This is a string spanning
two lines`,
  "b",
]"#,
            result.as_code()
        );
    }
    #[test]
    fn it_breaks_a_group_if_it_contains_a_hard_line_break() {
        let result = format(&group(&format_args![token("a"), block_indent(&token("b"))]));

        assert_eq!("a\n  b\n", result.as_code());
    }

    #[test]
    fn it_breaks_parent_groups_if_they_dont_fit_on_a_single_line() {
        let result = format(&FormatArrayElements {
            items: vec![
                &token("\"a\""),
                &token("\"b\""),
                &token("\"c\""),
                &token("\"d\""),
                &FormatArrayElements {
                    items: vec![
                        &token("\"0123456789\""),
                        &token("\"0123456789\""),
                        &token("\"0123456789\""),
                        &token("\"0123456789\""),
                        &token("\"0123456789\""),
                    ],
                },
            ],
        });

        assert_eq!(
            r#"[
  "a",
  "b",
  "c",
  "d",
  ["0123456789", "0123456789", "0123456789", "0123456789", "0123456789"],
]"#,
            result.as_code()
        );
    }

    #[test]
    fn it_use_the_indent_character_specified_in_the_options() {
        let options = PrinterOptions {
            indent_style: IndentStyle::Tab,
            indent_width: IndentWidth::try_from(4).unwrap(),
            line_width: LineWidth::try_from(19).unwrap(),
            ..PrinterOptions::default()
        };

        let result = format_with_options(
            &FormatArrayElements {
                items: vec![&token("'a'"), &token("'b'"), &token("'c'"), &token("'d'")],
            },
            options,
        );

        assert_eq!("[\n\t'a',\n\t\'b',\n\t\'c',\n\t'd',\n]", result.as_code());
    }

    #[test]
    fn it_prints_consecutive_hard_lines_as_one() {
        let result = format(&format_args![
            token("a"),
            hard_line_break(),
            hard_line_break(),
            hard_line_break(),
            token("b"),
        ]);

        assert_eq!("a\nb", result.as_code());
    }

    #[test]
    fn it_prints_consecutive_empty_lines_as_many() {
        let result = format(&format_args![
            token("a"),
            empty_line(),
            empty_line(),
            empty_line(),
            token("b"),
        ]);

        assert_eq!("a\n\n\n\nb", result.as_code());
    }

    #[test]
    fn it_prints_consecutive_mixed_lines_as_many() {
        let result = format(&format_args![
            token("a"),
            empty_line(),
            hard_line_break(),
            empty_line(),
            hard_line_break(),
            token("b"),
        ]);

        assert_eq!("a\n\n\nb", result.as_code());
    }

    #[test]
    fn test_fill_breaks() {
        let mut state = FormatState::new(SimpleFormatContext::default());
        let mut buffer = VecBuffer::new(&mut state);
        let mut formatter = Formatter::new(&mut buffer);

        formatter
            .fill()
            // These all fit on the same line together
            .entry(
                &soft_line_break_or_space(),
                &format_args!(token("1"), token(",")),
            )
            .entry(
                &soft_line_break_or_space(),
                &format_args!(token("2"), token(",")),
            )
            .entry(
                &soft_line_break_or_space(),
                &format_args!(token("3"), token(",")),
            )
            // This one fits on a line by itself,
            .entry(
                &soft_line_break_or_space(),
                &format_args!(token("723493294"), token(",")),
            )
            // fits without breaking
            .entry(
                &soft_line_break_or_space(),
                &group(&format_args!(
                    token("["),
                    soft_block_indent(&token("5")),
                    token("],")
                )),
            )
            // this one must be printed in expanded mode to fit
            .entry(
                &soft_line_break_or_space(),
                &group(&format_args!(
                    token("["),
                    soft_block_indent(&token("123456789")),
                    token("]"),
                )),
            )
            .finish()
            .unwrap();

        let document = Document::from(buffer.into_vec());

        let printed = Printer::new(
            SourceCode::default(),
            PrinterOptions::default().with_line_width(LineWidth::try_from(10).unwrap()),
        )
        .print(&document)
        .unwrap();

        assert_eq!(
            printed.as_code(),
            "1, 2, 3,\n723493294,\n[5],\n[\n\t123456789\n]"
        );
    }

    #[test]
    fn line_suffix_printed_at_end() {
        let printed = format(&format_args![
            group(&format_args![
                token("["),
                soft_block_indent(&format_with(|f| {
                    f.fill()
                        .entry(
                            &soft_line_break_or_space(),
                            &format_args!(token("1"), token(",")),
                        )
                        .entry(
                            &soft_line_break_or_space(),
                            &format_args!(token("2"), token(",")),
                        )
                        .entry(
                            &soft_line_break_or_space(),
                            &format_args!(token("3"), if_group_breaks(&token(","))),
                        )
                        .finish()
                })),
                token("]")
            ]),
            token(";"),
            line_suffix(&format_args![space(), token("// trailing")], 0)
        ]);

        assert_eq!(printed.as_code(), "[1, 2, 3]; // trailing");
    }

    #[test]
    fn line_suffix_with_reserved_width() {
        let printed = format(&format_args![
            group(&format_args![
                token("["),
                soft_block_indent(&format_with(|f| {
                    f.fill()
                        .entry(
                            &soft_line_break_or_space(),
                            &format_args!(token("1"), token(",")),
                        )
                        .entry(
                            &soft_line_break_or_space(),
                            &format_args!(token("2"), token(",")),
                        )
                        .entry(
                            &soft_line_break_or_space(),
                            &format_args!(token("3"), if_group_breaks(&token(","))),
                        )
                        .finish()
                })),
                token("]")
            ]),
            token(";"),
            line_suffix(&format_args![space(), token("// Using reserved width causes this content to not fit even though it's a line suffix element")], 93)
        ]);

        assert_eq!(printed.as_code(), "[\n  1, 2, 3\n]; // Using reserved width causes this content to not fit even though it's a line suffix element");
    }

    #[test]
    fn conditional_with_group_id_in_fits() {
        let content = format_with(|f| {
            let group_id = f.group_id("test");
            write!(
                f,
                [
                    group(&format_args![
                        token("The referenced group breaks."),
                        hard_line_break()
                    ])
                    .with_group_id(Some(group_id)),
                    group(&format_args![
                        token("This group breaks because:"),
                        soft_line_break_or_space(),
                        if_group_fits_on_line(&token("This content fits but should not be printed.")).with_group_id(Some(group_id)),
                        if_group_breaks(&token("It measures with the 'if_group_breaks' variant because the referenced group breaks and that's just way too much text.")).with_group_id(Some(group_id)),
                    ])
                ]
            )
        });

        let printed = format(&content);

        assert_eq!(printed.as_code(), "The referenced group breaks.\nThis group breaks because:\nIt measures with the 'if_group_breaks' variant because the referenced group breaks and that's just way too much text.");
    }

    #[test]
    fn out_of_order_group_ids() {
        let content = format_with(|f| {
            let id_1 = f.group_id("id-1");
            let id_2 = f.group_id("id-2");

            write!(
                f,
                [
                    group(&token("Group with id-2")).with_group_id(Some(id_2)),
                    hard_line_break()
                ]
            )?;

            write!(
                f,
                [
                    group(&token("Group with id-1 does not fit on the line because it exceeds the line width of 80 characters by")).with_group_id(Some(id_1)),
                    hard_line_break()
                ]
            )?;

            write!(
                f,
                [
                    if_group_fits_on_line(&token("Group 2 fits")).with_group_id(Some(id_2)),
                    hard_line_break(),
                    if_group_breaks(&token("Group 1 breaks")).with_group_id(Some(id_1))
                ]
            )
        });

        let printed = format(&content);

        assert_eq!(
            printed.as_code(),
            "Group with id-2
Group with id-1 does not fit on the line because it exceeds the line width of 80 characters by
Group 2 fits
Group 1 breaks"
        );
    }

    struct FormatArrayElements<'a> {
        items: Vec<&'a dyn Format<SimpleFormatContext>>,
    }

    impl Format<SimpleFormatContext> for FormatArrayElements<'_> {
        fn fmt(&self, f: &mut Formatter<SimpleFormatContext>) -> FormatResult<()> {
            write!(
                f,
                [group(&format_args!(
                    token("["),
                    soft_block_indent(&format_args!(
                        format_with(|f| f
                            .join_with(format_args!(token(","), soft_line_break_or_space()))
                            .entries(&self.items)
                            .finish()),
                        if_group_breaks(&token(",")),
                    )),
                    token("]")
                ))]
            )
        }
    }
}
