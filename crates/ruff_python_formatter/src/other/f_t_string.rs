use ruff_python_ast::{AnyStringFlags, FString, TString};
use ruff_source_file::LineRanges;
use ruff_text_size::Ranged;

#[derive(Clone, Copy, Debug)]
pub(crate) struct FTStringContext {
    /// The string flags of the enclosing f-string part.
    enclosing_flags: AnyStringFlags,
    layout: FTStringLayout,
}

impl FTStringContext {
    pub(crate) const fn new(flags: AnyStringFlags, layout: FTStringLayout) -> Self {
        Self {
            enclosing_flags: flags,
            layout,
        }
    }

    pub(crate) fn flags(self) -> AnyStringFlags {
        self.enclosing_flags
    }

    pub(crate) const fn layout(self) -> FTStringLayout {
        self.layout
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) enum FTStringLayout {
    /// Original f-string is flat.
    /// Don't break expressions to keep the string flat.
    Flat,
    /// Original f-string has multiline expressions in the replacement fields.
    /// Allow breaking expressions across multiple lines.
    Multiline,
}

impl FTStringLayout {
    pub(crate) fn from_f_string(f_string: &FString, source: &str) -> Self {
        // Heuristic: Allow breaking the f-string expressions across multiple lines
        // only if there already is at least one multiline expression. This puts the
        // control in the hands of the user to decide if they want to break the
        // f-string expressions across multiple lines or not. This is similar to
        // how Prettier does it for template literals in JavaScript.
        //
        // If it's single quoted f-string and it contains a multiline expression, then we
        // assume that the target version of Python supports it (3.12+). If there are comments
        // used in any of the expression of the f-string, then it's always going to be multiline
        // and we assume that the target version of Python supports it (3.12+).
        //
        // Reference: https://prettier.io/docs/en/next/rationale.html#template-literals
        if f_string
            .elements
            .expressions()
            .any(|expr| source.contains_line_break(expr.range()))
        {
            Self::Multiline
        } else {
            Self::Flat
        }
    }

    pub(crate) fn from_t_string(t_string: &TString, source: &str) -> Self {
        // Heuristic: Allow breaking the f-string expressions across multiple lines
        // only if there already is at least one multiline expression. This puts the
        // control in the hands of the user to decide if they want to break the
        // f-string expressions across multiple lines or not. This is similar to
        // how Prettier does it for template literals in JavaScript.
        //
        // If it's single quoted f-string and it contains a multiline expression, then we
        // assume that the target version of Python supports it (3.12+). If there are comments
        // used in any of the expression of the f-string, then it's always going to be multiline
        // and we assume that the target version of Python supports it (3.12+).
        //
        // Reference: https://prettier.io/docs/en/next/rationale.html#template-literals
        if t_string
            .elements
            .expressions()
            .any(|expr| source.contains_line_break(expr.range()))
        {
            Self::Multiline
        } else {
            Self::Flat
        }
    }

    pub(crate) const fn is_flat(self) -> bool {
        matches!(self, FTStringLayout::Flat)
    }

    pub(crate) const fn is_multiline(self) -> bool {
        matches!(self, FTStringLayout::Multiline)
    }
}
