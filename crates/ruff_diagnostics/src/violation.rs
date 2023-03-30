use std::fmt::{Debug, Display};

#[derive(Copy, Clone)]
pub enum AutofixKind {
    Sometimes,
    Always,
    None,
}

impl Display for AutofixKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AutofixKind::Sometimes => write!(f, "Autofix is sometimes available."),
            AutofixKind::Always => write!(f, "Autofix is always available."),
            AutofixKind::None => write!(f, "Autofix is not available."),
        }
    }
}

pub trait Violation: Debug + PartialEq + Eq {
    /// `None` in the case an autofix is never available or otherwise Some
    /// [`AutofixKind`] describing the available autofix.
    const AUTOFIX: AutofixKind = AutofixKind::None;

    /// The message used to describe the violation.
    fn message(&self) -> String;

    /// The explanation used in documentation and elsewhere.
    fn explanation() -> Option<&'static str> {
        None
    }

    /// If autofix is (potentially) available for this violation returns another
    /// function that in turn can be used to obtain a string describing the
    /// autofix.
    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        None
    }

    /// Returns the format strings used by [`message`](Violation::message).
    fn message_formats() -> &'static [&'static str];
}

/// This trait exists just to make implementing the [`Violation`] trait more
/// convenient for violations that can always be autofixed.
pub trait AlwaysAutofixableViolation: Debug + PartialEq + Eq {
    /// The message used to describe the violation.
    fn message(&self) -> String;

    /// The explanation used in documentation and elsewhere.
    fn explanation() -> Option<&'static str> {
        None
    }

    /// The title displayed for the available autofix.
    fn autofix_title(&self) -> String;

    /// Returns the format strings used by
    /// [`message`](AlwaysAutofixableViolation::message).
    fn message_formats() -> &'static [&'static str];
}

/// A blanket implementation.
impl<VA: AlwaysAutofixableViolation> Violation for VA {
    const AUTOFIX: AutofixKind = AutofixKind::Always;

    fn message(&self) -> String {
        <Self as AlwaysAutofixableViolation>::message(self)
    }

    fn explanation() -> Option<&'static str> {
        <Self as AlwaysAutofixableViolation>::explanation()
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        Some(Self::autofix_title)
    }

    fn message_formats() -> &'static [&'static str] {
        <Self as AlwaysAutofixableViolation>::message_formats()
    }
}
