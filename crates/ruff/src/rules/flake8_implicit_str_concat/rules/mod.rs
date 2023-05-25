pub(crate) use explicit::{explicit, ExplicitStringConcatenation};
pub(crate) use implicit::{
    implicit, MultiLineImplicitStringConcatenation, SingleLineImplicitStringConcatenation,
};

mod explicit;
mod implicit;
