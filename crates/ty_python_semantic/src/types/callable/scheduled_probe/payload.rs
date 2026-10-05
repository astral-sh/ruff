//! Bounds inspection and copying of stored callable payloads before a task resumes.

use super::{Boundary, CallableTypes};
use crate::Db;
use crate::types::signatures::effects::SignatureEffect;

struct Reservation {
    remaining: usize,
    used: usize,
    copies: usize,
}

impl Reservation {
    fn reserve(&mut self, count: usize) -> Result<bool, Boundary> {
        let cost = count
            .checked_mul(4)
            .and_then(|cost| cost.checked_mul(self.copies))
            .ok_or(Boundary::CostOverflow)?;
        let Some(remaining) = self.remaining.checked_sub(cost) else {
            return Ok(false);
        };
        self.remaining = remaining;
        self.used += cost;
        Ok(true)
    }
}

pub(super) struct CallableDebit {
    pub(super) work: usize,
    pub(super) boundary: Option<Boundary>,
}

/// `None` leaves the task pending: even discovering a debit stops at the remaining allowance.
pub(super) fn callable_debit<'db>(
    db: &'db dyn Db,
    callables: &CallableTypes<'db>,
    copies: usize,
    rehash: bool,
    allowance: usize,
) -> Result<Option<CallableDebit>, Boundary> {
    let mut reservation = Reservation {
        remaining: allowance,
        used: 0,
        copies,
    };
    if !reservation.reserve(callables.iter().len())? {
        return Ok(None);
    }
    if !rehash {
        return Ok(Some(CallableDebit {
            work: reservation.used,
            boundary: None,
        }));
    }
    for callable in callables {
        let signatures = callable.signatures(db);
        if !reservation.reserve(signatures.overloads.len())? {
            return Ok(None);
        }
        for signature in signatures {
            // Receiver constraints own a separate graph. Hashing that graph requires its own
            // bounded payload contract before these signatures can be re-interned.
            if signature.receiver_constraints().is_some() {
                return Ok(Some(CallableDebit {
                    work: reservation.used,
                    boundary: Some(Boundary::SignatureEffect(
                        SignatureEffect::ReceiverConstraints,
                    )),
                }));
            }
            let parameters = signature.parameters();
            if !reservation.reserve(parameters.as_slice().len())? {
                return Ok(None);
            }
            for parameter in parameters {
                if let Some(name) = parameter.name()
                    && !reservation.reserve(name.len())?
                {
                    return Ok(None);
                }
            }
        }
    }
    Ok(Some(CallableDebit {
        work: reservation.used,
        boundary: None,
    }))
}
