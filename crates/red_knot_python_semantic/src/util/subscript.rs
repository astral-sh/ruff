//! This module provides utility functions for indexing (`PyIndex`) and slicing
//! operations (`PySlice`) on iterators, following the semantics of equivalent
//! operations in Python.

use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct OutOfBoundsError;

pub(crate) trait PyIndex {
    type Item;

    fn py_index(&mut self, index: i32) -> Result<Self::Item, OutOfBoundsError>;
}

enum Nth {
    FromStart(usize),
    FromEnd(usize),
}

fn from_nonnegative_i32(index: i32) -> usize {
    static_assertions::const_assert!(usize::BITS >= 32);
    debug_assert!(index >= 0);

    // SAFETY: `index` is non-negative, and `usize` is at least 32 bits.
    usize::try_from(index).unwrap()
}

fn from_negative_i32(index: i32) -> usize {
    static_assertions::const_assert!(usize::BITS >= 32);

    index.checked_neg().map(from_nonnegative_i32).unwrap_or({
        // 'checked_neg' only fails for i32::MIN. We can not
        // represent -i32::MIN as a i32, but we can represent
        // it as a usize, since usize is at least 32 bits.
        from_nonnegative_i32(i32::MAX) + 1
    })
}

impl Nth {
    fn from_index(index: i32) -> Self {
        if index >= 0 {
            Nth::FromStart(from_nonnegative_i32(index))
        } else {
            Nth::FromEnd(from_negative_i32(index) - 1)
        }
    }

    fn to_nonnegative_index(&self, len: usize) -> usize {
        debug_assert!(len > 0);

        match self {
            Nth::FromStart(nth) => *nth,
            Nth::FromEnd(nth_rev) => (len - 1).saturating_sub(*nth_rev),
        }
    }
}

impl<I, T> PyIndex for T
where
    T: DoubleEndedIterator<Item = I>,
{
    type Item = I;

    fn py_index(&mut self, index: i32) -> Result<I, OutOfBoundsError> {
        match Nth::from_index(index) {
            Nth::FromStart(nth) => self.nth(nth).ok_or(OutOfBoundsError),
            Nth::FromEnd(nth_rev) => self.nth_back(nth_rev).ok_or(OutOfBoundsError),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct StepSizeZeroError;

pub(crate) trait PySlice {
    type Item;

    fn py_slice<'a>(
        &'a mut self,
        start: Option<i32>,
        stop: Option<i32>,
        step: Option<i32>,
    ) -> Result<Box<dyn Iterator<Item = Self::Item> + 'a>, StepSizeZeroError>
    where
        Self::Item: 'a;
}

impl<I, T> PySlice for T
where
    T: DoubleEndedIterator<Item = I> + ExactSizeIterator<Item = I> + Iterator<Item = I>,
{
    type Item = I;

    fn py_slice<'a>(
        &'a mut self,
        start: Option<i32>,
        stop: Option<i32>,
        step_int: Option<i32>,
    ) -> Result<Box<dyn Iterator<Item = I> + 'a>, StepSizeZeroError>
    where
        I: 'a,
    {
        let step_int = step_int.unwrap_or(1);
        if step_int == 0 {
            return Err(StepSizeZeroError);
        }

        let len = self.len();
        if len == 0 {
            return Ok(Box::new(std::iter::empty()));
        }

        let to_nonnegative_index = |index| Nth::from_index(index).to_nonnegative_index(len);

        if step_int > 0 {
            let step = from_nonnegative_i32(step_int);
            let start = start.map(to_nonnegative_index).unwrap_or(0).clamp(0, len);
            let stop = stop.map(to_nonnegative_index).unwrap_or(len).clamp(0, len);

            let (skip, take, step) = match start.cmp(&stop) {
                Ordering::Less => (start, stop - start, step),
                Ordering::Equal | Ordering::Greater => (start, 0, step),
            };

            Ok(Box::new(self.skip(skip).take(take).step_by(step)))
        } else {
            let step = from_negative_i32(step_int);
            let start = start
                .map(to_nonnegative_index)
                .unwrap_or(len)
                .clamp(0, len - 1);

            let (skip, take, step) = match stop {
                Some(stop) if i32::try_from(len).map(|len| stop < -len).unwrap_or(false) => {
                    ((len - 1) - start, len, step)
                }
                None => ((len - 1) - start, len, step),
                Some(stop) => {
                    let stop = to_nonnegative_index(stop).clamp(0, len - 1);

                    match start.cmp(&stop) {
                        Ordering::Greater => ((len - 1) - start, start - stop, step),
                        Ordering::Less | Ordering::Equal => (len - start, 0, step),
                    }
                }
            };

            Ok(Box::new(self.rev().skip(skip).take(take).step_by(step)))
        }
    }
}

#[cfg(test)]
#[allow(clippy::redundant_clone)]
mod tests {
    use crate::util::subscript::{OutOfBoundsError, StepSizeZeroError};

    use super::{PyIndex, PySlice};
    use itertools::assert_equal;

    #[test]
    fn py_index_empty() {
        let iter = std::iter::empty::<char>();

        assert_eq!(iter.clone().py_index(0), Err(OutOfBoundsError));
        assert_eq!(iter.clone().py_index(1), Err(OutOfBoundsError));
        assert_eq!(iter.clone().py_index(-1), Err(OutOfBoundsError));
        assert_eq!(iter.clone().py_index(i32::MIN), Err(OutOfBoundsError));
        assert_eq!(iter.clone().py_index(i32::MAX), Err(OutOfBoundsError));
    }

    #[test]
    fn py_index_single_element() {
        let iter = ['a'].into_iter();

        assert_eq!(iter.clone().py_index(0), Ok('a'));
        assert_eq!(iter.clone().py_index(1), Err(OutOfBoundsError));
        assert_eq!(iter.clone().py_index(-1), Ok('a'));
        assert_eq!(iter.clone().py_index(-2), Err(OutOfBoundsError));
    }

    #[test]
    fn py_index_more_elements() {
        let iter = ['a', 'b', 'c', 'd', 'e'].into_iter();

        assert_eq!(iter.clone().py_index(0), Ok('a'));
        assert_eq!(iter.clone().py_index(1), Ok('b'));
        assert_eq!(iter.clone().py_index(4), Ok('e'));
        assert_eq!(iter.clone().py_index(5), Err(OutOfBoundsError));

        assert_eq!(iter.clone().py_index(-1), Ok('e'));
        assert_eq!(iter.clone().py_index(-2), Ok('d'));
        assert_eq!(iter.clone().py_index(-5), Ok('a'));
        assert_eq!(iter.clone().py_index(-6), Err(OutOfBoundsError));
    }

    #[test]
    fn py_index_uses_full_index_range() {
        let iter = 0..=u32::MAX;

        // u32::MAX - |i32::MIN| + 1 = 2^32 - 1 - 2^31 + 1 = 2^31
        assert_eq!(iter.clone().py_index(i32::MIN), Ok(2u32.pow(31)));
        assert_eq!(iter.clone().py_index(-2), Ok(u32::MAX - 2 + 1));
        assert_eq!(iter.clone().py_index(-1), Ok(u32::MAX - 1 + 1));

        assert_eq!(iter.clone().py_index(0), Ok(0));
        assert_eq!(iter.clone().py_index(1), Ok(1));
        assert_eq!(iter.clone().py_index(i32::MAX), Ok(i32::MAX as u32));
    }

    #[track_caller]
    fn assert_eq_slice<const N: usize, const M: usize>(
        input: &[char; N],
        start: Option<i32>,
        stop: Option<i32>,
        step: Option<i32>,
        expected: &[char; M],
    ) {
        assert_equal(
            input.iter().py_slice(start, stop, step).unwrap(),
            expected.iter(),
        );
    }

    #[test]
    fn py_slice_empty_input() {
        let input = [];

        assert_eq_slice(&input, None, None, None, &[]);
        assert_eq_slice(&input, Some(0), None, None, &[]);
        assert_eq_slice(&input, None, Some(0), None, &[]);
        assert_eq_slice(&input, Some(0), Some(0), None, &[]);
        assert_eq_slice(&input, Some(-5), Some(-5), None, &[]);
        assert_eq_slice(&input, None, None, Some(-1), &[]);
        assert_eq_slice(&input, None, None, Some(2), &[]);
    }

    #[test]
    fn py_slice_single_element_input() {
        let input = ['a'];

        assert_eq_slice(&input, None, None, None, &['a']);

        assert_eq_slice(&input, Some(0), None, None, &['a']);
        assert_eq_slice(&input, None, Some(0), None, &[]);
        assert_eq_slice(&input, Some(0), Some(0), None, &[]);
        assert_eq_slice(&input, Some(0), Some(1), None, &['a']);
        assert_eq_slice(&input, Some(0), Some(2), None, &['a']);

        assert_eq_slice(&input, Some(-1), None, None, &['a']);
        assert_eq_slice(&input, Some(-1), Some(-1), None, &[]);
        assert_eq_slice(&input, Some(-1), Some(0), None, &[]);
        assert_eq_slice(&input, Some(-1), Some(1), None, &['a']);
        assert_eq_slice(&input, Some(-1), Some(2), None, &['a']);
        assert_eq_slice(&input, None, Some(-1), None, &[]);

        assert_eq_slice(&input, Some(-2), None, None, &['a']);
        assert_eq_slice(&input, Some(-2), Some(-1), None, &[]);
        assert_eq_slice(&input, Some(-2), Some(0), None, &[]);
        assert_eq_slice(&input, Some(-2), Some(1), None, &['a']);
        assert_eq_slice(&input, Some(-2), Some(2), None, &['a']);
    }

    #[test]
    fn py_slice_nonnegative_indices() {
        let input = ['a', 'b', 'c', 'd', 'e'];

        assert_eq_slice(&input, None, Some(0), None, &[]);
        assert_eq_slice(&input, None, Some(1), None, &['a']);
        assert_eq_slice(&input, None, Some(4), None, &['a', 'b', 'c', 'd']);
        assert_eq_slice(&input, None, Some(5), None, &['a', 'b', 'c', 'd', 'e']);
        assert_eq_slice(&input, None, Some(6), None, &['a', 'b', 'c', 'd', 'e']);
        assert_eq_slice(&input, None, None, None, &['a', 'b', 'c', 'd', 'e']);

        assert_eq_slice(&input, Some(0), Some(0), None, &[]);
        assert_eq_slice(&input, Some(0), Some(1), None, &['a']);
        assert_eq_slice(&input, Some(0), Some(4), None, &['a', 'b', 'c', 'd']);
        assert_eq_slice(&input, Some(0), Some(5), None, &['a', 'b', 'c', 'd', 'e']);
        assert_eq_slice(&input, Some(0), Some(6), None, &['a', 'b', 'c', 'd', 'e']);
        assert_eq_slice(&input, Some(0), None, None, &['a', 'b', 'c', 'd', 'e']);

        assert_eq_slice(&input, Some(1), Some(0), None, &[]);
        assert_eq_slice(&input, Some(1), Some(1), None, &[]);
        assert_eq_slice(&input, Some(1), Some(2), None, &['b']);
        assert_eq_slice(&input, Some(1), Some(4), None, &['b', 'c', 'd']);
        assert_eq_slice(&input, Some(1), Some(5), None, &['b', 'c', 'd', 'e']);
        assert_eq_slice(&input, Some(1), Some(6), None, &['b', 'c', 'd', 'e']);
        assert_eq_slice(&input, Some(1), None, None, &['b', 'c', 'd', 'e']);

        assert_eq_slice(&input, Some(4), Some(0), None, &[]);
        assert_eq_slice(&input, Some(4), Some(4), None, &[]);
        assert_eq_slice(&input, Some(4), Some(5), None, &['e']);
        assert_eq_slice(&input, Some(4), Some(6), None, &['e']);
        assert_eq_slice(&input, Some(4), None, None, &['e']);

        assert_eq_slice(&input, Some(5), Some(0), None, &[]);
        assert_eq_slice(&input, Some(5), Some(5), None, &[]);
        assert_eq_slice(&input, Some(5), Some(6), None, &[]);
        assert_eq_slice(&input, Some(5), None, None, &[]);

        assert_eq_slice(&input, Some(6), Some(0), None, &[]);
        assert_eq_slice(&input, Some(6), Some(6), None, &[]);
        assert_eq_slice(&input, Some(6), None, None, &[]);
    }

    #[test]
    fn py_slice_negatice_indices() {
        let input = ['a', 'b', 'c', 'd', 'e'];

        assert_eq_slice(&input, Some(-6), None, None, &['a', 'b', 'c', 'd', 'e']);
        assert_eq_slice(&input, Some(-6), Some(-1), None, &['a', 'b', 'c', 'd']);
        assert_eq_slice(&input, Some(-6), Some(-4), None, &['a']);
        assert_eq_slice(&input, Some(-6), Some(-5), None, &[]);
        assert_eq_slice(&input, Some(-6), Some(-6), None, &[]);
        assert_eq_slice(&input, Some(-6), Some(-10), None, &[]);

        assert_eq_slice(&input, Some(-5), None, None, &['a', 'b', 'c', 'd', 'e']);
        assert_eq_slice(&input, Some(-5), Some(-1), None, &['a', 'b', 'c', 'd']);
        assert_eq_slice(&input, Some(-5), Some(-4), None, &['a']);
        assert_eq_slice(&input, Some(-5), Some(-5), None, &[]);
        assert_eq_slice(&input, Some(-5), Some(-6), None, &[]);
        assert_eq_slice(&input, Some(-5), Some(-10), None, &[]);

        assert_eq_slice(&input, Some(-4), None, None, &['b', 'c', 'd', 'e']);
        assert_eq_slice(&input, Some(-4), Some(-1), None, &['b', 'c', 'd']);
        assert_eq_slice(&input, Some(-4), Some(-3), None, &['b']);
        assert_eq_slice(&input, Some(-4), Some(-4), None, &[]);
        assert_eq_slice(&input, Some(-4), Some(-10), None, &[]);

        assert_eq_slice(&input, Some(-1), None, None, &['e']);
        assert_eq_slice(&input, Some(-1), Some(-1), None, &[]);
        assert_eq_slice(&input, Some(-1), Some(-10), None, &[]);

        assert_eq_slice(&input, None, Some(-1), None, &['a', 'b', 'c', 'd']);
        assert_eq_slice(&input, None, Some(-4), None, &['a']);
        assert_eq_slice(&input, None, Some(-5), None, &[]);
        assert_eq_slice(&input, None, Some(-6), None, &[]);
    }

    #[test]
    fn py_slice_mixed_positive_negative_indices() {
        let input = ['a', 'b', 'c', 'd', 'e'];

        assert_eq_slice(&input, Some(0), Some(-1), None, &['a', 'b', 'c', 'd']);
        assert_eq_slice(&input, Some(1), Some(-1), None, &['b', 'c', 'd']);
        assert_eq_slice(&input, Some(3), Some(-1), None, &['d']);
        assert_eq_slice(&input, Some(4), Some(-1), None, &[]);
        assert_eq_slice(&input, Some(5), Some(-1), None, &[]);

        assert_eq_slice(&input, Some(0), Some(-4), None, &['a']);
        assert_eq_slice(&input, Some(1), Some(-4), None, &[]);
        assert_eq_slice(&input, Some(3), Some(-4), None, &[]);

        assert_eq_slice(&input, Some(0), Some(-5), None, &[]);
        assert_eq_slice(&input, Some(1), Some(-5), None, &[]);
        assert_eq_slice(&input, Some(3), Some(-5), None, &[]);

        assert_eq_slice(&input, Some(0), Some(-6), None, &[]);
        assert_eq_slice(&input, Some(1), Some(-6), None, &[]);

        assert_eq_slice(&input, Some(-6), Some(6), None, &['a', 'b', 'c', 'd', 'e']);
        assert_eq_slice(&input, Some(-6), Some(5), None, &['a', 'b', 'c', 'd', 'e']);
        assert_eq_slice(&input, Some(-6), Some(4), None, &['a', 'b', 'c', 'd']);
        assert_eq_slice(&input, Some(-6), Some(1), None, &['a']);
        assert_eq_slice(&input, Some(-6), Some(0), None, &[]);

        assert_eq_slice(&input, Some(-5), Some(6), None, &['a', 'b', 'c', 'd', 'e']);
        assert_eq_slice(&input, Some(-5), Some(5), None, &['a', 'b', 'c', 'd', 'e']);
        assert_eq_slice(&input, Some(-5), Some(4), None, &['a', 'b', 'c', 'd']);
        assert_eq_slice(&input, Some(-5), Some(1), None, &['a']);
        assert_eq_slice(&input, Some(-5), Some(0), None, &[]);

        assert_eq_slice(&input, Some(-4), Some(6), None, &['b', 'c', 'd', 'e']);
        assert_eq_slice(&input, Some(-4), Some(5), None, &['b', 'c', 'd', 'e']);
        assert_eq_slice(&input, Some(-4), Some(4), None, &['b', 'c', 'd']);
        assert_eq_slice(&input, Some(-4), Some(2), None, &['b']);
        assert_eq_slice(&input, Some(-4), Some(1), None, &[]);
        assert_eq_slice(&input, Some(-4), Some(0), None, &[]);

        assert_eq_slice(&input, Some(-1), Some(6), None, &['e']);
        assert_eq_slice(&input, Some(-1), Some(5), None, &['e']);
        assert_eq_slice(&input, Some(-1), Some(4), None, &[]);
        assert_eq_slice(&input, Some(-1), Some(1), None, &[]);
    }

    #[test]
    fn py_slice_step_forward() {
        // indices:   0    1    2    3    4    5    6
        let input = ['a', 'b', 'c', 'd', 'e', 'f', 'g'];

        // Step size zero is invalid:
        assert!(matches!(
            input.iter().py_slice(None, None, Some(0)),
            Err(StepSizeZeroError)
        ));
        assert!(matches!(
            input.iter().py_slice(Some(0), Some(5), Some(0)),
            Err(StepSizeZeroError)
        ));
        assert!(matches!(
            input.iter().py_slice(Some(0), Some(0), Some(0)),
            Err(StepSizeZeroError)
        ));

        assert_eq_slice(&input, Some(0), Some(8), Some(2), &['a', 'c', 'e', 'g']);
        assert_eq_slice(&input, Some(0), Some(7), Some(2), &['a', 'c', 'e', 'g']);
        assert_eq_slice(&input, Some(0), Some(6), Some(2), &['a', 'c', 'e']);
        assert_eq_slice(&input, Some(0), Some(5), Some(2), &['a', 'c', 'e']);
        assert_eq_slice(&input, Some(0), Some(4), Some(2), &['a', 'c']);
        assert_eq_slice(&input, Some(0), Some(3), Some(2), &['a', 'c']);
        assert_eq_slice(&input, Some(0), Some(2), Some(2), &['a']);
        assert_eq_slice(&input, Some(0), Some(1), Some(2), &['a']);
        assert_eq_slice(&input, Some(0), Some(0), Some(2), &[]);
        assert_eq_slice(&input, Some(1), Some(5), Some(2), &['b', 'd']);

        assert_eq_slice(&input, Some(0), Some(7), Some(3), &['a', 'd', 'g']);
        assert_eq_slice(&input, Some(0), Some(6), Some(3), &['a', 'd']);

        assert_eq_slice(&input, Some(0), None, Some(10), &['a']);
    }

    #[test]
    fn py_slice_step_backward() {
        // indices:   0    1    2    3    4    5    6
        let input = ['a', 'b', 'c', 'd', 'e', 'f', 'g'];

        assert_eq_slice(&input, Some(7), Some(0), Some(-2), &['g', 'e', 'c']);
        assert_eq_slice(&input, Some(6), Some(0), Some(-2), &['g', 'e', 'c']);
        assert_eq_slice(&input, Some(5), Some(0), Some(-2), &['f', 'd', 'b']);
        assert_eq_slice(&input, Some(4), Some(0), Some(-2), &['e', 'c']);
        assert_eq_slice(&input, Some(3), Some(0), Some(-2), &['d', 'b']);
        assert_eq_slice(&input, Some(2), Some(0), Some(-2), &['c']);
        assert_eq_slice(&input, Some(1), Some(0), Some(-2), &['b']);
        assert_eq_slice(&input, Some(0), Some(0), Some(-2), &[]);

        assert_eq_slice(&input, Some(7), None, Some(-2), &['g', 'e', 'c', 'a']);
        assert_eq_slice(&input, None, None, Some(-2), &['g', 'e', 'c', 'a']);
        assert_eq_slice(&input, None, Some(0), Some(-2), &['g', 'e', 'c']);

        assert_eq_slice(&input, Some(5), Some(1), Some(-2), &['f', 'd']);
        assert_eq_slice(&input, Some(5), Some(2), Some(-2), &['f', 'd']);
        assert_eq_slice(&input, Some(5), Some(3), Some(-2), &['f']);
        assert_eq_slice(&input, Some(5), Some(4), Some(-2), &['f']);
        assert_eq_slice(&input, Some(5), Some(5), Some(-2), &[]);

        assert_eq_slice(&input, Some(6), None, Some(-3), &['g', 'd', 'a']);
        assert_eq_slice(&input, Some(6), Some(0), Some(-3), &['g', 'd']);

        assert_eq_slice(&input, Some(7), None, Some(-10), &['g']);

        assert_eq_slice(&input, Some(-6), Some(-9), Some(-1), &['b', 'a']);
        assert_eq_slice(&input, Some(-6), Some(-8), Some(-1), &['b', 'a']);
        assert_eq_slice(&input, Some(-6), Some(-7), Some(-1), &['b']);
        assert_eq_slice(&input, Some(-6), Some(-6), Some(-1), &[]);
    }
}
