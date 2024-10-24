pub(crate) trait PythonSubscript {
    type Item;

    fn python_subscript(&mut self, index: i64) -> Option<Self::Item>;
}

impl<I, T: DoubleEndedIterator<Item = I>> PythonSubscript for T {
    type Item = I;

    fn python_subscript(&mut self, index: i64) -> Option<I> {
        if index >= 0 {
            self.nth(usize::try_from(index).ok()?)
        } else {
            let nth_rev = usize::try_from(index.checked_neg()?).ok()?.checked_sub(1)?;
            self.rev().nth(nth_rev)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PythonSubscript;

    #[test]
    fn python_subscript_basic() {
        let iter = 'a'..='e';

        assert_eq!(iter.clone().python_subscript(0), Some('a'));
        assert_eq!(iter.clone().python_subscript(1), Some('b'));
        assert_eq!(iter.clone().python_subscript(4), Some('e'));
        assert_eq!(iter.clone().python_subscript(5), None);

        assert_eq!(iter.clone().python_subscript(-1), Some('e'));
        assert_eq!(iter.clone().python_subscript(-2), Some('d'));
        assert_eq!(iter.clone().python_subscript(-5), Some('a'));
        assert_eq!(iter.clone().python_subscript(-6), None);
    }

    #[test]
    fn python_subscript_empty() {
        let iter = 'a'..'a';

        assert_eq!(iter.clone().python_subscript(0), None);
        assert_eq!(iter.clone().python_subscript(1), None);
        assert_eq!(iter.clone().python_subscript(-1), None);
    }

    #[test]
    fn python_subscript_single_element() {
        let iter = 'a'..='a';

        assert_eq!(iter.clone().python_subscript(0), Some('a'));
        assert_eq!(iter.clone().python_subscript(1), None);
        assert_eq!(iter.clone().python_subscript(-1), Some('a'));
        assert_eq!(iter.clone().python_subscript(-2), None);
    }

    #[test]
    fn python_subscript_uses_full_index_range() {
        let iter = 0..=u64::MAX;

        assert_eq!(iter.clone().python_subscript(0), Some(0));
        assert_eq!(iter.clone().python_subscript(1), Some(1));
        assert_eq!(
            iter.clone().python_subscript(i64::MAX),
            Some(i64::MAX as u64)
        );

        assert_eq!(iter.clone().python_subscript(-1), Some(u64::MAX));
        assert_eq!(iter.clone().python_subscript(-2), Some(u64::MAX - 1));

        // i64::MIN is not representable as a positive number, so it is not
        // a valid index:
        assert_eq!(iter.clone().python_subscript(i64::MIN), None);

        // but i64::MIN +1 is:
        assert_eq!(
            iter.clone().python_subscript(i64::MIN + 1),
            Some(2u64.pow(63) + 1)
        );
    }
}
