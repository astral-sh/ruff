mod call_stack;
mod line_suffixes;
mod printer_options;
mod queue;
mod stack;

use crate::format_element::document::Document;
use crate::format_element::tag::{Condition, GroupMode};
use crate::format_element::{BestFittingVariants, LineMode, PrintMode};
use crate::prelude::tag;
use crate::prelude::tag::{DedentMode, Tag, TagKind, VerbatimKind};
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
use drop_bomb::DebugDropBomb;
pub use printer_options::*;
use ruff_text_size::{TextLen, TextSize};
use std::num::NonZeroU8;
use unicode_width::UnicodeWidthChar;

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
            state: PrinterState::default(),
        }
    }

    /// Prints the passed in element as well as all its content
    pub fn print(self, document: &'a Document) -> PrintResult<Printed> {
        self.print_with_indent(document, 0)
    }

    /// Prints the passed in element as well as all its content,
    /// starting at the specified indentation level
    pub fn print_with_indent(
        mut self,
        document: &'a Document,
        indent: u16,
    ) -> PrintResult<Printed> {
        tracing::debug_span!("Printer::print").in_scope(move || {
            let mut stack = PrintCallStack::new(PrintElementArgs::new(Indention::Level(indent)));
            let mut queue: PrintQueue<'a> = PrintQueue::new(document.as_ref());

            while let Some(element) = queue.pop() {
                self.print_element(&mut stack, &mut queue, element)?;

                if queue.is_empty() {
                    self.flush_line_suffixes(&mut queue, &mut stack, None);
                }
            }

            Ok(Printed::new(
                self.state.buffer,
                None,
                self.state.source_markers,
                self.state.verbatim_markers,
            ))
        })
    }

    /// Prints a single element and push the following elements to queue
    fn print_element(
        &mut self,
        stack: &mut PrintCallStack,
        queue: &mut PrintQueue<'a>,
        element: &'a FormatElement,
    ) -> PrintResult<()> {
        use Tag::*;

        let args = stack.top();

        match element {
            FormatElement::Space => self.print_text(" ", None),
            FormatElement::StaticText { text } => self.print_text(text, None),
            FormatElement::DynamicText { text } => self.print_text(text, None),
            FormatElement::SourceCodeSlice { slice, .. } => {
                let text = slice.text(self.source_code);
                self.print_text(text, Some(slice.range()))
            }
            FormatElement::Line(line_mode) => {
                if args.mode().is_flat()
                    && matches!(line_mode, LineMode::Soft | LineMode::SoftOrSpace)
                {
                    if line_mode == &LineMode::SoftOrSpace {
                        self.print_text(" ", None);
                    }
                } else if self.state.line_suffixes.has_pending() {
                    self.flush_line_suffixes(queue, stack, Some(element));
                } else {
                    // Only print a newline if the current line isn't already empty
                    if self.state.line_width > 0 {
                        self.print_str("\n");
                    }

                    // Print a second line break if this is an empty line
                    if line_mode == &LineMode::Empty {
                        self.print_str("\n");
                    }

                    self.state.pending_space = false;
                    self.state.pending_indent = args.indention();
                }
            }

            FormatElement::ExpandParent => {
                // Handled in `Document::propagate_expands()
            }

            FormatElement::SourcePosition(position) => {
                self.state.source_position = *position;
                self.push_marker();
            }

            FormatElement::LineSuffixBoundary => {
                const HARD_BREAK: &FormatElement = &FormatElement::Line(LineMode::Hard);
                self.flush_line_suffixes(queue, stack, Some(HARD_BREAK));
            }

            FormatElement::BestFitting { variants } => {
                self.print_best_fitting(variants, queue, stack)?;
            }

            FormatElement::Interned(content) => {
                queue.extend_back(content);
            }

            FormatElement::Tag(StartGroup(group)) => {
                let print_mode =
                    self.print_group(TagKind::Group, group.mode(), args, queue, stack)?;

                if let Some(id) = group.id() {
                    self.state.group_modes.insert_print_mode(id, print_mode);
                }
            }

            FormatElement::Tag(StartConditionalGroup(group)) => {
                let condition = group.condition();
                let expected_mode = match condition.group_id {
                    None => args.mode(),
                    Some(id) => self.state.group_modes.unwrap_print_mode(id, element),
                };

                if expected_mode == condition.mode {
                    self.print_group(TagKind::ConditionalGroup, group.mode(), args, queue, stack)?;
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
                    Some(id) => self.state.group_modes.unwrap_print_mode(*id, element),
                };

                if *mode == group_mode {
                    stack.push(TagKind::ConditionalContent, args);
                } else {
                    queue.skip_content(TagKind::ConditionalContent);
                }
            }

            FormatElement::Tag(StartIndentIfGroupBreaks(group_id)) => {
                let group_mode = self.state.group_modes.unwrap_print_mode(*group_id, element);

                let args = match group_mode {
                    PrintMode::Flat => args,
                    PrintMode::Expanded => args.increment_indent_level(self.options.indent_style),
                };

                stack.push(TagKind::IndentIfGroupBreaks, args);
            }

            FormatElement::Tag(StartLineSuffix) => {
                self.state
                    .line_suffixes
                    .extend(args, queue.iter_content(TagKind::LineSuffix));
            }

            FormatElement::Tag(StartVerbatim(kind)) => {
                if let VerbatimKind::Verbatim { length } = kind {
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
                            Some(group_id) => {
                                self.state.group_modes.unwrap_print_mode(group_id, element)
                            }
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

            FormatElement::Tag(tag @ (StartLabelled(_) | StartEntry)) => {
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
                | EndFill),
            ) => {
                stack.pop(tag.kind())?;
            }
        };

        Ok(())
    }

    fn fits(&mut self, queue: &PrintQueue<'a>, stack: &PrintCallStack) -> PrintResult<bool> {
        let mut measure = FitsMeasurer::new(queue, stack, self);
        let result = measure.fits(&mut AllPredicate);
        measure.finish();
        result
    }

    fn print_group(
        &mut self,
        kind: TagKind,
        mode: GroupMode,
        args: PrintElementArgs,
        queue: &mut PrintQueue<'a>,
        stack: &mut PrintCallStack,
    ) -> PrintResult<PrintMode> {
        let group_mode = match mode {
            GroupMode::Expand | GroupMode::Propagated => PrintMode::Expanded,
            GroupMode::Flat => {
                match args.mode() {
                    PrintMode::Flat if self.state.measured_group_fits => {
                        // A parent group has already verified that this group fits on a single line
                        // Thus, just continue in flat mode
                        PrintMode::Flat
                    }
                    // The printer is either in expanded mode or it's necessary to re-measure if the group fits
                    // because the printer printed a line break
                    _ => {
                        self.state.measured_group_fits = true;

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
                }
            }
        };

        stack.push(kind, args.with_print_mode(group_mode));

        Ok(group_mode)
    }

    fn print_text(&mut self, text: &str, source_range: Option<TextRange>) {
        if !self.state.pending_indent.is_empty() {
            let (indent_char, repeat_count) = match self.options.indent_style() {
                IndentStyle::Tab => ('\t', 1),
                IndentStyle::Space(count) => (' ', count),
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

        // Insert source map markers before and after the token
        //
        // If the token has source position information the start marker
        // will use the start position of the original token, and the end
        // marker will use that position + the text length of the token
        //
        // If the token has no source position (was created by the formatter)
        // both the start and end marker will use the last known position
        // in the input source (from state.source_position)
        if let Some(range) = source_range {
            self.state.source_position = range.start();
        }

        self.push_marker();

        self.print_str(text);

        if let Some(range) = source_range {
            self.state.source_position = range.end();
        }

        self.push_marker();
    }

    fn push_marker(&mut self) {
        let marker = SourceMarker {
            source: self.state.source_position,
            dest: self.state.buffer.text_len(),
        };

        if let Some(last) = self.state.source_markers.last() {
            if last != &marker {
                self.state.source_markers.push(marker)
            }
        } else {
            self.state.source_markers.push(marker);
        }
    }

    fn flush_line_suffixes(
        &mut self,
        queue: &mut PrintQueue<'a>,
        stack: &mut PrintCallStack,
        line_break: Option<&'a FormatElement>,
    ) {
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
                        stack.push(TagKind::LineSuffix, args);
                        const LINE_SUFFIX_END: &FormatElement =
                            &FormatElement::Tag(Tag::EndLineSuffix);

                        queue.push(LINE_SUFFIX_END);
                    }
                }
            }
        }
    }

    fn print_best_fitting(
        &mut self,
        variants: &'a BestFittingVariants,
        queue: &mut PrintQueue<'a>,
        stack: &mut PrintCallStack,
    ) -> PrintResult<()> {
        let args = stack.top();

        if args.mode().is_flat() && self.state.measured_group_fits {
            queue.extend_back(variants.most_flat());
            self.print_entry(queue, stack, args)
        } else {
            self.state.measured_group_fits = true;
            let normal_variants = &variants[..variants.len() - 1];

            for variant in normal_variants.iter() {
                // Test if this variant fits and if so, use it. Otherwise try the next
                // variant.

                // Try to fit only the first variant on a single line
                if !matches!(variant.first(), Some(&FormatElement::Tag(Tag::StartEntry))) {
                    return invalid_start_tag(TagKind::Entry, variant.first());
                }

                // Skip the first element because we want to override the args for the entry and the
                // args must be popped from the stack as soon as it sees the matching end entry.
                let content = &variant[1..];

                let entry_args = args.with_print_mode(PrintMode::Flat);

                queue.extend_back(content);
                stack.push(TagKind::Entry, entry_args);
                let variant_fits = self.fits(queue, stack)?;
                stack.pop(TagKind::Entry)?;

                // Remove the content slice because printing needs the variant WITH the start entry
                let popped_slice = queue.pop_slice();
                debug_assert_eq!(popped_slice, Some(content));

                if variant_fits {
                    queue.extend_back(variant);
                    return self.print_entry(queue, stack, args.with_print_mode(PrintMode::Flat));
                }
            }

            // No variant fits, take the last (most expanded) as fallback
            let most_expanded = variants.most_expanded();
            queue.extend_back(most_expanded);
            self.print_entry(queue, stack, args.with_print_mode(PrintMode::Expanded))
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

    /// Semantic alias for [Self::print_entry] for fill items.
    fn print_fill_item(
        &mut self,
        queue: &mut PrintQueue<'a>,
        stack: &mut PrintCallStack,
        args: PrintElementArgs,
    ) -> PrintResult<()> {
        self.print_entry(queue, stack, args)
    }

    /// Semantic alias for [Self::print_entry] for fill separators.
    fn print_fill_separator(
        &mut self,
        queue: &mut PrintQueue<'a>,
        stack: &mut PrintCallStack,
        args: PrintElementArgs,
    ) -> PrintResult<()> {
        self.print_entry(queue, stack, args)
    }

    /// Fully print an element (print the element itself and all its descendants)
    ///
    /// Unlike [print_element], this function ensures the entire element has
    /// been printed when it returns and the queue is back to its original state
    fn print_entry(
        &mut self,
        queue: &mut PrintQueue<'a>,
        stack: &mut PrintCallStack,
        args: PrintElementArgs,
    ) -> PrintResult<()> {
        let start_entry = queue.top();

        if !matches!(start_entry, Some(&FormatElement::Tag(Tag::StartEntry))) {
            return invalid_start_tag(TagKind::Entry, start_entry);
        }

        let mut depth = 0;

        while let Some(element) = queue.pop() {
            match element {
                FormatElement::Tag(Tag::StartEntry) => {
                    // Handle the start of the first element by pushing the args on the stack.
                    if depth == 0 {
                        depth = 1;
                        stack.push(TagKind::Entry, args);
                        continue;
                    }

                    depth += 1;
                }
                FormatElement::Tag(Tag::EndEntry) => {
                    depth -= 1;
                    // Reached the end entry, pop the entry from the stack and return.
                    if depth == 0 {
                        stack.pop(TagKind::Entry)?;
                        return Ok(());
                    }
                }
                _ => {
                    // Fall through
                }
            }

            self.print_element(stack, queue, element)?;
        }

        invalid_end_tag(TagKind::Entry, stack.top_kind())
    }

    fn print_str(&mut self, content: &str) {
        for char in content.chars() {
            self.print_char(char);
        }
    }

    fn print_char(&mut self, char: char) {
        if char == '\n' {
            self.state
                .buffer
                .push_str(self.options.line_ending.as_str());

            self.state.generated_line += 1;
            self.state.generated_column = 0;
            self.state.line_width = 0;

            // Fit's only tests if groups up to the first line break fit.
            // The next group must re-measure if it still fits.
            self.state.measured_group_fits = false;
        } else {
            self.state.buffer.push(char);
            self.state.generated_column += 1;

            let char_width = if char == '\t' {
                self.options.tab_width as usize
            } else {
                char.width().unwrap_or(0)
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
    buffer: String,
    source_markers: Vec<SourceMarker>,
    source_position: TextSize,
    pending_indent: Indention,
    pending_space: bool,
    measured_group_fits: bool,
    generated_line: usize,
    generated_column: usize,
    line_width: usize,
    line_suffixes: LineSuffixes<'a>,
    verbatim_markers: Vec<TextRange>,
    group_modes: GroupModes,
    // Re-used queue to measure if a group fits. Optimisation to avoid re-allocating a new
    // vec every time a group gets measured
    fits_stack: Vec<StackFrame>,
    fits_queue: Vec<&'a [FormatElement]>,
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

    fn get_print_mode(&self, group_id: GroupId) -> Option<PrintMode> {
        let index = u32::from(group_id) as usize;
        self.0
            .get(index)
            .and_then(|option| option.as_ref().copied())
    }

    fn unwrap_print_mode(&self, group_id: GroupId, next_element: &FormatElement) -> PrintMode {
        self.get_print_mode(group_id).unwrap_or_else(|| {
            panic!("Expected group with id {group_id:?} to exist but it wasn't present in the document. Ensure that a group with such a document appears in the document before the element {next_element:?}.")
        })
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum Indention {
    /// Indent the content by `count` levels by using the indention sequence specified by the printer options.
    Level(u16),

    /// Indent the content by n-`level`s using the indention sequence specified by the printer options and `align` spaces.
    Align { level: u16, align: NonZeroU8 },
}

impl Indention {
    const fn is_empty(&self) -> bool {
        matches!(self, Indention::Level(0))
    }

    /// Creates a new indention level with a zero-indent.
    const fn new() -> Self {
        Indention::Level(0)
    }

    /// Returns the indention level
    fn level(&self) -> u16 {
        match self {
            Indention::Level(count) => *count,
            Indention::Align { level: indent, .. } => *indent,
        }
    }

    /// Returns the number of trailing align spaces or 0 if none
    fn align(&self) -> u8 {
        match self {
            Indention::Level(_) => 0,
            Indention::Align { align, .. } => (*align).into(),
        }
    }

    /// Increments the level by one.
    ///
    /// The behaviour depends on the [`indent_style`][IndentStyle] if this is an [Indent::Align]:
    /// - **Tabs**: `align` is converted into an indent. This results in `level` increasing by two: once for the align, once for the level increment
    /// - **Spaces**: Increments the `level` by one and keeps the `align` unchanged.
    /// Keeps any  the current value is [Indent::Align] and increments the level by one.
    fn increment_level(self, indent_style: IndentStyle) -> Self {
        match self {
            Indention::Level(count) => Indention::Level(count + 1),
            // Increase the indent AND convert the align to an indent
            Indention::Align { level, .. } if indent_style.is_tab() => Indention::Level(level + 2),
            Indention::Align {
                level: indent,
                align,
            } => Indention::Align {
                level: indent + 1,
                align,
            },
        }
    }

    /// Decrements the indent by one by:
    /// - Reducing the level by one if this is [Indent::Level]
    /// - Removing the `align` if this is [Indent::Align]
    ///
    /// No-op if the level is already zero.
    fn decrement(self) -> Self {
        match self {
            Indention::Level(level) => Indention::Level(level.saturating_sub(1)),
            Indention::Align { level, .. } => Indention::Level(level),
        }
    }

    /// Adds an `align` of `count` spaces to the current indention.
    ///
    /// It increments the `level` value if the current value is [Indent::IndentAlign].
    fn set_align(self, count: NonZeroU8) -> Self {
        match self {
            Indention::Level(indent_count) => Indention::Align {
                level: indent_count,
                align: count,
            },

            // Convert the existing align to an indent
            Indention::Align { level: indent, .. } => Indention::Align {
                level: indent + 1,
                align: count,
            },
        }
    }
}

impl Default for Indention {
    fn default() -> Self {
        Indention::new()
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

impl<'a, 'print> FitsMeasurer<'a, 'print> {}

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

    /// Tests if the content of a `Fill` item fits in [PrintMode::Flat].
    ///
    /// Returns `Err` if the top element of the queue is not a [Tag::StartEntry]
    /// or if the document has any mismatching start/end tags.
    fn fill_item_fits(&mut self) -> PrintResult<bool> {
        self.fill_entry_fits(PrintMode::Flat)
    }

    /// Tests if the content of a `Fill` separator fits with `mode`.
    ///
    /// Returns `Err` if the top element of the queue is not a [Tag::StartEntry]
    /// or if the document has any mismatching start/end tags.
    fn fill_separator_fits(&mut self, mode: PrintMode) -> PrintResult<bool> {
        self.fill_entry_fits(mode)
    }

    /// Tests if the elements between the [Tag::StartEntry] and [Tag::EndEntry]
    /// of a fill item or separator fits with `mode`.
    ///
    /// Returns `Err` if the queue isn't positioned at a [Tag::StartEntry] or if
    /// the matching [Tag::EndEntry] is missing.
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
        use Tag::*;

        let args = self.stack.top();

        match element {
            FormatElement::Space => return Ok(self.fits_text(" ")),

            FormatElement::Line(line_mode) => {
                match args.mode() {
                    PrintMode::Flat => match line_mode {
                        LineMode::SoftOrSpace => return Ok(self.fits_text(" ")),
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
                            MeasureMode::AllLines => {
                                // Continue measuring on the next line
                                self.state.line_width = 0;
                            }
                        }
                    }
                }
            }

            FormatElement::StaticText { text } => return Ok(self.fits_text(text)),
            FormatElement::DynamicText { text, .. } => return Ok(self.fits_text(text)),
            FormatElement::SourceCodeSlice { slice, .. } => {
                let text = slice.text(self.printer.source_code);
                return Ok(self.fits_text(text));
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

            FormatElement::BestFitting { variants } => {
                let slice = match args.mode() {
                    PrintMode::Flat => variants.most_flat(),
                    PrintMode::Expanded => variants.most_expanded(),
                };

                if !matches!(slice.first(), Some(FormatElement::Tag(Tag::StartEntry))) {
                    return invalid_start_tag(TagKind::Entry, slice.first());
                }

                self.stack.push(TagKind::Entry, args);
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
                return self.fits_group(TagKind::Group, group.mode(), group.id(), args);
            }

            FormatElement::Tag(StartConditionalGroup(group)) => {
                let condition = group.condition();

                let print_mode = match condition.group_id {
                    None => args.mode(),
                    Some(group_id) => self
                        .group_modes()
                        .get_print_mode(group_id)
                        .unwrap_or_else(|| args.mode()),
                };

                if condition.mode == print_mode {
                    return self.fits_group(TagKind::ConditionalGroup, group.mode(), None, args);
                } else {
                    self.stack.push(TagKind::ConditionalGroup, args);
                }
            }

            FormatElement::Tag(StartConditionalContent(condition)) => {
                let print_mode = match condition.group_id {
                    None => args.mode(),
                    Some(group_id) => self
                        .group_modes()
                        .get_print_mode(group_id)
                        .unwrap_or_else(|| args.mode()),
                };

                if condition.mode == print_mode {
                    self.stack.push(TagKind::ConditionalContent, args);
                } else {
                    self.queue.skip_content(TagKind::ConditionalContent);
                }
            }

            FormatElement::Tag(StartIndentIfGroupBreaks(id)) => {
                let print_mode = self
                    .group_modes()
                    .get_print_mode(*id)
                    .unwrap_or_else(|| args.mode());

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

            FormatElement::Tag(StartLineSuffix) => {
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
                let condition_met = match condition {
                    Some(condition) => {
                        let group_mode = match condition.group_id {
                            Some(group_id) => self
                                .group_modes()
                                .get_print_mode(group_id)
                                .unwrap_or_else(|| args.mode()),
                            None => args.mode(),
                        };

                        condition.mode == group_mode
                    }
                    None => true,
                };

                if condition_met {
                    // Measure in fully expanded mode.
                    self.stack.push(
                        TagKind::FitsExpanded,
                        args.with_print_mode(PrintMode::Expanded)
                            .with_measure_mode(MeasureMode::AllLines),
                    )
                } else {
                    if propagate_expand.get() && args.mode().is_flat() {
                        return Ok(Fits::No);
                    }

                    // As usual
                    self.stack.push(TagKind::FitsExpanded, args)
                }
            }

            FormatElement::Tag(
                tag @ (StartFill | StartVerbatim(_) | StartLabelled(_) | StartEntry),
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
        mode: GroupMode,
        id: Option<GroupId>,
        args: PrintElementArgs,
    ) -> PrintResult<Fits> {
        if self.must_be_flat && !mode.is_flat() {
            return Ok(Fits::No);
        }

        // Continue printing groups in expanded mode if measuring a `best_fitting` element where
        // a group expands.
        let print_mode = if !mode.is_flat() {
            PrintMode::Expanded
        } else {
            args.mode()
        };

        self.stack.push(kind, args.with_print_mode(print_mode));

        if let Some(id) = id {
            self.group_modes_mut().insert_print_mode(id, print_mode);
        }

        Ok(Fits::Maybe)
    }

    fn fits_text(&mut self, text: &str) -> Fits {
        let indent = std::mem::take(&mut self.state.pending_indent);
        self.state.line_width += indent.level() as usize * self.options().indent_width() as usize
            + indent.align() as usize;

        for c in text.chars() {
            let char_width = match c {
                '\t' => self.options().tab_width as usize,
                '\n' => {
                    return if self.must_be_flat {
                        Fits::No
                    } else {
                        Fits::Yes
                    };
                }
                c => c.width().unwrap_or(0),
            };
            self.state.line_width += char_width;
        }

        if self.state.line_width > self.options().print_width.into() {
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
        match value {
            true => Fits::Yes,
            false => Fits::No,
        }
    }
}

/// State used when measuring if a group fits on a single line
#[derive(Debug)]
struct FitsState {
    pending_indent: Indention,
    has_line_suffix: bool,
    line_width: usize,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum MeasureMode {
    /// The content fits if a hard line break or soft line break in [`PrintMode::Expanded`] is seen
    /// before exceeding the configured print width.
    /// Returns
    FirstLine,

    /// The content only fits if non of the lines exceed the print width. Lines are terminated by either
    /// a hard line break or a soft line break in [`PrintMode::Expanded`].
    AllLines,
}

#[cfg(test)]
mod tests {
    use crate::prelude::*;
    use crate::printer::{LineEnding, PrintWidth, Printer, PrinterOptions};
    use crate::source_code::SourceCode;
    use crate::{format_args, write, Document, FormatState, IndentStyle, Printed, VecBuffer};

    fn format(root: &dyn Format<SimpleFormatContext>) -> Printed {
        format_with_options(
            root,
            PrinterOptions {
                indent_style: IndentStyle::Space(2),
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
                &text("\"a\""),
                &text("\"b\""),
                &text("\"c\""),
                &text("\"d\""),
            ],
        });

        assert_eq!(r#"["a", "b", "c", "d"]"#, result.as_code())
    }

    #[test]
    fn it_tracks_the_indent_for_each_token() {
        let formatted = format(&format_args!(
            text("a"),
            soft_block_indent(&format_args!(
                text("b"),
                soft_block_indent(&format_args!(
                    text("c"),
                    soft_block_indent(&format_args!(text("d"), soft_line_break(), text("d"),)),
                    text("c"),
                )),
                text("b"),
            )),
            text("a")
        ));

        assert_eq!(
            r#"a
  b
    c
      d
      d
    c
  b
a"#,
            formatted.as_code()
        )
    }

    #[test]
    fn it_converts_line_endings() {
        let options = PrinterOptions {
            line_ending: LineEnding::CarriageReturnLineFeed,
            ..PrinterOptions::default()
        };

        let result = format_with_options(
            &format_args![
                text("function main() {"),
                block_indent(&text("let x = `This is a multiline\nstring`;")),
                text("}"),
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
                &text("\"b\""),
            ],
        });

        assert_eq!(
            r#"[
  `This is a string spanning
two lines`,
  "b",
]"#,
            result.as_code()
        )
    }
    #[test]
    fn it_breaks_a_group_if_it_contains_a_hard_line_break() {
        let result = format(&group(&format_args![text("a"), block_indent(&text("b"))]));

        assert_eq!("a\n  b\n", result.as_code())
    }

    #[test]
    fn it_breaks_parent_groups_if_they_dont_fit_on_a_single_line() {
        let result = format(&FormatArrayElements {
            items: vec![
                &text("\"a\""),
                &text("\"b\""),
                &text("\"c\""),
                &text("\"d\""),
                &FormatArrayElements {
                    items: vec![
                        &text("\"0123456789\""),
                        &text("\"0123456789\""),
                        &text("\"0123456789\""),
                        &text("\"0123456789\""),
                        &text("\"0123456789\""),
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
            tab_width: 4,
            print_width: PrintWidth::new(19),
            ..PrinterOptions::default()
        };

        let result = format_with_options(
            &FormatArrayElements {
                items: vec![&text("'a'"), &text("'b'"), &text("'c'"), &text("'d'")],
            },
            options,
        );

        assert_eq!("[\n\t'a',\n\t\'b',\n\t\'c',\n\t'd',\n]", result.as_code());
    }

    #[test]
    fn it_prints_consecutive_hard_lines_as_one() {
        let result = format(&format_args![
            text("a"),
            hard_line_break(),
            hard_line_break(),
            hard_line_break(),
            text("b"),
        ]);

        assert_eq!("a\nb", result.as_code())
    }

    #[test]
    fn it_prints_consecutive_empty_lines_as_many() {
        let result = format(&format_args![
            text("a"),
            empty_line(),
            empty_line(),
            empty_line(),
            text("b"),
        ]);

        assert_eq!("a\n\n\n\nb", result.as_code())
    }

    #[test]
    fn it_prints_consecutive_mixed_lines_as_many() {
        let result = format(&format_args![
            text("a"),
            empty_line(),
            hard_line_break(),
            empty_line(),
            hard_line_break(),
            text("b"),
        ]);

        assert_eq!("a\n\n\nb", result.as_code())
    }

    #[test]
    fn test_fill_breaks() {
        let mut state = FormatState::new(());
        let mut buffer = VecBuffer::new(&mut state);
        let mut formatter = Formatter::new(&mut buffer);

        formatter
            .fill()
            // These all fit on the same line together
            .entry(
                &soft_line_break_or_space(),
                &format_args!(text("1"), text(",")),
            )
            .entry(
                &soft_line_break_or_space(),
                &format_args!(text("2"), text(",")),
            )
            .entry(
                &soft_line_break_or_space(),
                &format_args!(text("3"), text(",")),
            )
            // This one fits on a line by itself,
            .entry(
                &soft_line_break_or_space(),
                &format_args!(text("723493294"), text(",")),
            )
            // fits without breaking
            .entry(
                &soft_line_break_or_space(),
                &group(&format_args!(
                    text("["),
                    soft_block_indent(&text("5")),
                    text("],")
                )),
            )
            // this one must be printed in expanded mode to fit
            .entry(
                &soft_line_break_or_space(),
                &group(&format_args!(
                    text("["),
                    soft_block_indent(&text("123456789")),
                    text("]"),
                )),
            )
            .finish()
            .unwrap();

        let document = Document::from(buffer.into_vec());

        let printed = Printer::new(
            SourceCode::default(),
            PrinterOptions::default().with_print_width(PrintWidth::new(10)),
        )
        .print(&document)
        .unwrap();

        assert_eq!(
            printed.as_code(),
            "1, 2, 3,\n723493294,\n[5],\n[\n\t123456789\n]"
        )
    }

    #[test]
    fn line_suffix_printed_at_end() {
        let printed = format(&format_args![
            group(&format_args![
                text("["),
                soft_block_indent(&format_with(|f| {
                    f.fill()
                        .entry(
                            &soft_line_break_or_space(),
                            &format_args!(text("1"), text(",")),
                        )
                        .entry(
                            &soft_line_break_or_space(),
                            &format_args!(text("2"), text(",")),
                        )
                        .entry(
                            &soft_line_break_or_space(),
                            &format_args!(text("3"), if_group_breaks(&text(","))),
                        )
                        .finish()
                })),
                text("]")
            ]),
            text(";"),
            &line_suffix(&format_args![space(), text("// trailing")])
        ]);

        assert_eq!(printed.as_code(), "[1, 2, 3]; // trailing")
    }

    #[test]
    fn conditional_with_group_id_in_fits() {
        let content = format_with(|f| {
            let group_id = f.group_id("test");
            write!(
                f,
                [
                    group(&format_args![
                        text("The referenced group breaks."),
                        hard_line_break()
                    ])
                    .with_group_id(Some(group_id)),
                    group(&format_args![
                        text("This group breaks because:"),
                        soft_line_break_or_space(),
                        if_group_fits_on_line(&text("This content fits but should not be printed.")).with_group_id(Some(group_id)),
                        if_group_breaks(&text("It measures with the 'if_group_breaks' variant because the referenced group breaks and that's just way too much text.")).with_group_id(Some(group_id)),
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
                    group(&text("Group with id-2")).with_group_id(Some(id_2)),
                    hard_line_break()
                ]
            )?;

            write!(
                f,
                [
                    group(&text("Group with id-1 does not fit on the line because it exceeds the line width of 80 characters by")).with_group_id(Some(id_1)),
                    hard_line_break()
                ]
            )?;

            write!(
                f,
                [
                    if_group_fits_on_line(&text("Group 2 fits")).with_group_id(Some(id_2)),
                    hard_line_break(),
                    if_group_breaks(&text("Group 1 breaks")).with_group_id(Some(id_1))
                ]
            )
        });

        let printed = format(&content);

        assert_eq!(
            printed.as_code(),
            r#"Group with id-2
Group with id-1 does not fit on the line because it exceeds the line width of 80 characters by
Group 2 fits
Group 1 breaks"#
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
                    text("["),
                    soft_block_indent(&format_args!(
                        format_with(|f| f
                            .join_with(format_args!(text(","), soft_line_break_or_space()))
                            .entries(&self.items)
                            .finish()),
                        if_group_breaks(&text(",")),
                    )),
                    text("]")
                ))]
            )
        }
    }
}
