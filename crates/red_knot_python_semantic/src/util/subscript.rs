pub(crate) trait PythonSubscript {
    type Item;

    fn py_index(&mut self, index: i64) -> Option<Self::Item>;
    fn py_slice(
        &mut self,
        start: Option<i64>,
        stop: Option<i64>,
        step: Option<i64>,
    ) -> Option<impl Iterator<Item = Self::Item>>;
}

enum Nth {
    FromStart(usize),
    FromEnd(usize),
}

impl Nth {
    fn from_index(index: i64) -> Option<Self> {
        if index >= 0 {
            Some(Nth::FromStart(usize::try_from(index).ok()?))
        } else {
            let nth_rev = usize::try_from(index.checked_neg()?).ok()?.checked_sub(1)?;
            Some(Nth::FromEnd(nth_rev))
        }
    }

    fn to_nonnegative_index(&self, len: usize) -> usize {
        match self {
            Nth::FromStart(nth) => *nth,
            Nth::FromEnd(nth_rev) => len - (*nth_rev).min(len - 1) - 1,
        }
    }
}

fn transpose<T>(value: Option<Option<T>>) -> Option<Option<T>> {
    match value {
        Some(Some(value)) => Some(Some(value)),
        Some(None) => None,
        None => Some(None),
    }
}

impl<I, T> PythonSubscript for T
where
    T: DoubleEndedIterator<Item = I> + ExactSizeIterator<Item = I>,
{
    type Item = I;

    fn py_index(&mut self, index: i64) -> Option<I> {
        match Nth::from_index(index)? {
            Nth::FromStart(nth) => self.nth(nth),
            Nth::FromEnd(nth_rev) => self.rev().nth(nth_rev),
        }
    }

    fn py_slice(
        &mut self,
        start: Option<i64>,
        stop: Option<i64>,
        step_int: Option<i64>,
    ) -> Option<impl Iterator<Item = I>> {
        let len = self.len();

        let start = transpose(start.map(Nth::from_index))?
            .map(|start| start.to_nonnegative_index(len))
            .unwrap_or(0)
            .clamp(0, len);
        let stop = transpose(stop.map(Nth::from_index))?
            .map(|stop| stop.to_nonnegative_index(len))
            .unwrap_or(len)
            .clamp(0, len);

        let step_int = step_int.unwrap_or(1);
        if step_int == 0 {
            return None;
        }

        let (skip, take_n, step) = if step_int > 0 {
            let step = usize::try_from(step_int).ok()?;

            if start == stop {
                (start, 0, step)
            } else if start < stop {
                (start, stop - start, step)
            } else {
                (stop + 1, 0, step)
            }
        } else {
            todo!()
        };

        Some(self.skip(skip).take(take_n).step_by(step))
    }
}

#[cfg(test)]
mod tests {
    use super::PythonSubscript;
    use itertools::assert_equal;

    #[test]
    fn subscript_index_basic() {
        let iter = ['a', 'b', 'c', 'd', 'e'].into_iter();

        assert_eq!(iter.clone().py_index(0), Some('a'));
        assert_eq!(iter.clone().py_index(1), Some('b'));
        assert_eq!(iter.clone().py_index(4), Some('e'));
        assert_eq!(iter.clone().py_index(5), None);

        assert_eq!(iter.clone().py_index(-1), Some('e'));
        assert_eq!(iter.clone().py_index(-2), Some('d'));
        assert_eq!(iter.clone().py_index(-5), Some('a'));
        assert_eq!(iter.clone().py_index(-6), None);
    }

    #[test]
    fn subscript_index_empty() {
        let iter = std::iter::empty::<char>();

        assert!(iter.clone().py_index(0).is_none());
        assert!(iter.clone().py_index(1).is_none());
        assert!(iter.clone().py_index(-1).is_none());
    }

    #[test]
    fn subscript_index_single_element() {
        let iter = ['a'].into_iter();

        assert_eq!(iter.clone().py_index(0), Some('a'));
        assert_eq!(iter.clone().py_index(1), None);
        assert_eq!(iter.clone().py_index(-1), Some('a'));
        assert_eq!(iter.clone().py_index(-2), None);
    }

    // #[test]
    // fn subscript_index_uses_full_index_range() {
    //     let iter = 0..=u64::MAX;

    //     assert_eq!(iter.clone().subscript_index(0), Some(0));
    //     assert_eq!(iter.clone().subscript_index(1), Some(1));
    //     assert_eq!(
    //         iter.clone().subscript_index(i64::MAX),
    //         Some(i64::MAX as u64)
    //     );

    //     assert_eq!(iter.clone().subscript_index(-1), Some(u64::MAX));
    //     assert_eq!(iter.clone().subscript_index(-2), Some(u64::MAX - 1));

    //     // i64::MIN is not representable as a positive number, so it is not
    //     // a valid index:
    //     assert_eq!(iter.clone().subscript_index(i64::MIN), None);

    //     // but i64::MIN +1 is:
    //     assert_eq!(
    //         iter.clone().subscript_index(i64::MIN + 1),
    //         Some(2u64.pow(63) + 1)
    //     );
    // }

    #[track_caller]
    fn assert_eq_slice<const N: usize, const M: usize>(
        input: &[char; N],
        start: Option<i64>,
        stop: Option<i64>,
        step: Option<i64>,
        expected: &[char; M],
    ) {
        assert_equal(
            input.iter().py_slice(start, stop, step).unwrap(),
            expected.iter(),
        );
    }

    #[track_caller]
    fn assert_slice_returns_none<const N: usize>(
        input: &[char; N],
        start: Option<i64>,
        stop: Option<i64>,
        step: Option<i64>,
    ) {
        assert!(input.iter().py_slice(start, stop, step).is_none());
    }

    #[test]
    fn py_slice_basic() {
        let input = ['a', 'b', 'c', 'd', 'e'];

        assert_eq_slice(&input, Some(0), Some(0), None, &[]);
        assert_eq_slice(&input, Some(0), Some(1), None, &['a']);
        assert_eq_slice(&input, Some(0), Some(4), None, &['a', 'b', 'c', 'd']);
        assert_eq_slice(&input, Some(0), Some(5), None, &['a', 'b', 'c', 'd', 'e']);
        assert_eq_slice(&input, Some(0), Some(6), None, &['a', 'b', 'c', 'd', 'e']);
        assert_eq_slice(&input, Some(1), Some(3), None, &['b', 'c']);

        assert_eq_slice(&input, Some(1), Some(1), None, &[]);
        assert_eq_slice(&input, Some(2), Some(1), None, &[]);

        assert_eq_slice(&input, None, None, None, &['a', 'b', 'c', 'd', 'e']);
        assert_eq_slice(&input, Some(1), None, None, &['b', 'c', 'd', 'e']);
        assert_eq_slice(&input, None, Some(3), None, &['a', 'b', 'c']);
        assert_eq_slice(&input, None, Some(6), None, &['a', 'b', 'c', 'd', 'e']);
    }

    #[test]
    fn py_slice_negatice_indices() {
        let input = ['a', 'b', 'c', 'd', 'e'];

        assert_eq_slice(&input, Some(-5), Some(-5), None, &[]);
        assert_eq_slice(&input, Some(-5), Some(-3), None, &['a', 'b']);
        assert_eq_slice(&input, Some(-5), Some(-1), None, &['a', 'b', 'c', 'd']);
        assert_eq_slice(&input, Some(-6), Some(-1), None, &['a', 'b', 'c', 'd']);
        assert_eq_slice(&input, Some(-3), Some(-1), None, &['c', 'd']);

        assert_eq_slice(&input, Some(-3), Some(-5), None, &[]);
        assert_eq_slice(&input, Some(-2), Some(-7), None, &[]);
    }

    #[test]
    fn py_slice_mixed_positive_negative_indices() {
        let input = ['a', 'b', 'c', 'd', 'e'];

        assert_eq_slice(&input, Some(0), Some(-1), None, &['a', 'b', 'c', 'd']);
        assert_eq_slice(&input, Some(1), Some(-1), None, &['b', 'c', 'd']);
        assert_eq_slice(&input, Some(3), Some(-1), None, &['d']);
        assert_eq_slice(&input, Some(4), Some(-1), None, &[]);
        assert_eq_slice(&input, Some(6), Some(-1), None, &[]);

        assert_eq_slice(&input, Some(-5), Some(3), None, &['a', 'b', 'c']);
        assert_eq_slice(&input, Some(-5), Some(1), None, &['a']);
        assert_eq_slice(&input, Some(-5), Some(0), None, &[]);

        assert_eq_slice(&input, Some(-7), Some(7), None, &['a', 'b', 'c', 'd', 'e']);
    }

    #[test]
    fn py_slice_step() {
        // indices:   0    1    2    3    4    5    6
        let input = ['a', 'b', 'c', 'd', 'e', 'f', 'g'];

        // Step size zero is invalid:
        assert_slice_returns_none(&input, Some(0), Some(5), Some(0));
        assert_slice_returns_none(&input, Some(0), Some(0), Some(0));

        assert_eq_slice(&input, Some(0), Some(7), Some(2), &['a', 'c', 'e', 'g']);
        assert_eq_slice(&input, Some(0), Some(6), Some(2), &['a', 'c', 'e']);
        assert_eq_slice(&input, Some(0), Some(5), Some(2), &['a', 'c', 'e']);
        assert_eq_slice(&input, Some(0), Some(4), Some(2), &['a', 'c']);
        assert_eq_slice(&input, Some(0), Some(3), Some(2), &['a', 'c']);
        assert_eq_slice(&input, Some(0), Some(2), Some(2), &['a']);
        assert_eq_slice(&input, Some(0), Some(1), Some(2), &['a']);
        assert_eq_slice(&input, Some(1), Some(5), Some(2), &['b', 'd']);

        assert_eq_slice(&input, Some(0), Some(7), Some(3), &['a', 'd', 'g']);
        assert_eq_slice(&input, Some(0), Some(6), Some(3), &['a', 'd']);
    }
}
