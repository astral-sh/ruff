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
