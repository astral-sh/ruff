//! Controlled interning of the shared class-member and instance-member lookup key.

#[cfg(any(test, feature = "experimental-analysis"))]
use ruff_python_ast::name::Name;
#[cfg(test)]
use salsa::execution_probe::{ExecutionWork, RunError};
#[cfg(any(test, feature = "experimental-analysis"))]
use salsa::execution_probe::{
    FixedQueryKeyProfile, InternedValues, PassiveMemoSchema, RegistryBuilder, RunResult,
    TaskEndpoint,
};
#[cfg(any(test, feature = "experimental-analysis"))]
use salsa::plumbing::interned::FiniteInternedConfiguration;
#[cfg(any(test, feature = "experimental-analysis"))]
use salsa::plumbing::{QuoteError, QuoteFuel};

#[cfg(any(test, feature = "experimental-analysis"))]
use super::{
    MemberLookupError, MemberLookupErrorKind, MemberLookupKey, MemberLookupPolicy, MemberMetadata,
    ResolvedMember, Type,
};
#[cfg(any(test, feature = "experimental-analysis"))]
use crate::{Db, Program};

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) type MemberLookupMemoSchema<'db> = (
    salsa::execution_probe::PassiveMemo<
        'db,
        crate::types::MemberLookupKey<'static>,
        crate::types::ClassMemberWithPolicyInnerConfiguration,
        salsa::execution_probe::FixedQueryKeyProfile,
    >,
    salsa::execution_probe::PassiveMemo<
        'db,
        crate::types::MemberLookupKey<'static>,
        crate::types::MemberLookupWithPolicyInnerConfiguration,
        salsa::execution_probe::FixedQueryKeyProfile,
    >,
);

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) fn register_member_lookup_values<'run, 'db: 'run>(
    db: &'db dyn Db,
    registry: &mut RegistryBuilder<'run, 'db>,
) -> RunResult<
    salsa::execution_probe::InternedValues<
        'db,
        crate::types::MemberLookupKey<'static>,
        crate::types::member_lookup::MemberLookupMemoSchema<'db>,
    >,
> {
    let owner = MemberLookupKey::ingredient(db.zalsa());
    let class = registry.passive_memo::<_, _, FixedQueryKeyProfile>(
        owner,
        super::class_member_lookup_ingredient(db),
    )?;
    let member = registry
        .passive_memo::<_, _, FixedQueryKeyProfile>(owner, super::member_lookup_ingredient(db))?;
    registry.finite_interned_values_with_memos(owner, (class, member))
}

#[cfg(any(test, feature = "experimental-analysis"))]
impl FiniteInternedConfiguration for MemberLookupKey<'static> {
    fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
        let (_, ty, name, _) = fields;
        // Hashing and equality read the handles, scalar fields and inline bytes without
        // resolving types. The same quote covers incoming and retained generation fields.
        4usize
            .checked_add(name.as_str().len())?
            .checked_add(ty.inline_payload_bytes())
    }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        Self::field_work(fields).ok_or(QuoteError::Overflow)
    }
}

#[cfg(any(test, feature = "experimental-analysis"))]
impl FiniteInternedConfiguration for MemberMetadata<'static> {
    fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
        // Property and descriptor collections are retained by interned identity. Hashing these
        // fields visits their scalar contents and inline type payloads, not those collections.
        size_of::<Self::Fields<'_>>().checked_add(
            runtime_profile::NativeOutputProfile::inline_payload_bytes(&fields.0),
        )
    }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        Self::field_work(fields).ok_or(QuoteError::Overflow)
    }
}

#[cfg(any(test, feature = "experimental-analysis"))]
impl FiniteInternedConfiguration for MemberLookupError<'static> {
    fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
        let member_bytes = match fields.0 {
            ResolvedMember::Plain(place) => {
                runtime_profile::NativeOutputProfile::inline_payload_bytes(&place)
            }
            ResolvedMember::WithMetadata(_) => 0,
        };
        let error_bytes = match fields.1 {
            MemberLookupErrorKind::DescriptorGet(_) => 0,
            MemberLookupErrorKind::GetAttr {
                receiver: first,
                name: second,
            }
            | MemberLookupErrorKind::GetAttribute {
                receiver: first,
                name: second,
            }
            | MemberLookupErrorKind::ModuleGetAttr {
                callable: first,
                name: second,
            } => first
                .inline_payload_bytes()
                .checked_add(second.inline_payload_bytes())?,
        };
        size_of::<Self::Fields<'_>>()
            .checked_add(member_bytes)?
            .checked_add(error_bytes)
    }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        Self::field_work(fields).ok_or(QuoteError::Overflow)
    }
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) async fn intern_member_lookup_key<'call, 'run: 'call, 'db: 'run, S>(
    endpoint: &'call TaskEndpoint<'run, 'db>,
    values: &'call InternedValues<'db, MemberLookupKey<'static>, S>,
    program: Program<'db>,
    ty: Type<'db>,
    name: Name,
    policy: MemberLookupPolicy,
) -> MemberLookupKey<'db>
where
    S: PassiveMemoSchema<'db, MemberLookupKey<'static>> + 'call,
{
    endpoint
        .intern_value(values, (program, ty, name, policy))
        .await
}

#[cfg(test)]
pub(in crate::types) async fn intern_member_lookup_key_from_str<'call, 'run: 'call, 'db: 'run, S>(
    endpoint: &'call TaskEndpoint<'run, 'db>,
    values: &'call InternedValues<'db, MemberLookupKey<'static>, S>,
    program: Program<'db>,
    ty: Type<'db>,
    name: &'call str,
    policy: MemberLookupPolicy,
) -> MemberLookupKey<'db>
where
    S: PassiveMemoSchema<'db, MemberLookupKey<'static>> + 'call,
{
    let name = endpoint
        .local_call(|| {
            let units = 1usize
                .checked_add(name.len())
                .ok_or(RunError::Contract("member name copy quote overflow"))?;
            let requested_bytes = size_of::<Name>()
                .checked_add(name.len())
                .ok_or(RunError::Contract("member name copy quote overflow"))?;
            endpoint.admit_work(units)?;
            endpoint.admit(ExecutionWork::Resource { requested_bytes })?;
            Ok(Name::new(name))
        })
        .await;
    intern_member_lookup_key(endpoint, values, program, ty, name, policy).await
}

pub(in crate::types) mod class_dispatch;
pub(in crate::types) mod class_object;
pub(in crate::types) mod class_object_entry;
pub(in crate::types) mod finalization;
pub(in crate::types) mod general;
pub(in crate::types) mod mro_dispatch;
pub(in crate::types) mod normalization;
pub(in crate::types) mod self_binding;

#[cfg(test)]
pub(in crate::types) mod runtime;

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) mod runtime_profile;

#[cfg(test)]
mod tests;
