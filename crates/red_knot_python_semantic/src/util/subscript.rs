pub(crate) trait PythonSubscript {
    type Item;

    fn subscript_index(&mut self, index: i64) -> Option<Self::Item>;
    fn subscript_slice(
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
}

fn transpose<T>(value: Option<Option<T>>) -> Option<Option<T>> {
    match value {
        Some(Some(value)) => Some(Some(value)),
        Some(None) => None,
        None => Some(None),
    }
}

impl<I, T: DoubleEndedIterator<Item = I>> PythonSubscript for T {
    type Item = I;

    fn subscript_index(&mut self, index: i64) -> Option<I> {
        match Nth::from_index(index)? {
            Nth::FromStart(nth) => self.nth(nth),
            Nth::FromEnd(nth_rev) => self.rev().nth(nth_rev),
        }
    }

    fn subscript_slice(
        &mut self,
        start: Option<i64>,
        stop: Option<i64>,
        step: Option<i64>,
    ) -> Option<impl Iterator<Item = I>> {
        let start = transpose(start.map(Nth::from_index))?;
        let stop = transpose(stop.map(Nth::from_index))?;
        let step = step.map(|step| usize::try_from(step).ok());

        match (start, stop, step) {
            (Some(Nth::FromStart(start)), Some(Nth::FromStart(stop)), None) => {
                Some(self.skip(start).take(stop - start))
            }
            _ => {
                todo!()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PythonSubscript;

    #[test]
    fn subscript_index_basic() {
        let iter = 'a'..='e';

        assert_eq!(iter.clone().subscript_index(0), Some('a'));
        assert_eq!(iter.clone().subscript_index(1), Some('b'));
        assert_eq!(iter.clone().subscript_index(4), Some('e'));
        assert_eq!(iter.clone().subscript_index(5), None);

        assert_eq!(iter.clone().subscript_index(-1), Some('e'));
        assert_eq!(iter.clone().subscript_index(-2), Some('d'));
        assert_eq!(iter.clone().subscript_index(-5), Some('a'));
        assert_eq!(iter.clone().subscript_index(-6), None);
    }

    #[test]
    fn subscript_index_empty() {
        let iter = 'a'..'a';

        assert_eq!(iter.clone().subscript_index(0), None);
        assert_eq!(iter.clone().subscript_index(1), None);
        assert_eq!(iter.clone().subscript_index(-1), None);
    }

    #[test]
    fn subscript_index_single_element() {
        let iter = 'a'..='a';

        assert_eq!(iter.clone().subscript_index(0), Some('a'));
        assert_eq!(iter.clone().subscript_index(1), None);
        assert_eq!(iter.clone().subscript_index(-1), Some('a'));
        assert_eq!(iter.clone().subscript_index(-2), None);
    }

    #[test]
    fn subscript_index_uses_full_index_range() {
        let iter = 0..=u64::MAX;

        assert_eq!(iter.clone().subscript_index(0), Some(0));
        assert_eq!(iter.clone().subscript_index(1), Some(1));
        assert_eq!(
            iter.clone().subscript_index(i64::MAX),
            Some(i64::MAX as u64)
        );

        assert_eq!(iter.clone().subscript_index(-1), Some(u64::MAX));
        assert_eq!(iter.clone().subscript_index(-2), Some(u64::MAX - 1));

        // i64::MIN is not representable as a positive number, so it is not
        // a valid index:
        assert_eq!(iter.clone().subscript_index(i64::MIN), None);

        // but i64::MIN +1 is:
        assert_eq!(
            iter.clone().subscript_index(i64::MIN + 1),
            Some(2u64.pow(63) + 1)
        );
    }

    #[test]
    fn subscript_slice_basic() {
        let iter = 'a'..='e';

        itertools::assert_equal(
            iter.clone()
                .subscript_slice(Some(0), Some(0), None)
                .unwrap(),
            [],
        );
        itertools::assert_equal(
            iter.clone()
                .subscript_slice(Some(0), Some(1), None)
                .unwrap(),
            ['a'],
        );
        itertools::assert_equal(
            iter.clone()
                .subscript_slice(Some(0), Some(4), None)
                .unwrap(),
            ['a', 'b', 'c', 'd'],
        );
    }
}
