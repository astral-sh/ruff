use crate::prelude::*;
use std::cell::OnceCell;
use std::marker::PhantomData;

use crate::Buffer;

/// Utility trait that allows memorizing the output of a [`Format`].
/// Useful to avoid re-formatting the same object twice.
pub trait MemoizeFormat<Context> {
    /// Returns a formattable object that memoizes the result of `Format` by cloning.
    /// Mainly useful if the same sub-tree can appear twice in the formatted output because it's
    /// used inside of `if_group_breaks` or `if_group_fits_single_line`.
    ///
    /// ```
    /// use std::cell::Cell;
    /// use ruff_formatter::{format, write};
    /// use ruff_formatter::prelude::*;
    /// use ruff_text_size::{Ranged, TextSize};
    ///
    /// struct MyFormat {
    ///   value: Cell<u64>
    /// }
    ///
    /// impl MyFormat {
    ///     pub fn new() -> Self {
    ///         Self { value: Cell::new(1) }
    ///     }
    /// }
    ///
    /// impl Format<SimpleFormatContext> for MyFormat {
    ///     fn fmt(&self, f: &mut Formatter<SimpleFormatContext>) -> FormatResult<()> {
    ///         let value = self.value.get();
    ///         self.value.set(value + 1);
    ///
    ///         write!(f, [text(&std::format!("Formatted {value} times."))])
    ///     }
    /// }
    ///
    /// # fn main() -> FormatResult<()> {
    /// let normal = MyFormat::new();
    ///
    /// // Calls `format` every time the object gets formatted
    /// assert_eq!(
    ///     "Formatted 1 times. Formatted 2 times.",
    ///     format!(SimpleFormatContext::default(), [normal, space(), normal])?.print()?.as_code()
    /// );
    ///
    /// // Memoized memoizes the result and calls `format` only once.
    /// let memoized = normal.memoized();
    /// assert_eq!(
    ///     "Formatted 3 times. Formatted 3 times.",
    ///     format![SimpleFormatContext::default(), [memoized, space(), memoized]]?.print()?.as_code()
    /// );
    /// # Ok(())
    /// # }
    /// ```
    fn memoized(self) -> Memoized<Self, Context>
    where
        Self: Sized + Format<Context>,
    {
        Memoized::new(self)
    }
}

impl<T, Context> MemoizeFormat<Context> for T where T: Format<Context> {}

/// Memoizes the output of its inner [`Format`] to avoid re-formatting a potential expensive object.
#[derive(Debug)]
pub struct Memoized<F, Context> {
    inner: F,
    memory: OnceCell<FormatResult<Option<FormatElement>>>,
    options: PhantomData<Context>,
}

impl<F, Context> Memoized<F, Context>
where
    F: Format<Context>,
{
    fn new(inner: F) -> Self {
        Self {
            inner,
            memory: OnceCell::new(),
            options: PhantomData,
        }
    }

    /// Gives access to the memoized content.
    ///
    /// Performs the formatting if the content hasn't been formatted at this point.
    ///
    /// # Example
    ///
    /// Inspect if some memoized content breaks.
    ///
    /// ```rust
    /// use std::cell::Cell;
    /// use ruff_formatter::{format, write};
    /// use ruff_formatter::prelude::*;
    /// use ruff_text_size::{Ranged, TextSize};
    ///
    /// #[derive(Default)]
    /// struct Counter {
    ///   value: Cell<u64>
    /// }
    ///
    /// impl Format<SimpleFormatContext> for Counter {
    ///     fn fmt(&self, f: &mut Formatter<SimpleFormatContext>) -> FormatResult<()> {
    ///         let current = self.value.get();
    ///
    ///         write!(f, [
    ///             token("Count:"),
    ///             space(),
    ///             text(&std::format!("{current}")),
    ///             hard_line_break()
    ///         ])?;
    ///
    ///         self.value.set(current + 1);
    ///         Ok(())
    ///     }
    /// }
    ///
    /// # fn main() -> FormatResult<()> {
    /// let content = format_with(|f| {
    ///     let mut counter = Counter::default().memoized();
    ///     let counter_content = counter.inspect(f)?;
    ///
    ///     if counter_content.will_break() {
    ///         write!(f, [token("Counter:"), block_indent(&counter)])
    ///     } else {
    ///         write!(f, [token("Counter:"), counter])
    ///     }?;
    ///
    ///     write!(f, [counter])
    /// });
    ///
    ///
    /// let formatted = format!(SimpleFormatContext::default(), [content])?;
    /// assert_eq!("Counter:\n\tCount: 0\nCount: 0\n", formatted.print()?.as_code());
    /// # Ok(())
    /// # }
    /// ```
    pub fn inspect(&self, f: &mut Formatter<Context>) -> FormatResult<&[FormatElement]> {
        let result = self.memory.get_or_init(|| f.intern(&self.inner));

        match result.as_ref() {
            Ok(Some(FormatElement::Interned(interned))) => Ok(&**interned),
            Ok(Some(other)) => Ok(std::slice::from_ref(other)),
            Ok(None) => Ok(&[]),
            Err(error) => Err(*error),
        }
    }
}

impl<F, Context> Format<Context> for Memoized<F, Context>
where
    F: Format<Context>,
{
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        let result = self.memory.get_or_init(|| f.intern(&self.inner));

        match result {
            Ok(Some(elements)) => {
                f.write_element(elements.clone());

                Ok(())
            }
            Ok(None) => Ok(()),
            Err(err) => Err(*err),
        }
    }
}
