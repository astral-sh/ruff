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

    fn by(self, amount: u32) -> Self {
        Self {
            character: Character(self.character.0 * amount),
            column: Column(self.column.0 * amount),
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
            IndentationsInner::Stack(stack) => stack.push(indent),
            IndentationsInner::Counter(inner) => {
                if inner.indent(indent) {
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
            IndentationsInner::Stack(stack) => stack.pop(),
            IndentationsInner::Counter(inner) => inner.dedent(),
        }
    }

    pub(super) fn current(&self) -> Indentation {
        static ROOT: Indentation = Indentation::root();

        match &self.inner {
            IndentationsInner::Stack(stack) => *stack.last().unwrap_or(&ROOT),
            IndentationsInner::Counter(inner) => inner.current().unwrap_or(ROOT),
        }
    }

    pub(crate) fn checkpoint(&self) -> IndentationsCheckpoint {
        IndentationsCheckpoint(self.inner.clone())
    }

    pub(crate) fn rewind(&mut self, checkpoint: IndentationsCheckpoint) {
        self.inner = checkpoint.0;
    }

    fn make_stack(&mut self) -> &mut Vec<Indentation> {
        if let IndentationsInner::Counter(IndentationCounter { first, level }) = self.inner {
            *self = Indentations {
                inner: IndentationsInner::Stack(first.map_or_else(Vec::new, |first_indent| {
                    (1..=level).map(|level| first_indent.by(level)).collect()
                })),
            };
        }
        match &mut self.inner {
            IndentationsInner::Stack(stack) => stack,
            IndentationsInner::Counter(_) => unreachable!(),
        }
    }
}

#[derive(Debug, Default, Clone)]
struct IndentationCounter {
    /// The first [`Indentation`] in the source code.
    first: Option<Indentation>,
    /// The current level of indentation.
    level: u32,
}

impl IndentationCounter {
    fn indent(&mut self, indent: Indentation) -> bool {
        let first_indent = self.first.get_or_insert(indent);
        if first_indent.by(self.level + 1) == indent {
            self.level += 1;
            true
        } else {
            false
        }
    }

    fn dedent(&mut self) -> Option<Indentation> {
        if self.level == 0 {
            None
        } else {
            let current = self.current();
            self.level -= 1;
            current
        }
    }

    fn current(&self) -> Option<Indentation> {
        self.first.map(|indent| indent.by(self.level))
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
