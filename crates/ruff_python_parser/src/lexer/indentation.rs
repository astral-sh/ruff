use std::cmp::Ordering;
use std::fmt::Debug;

use static_assertions::assert_eq_size;

/// The column index of an indentation.
///
/// A space increments the column by one. A tab adds up to 2 (if tab size is 2) indices, but just one
/// if the column isn't even.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Default)]
pub(super) struct Column(u32);

/// The number of characters in an indentation. Each character accounts for 1.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Default)]
pub(super) struct Character(u32);

/// The [Indentation](https://docs.python.org/3/reference/lexical_analysis.html#indentation) of a logical line.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub(super) struct Indentation {
    column: Column,
    character: Character,
}

impl Indentation {
    const TAB_SIZE: u32 = 2;

    const ROOT: Indentation = Indentation {
        column: Column(0),
        character: Character(0),
    };

    pub(super) const fn root() -> Self {
        Self {
            column: Column(0),
            character: Character(0),
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

    /// Computes the indentation at the given level based on the current indentation.
    const fn at(self, level: u32) -> Self {
        Self {
            character: Character(self.character.0 * level),
            column: Column(self.column.0 * level),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub(super) struct UnexpectedIndentation;

/// The indentations stack is used to keep track of the current indentation level
/// [See Indentation](docs.python.org/3/reference/lexical_analysis.html#indentation).
#[derive(Debug)]
pub(super) struct Indentations {
    inner: IndentationsInner,
}

#[derive(Debug, Clone)]
enum IndentationsInner {
    Stack(Vec<Indentation>),
    Counter(IndentationCounter),
}

impl Default for Indentations {
    fn default() -> Self {
        Indentations {
            inner: IndentationsInner::Counter(IndentationCounter::default()),
        }
    }
}

impl Indentations {
    pub(super) fn indent(&mut self, indent: Indentation) {
        debug_assert_eq!(self.current().try_compare(indent), Ok(Ordering::Less));

        match &mut self.inner {
            IndentationsInner::Stack(indentations) => indentations.push(indent),
            IndentationsInner::Counter(indentations) => {
                if indentations.indent(indent) {
                    return;
                }
                self.make_stack().push(indent);
            }
        }
    }

    /// Dedent one level to eventually reach `new_indentation`.
    ///
    /// Returns `Err` if the `new_indentation` is greater than the new current indentation level.
    pub(super) fn dedent_one(
        &mut self,
        new_indentation: Indentation,
    ) -> Result<Option<Indentation>, UnexpectedIndentation> {
        let previous = self.dedent();

        match new_indentation.try_compare(self.current())? {
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
        match &mut self.inner {
            IndentationsInner::Stack(indentations) => indentations.pop(),
            IndentationsInner::Counter(indentations) => indentations.dedent(),
        }
    }

    pub(super) fn current(&self) -> Indentation {
        match &self.inner {
            IndentationsInner::Stack(indentations) => {
                *indentations.last().unwrap_or(&Indentation::ROOT)
            }
            IndentationsInner::Counter(indentations) => indentations.current(),
        }
    }

    pub(crate) fn checkpoint(&self) -> IndentationsCheckpoint {
        IndentationsCheckpoint(self.inner.clone())
    }

    pub(crate) fn rewind(&mut self, checkpoint: IndentationsCheckpoint) {
        self.inner = checkpoint.0;
    }

    fn make_stack(&mut self) -> &mut Vec<Indentation> {
        if let IndentationsInner::Counter(IndentationCounter { first, level, .. }) = self.inner {
            if level == 0 {
                *self = Indentations {
                    inner: IndentationsInner::Stack(vec![]),
                };
            } else {
                *self = Indentations {
                    inner: IndentationsInner::Stack(first.map_or_else(Vec::new, |first| {
                        (1..=level).map(|level| first.at(level)).collect()
                    })),
                };
            }
        }
        match &mut self.inner {
            IndentationsInner::Stack(stack) => stack,
            IndentationsInner::Counter(_) => unreachable!(),
        }
    }
}

#[derive(Debug, Default, Clone)]
struct IndentationCounter {
    /// The current indentation.
    current: Indentation,
    /// The first indentation in the source code.
    first: Option<Indentation>,
    /// The current indentation level.
    level: u32,
}

impl IndentationCounter {
    fn indent(&mut self, indent: Indentation) -> bool {
        if let Some(first) = self.first {
            if first.at(self.level + 1) == indent {
                self.current = indent;
                self.level += 1;
                true
            } else {
                false
            }
        } else {
            self.first = Some(indent);
            self.current = indent;
            self.level = 1;
            true
        }
    }

    fn dedent(&mut self) -> Option<Indentation> {
        if self.level == 0 {
            None
        } else if let Some(first) = self.first {
            let current = self.current;
            self.level -= 1;
            self.current = first.at(self.level);
            Some(current)
        } else {
            unreachable!()
        }
    }

    fn current(&self) -> Indentation {
        self.current
    }
}

pub(super) struct IndentationsCheckpoint(IndentationsInner);

assert_eq_size!(Indentation, u64);

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use super::{Character, Column, Indentation};

    #[test]
    fn indentation_try_compare() {
        let tab = Indentation::new(Column(8), Character(1));

        assert_eq!(tab.try_compare(tab), Ok(Ordering::Equal));

        let two_tabs = Indentation::new(Column(16), Character(2));
        assert_eq!(two_tabs.try_compare(tab), Ok(Ordering::Greater));
        assert_eq!(tab.try_compare(two_tabs), Ok(Ordering::Less));
    }
}
