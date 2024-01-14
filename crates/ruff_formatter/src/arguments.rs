use super::{Buffer, Format, Formatter};
use crate::FormatResult;

/// A convenience wrapper for representing a formattable argument.
pub struct Argument<'fmt, Context> {
    value: &'fmt dyn Format<Context>,
}

impl<Context> Clone for Argument<'_, Context> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<Context> Copy for Argument<'_, Context> {}

impl<'fmt, Context> Argument<'fmt, Context> {
    /// Called by the [ruff_formatter::format_args] macro.
    #[doc(hidden)]
    #[inline]
    pub fn new<F: Format<Context>>(value: &'fmt F) -> Self {
        Self { value }
    }

    /// Formats the value stored by this argument using the given formatter.
    #[inline]
    // Seems to only be triggered on wasm32 and looks like a false positive?
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub(super) fn format(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        self.value.fmt(f)
    }
}

/// Sequence of objects that should be formatted in the specified order.
///
/// The [`format_args!`] macro will safely create an instance of this structure.
///
/// You can use the `Arguments<a>` that [`format_args!`] return in `Format` context as seen below.
/// It will call the `format` function for each of its objects.
///
/// ```rust
/// use ruff_formatter::prelude::*;
/// use ruff_formatter::{format, format_args};
///
/// # fn main() -> FormatResult<()> {
/// let formatted = format!(SimpleFormatContext::default(), [
///     format_args!(token("a"), space(), token("b"))
/// ])?;
///
/// assert_eq!("a b", formatted.print()?.as_code());
/// # Ok(())
/// # }
/// ```
pub struct Arguments<'fmt, Context>(pub &'fmt [Argument<'fmt, Context>]);

impl<'fmt, Context> Arguments<'fmt, Context> {
    #[doc(hidden)]
    #[inline]
    pub fn new(arguments: &'fmt [Argument<'fmt, Context>]) -> Self {
        Self(arguments)
    }

    /// Returns the arguments
    #[inline]
    #[allow(clippy::trivially_copy_pass_by_ref)] // Bug in Clippy? Sizeof Arguments is 16
    pub(super) fn items(&self) -> &'fmt [Argument<'fmt, Context>] {
        self.0
    }
}

impl<Context> Copy for Arguments<'_, Context> {}

impl<Context> Clone for Arguments<'_, Context> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Context> Format<Context> for Arguments<'_, Context> {
    #[inline]
    fn fmt(&self, formatter: &mut Formatter<Context>) -> FormatResult<()> {
        formatter.write_fmt(*self)
    }
}

impl<Context> std::fmt::Debug for Arguments<'_, Context> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Arguments[...]")
    }
}

impl<'fmt, Context> From<&'fmt Argument<'fmt, Context>> for Arguments<'fmt, Context> {
    fn from(argument: &'fmt Argument<'fmt, Context>) -> Self {
        Arguments::new(std::slice::from_ref(argument))
    }
}

#[cfg(test)]
mod tests {
    use crate::format_element::tag::Tag;
    use crate::prelude::*;
    use crate::{format_args, write, FormatState, VecBuffer};

    #[test]
    fn test_nesting() {
        let mut context = FormatState::new(SimpleFormatContext::default());
        let mut buffer = VecBuffer::new(&mut context);

        write!(
            &mut buffer,
            [
                token("function"),
                space(),
                token("a"),
                space(),
                group(&format_args!(token("("), token(")")))
            ]
        )
        .unwrap();

        assert_eq!(
            buffer.into_vec(),
            vec![
                FormatElement::Token { text: "function" },
                FormatElement::Space,
                FormatElement::Token { text: "a" },
                FormatElement::Space,
                // Group
                FormatElement::Tag(Tag::StartGroup(tag::Group::new())),
                FormatElement::Token { text: "(" },
                FormatElement::Token { text: ")" },
                FormatElement::Tag(Tag::EndGroup)
            ]
        );
    }
}
