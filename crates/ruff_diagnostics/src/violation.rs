use std::fmt::{Debug, Display};

#[derive(Debug, Copy, Clone)]
pub enum FixKind {
    Sometimes,
    Always,
    None,
}

impl Display for FixKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FixKind::Sometimes => write!(f, "Fix is sometimes available."),
            FixKind::Always => write!(f, "Fix is always available."),
            FixKind::None => write!(f, "Fix is not available."),
        }
    }
}

pub trait Violation: Debug + PartialEq + Eq {
    /// `None` in the case an fix is never available or otherwise Some
    /// [`FixKind`] describing the available fix.
    const FIX_KIND: FixKind = FixKind::None;

    /// The message used to describe the violation.
    fn message(&self) -> String;

    /// The explanation used in documentation and elsewhere.
    fn explanation() -> Option<&'static str> {
        None
    }

    // TODO(micha): Move `fix_title` to `Fix`, add new `advice` method that is shown as an advice.
    // Change the `Diagnostic` renderer to show the advice, and render the fix message after the `Suggested fix: <here>`

    /// Returns the title for the fix. The message is also shown as an advice as part of the diagnostics.
    ///
    /// Required for rules that have fixes.
    fn fix_title(&self) -> Option<String> {
        None
    }

    /// Returns the format strings used by [`message`](Violation::message).
    fn message_formats() -> &'static [&'static str];
}

/// This trait exists just to make implementing the [`Violation`] trait more
/// convenient for violations that can always be fixed.
pub trait AlwaysFixableViolation: Debug + PartialEq + Eq {
    /// The message used to describe the violation.
    fn message(&self) -> String;

    /// The explanation used in documentation and elsewhere.
    fn explanation() -> Option<&'static str> {
        None
    }

    /// The title displayed for the available fix.
    fn fix_title(&self) -> String;

    /// Returns the format strings used by
    /// [`message`](AlwaysFixableViolation::message).
    fn message_formats() -> &'static [&'static str];
}

/// A blanket implementation.
impl<V: AlwaysFixableViolation> Violation for V {
    const FIX_KIND: FixKind = FixKind::Always;

    fn message(&self) -> String {
        <Self as AlwaysFixableViolation>::message(self)
    }

    fn explanation() -> Option<&'static str> {
        <Self as AlwaysFixableViolation>::explanation()
    }

    fn fix_title(&self) -> Option<String> {
        Some(<Self as AlwaysFixableViolation>::fix_title(self))
    }

    fn message_formats() -> &'static [&'static str] {
        <Self as AlwaysFixableViolation>::message_formats()
    }
}
