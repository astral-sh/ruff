use std::fmt::Debug;

use serde::de::DeserializeOwned;
use serde::Serialize;

pub trait Violation: Debug + PartialEq + Eq + Serialize + DeserializeOwned {
    /// `None` in the case an autofix is never available or otherwise Some
    /// [`AutofixKind`] describing the available autofix.
    const AUTOFIX: Option<AutofixKind> = None;

    /// The message used to describe the violation.
    fn message(&self) -> String;

    /// If autofix is (potentially) available for this violation returns another
    /// function that in turn can be used to obtain a string describing the
    /// autofix.
    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        None
    }

    /// Returns the format strings used by [`message`](Violation::message).
    fn message_formats() -> &'static [&'static str];
}

pub struct AutofixKind {
    pub available: Availability,
}

pub enum Availability {
    Sometimes,
    Always,
}

impl AutofixKind {
    pub const fn new(available: Availability) -> Self {
        Self { available }
    }
}

/// This trait exists just to make implementing the [`Violation`] trait more
/// convenient for violations that can always be autofixed.
pub trait AlwaysAutofixableViolation:
    Debug + PartialEq + Eq + Serialize + DeserializeOwned
{
    /// The message used to describe the violation.
    fn message(&self) -> String;

    /// The title displayed for the available autofix.
    fn autofix_title(&self) -> String;

    /// Returns the format strings used by
    /// [`message`](AlwaysAutofixableViolation::message).
    fn message_formats() -> &'static [&'static str];
}

/// A blanket implementation.
impl<VA: AlwaysAutofixableViolation> Violation for VA {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Always));

    fn message(&self) -> String {
        <Self as AlwaysAutofixableViolation>::message(self)
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        Some(Self::autofix_title)
    }

    fn message_formats() -> &'static [&'static str] {
        <Self as AlwaysAutofixableViolation>::message_formats()
    }
}

/// This macro just exists so that you don't have to add the `#[derive]`
/// attribute every time you define a new violation.  And so that new traits can
/// be easily derived everywhere by just changing a single line.
#[macro_export]
macro_rules! define_violation {
    ($($struct:tt)*) => {
        #[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        $($struct)*
    };
}

#[macro_export]
/// This macro exists to make it simpler to define violations that only have a
/// simple message, and no parametrization.
macro_rules! define_simple_violation {
    ($name:ident, $message:expr) => {
        $crate::define_violation!(
            pub struct $name;
        );
        impl Violation for $name {
            #[derive_message_formats]
            fn message(&self) -> String {
                format!($message)
            }
        }
    };
}

#[macro_export]
/// This macro exists to make it simpler to define violations that only have a
/// simple message, and no parametrization.
macro_rules! define_simple_autofix_violation {
    ($name:ident, $message:expr, $autofix_title:expr) => {
        $crate::define_violation!(
            pub struct $name;
        );
        impl AlwaysAutofixableViolation for $name {
            #[derive_message_formats]
            fn message(&self) -> String {
                format!($message)
            }
            fn autofix_title(&self) -> String {
                $autofix_title.to_string()
            }
        }
    };
}
