use std::fmt::{Debug, Display};

#[derive(Debug, Copy, Clone)]
pub enum FixAvailability {
    Sometimes,
    Always,
    None,
}

impl Display for FixAvailability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FixAvailability::Sometimes => write!(f, "Fix is sometimes available."),
            FixAvailability::Always => write!(f, "Fix is always available."),
            FixAvailability::None => write!(f, "Fix is not available."),
        }
    }
}

pub trait Violation: Debug + PartialEq + Eq {
    /// `None` in the case a fix is never available or otherwise Some
    /// [`FixAvailability`] describing the available fix.
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    /// The message used to describe the violation.
    fn message(&self) -> String;

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

    /// The title displayed for the available fix.
    fn fix_title(&self) -> String;

    /// Returns the format strings used by
    /// [`message`](AlwaysFixableViolation::message).
    fn message_formats() -> &'static [&'static str];
}

/// A blanket implementation.
impl<V: AlwaysFixableViolation> Violation for V {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    fn message(&self) -> String {
        <Self as AlwaysFixableViolation>::message(self)
    }

    fn fix_title(&self) -> Option<String> {
        Some(<Self as AlwaysFixableViolation>::fix_title(self))
    }

    fn message_formats() -> &'static [&'static str] {
        <Self as AlwaysFixableViolation>::message_formats()
    }
}
