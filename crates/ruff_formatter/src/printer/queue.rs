use crate::format_element::tag::TagKind;
use crate::prelude::Tag;
use crate::printer::{invalid_end_tag, invalid_start_tag};
use crate::{FormatElement, PrintResult};
use std::fmt::Debug;
use std::iter::FusedIterator;
use std::marker::PhantomData;

/// Queue of [`FormatElement`]s.
pub(super) trait Queue<'a> {
    /// Pops the element at the end of the queue.
    fn pop(&mut self) -> Option<&'a FormatElement>;

    /// Returns the next element, not traversing into [`FormatElement::Interned`].
    fn top_with_interned(&self) -> Option<&'a FormatElement>;

    /// Returns the next element, recursively resolving the first element of [`FormatElement::Interned`].
    fn top(&self) -> Option<&'a FormatElement> {
        let mut top = self.top_with_interned();

        while let Some(FormatElement::Interned(interned)) = top {
            top = interned.first();
        }

        top
    }

    /// Queues a single element to process before the other elements in this queue.
    fn push(&mut self, element: &'a FormatElement) {
        self.extend_back(std::slice::from_ref(element));
    }

    /// Queues a slice of elements to process before the other elements in this queue.
    fn extend_back(&mut self, elements: &'a [FormatElement]);

    /// Removes top slice.
    fn pop_slice(&mut self) -> Option<&'a [FormatElement]>;

    /// Skips all content until it finds the corresponding end tag with the given kind.
    fn skip_content(&mut self, kind: TagKind)
    where
        Self: Sized,
    {
        let iter = self.iter_content(kind);

        for _ in iter {
            // consume whole iterator until end
        }
    }

    /// Iterates over all elements until it finds the matching end tag of the specified kind.
    fn iter_content<'q>(&'q mut self, kind: TagKind) -> QueueContentIterator<'a, 'q, Self>
    where
        Self: Sized,
    {
        QueueContentIterator::new(self, kind)
    }
}

/// Queue with the elements to print.
#[derive(Debug, Default, Clone)]
pub(super) struct PrintQueue<'a> {
    element_slices: Vec<std::slice::Iter<'a, FormatElement>>,
}

impl<'a> PrintQueue<'a> {
    pub(super) fn new(slice: &'a [FormatElement]) -> Self {
        Self {
            element_slices: if slice.is_empty() {
                Vec::new()
            } else {
                vec![slice.iter()]
            },
        }
    }
}

impl<'a> Queue<'a> for PrintQueue<'a> {
    fn pop(&mut self) -> Option<&'a FormatElement> {
        let elements = self.element_slices.last_mut()?;
        elements.next().or_else(
            #[cold]
            || {
                self.element_slices.pop();
                let elements = self.element_slices.last_mut()?;
                elements.next()
            },
        )
    }

    fn top_with_interned(&self) -> Option<&'a FormatElement> {
        let mut slices = self.element_slices.iter().rev();
        let slice = slices.next()?;

        slice.as_slice().first().or_else(
            #[cold]
            || {
                slices
                    .next()
                    .and_then(|next_elements| next_elements.as_slice().first())
            },
        )
    }

    fn extend_back(&mut self, elements: &'a [FormatElement]) {
        if !elements.is_empty() {
            self.element_slices.push(elements.iter());
        }
    }

    /// Removes top slice.
    fn pop_slice(&mut self) -> Option<&'a [FormatElement]> {
        self.element_slices
            .pop()
            .map(|elements| elements.as_slice())
    }
}

/// Queue for measuring if an element fits on the line.
///
/// The queue is a view on top of the [`PrintQueue`] because no elements should be removed
/// from the [`PrintQueue`] while measuring.
#[must_use]
#[derive(Debug)]
pub(super) struct FitsQueue<'a, 'print> {
    queue: PrintQueue<'a>,
    rest_elements: std::slice::Iter<'print, std::slice::Iter<'a, FormatElement>>,
}

impl<'a, 'print> FitsQueue<'a, 'print> {
    pub(super) fn new(
        rest_queue: &'print PrintQueue<'a>,
        queue_vec: Vec<std::slice::Iter<'a, FormatElement>>,
    ) -> Self {
        Self {
            queue: PrintQueue {
                element_slices: queue_vec,
            },
            rest_elements: rest_queue.element_slices.iter(),
        }
    }

    pub(super) fn finish(self) -> Vec<std::slice::Iter<'a, FormatElement>> {
        self.queue.element_slices
    }
}

impl<'a, 'print> Queue<'a> for FitsQueue<'a, 'print> {
    fn pop(&mut self) -> Option<&'a FormatElement> {
        self.queue.pop().or_else(
            #[cold]
            || {
                if let Some(next_slice) = self.rest_elements.next_back() {
                    self.queue.extend_back(next_slice.as_slice());
                    self.queue.pop()
                } else {
                    None
                }
            },
        )
    }

    fn top_with_interned(&self) -> Option<&'a FormatElement> {
        self.queue.top_with_interned().or_else(
            #[cold]
            || {
                if let Some(next_elements) = self.rest_elements.as_slice().last() {
                    next_elements.as_slice().first()
                } else {
                    None
                }
            },
        )
    }

    fn extend_back(&mut self, elements: &'a [FormatElement]) {
        if !elements.is_empty() {
            self.queue.extend_back(elements);
        }
    }

    /// Removes top slice.
    fn pop_slice(&mut self) -> Option<&'a [FormatElement]> {
        self.queue.pop_slice().or_else(|| {
            self.rest_elements
                .next_back()
                .map(std::slice::Iter::as_slice)
        })
    }
}

pub(super) struct QueueContentIterator<'a, 'q, Q: Queue<'a>> {
    queue: &'q mut Q,
    kind: TagKind,
    depth: usize,
    lifetime: PhantomData<&'a ()>,
}

impl<'a, 'q, Q> QueueContentIterator<'a, 'q, Q>
where
    Q: Queue<'a>,
{
    fn new(queue: &'q mut Q, kind: TagKind) -> Self {
        Self {
            queue,
            kind,
            depth: 1,
            lifetime: PhantomData,
        }
    }
}

impl<'a, Q> Iterator for QueueContentIterator<'a, '_, Q>
where
    Q: Queue<'a>,
{
    type Item = &'a FormatElement;

    fn next(&mut self) -> Option<Self::Item> {
        if self.depth == 0 {
            None
        } else {
            let mut top = self.queue.pop();

            while let Some(FormatElement::Interned(interned)) = top {
                self.queue.extend_back(interned);
                top = self.queue.pop();
            }

            match top.expect("Missing end signal.") {
                element @ FormatElement::Tag(tag) if tag.kind() == self.kind => {
                    if tag.is_start() {
                        self.depth += 1;
                    } else {
                        self.depth -= 1;

                        if self.depth == 0 {
                            return None;
                        }
                    }

                    Some(element)
                }
                element => Some(element),
            }
        }
    }
}

impl<'a, Q> FusedIterator for QueueContentIterator<'a, '_, Q> where Q: Queue<'a> {}

/// A predicate determining when to end measuring if some content fits on the line.
///
/// Called for every [`element`](FormatElement) in the [`FitsQueue`] when measuring if a content
/// fits on the line. The measuring of the content ends after the first element [`element`](FormatElement) for which this
/// predicate returns `true` (similar to a take while iterator except that it takes while the predicate returns `false`).
pub(super) trait FitsEndPredicate {
    fn is_end(&mut self, element: &FormatElement) -> PrintResult<bool>;
}

/// Filter that includes all elements until it reaches the end of the document.
pub(super) struct AllPredicate;

impl FitsEndPredicate for AllPredicate {
    fn is_end(&mut self, _element: &FormatElement) -> PrintResult<bool> {
        Ok(false)
    }
}

/// Filter that takes all elements between two matching [`Tag::StartEntry`] and [`Tag::EndEntry`] tags.
#[derive(Debug)]
pub(super) enum SingleEntryPredicate {
    Entry { depth: usize },
    Done,
}

impl SingleEntryPredicate {
    pub(super) const fn is_done(&self) -> bool {
        matches!(self, SingleEntryPredicate::Done)
    }
}

impl Default for SingleEntryPredicate {
    fn default() -> Self {
        SingleEntryPredicate::Entry { depth: 0 }
    }
}

impl FitsEndPredicate for SingleEntryPredicate {
    fn is_end(&mut self, element: &FormatElement) -> PrintResult<bool> {
        let result = match self {
            SingleEntryPredicate::Done => true,
            SingleEntryPredicate::Entry { depth } => match element {
                FormatElement::Tag(Tag::StartEntry) => {
                    *depth += 1;

                    false
                }
                FormatElement::Tag(Tag::EndEntry) => {
                    if *depth == 0 {
                        return invalid_end_tag(TagKind::Entry, None);
                    }

                    *depth -= 1;

                    let is_end = *depth == 0;

                    if is_end {
                        *self = SingleEntryPredicate::Done;
                    }

                    is_end
                }
                FormatElement::Interned(_) => false,
                element if *depth == 0 => {
                    return invalid_start_tag(TagKind::Entry, Some(element));
                }
                _ => false,
            },
        };

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use crate::format_element::LineMode;
    use crate::prelude::Tag;
    use crate::printer::queue::{PrintQueue, Queue};
    use crate::FormatElement;

    #[test]
    fn extend_back_pop_last() {
        let mut queue =
            PrintQueue::new(&[FormatElement::Tag(Tag::StartEntry), FormatElement::Space]);

        assert_eq!(queue.pop(), Some(&FormatElement::Tag(Tag::StartEntry)));

        queue.extend_back(&[FormatElement::Line(LineMode::SoftOrSpace)]);

        assert_eq!(
            queue.pop(),
            Some(&FormatElement::Line(LineMode::SoftOrSpace))
        );
        assert_eq!(queue.pop(), Some(&FormatElement::Space));

        assert_eq!(queue.pop(), None);
    }

    #[test]
    fn extend_back_empty_queue() {
        let mut queue =
            PrintQueue::new(&[FormatElement::Tag(Tag::StartEntry), FormatElement::Space]);

        assert_eq!(queue.pop(), Some(&FormatElement::Tag(Tag::StartEntry)));
        assert_eq!(queue.pop(), Some(&FormatElement::Space));

        queue.extend_back(&[FormatElement::Line(LineMode::SoftOrSpace)]);

        assert_eq!(
            queue.pop(),
            Some(&FormatElement::Line(LineMode::SoftOrSpace))
        );

        assert_eq!(queue.pop(), None);
    }
}
