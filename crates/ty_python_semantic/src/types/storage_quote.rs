use std::alloc::Layout;

#[derive(Clone, Copy, Debug, Default)]
pub(in crate::types) struct StorageQuote {
    pub work: usize,
    pub bytes: usize,
}

impl StorageQuote {
    #[cfg(feature = "experimental-analysis")]
    pub(in crate::types) fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            work: self.work.checked_add(other.work)?,
            bytes: self.bytes.checked_add(other.bytes)?,
        })
    }
}

pub(in crate::types) fn sequence_merge<T>(
    len: usize,
    capacity: usize,
    incoming: usize,
) -> Option<StorageQuote> {
    let required = len.checked_add(incoming)?;
    let grows = required > capacity;
    Some(StorageQuote {
        work: incoming
            .checked_mul(3)?
            .checked_add(if grows { len } else { 0 })?
            .checked_add(4)?,
        bytes: if grows {
            capacity
                .checked_mul(2)?
                .max(required)
                .max(4)
                .checked_mul(size_of::<T>())?
        } else {
            0
        },
    })
}

pub(in crate::types) fn buffer_retirement<T>(
    (len, capacity, spilled): (usize, usize, bool),
) -> Option<usize> {
    len.checked_add(if spilled { capacity } else { 0 })?
        .checked_mul(size_of::<T>())?
        .checked_add(4)
}

pub(in crate::types) fn buffer_push_quote<T>(
    storage: (usize, usize, bool),
) -> Option<StorageQuote> {
    let (len, capacity, _) = storage;
    let mut quote = sequence_merge::<T>(len, capacity, 1)?;
    quote.work = quote.work.checked_add(size_of::<T>().checked_mul(2)?)?;
    if quote.bytes != 0 {
        Layout::array::<T>(quote.bytes.checked_div(size_of::<T>())?).ok()?;
        quote.work = quote
            .work
            .checked_add(buffer_retirement::<T>(storage)?)?
            .checked_add(quote.bytes.checked_mul(2)?)?;
    }
    Some(quote)
}
