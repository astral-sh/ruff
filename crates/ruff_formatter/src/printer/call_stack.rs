use crate::format_element::tag::TagKind;
use crate::format_element::PrintMode;
use crate::printer::stack::{Stack, StackedStack};
use crate::printer::{Indentation, MeasureMode};
use crate::{IndentStyle, InvalidDocumentError, PrintError, PrintResult};
use std::fmt::Debug;
use std::num::NonZeroU8;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(super) enum StackFrameKind {
    Root,
    Tag(TagKind),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(super) struct StackFrame {
    kind: StackFrameKind,
    args: PrintElementArgs,
}

/// Stores arguments passed to `print_element` call, holding the state specific to printing an element.
/// E.g. the `indent` depends on the token the Printer's currently processing. That's why
/// it must be stored outside of the [`PrinterState`] that stores the state common to all elements.
///
/// The state is passed by value, which is why it's important that it isn't storing any heavy
/// data structures. Such structures should be stored on the [`PrinterState`] instead.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(super) struct PrintElementArgs {
    indent: Indentation,
    mode: PrintMode,
    measure_mode: MeasureMode,
}

impl PrintElementArgs {
    pub(crate) fn new(indent: Indentation) -> Self {
        Self {
            indent,
            ..Self::default()
        }
    }

    pub(super) fn mode(self) -> PrintMode {
        self.mode
    }

    pub(super) fn measure_mode(self) -> MeasureMode {
        self.measure_mode
    }

    pub(super) fn indentation(self) -> Indentation {
        self.indent
    }

    pub(crate) fn increment_indent_level(mut self, indent_style: IndentStyle) -> Self {
        self.indent = self.indent.increment_level(indent_style);
        self
    }

    pub(crate) fn decrement_indent(mut self) -> Self {
        self.indent = self.indent.decrement();
        self
    }

    pub(crate) fn reset_indent(mut self) -> Self {
        self.indent = Indentation::default();
        self
    }

    pub(crate) fn set_indent_align(mut self, count: NonZeroU8) -> Self {
        self.indent = self.indent.set_align(count);
        self
    }

    pub(crate) fn with_print_mode(mut self, mode: PrintMode) -> Self {
        self.mode = mode;
        self
    }

    pub(crate) fn with_measure_mode(mut self, mode: MeasureMode) -> Self {
        self.measure_mode = mode;
        self
    }
}

impl Default for PrintElementArgs {
    fn default() -> Self {
        Self {
            indent: Indentation::Level(0),
            mode: PrintMode::Expanded,
            measure_mode: MeasureMode::FirstLine,
        }
    }
}

/// Call stack that stores the [`PrintElementCallArgs`].
///
/// New [`PrintElementCallArgs`] are pushed onto the stack for every [`start`](Tag::is_start) [`Tag`](FormatElement::Tag)
/// and popped when reaching the corresponding [`end`](Tag::is_end) [`Tag`](FormatElement::Tag).
pub(super) trait CallStack {
    type Stack: Stack<StackFrame> + Debug;

    fn stack(&self) -> &Self::Stack;

    fn stack_mut(&mut self) -> &mut Self::Stack;

    /// Pops the call arguments at the top and asserts that they correspond to a start tag of `kind`.
    ///
    /// Returns `Ok` with the arguments if the kind of the top stack frame matches `kind`, otherwise
    /// returns `Err`.
    fn pop(&mut self, kind: TagKind) -> PrintResult<PrintElementArgs> {
        let last = self.stack_mut().pop();

        match last {
            Some(StackFrame {
                kind: StackFrameKind::Tag(actual_kind),
                args,
            }) if actual_kind == kind => Ok(args),
            // Start / End kind don't match
            Some(StackFrame {
                kind: StackFrameKind::Tag(expected_kind),
                ..
            }) => Err(PrintError::InvalidDocument(Self::invalid_document_error(
                kind,
                Some(expected_kind),
            ))),
            // Tried to pop the outer most stack frame, which is not valid
            Some(
                frame @ StackFrame {
                    kind: StackFrameKind::Root,
                    ..
                },
            ) => {
                // Put it back in to guarantee that the stack is never empty
                self.stack_mut().push(frame);
                Err(PrintError::InvalidDocument(Self::invalid_document_error(
                    kind, None,
                )))
            }

            // This should be unreachable but having it for completeness. Happens if the stack is empty.
            None => Err(PrintError::InvalidDocument(Self::invalid_document_error(
                kind, None,
            ))),
        }
    }

    #[cold]
    fn invalid_document_error(
        end_kind: TagKind,
        start_kind: Option<TagKind>,
    ) -> InvalidDocumentError {
        match start_kind {
            None => InvalidDocumentError::StartTagMissing { kind: end_kind },
            Some(start_kind) => InvalidDocumentError::StartEndTagMismatch {
                start_kind,
                end_kind,
            },
        }
    }

    /// Returns the [`PrintElementArgs`] for the current stack frame.
    fn top(&self) -> PrintElementArgs {
        self.stack()
            .top()
            .expect("Expected `stack` to never be empty.")
            .args
    }

    /// Returns the [`TagKind`] of the current stack frame or [None] if this is the root stack frame.
    fn top_kind(&self) -> Option<TagKind> {
        match self
            .stack()
            .top()
            .expect("Expected `stack` to never be empty.")
            .kind
        {
            StackFrameKind::Root => None,
            StackFrameKind::Tag(kind) => Some(kind),
        }
    }

    /// Creates a new stack frame for a [`FormatElement::Tag`] of `kind` with `args` as the call arguments.
    fn push(&mut self, kind: TagKind, args: PrintElementArgs) {
        self.stack_mut().push(StackFrame {
            kind: StackFrameKind::Tag(kind),
            args,
        });
    }
}

/// Call stack used for printing the [`FormatElement`]s
#[derive(Debug, Clone)]
pub(super) struct PrintCallStack(Vec<StackFrame>);

impl PrintCallStack {
    pub(super) fn new(args: PrintElementArgs) -> Self {
        Self(vec![StackFrame {
            kind: StackFrameKind::Root,
            args,
        }])
    }
}

impl CallStack for PrintCallStack {
    type Stack = Vec<StackFrame>;

    fn stack(&self) -> &Self::Stack {
        &self.0
    }

    fn stack_mut(&mut self) -> &mut Self::Stack {
        &mut self.0
    }
}

/// Call stack used for measuring if some content fits on the line.
///
/// The stack is a view on top of the [`PrintCallStack`] because the stack frames are still necessary for printing.
#[must_use]
pub(super) struct FitsCallStack<'print> {
    stack: StackedStack<'print, StackFrame>,
}

impl<'print> FitsCallStack<'print> {
    pub(super) fn new(print: &'print PrintCallStack, saved: Vec<StackFrame>) -> Self {
        let stack = StackedStack::with_vec(&print.0, saved);

        Self { stack }
    }

    pub(super) fn finish(self) -> Vec<StackFrame> {
        self.stack.into_vec()
    }
}

impl<'a> CallStack for FitsCallStack<'a> {
    type Stack = StackedStack<'a, StackFrame>;

    fn stack(&self) -> &Self::Stack {
        &self.stack
    }

    fn stack_mut(&mut self) -> &mut Self::Stack {
        &mut self.stack
    }
}
