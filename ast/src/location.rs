//! Datatypes to support source location information.

use std::fmt;

/// A location somewhere in the sourcecode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Location(rustpython_compiler_core::Location);

impl std::ops::Deref for Location {
    type Target = rustpython_compiler_core::Location;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Location {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "line {} column {}", self.row(), self.column())
    }
}

impl Location {
    pub fn visualize<'a>(
        &self,
        line: &'a str,
        desc: impl fmt::Display + 'a,
    ) -> impl fmt::Display + 'a {
        struct Visualize<'a, D: fmt::Display> {
            loc: Location,
            line: &'a str,
            desc: D,
        }
        impl<D: fmt::Display> fmt::Display for Visualize<'_, D> {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(
                    f,
                    "{}\n{}{arrow:>pad$}",
                    self.desc,
                    self.line,
                    pad = self.loc.column(),
                    arrow = "^",
                )
            }
        }
        Visualize {
            loc: *self,
            line,
            desc,
        }
    }
}

impl Location {
    pub fn new(row: usize, column: usize) -> Self {
        Location(rustpython_compiler_core::Location::new(row, column))
    }
}
