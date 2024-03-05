use crate::printer::call_stack::PrintElementArgs;
use crate::FormatElement;

/// Stores the queued line suffixes.
#[derive(Debug, Default)]
pub(super) struct LineSuffixes<'a> {
    suffixes: Vec<LineSuffixEntry<'a>>,
}

impl<'a> LineSuffixes<'a> {
    /// Extends the line suffixes with `elements`, storing their call stack arguments with them.
    pub(super) fn extend<I>(&mut self, args: PrintElementArgs, elements: I)
    where
        I: IntoIterator<Item = &'a FormatElement>,
    {
        self.suffixes
            .extend(elements.into_iter().map(LineSuffixEntry::Suffix));
        self.suffixes.push(LineSuffixEntry::Args(args));
    }

    /// Takes all the pending line suffixes.
    pub(super) fn take_pending<'l>(
        &'l mut self,
    ) -> impl DoubleEndedIterator<Item = LineSuffixEntry<'a>> + 'l + ExactSizeIterator {
        self.suffixes.drain(..)
    }

    /// Returns `true` if there are any line suffixes and `false` otherwise.
    pub(super) fn has_pending(&self) -> bool {
        !self.suffixes.is_empty()
    }
}

#[derive(Debug, Copy, Clone)]
pub(super) enum LineSuffixEntry<'a> {
    /// A line suffix to print
    Suffix(&'a FormatElement),

    /// Potentially changed call arguments that should be used to format any following items.  
    Args(PrintElementArgs),
}
