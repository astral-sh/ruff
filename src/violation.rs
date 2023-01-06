use std::fmt::Debug;

use serde::de::DeserializeOwned;
use serde::Serialize;

pub trait Violation: Debug + PartialEq + Eq + Serialize + DeserializeOwned {
    /// The message used to describe the violation.
    fn message(&self) -> String;

    /// If autofix is (potentially) available for this violation returns another
    /// function that in turn can be used to obtain a string describing the
    /// autofix.
    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        None
    }

    /// A placeholder instance of the violation.
    fn placeholder() -> Self;
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

    /// A placeholder instance of the violation.
    fn placeholder() -> Self;
}

/// A blanket implementation.
impl<VA: AlwaysAutofixableViolation> Violation for VA {
    fn message(&self) -> String {
        <Self as AlwaysAutofixableViolation>::message(self)
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        Some(Self::autofix_title)
    }

    fn placeholder() -> Self {
        <Self as AlwaysAutofixableViolation>::placeholder()
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
