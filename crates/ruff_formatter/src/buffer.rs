use super::{write, Arguments, FormatElement};
use crate::format_element::Interned;
use crate::prelude::LineMode;
use crate::{Format, FormatResult, FormatState};
use rustc_hash::FxHashMap;
use std::any::{Any, TypeId};
use std::fmt::Debug;
use std::ops::{Deref, DerefMut};

/// A trait for writing or formatting into [FormatElement]-accepting buffers or streams.
pub trait Buffer {
    /// The context used during formatting
    type Context;

    /// Writes a [crate::FormatElement] into this buffer, returning whether the write succeeded.
    ///
    /// # Errors
    /// This function will return an instance of [crate::FormatError] on error.
    ///
    /// # Examples
    ///
    /// ```
    /// use ruff_formatter::{Buffer, FormatElement, FormatState, SimpleFormatContext, VecBuffer};
    ///
    /// let mut state = FormatState::new(SimpleFormatContext::default());
    /// let mut buffer = VecBuffer::new(&mut state);
    ///
    /// buffer.write_element(FormatElement::StaticText { text: "test"}).unwrap();
    ///
    /// assert_eq!(buffer.into_vec(), vec![FormatElement::StaticText { text: "test" }]);
    /// ```
    fn write_element(&mut self, element: FormatElement) -> FormatResult<()>;

    /// Returns a slice containing all elements written into this buffer.
    ///
    /// Prefer using [BufferExtensions::start_recording] over accessing [Buffer::elements] directly.
    #[doc(hidden)]
    fn elements(&self) -> &[FormatElement];

    /// Glue for usage of the [`write!`] macro with implementors of this trait.
    ///
    /// This method should generally not be invoked manually, but rather through the [`write!`] macro itself.
    ///
    /// # Examples
    ///
    /// ```
    /// use ruff_formatter::prelude::*;
    /// use ruff_formatter::{Buffer, FormatState, SimpleFormatContext, VecBuffer, format_args};
    ///
    /// let mut state = FormatState::new(SimpleFormatContext::default());
    /// let mut buffer = VecBuffer::new(&mut state);
    ///
    /// buffer.write_fmt(format_args!(text("Hello World"))).unwrap();
    ///
    /// assert_eq!(buffer.into_vec(), vec![FormatElement::StaticText{ text: "Hello World" }]);
    /// ```
    fn write_fmt(mut self: &mut Self, arguments: Arguments<Self::Context>) -> FormatResult<()> {
        write(&mut self, arguments)
    }

    /// Returns the formatting state relevant for this formatting session.
    fn state(&self) -> &FormatState<Self::Context>;

    /// Returns the mutable formatting state relevant for this formatting session.
    fn state_mut(&mut self) -> &mut FormatState<Self::Context>;

    /// Takes a snapshot of the Buffers state, excluding the formatter state.
    fn snapshot(&self) -> BufferSnapshot;

    /// Restores the snapshot buffer
    ///
    /// ## Panics
    /// If the passed snapshot id is a snapshot of another buffer OR
    /// if the snapshot is restored out of order
    fn restore_snapshot(&mut self, snapshot: BufferSnapshot);
}

/// Snapshot of a buffer state that can be restored at a later point.
///
/// Used in cases where the formatting of an object fails but a parent formatter knows an alternative
/// strategy on how to format the object that might succeed.
#[derive(Debug)]
pub enum BufferSnapshot {
    /// Stores an absolute position of a buffers state, for example, the offset of the last written element.
    Position(usize),

    /// Generic structure for custom buffers that need to store more complex data. Slightly more
    /// expensive because it requires allocating the buffer state on the heap.
    Any(Box<dyn Any>),
}

impl BufferSnapshot {
    /// Creates a new buffer snapshot that points to the specified position.
    pub const fn position(index: usize) -> Self {
        Self::Position(index)
    }

    /// Unwraps the position value.
    ///
    /// # Panics
    ///
    /// If self is not a [`BufferSnapshot::Position`]
    pub fn unwrap_position(&self) -> usize {
        match self {
            BufferSnapshot::Position(index) => *index,
            BufferSnapshot::Any(_) => panic!("Tried to unwrap Any snapshot as a position."),
        }
    }

    /// Unwraps the any value.
    ///
    /// # Panics
    ///
    /// If `self` is not a [`BufferSnapshot::Any`].
    pub fn unwrap_any<T: 'static>(self) -> T {
        match self {
            BufferSnapshot::Position(_) => {
                panic!("Tried to unwrap Position snapshot as Any snapshot.")
            }
            BufferSnapshot::Any(value) => match value.downcast::<T>() {
                Ok(snapshot) => *snapshot,
                Err(err) => {
                    panic!(
                        "Tried to unwrap snapshot of type {:?} as {:?}",
                        err.type_id(),
                        TypeId::of::<T>()
                    )
                }
            },
        }
    }
}

/// Implements the `[Buffer]` trait for all mutable references of objects implementing [Buffer].
impl<W: Buffer<Context = Context> + ?Sized, Context> Buffer for &mut W {
    type Context = Context;

    fn write_element(&mut self, element: FormatElement) -> FormatResult<()> {
        (**self).write_element(element)
    }

    fn elements(&self) -> &[FormatElement] {
        (**self).elements()
    }

    fn write_fmt(&mut self, args: Arguments<Context>) -> FormatResult<()> {
        (**self).write_fmt(args)
    }

    fn state(&self) -> &FormatState<Self::Context> {
        (**self).state()
    }

    fn state_mut(&mut self) -> &mut FormatState<Self::Context> {
        (**self).state_mut()
    }

    fn snapshot(&self) -> BufferSnapshot {
        (**self).snapshot()
    }

    fn restore_snapshot(&mut self, snapshot: BufferSnapshot) {
        (**self).restore_snapshot(snapshot)
    }
}

/// Vector backed [`Buffer`] implementation.
///
/// The buffer writes all elements into the internal elements buffer.
#[derive(Debug)]
pub struct VecBuffer<'a, Context> {
    state: &'a mut FormatState<Context>,
    elements: Vec<FormatElement>,
}

impl<'a, Context> VecBuffer<'a, Context> {
    pub fn new(state: &'a mut FormatState<Context>) -> Self {
        Self::new_with_vec(state, Vec::new())
    }

    pub fn new_with_vec(state: &'a mut FormatState<Context>, elements: Vec<FormatElement>) -> Self {
        Self { state, elements }
    }

    /// Creates a buffer with the specified capacity
    pub fn with_capacity(capacity: usize, state: &'a mut FormatState<Context>) -> Self {
        Self {
            state,
            elements: Vec::with_capacity(capacity),
        }
    }

    /// Consumes the buffer and returns the written [`FormatElement]`s as a vector.
    pub fn into_vec(self) -> Vec<FormatElement> {
        self.elements
    }

    /// Takes the elements without consuming self
    pub fn take_vec(&mut self) -> Vec<FormatElement> {
        std::mem::take(&mut self.elements)
    }
}

impl<Context> Deref for VecBuffer<'_, Context> {
    type Target = [FormatElement];

    fn deref(&self) -> &Self::Target {
        &self.elements
    }
}

impl<Context> DerefMut for VecBuffer<'_, Context> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.elements
    }
}

impl<Context> Buffer for VecBuffer<'_, Context> {
    type Context = Context;

    fn write_element(&mut self, element: FormatElement) -> FormatResult<()> {
        self.elements.push(element);

        Ok(())
    }

    fn elements(&self) -> &[FormatElement] {
        self
    }

    fn state(&self) -> &FormatState<Self::Context> {
        self.state
    }

    fn state_mut(&mut self) -> &mut FormatState<Self::Context> {
        self.state
    }

    fn snapshot(&self) -> BufferSnapshot {
        BufferSnapshot::position(self.elements.len())
    }

    fn restore_snapshot(&mut self, snapshot: BufferSnapshot) {
        let position = snapshot.unwrap_position();
        assert!(
            self.elements.len() >= position,
            r#"Outdated snapshot. This buffer contains fewer elements than at the time the snapshot was taken.
Make sure that you take and restore the snapshot in order and that this snapshot belongs to the current buffer."#
        );

        self.elements.truncate(position);
    }
}

/// This struct wraps an existing buffer and emits a preamble text when the first text is written.
///
/// This can be useful if you, for example, want to write some content if what gets written next isn't empty.
///
/// # Examples
///
/// ```
/// use ruff_formatter::{FormatState, Formatted, PreambleBuffer, SimpleFormatContext, VecBuffer, write};
/// use ruff_formatter::prelude::*;
///
/// struct Preamble;
///
/// impl Format<SimpleFormatContext> for Preamble {
///     fn fmt(&self, f: &mut Formatter<SimpleFormatContext>) -> FormatResult<()> {
///         write!(f, [text("# heading"), hard_line_break()])
///     }
/// }
///
/// # fn main() -> FormatResult<()> {
/// let mut state = FormatState::new(SimpleFormatContext::default());
/// let mut buffer = VecBuffer::new(&mut state);
///
/// {
///     let mut with_preamble = PreambleBuffer::new(&mut buffer, Preamble);
///
///     write!(&mut with_preamble, [text("this text will be on a new line")])?;
/// }
///
/// let formatted = Formatted::new(Document::from(buffer.into_vec()), SimpleFormatContext::default());
/// assert_eq!("# heading\nthis text will be on a new line", formatted.print()?.as_code());
///
/// # Ok(())
/// # }
/// ```
///
/// The pre-amble does not get written if no content is written to the buffer.
///
/// ```
/// use ruff_formatter::{FormatState, Formatted, PreambleBuffer, SimpleFormatContext, VecBuffer, write};
/// use ruff_formatter::prelude::*;
///
/// struct Preamble;
///
/// impl Format<SimpleFormatContext> for Preamble {
///     fn fmt(&self, f: &mut Formatter<SimpleFormatContext>) -> FormatResult<()> {
///         write!(f, [text("# heading"), hard_line_break()])
///     }
/// }
///
/// # fn main() -> FormatResult<()> {
/// let mut state = FormatState::new(SimpleFormatContext::default());
/// let mut buffer = VecBuffer::new(&mut state);
/// {
///     let mut with_preamble = PreambleBuffer::new(&mut buffer, Preamble);
/// }
///
/// let formatted = Formatted::new(Document::from(buffer.into_vec()), SimpleFormatContext::default());
/// assert_eq!("", formatted.print()?.as_code());
/// # Ok(())
/// # }
/// ```
pub struct PreambleBuffer<'buf, Preamble, Context> {
    /// The wrapped buffer
    inner: &'buf mut dyn Buffer<Context = Context>,

    /// The pre-amble to write once the first content gets written to this buffer.
    preamble: Preamble,

    /// Whether some content (including the pre-amble) has been written at this point.
    empty: bool,
}

impl<'buf, Preamble, Context> PreambleBuffer<'buf, Preamble, Context> {
    pub fn new(inner: &'buf mut dyn Buffer<Context = Context>, preamble: Preamble) -> Self {
        Self {
            inner,
            preamble,
            empty: true,
        }
    }

    /// Returns `true` if the preamble has been written, `false` otherwise.
    pub fn did_write_preamble(&self) -> bool {
        !self.empty
    }
}

impl<Preamble, Context> Buffer for PreambleBuffer<'_, Preamble, Context>
where
    Preamble: Format<Context>,
{
    type Context = Context;

    fn write_element(&mut self, element: FormatElement) -> FormatResult<()> {
        if self.empty {
            write!(self.inner, [&self.preamble])?;
            self.empty = false;
        }

        self.inner.write_element(element)
    }

    fn elements(&self) -> &[FormatElement] {
        self.inner.elements()
    }

    fn state(&self) -> &FormatState<Self::Context> {
        self.inner.state()
    }

    fn state_mut(&mut self) -> &mut FormatState<Self::Context> {
        self.inner.state_mut()
    }

    fn snapshot(&self) -> BufferSnapshot {
        BufferSnapshot::Any(Box::new(PreambleBufferSnapshot {
            inner: self.inner.snapshot(),
            empty: self.empty,
        }))
    }

    fn restore_snapshot(&mut self, snapshot: BufferSnapshot) {
        let snapshot = snapshot.unwrap_any::<PreambleBufferSnapshot>();

        self.empty = snapshot.empty;
        self.inner.restore_snapshot(snapshot.inner);
    }
}

struct PreambleBufferSnapshot {
    inner: BufferSnapshot,
    empty: bool,
}

/// Buffer that allows you inspecting elements as they get written to the formatter.
pub struct Inspect<'inner, Context, Inspector> {
    inner: &'inner mut dyn Buffer<Context = Context>,
    inspector: Inspector,
}

impl<'inner, Context, Inspector> Inspect<'inner, Context, Inspector> {
    fn new(inner: &'inner mut dyn Buffer<Context = Context>, inspector: Inspector) -> Self {
        Self { inner, inspector }
    }
}

impl<'inner, Context, Inspector> Buffer for Inspect<'inner, Context, Inspector>
where
    Inspector: FnMut(&FormatElement),
{
    type Context = Context;

    fn write_element(&mut self, element: FormatElement) -> FormatResult<()> {
        (self.inspector)(&element);
        self.inner.write_element(element)
    }

    fn elements(&self) -> &[FormatElement] {
        self.inner.elements()
    }

    fn state(&self) -> &FormatState<Self::Context> {
        self.inner.state()
    }

    fn state_mut(&mut self) -> &mut FormatState<Self::Context> {
        self.inner.state_mut()
    }

    fn snapshot(&self) -> BufferSnapshot {
        self.inner.snapshot()
    }

    fn restore_snapshot(&mut self, snapshot: BufferSnapshot) {
        self.inner.restore_snapshot(snapshot)
    }
}

/// A Buffer that removes any soft line breaks.
///
/// - Removes [`lines`](FormatElement::Line) with the mode [`Soft`](LineMode::Soft).
/// - Replaces [`lines`](FormatElement::Line) with the mode [`Soft`](LineMode::SoftOrSpace) with a [`Space`](FormatElement::Space)
///
/// # Examples
///
/// ```
/// use ruff_formatter::prelude::*;
/// use ruff_formatter::{format, write};
///
/// # fn main() -> FormatResult<()> {
/// use ruff_formatter::{RemoveSoftLinesBuffer, SimpleFormatContext, VecBuffer};
/// use ruff_formatter::prelude::format_with;
/// let formatted = format!(
///     SimpleFormatContext::default(),
///     [format_with(|f| {
///         let mut buffer = RemoveSoftLinesBuffer::new(f);
///
///         write!(
///             buffer,
///             [
///                 text("The next soft line or space gets replaced by a space"),
///                 soft_line_break_or_space(),
///                 text("and the line here"),
///                 soft_line_break(),
///                 text("is removed entirely.")
///             ]
///         )
///     })]
/// )?;
///
/// assert_eq!(
///     formatted.document().as_ref(),
///     &[
///         FormatElement::StaticText { text: "The next soft line or space gets replaced by a space" },
///         FormatElement::Space,
///         FormatElement::StaticText { text: "and the line here" },
///         FormatElement::StaticText { text: "is removed entirely." }
///     ]
/// );
///
/// # Ok(())
/// # }
/// ```
pub struct RemoveSoftLinesBuffer<'a, Context> {
    inner: &'a mut dyn Buffer<Context = Context>,

    /// Caches the interned elements after the soft line breaks have been removed.
    ///
    /// The `key` is the [Interned] element as it has been passed to [Self::write_element] or the child of another
    /// [Interned] element. The `value` is the matching document of the key where all soft line breaks have been removed.
    ///
    /// It's fine to not snapshot the cache. The worst that can happen is that it holds on interned elements
    /// that are now unused. But there's little harm in that and the cache is cleaned when dropping the buffer.
    interned_cache: FxHashMap<Interned, Interned>,
}

impl<'a, Context> RemoveSoftLinesBuffer<'a, Context> {
    /// Creates a new buffer that removes the soft line breaks before writing them into `buffer`.
    pub fn new(inner: &'a mut dyn Buffer<Context = Context>) -> Self {
        Self {
            inner,
            interned_cache: FxHashMap::default(),
        }
    }

    /// Removes the soft line breaks from an interned element.
    fn clean_interned(&mut self, interned: &Interned) -> Interned {
        clean_interned(interned, &mut self.interned_cache)
    }
}

// Extracted to function to avoid monomorphization
fn clean_interned(
    interned: &Interned,
    interned_cache: &mut FxHashMap<Interned, Interned>,
) -> Interned {
    match interned_cache.get(interned) {
        Some(cleaned) => cleaned.clone(),
        None => {
            // Find the first soft line break element or interned element that must be changed
            let result = interned
                .iter()
                .enumerate()
                .find_map(|(index, element)| match element {
                    FormatElement::Line(LineMode::Soft | LineMode::SoftOrSpace) => {
                        let mut cleaned = Vec::new();
                        cleaned.extend_from_slice(&interned[..index]);
                        Some((cleaned, &interned[index..]))
                    }
                    FormatElement::Interned(inner) => {
                        let cleaned_inner = clean_interned(inner, interned_cache);

                        if &cleaned_inner != inner {
                            let mut cleaned = Vec::with_capacity(interned.len());
                            cleaned.extend_from_slice(&interned[..index]);
                            cleaned.push(FormatElement::Interned(cleaned_inner));
                            Some((cleaned, &interned[index + 1..]))
                        } else {
                            None
                        }
                    }

                    _ => None,
                });

            let result = match result {
                // Copy the whole interned buffer so that becomes possible to change the necessary elements.
                Some((mut cleaned, rest)) => {
                    for element in rest {
                        let element = match element {
                            FormatElement::Line(LineMode::Soft) => continue,
                            FormatElement::Line(LineMode::SoftOrSpace) => FormatElement::Space,
                            FormatElement::Interned(interned) => {
                                FormatElement::Interned(clean_interned(interned, interned_cache))
                            }
                            element => element.clone(),
                        };
                        cleaned.push(element)
                    }

                    Interned::new(cleaned)
                }
                // No change necessary, return existing interned element
                None => interned.clone(),
            };

            interned_cache.insert(interned.clone(), result.clone());
            result
        }
    }
}

impl<Context> Buffer for RemoveSoftLinesBuffer<'_, Context> {
    type Context = Context;

    fn write_element(&mut self, element: FormatElement) -> FormatResult<()> {
        let element = match element {
            FormatElement::Line(LineMode::Soft) => return Ok(()),
            FormatElement::Line(LineMode::SoftOrSpace) => FormatElement::Space,
            FormatElement::Interned(interned) => {
                FormatElement::Interned(self.clean_interned(&interned))
            }
            element => element,
        };

        self.inner.write_element(element)
    }

    fn elements(&self) -> &[FormatElement] {
        self.inner.elements()
    }

    fn state(&self) -> &FormatState<Self::Context> {
        self.inner.state()
    }

    fn state_mut(&mut self) -> &mut FormatState<Self::Context> {
        self.inner.state_mut()
    }

    fn snapshot(&self) -> BufferSnapshot {
        self.inner.snapshot()
    }

    fn restore_snapshot(&mut self, snapshot: BufferSnapshot) {
        self.inner.restore_snapshot(snapshot)
    }
}

pub trait BufferExtensions: Buffer + Sized {
    /// Returns a new buffer that calls the passed inspector for every element that gets written to the output
    #[must_use]
    fn inspect<F>(&mut self, inspector: F) -> Inspect<Self::Context, F>
    where
        F: FnMut(&FormatElement),
    {
        Inspect::new(self, inspector)
    }

    /// Starts a recording that gives you access to all elements that have been written between the start
    /// and end of the recording
    ///
    /// #Examples
    ///
    /// ```
    /// use std::ops::Deref;
    /// use ruff_formatter::prelude::*;
    /// use ruff_formatter::{write, format, SimpleFormatContext};
    ///
    /// # fn main() -> FormatResult<()> {
    /// let formatted = format!(SimpleFormatContext::default(), [format_with(|f| {
    ///     let mut recording = f.start_recording();
    ///
    ///     write!(recording, [text("A")])?;
    ///     write!(recording, [text("B")])?;
    ///
    ///     write!(recording, [format_with(|f| write!(f, [text("C"), text("D")]))])?;
    ///
    ///     let recorded = recording.stop();
    ///     assert_eq!(
    ///         recorded.deref(),
    ///         &[
    ///             FormatElement::StaticText{ text: "A" },
    ///             FormatElement::StaticText{ text: "B" },
    ///             FormatElement::StaticText{ text: "C" },
    ///             FormatElement::StaticText{ text: "D" }
    ///         ]
    ///     );
    ///
    ///     Ok(())
    /// })])?;
    ///
    /// assert_eq!(formatted.print()?.as_code(), "ABCD");
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    fn start_recording(&mut self) -> Recording<Self> {
        Recording::new(self)
    }

    /// Writes a sequence of elements into this buffer.
    fn write_elements<I>(&mut self, elements: I) -> FormatResult<()>
    where
        I: IntoIterator<Item = FormatElement>,
    {
        for element in elements.into_iter() {
            self.write_element(element)?;
        }

        Ok(())
    }
}

impl<T> BufferExtensions for T where T: Buffer {}

#[derive(Debug)]
pub struct Recording<'buf, Buffer> {
    start: usize,
    buffer: &'buf mut Buffer,
}

impl<'buf, B> Recording<'buf, B>
where
    B: Buffer,
{
    fn new(buffer: &'buf mut B) -> Self {
        Self {
            start: buffer.elements().len(),
            buffer,
        }
    }

    #[inline(always)]
    pub fn write_fmt(&mut self, arguments: Arguments<B::Context>) -> FormatResult<()> {
        self.buffer.write_fmt(arguments)
    }

    #[inline(always)]
    pub fn write_element(&mut self, element: FormatElement) -> FormatResult<()> {
        self.buffer.write_element(element)
    }

    pub fn stop(self) -> Recorded<'buf> {
        let buffer: &'buf B = self.buffer;
        let elements = buffer.elements();

        let recorded = if self.start > elements.len() {
            // May happen if buffer was rewinded.
            &[]
        } else {
            &elements[self.start..]
        };

        Recorded(recorded)
    }
}

#[derive(Debug, Copy, Clone)]
pub struct Recorded<'a>(&'a [FormatElement]);

impl Deref for Recorded<'_> {
    type Target = [FormatElement];

    fn deref(&self) -> &Self::Target {
        self.0
    }
}
