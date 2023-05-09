mod attributed;
mod constant;
#[cfg(feature = "fold")]
mod fold_helpers;
mod generic {
    #![allow(clippy::derive_partial_eq_without_eq)]
    include!("gen/generic.rs");
}
mod impls;
#[cfg(feature = "source-code")]
mod source_locator;
#[cfg(feature = "unparse")]
mod unparse;

pub use attributed::Attributed;
pub use constant::Constant;
pub use generic::*;
pub use rustpython_parser_core::{text_size, ConversionFlag};

pub type Suite<U = ()> = Vec<Stmt<U>>;

#[cfg(feature = "source-code")]
pub mod located {
    include!("gen/located.rs");
}

pub use rustpython_parser_core::source_code;
