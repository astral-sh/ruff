pub(crate) fn iterator_at_index<T>(
    mut iter: impl DoubleEndedIterator<Item = T>,
    index: i64,
) -> Option<T> {
    if index < 0 {
        let nth_rev = index
            .checked_neg()
            .and_then(|int| usize::try_from(int).ok())?
            .checked_sub(1)?;

        iter.rev().nth(nth_rev)
    } else {
        let nth = usize::try_from(index).ok()?;
        iter.nth(nth)
    }
}

pub(crate) fn slice_at_index<T>(slice: &[T], index: i64) -> Option<&T> {
    let positive_index = if index < 0 {
        slice.len().checked_sub(
            index
                .checked_neg()
                .and_then(|int| usize::try_from(int).ok())?,
        )
    } else {
        usize::try_from(index).ok()
    };
    slice.get(positive_index?)
}

#[cfg(test)]
mod tests {
    use super::{iterator_at_index, slice_at_index};

    #[test]
    fn iterator_at_index_basic() {
        let iter = 'a'..='e';

        assert_eq!(iterator_at_index(iter.clone(), 0), Some('a'));
        assert_eq!(iterator_at_index(iter.clone(), 1), Some('b'));
        assert_eq!(iterator_at_index(iter.clone(), 4), Some('e'));
        assert_eq!(iterator_at_index(iter.clone(), 5), None);

        assert_eq!(iterator_at_index(iter.clone(), -1), Some('e'));
        assert_eq!(iterator_at_index(iter.clone(), -2), Some('d'));
        assert_eq!(iterator_at_index(iter.clone(), -5), Some('a'));
        assert_eq!(iterator_at_index(iter.clone(), -6), None);
    }

    #[test]
    fn iterator_at_index_empty() {
        let iter = 'a'..'a';

        assert_eq!(iterator_at_index(iter.clone(), 0), None);
        assert_eq!(iterator_at_index(iter.clone(), 1), None);
        assert_eq!(iterator_at_index(iter.clone(), -1), None);
    }

    #[test]
    fn iterator_at_index_single_element() {
        let iter = std::iter::once('a');

        assert_eq!(iterator_at_index(iter.clone(), 0), Some('a'));
        assert_eq!(iterator_at_index(iter.clone(), 1), None);
        assert_eq!(iterator_at_index(iter.clone(), -1), Some('a'));
        assert_eq!(iterator_at_index(iter.clone(), -2), None);
    }

    #[test]
    fn iterator_at_index_uses_full_index_range() {
        let iter = 0..=u64::MAX;

        assert_eq!(iterator_at_index(iter.clone(), 0), Some(0));
        assert_eq!(iterator_at_index(iter.clone(), 1), Some(1));
        assert_eq!(
            iterator_at_index(iter.clone(), i64::MAX),
            Some(i64::MAX as u64)
        );

        assert_eq!(iterator_at_index(iter.clone(), -1), Some(u64::MAX));
        assert_eq!(iterator_at_index(iter.clone(), -2), Some(u64::MAX - 1));
        // i64::MIN is not representable as a positive number, so the
        assert_eq!(
            iterator_at_index(iter.clone(), i64::MIN + 1),
            Some(2u64.pow(63) + 1)
        );
    }

    #[test]
    fn slice_at_index_basic() {
        let slice = &['a', 'b', 'c', 'd', 'e'];

        assert_eq!(slice_at_index(slice, 0), Some(&'a'));
        assert_eq!(slice_at_index(slice, 1), Some(&'b'));
        assert_eq!(slice_at_index(slice, 4), Some(&'e'));
        assert_eq!(slice_at_index(slice, 5), None);

        assert_eq!(slice_at_index(slice, -1), Some(&'e'));
        assert_eq!(slice_at_index(slice, -2), Some(&'d'));
        assert_eq!(slice_at_index(slice, -5), Some(&'a'));
        assert_eq!(slice_at_index(slice, -6), None);
    }

    #[test]
    fn slice_at_index_empty() {
        let slice: &[char] = &[];

        assert_eq!(slice_at_index(slice, 0), None);
        assert_eq!(slice_at_index(slice, 1), None);
        assert_eq!(slice_at_index(slice, -1), None);
    }

    #[test]
    fn slice_at_index_single_element() {
        let slice = &['a'];

        assert_eq!(slice_at_index(slice, 0), Some(&'a'));
        assert_eq!(slice_at_index(slice, 1), None);
        assert_eq!(slice_at_index(slice, -1), Some(&'a'));
        assert_eq!(slice_at_index(slice, -2), None);
    }
}
