use std::fmt::Debug;
use std::marker::PhantomData;

use ruff_python_ast::PySourceType;

use crate::{AsMode, Mode};

/// A trait for marking whether the source type of a Python file is known for [`ParserOptions`].
///
/// This is important for the safety of
/// [`parse_unchecked_source`](crate::parse_unchecked_source).
pub trait SourceType: std::fmt::Debug {}

/// The [`ParserOptions`] have an unknown source type, i.e. they were constructed by
/// [`ParserOptions::from_mode`].
#[derive(Debug)]
pub struct UnknownSource;

impl SourceType for UnknownSource {}

/// The [`ParserOptions`] have a known source type, i.e. they were constructed by
/// [`ParserOptions::from_source_type`].
#[derive(Debug)]
pub struct KnownSource;

impl SourceType for KnownSource {}

pub(crate) trait AsParserOptions: std::fmt::Debug {
    fn mode(&self) -> Mode;
}

impl<S: SourceType> AsParserOptions for ParserOptions<S> {
    fn mode(&self) -> Mode {
        self.mode
    }
}

#[derive(Debug)]
pub struct ParserOptions<S: SourceType> {
    /// Specify the mode in which the code will be parsed.
    pub(crate) mode: Mode,
    pub(crate) _type: PhantomData<S>,
}

impl ParserOptions<UnknownSource> {
    pub fn from_mode(mode: Mode) -> Self {
        Self {
            mode,
            _type: PhantomData,
        }
    }
}

impl ParserOptions<KnownSource> {
    pub fn from_source_type(source_type: PySourceType) -> Self {
        Self {
            mode: source_type.as_mode(),
            _type: PhantomData,
        }
    }
}
