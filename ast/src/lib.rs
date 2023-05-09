mod attributed;
mod constant;
#[cfg(feature = "fold")]
mod fold_helpers;
mod generic {
    #![allow(clippy::derive_partial_eq_without_eq)]
    include!("gen/generic.rs");
}
mod impls;
#[cfg(feature = "location")]
pub mod located {
    include!("gen/located.rs");
}
#[cfg(feature = "location")]
mod locator;
#[cfg(feature = "location")]
pub use crate::locator::locate;
#[cfg(feature = "location")]
pub use rustpython_compiler_core::SourceLocator;

#[cfg(feature = "unparse")]
mod unparse;

pub use attributed::Attributed;
pub use constant::{Constant, ConversionFlag};
pub use generic::*;

pub type Suite<U = ()> = Vec<Stmt<U>>;

pub mod location {
    pub use rustpython_compiler_core::source_code::{OneIndexed, SourceLocation};

    #[derive(Debug)]
    pub struct SourceRange {
        pub start: SourceLocation,
        pub end: Option<SourceLocation>,
    }

    impl SourceRange {
        pub fn new(start: SourceLocation, end: SourceLocation) -> Self {
            Self {
                start,
                end: Some(end),
            }
        }
        pub fn unwrap_end(&self) -> SourceLocation {
            self.end.unwrap()
        }
    }

    impl From<std::ops::Range<SourceLocation>> for SourceRange {
        fn from(value: std::ops::Range<SourceLocation>) -> Self {
            Self {
                start: value.start,
                end: Some(value.end),
            }
        }
    }
}
