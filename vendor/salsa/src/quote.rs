/// Why a bounded, passive quotation could not finish.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuoteError {
    Exhausted,
    Overflow,
    Unsupported,
}

/// Work already admitted for one passive quotation pass.
///
/// Consuming this receipt never invokes policy or database callbacks. Quotation code must
/// consume work before each variable visit. The returned payload-work total equals the
/// unbounded quote and is independent of the supplied quantum; consumed fuel pays only for
/// inspecting the metadata used to calculate that total.
/// Unused work remains charged to the enclosing attempt.
#[doc(hidden)]
pub struct QuoteFuel {
    remaining: usize,
}

impl QuoteFuel {
    pub(crate) fn admitted(units: usize) -> Self {
        Self { remaining: units }
    }

    pub fn consume(&mut self, units: usize) -> Result<(), QuoteError> {
        self.remaining = self
            .remaining
            .checked_sub(units)
            .ok_or(QuoteError::Exhausted)?;
        Ok(())
    }
}
