// These traits are copied verbatim from ../../ruff_formatter/shared_traits.rs.
// They should stay in sync. Originally, they were included via
// `include!("...")`, but this seems to break rust-analyzer as it treats the
// included file as unlinked. Since there isn't much to copy, we just do that.

/// Used to get an object that knows how to format this object.
pub trait AsFormat<Context> {
    type Format<'a>: ruff_formatter::Format<Context>
    where
        Self: 'a;

    /// Returns an object that is able to format this object.
    fn format(&self) -> Self::Format<'_>;
}

/// Implement [`AsFormat`] for references to types that implement [`AsFormat`].
impl<T, C> AsFormat<C> for &T
where
    T: AsFormat<C>,
{
    type Format<'a> = T::Format<'a> where Self: 'a;

    fn format(&self) -> Self::Format<'_> {
        AsFormat::format(&**self)
    }
}

/// Used to convert this object into an object that can be formatted.
///
/// The difference to [`AsFormat`] is that this trait takes ownership of `self`.
pub trait IntoFormat<Context> {
    type Format: ruff_formatter::Format<Context>;

    fn into_format(self) -> Self::Format;
}

/// Implement [`IntoFormat`] for [`Option`] when `T` implements [`IntoFormat`]
///
/// Allows to call format on optional AST fields without having to unwrap the
/// field first.
impl<T, Context> IntoFormat<Context> for Option<T>
where
    T: IntoFormat<Context>,
{
    type Format = Option<T::Format>;

    fn into_format(self) -> Self::Format {
        self.map(IntoFormat::into_format)
    }
}

/// Implement [`IntoFormat`] for references to types that implement [`AsFormat`].
impl<'a, T, C> IntoFormat<C> for &'a T
where
    T: AsFormat<C>,
{
    type Format = T::Format<'a>;

    fn into_format(self) -> Self::Format {
        AsFormat::format(self)
    }
}

/// Formatting specific [`Iterator`] extensions
pub trait FormattedIterExt {
    /// Converts every item to an object that knows how to format it.
    fn formatted<Context>(self) -> FormattedIter<Self, Self::Item, Context>
    where
        Self: Iterator + Sized,
        Self::Item: IntoFormat<Context>,
    {
        FormattedIter {
            inner: self,
            options: std::marker::PhantomData,
        }
    }
}

impl<I> FormattedIterExt for I where I: std::iter::Iterator {}

pub struct FormattedIter<Iter, Item, Context>
where
    Iter: Iterator<Item = Item>,
{
    inner: Iter,
    options: std::marker::PhantomData<Context>,
}

impl<Iter, Item, Context> std::iter::Iterator for FormattedIter<Iter, Item, Context>
where
    Iter: Iterator<Item = Item>,
    Item: IntoFormat<Context>,
{
    type Item = Item::Format;

    fn next(&mut self) -> Option<Self::Item> {
        Some(self.inner.next()?.into_format())
    }
}

impl<Iter, Item, Context> std::iter::FusedIterator for FormattedIter<Iter, Item, Context>
where
    Iter: std::iter::FusedIterator<Item = Item>,
    Item: IntoFormat<Context>,
{
}

impl<Iter, Item, Context> std::iter::ExactSizeIterator for FormattedIter<Iter, Item, Context>
where
    Iter: Iterator<Item = Item> + std::iter::ExactSizeIterator,
    Item: IntoFormat<Context>,
{
}
