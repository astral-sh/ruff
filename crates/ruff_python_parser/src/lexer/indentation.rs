use static_assertions::assert_eq_size;
use std::cmp::Ordering;
use std::fmt::Debug;

/// The column index of an indentation.
///
/// A space increments the column by one. A tab adds up to 2 (if tab size is 2) indices, but just one
/// if the column isn't even.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Default)]
pub(super) struct Column(u32);

impl Column {
    pub(super) const fn new(column: u32) -> Self {
        Self(column)
    }
}

/// The number of characters in an indentation. Each character accounts for 1.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Default)]
pub(super) struct Character(u32);

impl Character {
    pub(super) const fn new(characters: u32) -> Self {
        Self(characters)
    }
}

/// The [Indentation](https://docs.python.org/3/reference/lexical_analysis.html#indentation) of a logical line.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub(super) struct Indentation {
    column: Column,
    character: Character,
}

impl Indentation {
    const TAB_SIZE: u32 = 2;

    pub(super) const fn root() -> Self {
        Self {
            column: Column::new(0),
            character: Character::new(0),
        }
    }

    #[cfg(test)]
    pub(super) const fn new(column: Column, character: Character) -> Self {
        Self { column, character }
    }

    #[must_use]
    pub(super) fn add_space(self) -> Self {
        Self {
            character: Character(self.character.0 + 1),
            column: Column(self.column.0 + 1),
        }
    }

    #[must_use]
    pub(super) fn add_tab(self) -> Self {
        Self {
            character: Character(self.character.0 + 1),
            // Compute the column index:
            // * Adds `TAB_SIZE` if `column` is a multiple of `TAB_SIZE`
            // * Rounds `column` up to the next multiple of `TAB_SIZE` otherwise.
            // https://github.com/python/cpython/blob/2cf99026d6320f38937257da1ab014fc873a11a6/Parser/tokenizer.c#L1818
            column: Column((self.column.0 / Self::TAB_SIZE + 1) * Self::TAB_SIZE),
        }
    }

    pub(super) fn try_compare(self, other: Indentation) -> Result<Ordering, UnexpectedIndentation> {
        let column_ordering = self.column.cmp(&other.column);
        let character_ordering = self.character.cmp(&other.character);

        if column_ordering == character_ordering {
            Ok(column_ordering)
        } else {
            Err(UnexpectedIndentation)
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub(super) struct UnexpectedIndentation;

/// The indentations stack is used to keep track of the current indentation level
/// [See Indentation](docs.python.org/3/reference/lexical_analysis.html#indentation).
#[derive(Debug, Clone, Default)]
pub(super) struct Indentations {
    stack: Vec<Indentation>,
}

impl Indentations {
    pub(super) fn indent(&mut self, indent: Indentation) {
        debug_assert_eq!(self.current().try_compare(indent), Ok(Ordering::Less));

        self.stack.push(indent);
    }

    /// Dedent one level to eventually reach `new_indentation`.
    ///
    /// Returns `Err` if the `new_indentation` is greater than the new current indentation level.
    pub(super) fn dedent_one(
        &mut self,
        new_indentation: Indentation,
    ) -> Result<Option<Indentation>, UnexpectedIndentation> {
        let previous = self.dedent();

        match new_indentation.try_compare(*self.current())? {
            Ordering::Less | Ordering::Equal => Ok(previous),
            // ```python
            // if True:
            //     pass
            //   pass <- The indentation is greater than the expected indent of 0.
            // ```
            Ordering::Greater => Err(UnexpectedIndentation),
        }
    }

    pub(super) fn dedent(&mut self) -> Option<Indentation> {
        self.stack.pop()
    }

    pub(super) fn current(&self) -> &Indentation {
        static ROOT: Indentation = Indentation::root();
        self.stack.last().unwrap_or(&ROOT)
    }

    pub(crate) fn checkpoint(&self) -> IndentationsCheckpoint {
        IndentationsCheckpoint(self.stack.clone())
    }

    pub(crate) fn rewind(&mut self, checkpoint: IndentationsCheckpoint) {
        self.stack = checkpoint.0;
    }
}

#[derive(Debug, Clone)]
pub(crate) struct IndentationsCheckpoint(Vec<Indentation>);

assert_eq_size!(Indentation, u64);

#[cfg(test)]
mod tests {
    use super::{Character, Column, Indentation};
    use std::cmp::Ordering;

    #[test]
    fn indentation_try_compare() {
        let tab = Indentation::new(Column::new(8), Character::new(1));

        assert_eq!(tab.try_compare(tab), Ok(Ordering::Equal));

        let two_tabs = Indentation::new(Column::new(16), Character::new(2));
        assert_eq!(two_tabs.try_compare(tab), Ok(Ordering::Greater));
        assert_eq!(tab.try_compare(two_tabs), Ok(Ordering::Less));
    }
}
