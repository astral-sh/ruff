//! In-place sorting with admission before scalar transitions, comparisons, and element moves.

use std::cmp::Ordering;
use std::convert::Infallible;

#[derive(Clone, Copy, Debug)]
pub(in crate::types) enum SortStep {
    Initialize,
    Build,
    Sift,
    Descend,
    Extract,
}

impl SortStep {
    #[cfg(feature = "experimental-analysis")]
    pub(in crate::types) const fn work(self) -> usize {
        match self {
            // Read the length and divide it to select the first heap root.
            Self::Initialize => 2,
            // Test the remaining roots and decrement the next root index.
            Self::Build => 2,
            // Divide/test the parent bound (2), compute both children (3), test the right
            // bound (1), inspect two comparison results (2), select a child (1), and
            // bounds-check four comparison operands (4). Element comparisons are charged separately.
            Self::Sift => 13,
            // Replace the root index after an admitted swap.
            Self::Descend => 1,
            // Test the remaining heap length and decrement its exclusive end.
            Self::Extract => 2,
        }
    }
}

pub(in crate::types) trait SortControl<T> {
    type Error;

    fn checkpoint(&self, step: SortStep) -> Result<(), Self::Error>;
    fn compare(&self, left: &T, right: &T) -> Result<Ordering, Self::Error>;
    fn before_swap(&self) -> Result<(), Self::Error>;
}

pub(in crate::types) struct OrdinarySort;

impl<T: Ord> SortControl<T> for OrdinarySort {
    type Error = Infallible;

    fn checkpoint(&self, _step: SortStep) -> Result<(), Infallible> {
        Ok(())
    }

    fn compare(&self, left: &T, right: &T) -> Result<Ordering, Infallible> {
        Ok(left.cmp(right))
    }

    fn before_swap(&self) -> Result<(), Infallible> {
        Ok(())
    }
}

/// Sorts the slice in ascending order according to `control`.
///
/// If admission or comparison fails, the slice can be partially reordered but retains every element.
pub(in crate::types) fn heapsort_with<T, C: SortControl<T>>(
    values: &mut [T],
    control: &C,
) -> Result<(), C::Error> {
    control.checkpoint(SortStep::Initialize)?;
    let mut end = values.len();
    let mut start = end / 2;
    loop {
        control.checkpoint(SortStep::Build)?;
        if start == 0 {
            break;
        }
        start -= 1;
        sift_down(values, start, end, control)?;
    }
    loop {
        control.checkpoint(SortStep::Extract)?;
        if end <= 1 {
            return Ok(());
        }
        end -= 1;
        control.before_swap()?;
        values.swap(0, end);
        sift_down(values, 0, end, control)?;
    }
}

fn sift_down<T, C: SortControl<T>>(
    values: &mut [T],
    mut root: usize,
    end: usize,
    control: &C,
) -> Result<(), C::Error> {
    loop {
        control.checkpoint(SortStep::Sift)?;
        if root >= end / 2 {
            return Ok(());
        }
        // The parent bound proves left < end and right <= end without overflowing.
        let left = 2 * root + 1;
        let right = left + 1;
        let child =
            if right < end && control.compare(&values[left], &values[right])? == Ordering::Less {
                right
            } else {
                left
            };
        if control.compare(&values[root], &values[child])? != Ordering::Less {
            return Ok(());
        }
        control.before_swap()?;
        values.swap(root, child);
        control.checkpoint(SortStep::Descend)?;
        root = child;
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    const CASES: &[&[i32]] = &[
        &[],
        &[1],
        &[2, 1, 2, 0, 1],
        &[0, 1, 2, 3, 4],
        &[4, 3, 2, 1, 0],
        &[3, 0, 4, 1, 5, 2],
    ];

    #[test]
    fn agrees_with_slice_sorting() {
        for input in CASES {
            let mut expected = input.to_vec();
            expected.sort_unstable();
            let mut actual = input.to_vec();
            assert_eq!(heapsort_with(&mut actual, &OrdinarySort), Ok(()));
            assert_eq!(actual, expected);
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Refusal {
        Transition,
        Comparison,
        Swap,
    }

    struct LimitedSort {
        remaining: Cell<usize>,
        admitted: Cell<usize>,
    }

    impl LimitedSort {
        fn admit(&self, refusal: Refusal) -> Result<(), Refusal> {
            let Some(remaining) = self.remaining.get().checked_sub(1) else {
                return Err(refusal);
            };
            self.remaining.set(remaining);
            self.admitted.set(self.admitted.get() + 1);
            Ok(())
        }
    }

    impl SortControl<i32> for LimitedSort {
        type Error = Refusal;

        fn checkpoint(&self, _step: SortStep) -> Result<(), Refusal> {
            self.admit(Refusal::Transition)
        }

        fn compare(&self, left: &i32, right: &i32) -> Result<Ordering, Refusal> {
            self.admit(Refusal::Comparison)?;
            Ok(left.cmp(right))
        }

        fn before_swap(&self) -> Result<(), Refusal> {
            self.admit(Refusal::Swap)
        }
    }

    #[test]
    fn refusal_at_each_boundary_preserves_the_owned_permutation() {
        let mut comparison_refused = false;
        let mut swap_refused = false;
        for input in CASES {
            let funded = LimitedSort {
                remaining: Cell::new(usize::MAX),
                admitted: Cell::new(0),
            };
            let mut expected = input.to_vec();
            assert_eq!(heapsort_with(&mut expected, &funded), Ok(()));
            for allowance in 0..funded.admitted.get() {
                let control = LimitedSort {
                    remaining: Cell::new(allowance),
                    admitted: Cell::new(0),
                };
                let mut actual = input.to_vec();
                let refused = heapsort_with(&mut actual, &control);
                comparison_refused |= refused == Err(Refusal::Comparison);
                swap_refused |= refused == Err(Refusal::Swap);
                assert!(refused.is_err());
                assert_eq!(control.admitted.get(), allowance);
                actual.sort_unstable();
                assert_eq!(actual, expected);
            }
        }
        assert!(comparison_refused && swap_refused);
    }
}
