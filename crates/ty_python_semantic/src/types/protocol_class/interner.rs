//! Canonical interface construction with retained, finite collection operands.

use std::collections::BTreeMap;

use ruff_python_ast::name::Name;
use salsa::execution_probe::{FiniteInternedValues, TaskEndpoint};
use salsa::plumbing::function::Configuration;
use salsa::plumbing::interned::FiniteInternedConfiguration;
use salsa::plumbing::{QuoteError, QuoteFuel};

use super::{ProtocolInterface, ProtocolMemberData};
use crate::{Db, Program};

// Member types hash their scalar values and handles without resolving the referenced types.
// The map owns names and passive member data; dropping it cannot execute semantic work. Debug
// Todo labels are the variable inline Type payload, so their bytes are counted alongside names.
impl FiniteInternedConfiguration for ProtocolInterface<'static> {
    fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
        protocol_field_work(&fields.1, |_| Ok(())).ok()
    }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        protocol_field_work(&fields.1, |units| fuel.consume(units))
    }
}

fn protocol_field_work(
    members: &BTreeMap<Name, ProtocolMemberData<'_>>,
    mut consume: impl FnMut(usize) -> Result<(), QuoteError>,
) -> Result<usize, QuoteError> {
    consume(1)?;
    consume(members.len())?;
    members
        .iter()
        .try_fold(members.len(), |work, (name, member)| {
            let work = work
                .checked_add(name.as_str().len())
                .ok_or(QuoteError::Overflow)?;
            // Each member has at most three inline types, held in a fixed array.
            consume(3)?;
            member.kind.member_types().try_fold(work, |work, member| {
                work.checked_add(member.ty().inline_payload_bytes())
                    .ok_or(QuoteError::Overflow)
            })
        })
}

pub(in crate::types) async fn intern_protocol_interface<'call, 'run: 'call, 'db: 'run, M>(
    endpoint: &'call TaskEndpoint<'run, 'db>,
    values: &'call FiniteInternedValues<'db, ProtocolInterface<'static>, M>,
    program: Program<'db>,
    members: BTreeMap<Name, ProtocolMemberData<'db>>,
) -> ProtocolInterface<'db>
where
    M: for<'a> Configuration<
            DbView = dyn Db,
            SalsaStruct<'a> = ProtocolInterface<'a>,
            Output<'a> = usize,
        >,
{
    endpoint.intern_value(values, (program, members)).await
}
