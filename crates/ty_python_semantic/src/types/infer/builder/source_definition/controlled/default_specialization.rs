//! Admitted legacy-variable collection and ordered default-specialization construction.

use std::alloc::Layout;
use std::borrow::Cow;
use std::collections::btree_map;
use std::iter::{Copied, RepeatN};
use std::slice;

use ruff_python_ast::name::Name;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::definition::Definition;

use super::storage::{dense_finish, ordered_merge, sequence_merge};
use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::types::constraints::{OwnedConstraintSet, OwnedConstraintTypeCursor};
use crate::types::generics::binding::TypeVarBindingEffects;
use crate::types::generics::defaults::{
    DefaultArgumentEffects, DefaultSpecializationConstructionEffects, DefaultSpecializationFacts,
    DefaultSpecializationOperation, DefaultSpecializationWork, DefaultVariableCursor,
    fill_in_defaults_with_effects, specialize_partial_with_effects,
};
use crate::types::generics::prefix::{DefaultArgumentBuffer, PrefixWriteError};
use crate::types::instance::tuple_spec::TupleSpecEffects;
use crate::types::instance::{
    MaterializedProtocolType, NominalVisitorChildren, Protocol, SynthesizedProtocolType,
};
use crate::types::known_instance::{InternedType, MethodWrapper, UnionTypeInstance};
use crate::types::legacy_typevars::{
    GuardedLegacyTypeVarDependency, LEGACY_PENDING_INLINE_CAPACITY, LegacyParameterChildren,
    LegacyPendingStack, LegacyProtocolTypeStep, LegacySignatureChildren, LegacyTypeVarDependency,
    LegacyTypeVarFacts, LegacyTypeVarOperation, LegacyTypeVarTraversalEffects, LegacyTypeVarWork,
    LegacyVisitorScopes, Pending, collect_candidate_with_effects, collect_with_visitor_with_effects,
    enqueue_protocol_with_effects,
};
use crate::types::protocol_class::{
    ProtocolClass, ProtocolInterface, ProtocolMember, ProtocolMemberData,
};
use crate::types::mapping::source::MappingResourceAccess;
use crate::types::set_theoretic::NegativeIntersectionElements;
use crate::types::signatures::{Parameter, Signature};
use crate::types::storage_quote::StorageQuote;
use crate::types::tuple::{TupleSpec, TupleType};
use crate::types::visitor::TypeWalkEffects;
use crate::types::visitor::runtime::RuntimeTypeWalk;
use crate::types::{
    BindingContext, BoundTypeVarInstance, CallableType, EnumComplementType,
    FindLegacyTypeVarsVisitor, GenericAlias, GenericContext, IntersectionType, NominalInstanceType,
    ProtocolInstanceType, Specialization, Type, TypeFormType, TypeGuardType, TypeIsType, TypeVarKind,
    UnionType,
};
use crate::{Db, FxOrderSet, Program, ProgramEnvironment};

#[cfg(test)]
pub(in crate::types) mod observations;

pub(in crate::types::infer) enum DefaultArguments<'args, 'db> {
    Missing(RepeatN<Option<Type<'db>>>),
    Supplied(Copied<slice::Iter<'args, Option<Type<'db>>>>),
}

impl<'db> DefaultArguments<'_, 'db> {
    fn len(&self) -> usize {
        match self {
            Self::Missing(input) => input.len(),
            Self::Supplied(input) => input.len(),
        }
    }

    fn next(&mut self) -> Option<Option<Type<'db>>> {
        match self {
            Self::Missing(input) => input.next(),
            Self::Supplied(input) => input.next(),
        }
    }
}

/// Quotes one frame's initialization, enqueue and prepaid retirement separately from storage bytes.
/// Requests one fixed frame representation for construction and another for its enqueue transfer.
fn legacy_pending_quote(pending: &LegacyPendingStack<'_, '_>) -> RunResult<StorageQuote> {
    let storage = sequence_merge::<Pending<'_, '_>>(pending.len(), pending.capacity(), 1).ok_or(
        RunError::Contract("legacy-variable stack quotation overflow"),
    )?;
    // sequence_merge supplies three units for the incoming frame, four bookkeeping units,
    // and one per relocated entry. Buffer cleanup counts entries and slots, independently of
    // their byte width; buffer_push_quote's byte-based work is not this collector's contract.
    let mut work = storage.work;
    if storage.bytes != 0 {
        let replacement_slots = storage.bytes / size_of::<Pending<'_, '_>>();
        Layout::array::<Pending<'_, '_>>(replacement_slots)
            .map_err(|_| RunError::Contract("legacy-variable stack layout overflow"))?;
        let old_retirement = pending
            .len()
            .checked_add(if pending.spilled() {
                pending.capacity()
            } else {
                0
            })
            .and_then(|units| units.checked_add(4))
            .ok_or(RunError::Contract(
                "legacy-variable old stack retirement overflow",
            ))?;
        // Pay old backing retirement again without refunding its earlier prepaid cleanup.
        // The replacement's two slot passes cover initialization and later retirement.
        work = replacement_slots
            .checked_mul(2)
            .and_then(|units| units.checked_add(old_retirement))
            .and_then(|units| units.checked_add(work))
            .ok_or(RunError::Contract("legacy-variable stack work overflow"))?;
    }
    let bytes = storage
        .bytes
        .checked_add(2 * size_of::<Pending<'_, '_>>())
        .ok_or(RunError::Contract(
            "legacy-variable frame quotation overflow",
        ))?;
    Ok(StorageQuote { work, bytes })
}

/// Storage and capacity quoted for retaining one fresh member visitor.
#[derive(Debug)]
struct VisitorScopeQuote {
    work: usize,
    bytes: usize,
    replacement_capacity: Option<usize>,
}

/// Quotes adding one fresh visitor to the retained scope vector, including growth and cleanup.
/// Entry and backing work are independent of the visitor representation's byte width.
fn visitor_scope_quote(scopes: &LegacyVisitorScopes<'_>) -> RunResult<VisitorScopeQuote> {
    let storage =
        sequence_merge::<FindLegacyTypeVarsVisitor<'_>>(scopes.len(), scopes.capacity(), 1).ok_or(
            RunError::Contract("legacy visitor scope quotation overflow"),
        )?;
    let mut work = storage.work;
    let replacement_capacity = if storage.bytes == 0 {
        None
    } else {
        let slots = storage.bytes / size_of::<FindLegacyTypeVarsVisitor<'_>>();
        Layout::array::<FindLegacyTypeVarsVisitor<'_>>(slots)
            .map_err(|_| RunError::Contract("legacy visitor scope layout overflow"))?;
        let old_retirement = scopes
            .len()
            .checked_add(scopes.capacity())
            .and_then(|units| units.checked_add(4))
            .ok_or(RunError::Contract(
                "legacy visitor backing retirement overflow",
            ))?;
        work = slots
            .checked_mul(2)
            .and_then(|units| units.checked_add(old_retirement))
            .and_then(|units| units.checked_add(work))
            .ok_or(RunError::Contract("legacy visitor scope work overflow"))?;
        Some(slots)
    };
    let bytes = storage
        .bytes
        .checked_add(size_of::<FindLegacyTypeVarsVisitor<'_>>())
        .ok_or(RunError::Contract(
            "legacy visitor transfer quotation overflow",
        ))?;
    Ok(VisitorScopeQuote {
        work,
        bytes,
        replacement_capacity,
    })
}

/// Adds action/result transfer work and callback storage to a protocol operation's own quote.
/// The caller supplies operation temporaries separately; `T` is the action's actual result.
fn protocol_action_quote<T>(
    work: usize,
    bytes: usize,
    action_bytes: usize,
) -> RunResult<StorageQuote> {
    let work = work.checked_add(3).ok_or(RunError::Contract(
        "legacy protocol action work quotation overflow",
    ))?;
    let bytes = bytes
        .checked_add(action_bytes)
        .and_then(|bytes| bytes.checked_add(size_of::<Option<RunResult<T>>>()))
        .and_then(|bytes| bytes.checked_add(size_of::<RunResult<T>>()))
        .ok_or(RunError::Contract(
            "legacy protocol action byte quotation overflow",
        ))?;
    Ok(StorageQuote { work, bytes })
}

#[cfg(test)]
/// Captures retained fresh-visitor storage at an admission boundary.
fn visitor_scope_state(scopes: &LegacyVisitorScopes<'_>) -> observations::ScopeState {
    observations::ScopeState {
        len: scopes.len(),
        capacity: scopes.capacity(),
    }
}

#[cfg(test)]
fn pending_state(pending: &LegacyPendingStack<'_, '_>) -> observations::StackState {
    observations::StackState {
        len: pending.len(),
        capacity: pending.capacity(),
        spilled: pending.spilled(),
    }
}

#[cfg(test)]
fn pending_receiver_state(
    item: Option<&Pending<'_, '_>>,
) -> Option<crate::types::constraints::ReceiverCursorState> {
    match item {
        Some(Pending::ConstraintTypes(cursor)) => Some(cursor.state()),
        None
        | Some(
            Pending::Type(_)
            | Pending::Types(_)
            | Pending::Set(_, _)
            | Pending::Negative(_)
            | Pending::Tuple(_)
            | Pending::Specialization(_)
            | Pending::Variables(_, _)
            | Pending::Candidate(_)
            | Pending::Signatures(_)
            | Pending::Parameters(_)
            | Pending::ProtocolMembers(_)
            | Pending::ProtocolMemberTypes(_, _)
            | Pending::FinishFreshVisitor,
        ) => None,
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer::builder) async fn specialize_supplied(
        &self,
        context: GenericContext<'db>,
        supplied: &[Option<Type<'db>>],
    ) -> RunResult<Specialization<'db>> {
        let input = self
            .local(1, 0, || {
                DefaultArguments::Supplied(supplied.iter().copied())
            })
            .await?;
        specialize_partial_with_effects(self.db(), context, input, DefaultSpecializationFacts, self)
            .await
    }

    pub(in crate::types::infer::builder) async fn new_legacy_variables(
        &self,
    ) -> RunResult<FxOrderSet<BoundTypeVarInstance<'db>>> {
        self.initialize_value(FxOrderSet::default).await
    }

    /// Constructs and enqueues a frame only after admitting its transfer and any stack replacement.
    async fn push_legacy_pending<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        make: impl FnOnce() -> RunResult<Pending<'walk, 'db>>,
    ) -> RunResult<()> {
        self.push_pending_with_quote(pending, make, |quote, _| Ok(quote))
            .await
    }

    /// Constructs and enqueues a protocol frame after admitting storage and factory/result carriers.
    async fn push_protocol_pending<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        make: impl FnOnce() -> RunResult<Pending<'walk, 'db>>,
    ) -> RunResult<()> {
        self.push_pending_with_quote(pending, make, |quote, action_bytes| {
            let work = quote.work.checked_add(2).ok_or(RunError::Contract(
                "legacy protocol frame factory work overflow",
            ))?;
            let bytes = quote
                .bytes
                .checked_add(size_of::<RunResult<Pending<'walk, 'db>>>())
                .ok_or(RunError::Contract(
                    "legacy protocol frame result bytes overflow",
                ))?;
            protocol_action_quote::<RunResult<()>>(work, bytes, action_bytes)
        })
        .await
    }

    /// Constructs and enqueues a frame after storage and action admission.
    /// `push_protocol_pending` supplies additional action/result storage and transfer charges.
    async fn push_pending_with_quote<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        make: impl FnOnce() -> RunResult<Pending<'walk, 'db>>,
        quote_action: impl FnOnce(StorageQuote, usize) -> RunResult<StorageQuote>,
    ) -> RunResult<()> {
        let quote = legacy_pending_quote(pending)?;
        #[cfg(test)]
        let boundary = if pending.len() == pending.capacity() {
            observations::Boundary::StackSpill
        } else {
            observations::Boundary::ReusedTransfer
        };
        #[cfg(test)]
        let state = pending_state(pending);
        let action = || {
            pending.push(make()?);
            Ok::<(), RunError>(())
        };
        let StorageQuote { work, bytes } = quote_action(quote, size_of_val(&action))?;
        #[cfg(test)]
        observations::before(self.db(), boundary, work, bytes, Some(state), None);
        self.local(work, bytes, action).await??;
        #[cfg(test)]
        observations::accepted(self.db(), boundary, Some(pending_state(pending)), None);
        Ok(())
    }

    /// Collects stored legacy variables under the source builder owner used by ordinary inference.
    /// Retaining an empty builder makes cursor cleanup observable relative to that owner's lifetime.
    #[cfg(test)]
    pub(in crate::types::infer) async fn legacy_variables_for_test(
        &self,
        source: &super::PreparedSource<'db>,
        env: &ProgramEnvironment<'db>,
        expression: ty_python_core::expression::Expression<'db>,
        ty: Type<'db>,
    ) -> RunResult<FxOrderSet<BoundTypeVarInstance<'db>>> {
        let _observation = observations::owner_started();
        let _owner = self
            .empty_builder(
                source,
                env,
                crate::types::infer::InferenceRegion::Expression(
                    expression,
                    crate::types::TypeContext::default(),
                ),
            )
            .await?;
        let mut variables = self.new_legacy_variables().await?;
        crate::types::legacy_typevars::find_legacy_typevars_with_effects(
            self.db(),
            env,
            ty,
            None,
            &mut variables,
            self,
        )
        .await?;
        Ok(variables)
    }

    fn legacy_fields(&self) -> RuntimeTypeWalk<'_, 'run, 'db, (), &Self> {
        RuntimeTypeWalk {
            db: self.db(),
            endpoint: self.access.endpoint(),
            query: (),
            unavailable: self,
        }
    }

    /// Runs a protocol-local action after admitting its operation and callback carriers.
    async fn protocol_local<T>(
        &self,
        work: usize,
        bytes: usize,
        action: impl FnOnce() -> T,
    ) -> RunResult<T> {
        let quote = protocol_action_quote::<T>(work, bytes, size_of_val(&action))
            .map(|quote| (quote.work, quote.bytes));
        self.local_quoted(quote, action).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> LegacyTypeVarTraversalEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self, _work: LegacyTypeVarWork) -> RunResult<()> {
        self.work(1).await
    }

    async fn new_visitor(&self) -> RunResult<FindLegacyTypeVarsVisitor<'db>> {
        // Guarded descendants remain unavailable, so this detector stays empty. The two
        // operation units initialize it and prepay retirement of its empty internal storage.
        let action = FindLegacyTypeVarsVisitor::default;
        let StorageQuote { work, bytes } = protocol_action_quote::<FindLegacyTypeVarsVisitor<'db>>(
            2,
            size_of::<FindLegacyTypeVarsVisitor<'db>>(),
            size_of_val(&action),
        )?;
        #[cfg(test)]
        observations::before_protocol(
            self.db(),
            observations::Boundary::VisitorCreate,
            work,
            bytes,
            None,
        );
        let visitor = self.local(work, bytes, action).await?;
        #[cfg(test)]
        observations::accepted_protocol(self.db(), observations::Boundary::VisitorCreate, None);
        Ok(visitor)
    }

    async fn new_visitor_scopes(&self) -> RunResult<LegacyVisitorScopes<'db>> {
        let action = Vec::<FindLegacyTypeVarsVisitor<'db>>::new;
        let StorageQuote { work, bytes } = protocol_action_quote::<LegacyVisitorScopes<'db>>(
            2,
            size_of::<LegacyVisitorScopes<'db>>(),
            size_of_val(&action),
        )?;
        #[cfg(test)]
        observations::before_protocol(
            self.db(),
            observations::Boundary::ScopeStorageCreate,
            work,
            bytes,
            None,
        );
        let scopes = self.local(work, bytes, action).await?;
        #[cfg(test)]
        observations::accepted_protocol(
            self.db(),
            observations::Boundary::ScopeStorageCreate,
            Some(visitor_scope_state(&scopes)),
        );
        Ok(scopes)
    }

    async fn enter_fresh_visitor(&self, scopes: &mut LegacyVisitorScopes<'db>) -> RunResult<()> {
        let visitor = self.new_visitor().await?;
        self.protocol_local(
            2,
            size_of::<Option<FindLegacyTypeVarsVisitor<'db>>>(),
            || (),
        )
        .await?;
        // This future owns the slot until the accepted callback takes it. A refused insertion
        // therefore retains the fresh visitor while the runtime drains pending child tasks.
        let mut visitor = Some(visitor);
        let quote = visitor_scope_quote(scopes)?;
        #[cfg(test)]
        let state = visitor_scope_state(scopes);
        let action = || {
            if let Some(capacity) = quote.replacement_capacity {
                // Request the quoted capacity before push can choose an implicit growth policy.
                scopes.reserve_exact(capacity - scopes.len());
            }
            let visitor = visitor
                .take()
                .ok_or(RunError::Contract("legacy visitor transferred twice"))?;
            scopes.push(visitor);
            Ok::<(), RunError>(())
        };
        let StorageQuote { work, bytes } =
            protocol_action_quote::<RunResult<()>>(quote.work, quote.bytes, size_of_val(&action))?;
        #[cfg(test)]
        observations::before_protocol(
            self.db(),
            observations::Boundary::ScopeEnter,
            work,
            bytes,
            Some(state),
        );
        self.local(work, bytes, action).await??;
        #[cfg(test)]
        observations::accepted_protocol(
            self.db(),
            observations::Boundary::ScopeEnter,
            Some(visitor_scope_state(scopes)),
        );
        Ok(())
    }

    async fn finish_fresh_visitor(&self, scopes: &mut LegacyVisitorScopes<'db>) -> RunResult<()> {
        #[cfg(test)]
        let state = visitor_scope_state(scopes);
        let action = || {
            let visitor = scopes
                .pop()
                .ok_or(RunError::Contract("legacy visitor marker has no scope"))?;
            drop(visitor);
            Ok::<(), RunError>(())
        };
        let StorageQuote { work, bytes } = protocol_action_quote::<RunResult<()>>(
            2,
            size_of::<Option<FindLegacyTypeVarsVisitor<'db>>>(),
            size_of_val(&action),
        )?;
        #[cfg(test)]
        observations::before_protocol(
            self.db(),
            observations::Boundary::ScopeRestore,
            work,
            bytes,
            Some(state),
        );
        self.local(work, bytes, action).await??;
        #[cfg(test)]
        observations::accepted_protocol(
            self.db(),
            observations::Boundary::ScopeRestore,
            Some(visitor_scope_state(scopes)),
        );
        Ok(())
    }

    async fn new_pending<'walk>(&self) -> RunResult<LegacyPendingStack<'walk, 'db>>
    where
        'db: 'walk,
    {
        self.local(
            1 + LEGACY_PENDING_INLINE_CAPACITY,
            size_of::<LegacyPendingStack<'walk, 'db>>(),
            LegacyPendingStack::new,
        )
        .await
    }

    async fn push_type<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        ty: Type<'db>,
    ) -> RunResult<()> {
        self.push_legacy_pending(pending, || Ok(Pending::Type(ty)))
            .await
    }

    async fn push_types<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        types: &'db [Type<'db>],
    ) -> RunResult<()> {
        self.push_legacy_pending(pending, || Ok(Pending::Types(types)))
            .await
    }

    async fn push_set<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        types: &'db FxOrderSet<Type<'db>>,
        index: usize,
    ) -> RunResult<()> {
        self.push_legacy_pending(pending, || Ok(Pending::Set(types, index)))
            .await
    }

    async fn push_negative<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        types: &'db NegativeIntersectionElements<'db>,
    ) -> RunResult<()> {
        self.push_legacy_pending(pending, || Ok(Pending::Negative(types)))
            .await
    }

    async fn push_tuple<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        tuple: &'db TupleSpec<'db>,
    ) -> RunResult<()> {
        self.push_legacy_pending(pending, || Ok(Pending::Tuple(tuple)))
            .await
    }

    async fn push_specialization<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        specialization: Specialization<'db>,
    ) -> RunResult<()> {
        self.push_legacy_pending(pending, || Ok(Pending::Specialization(specialization)))
            .await
    }

    async fn push_variables<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        context: GenericContext<'db>,
        index: usize,
    ) -> RunResult<()> {
        self.push_legacy_pending(pending, || Ok(Pending::Variables(context, index)))
            .await
    }

    async fn push_candidate<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<()> {
        self.push_legacy_pending(pending, || Ok(Pending::Candidate(variable)))
            .await
    }

    async fn push_signatures<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        signatures: &'walk [Signature<'db>],
    ) -> RunResult<()> {
        self.push_legacy_pending(pending, || Ok(Pending::Signatures(signatures)))
            .await
    }

    async fn push_parameters<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        parameters: &'walk [Parameter<'db>],
    ) -> RunResult<()> {
        self.push_legacy_pending(pending, || Ok(Pending::Parameters(parameters)))
            .await
    }

    async fn push_protocol_members<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        members: btree_map::Iter<'db, Name, ProtocolMemberData<'db>>,
    ) -> RunResult<()> {
        self.protocol_local(
            2,
            size_of::<Option<btree_map::Iter<'db, Name, ProtocolMemberData<'db>>>>(),
            || (),
        )
        .await?;
        let mut members = Some(members);
        self.push_protocol_pending(pending, || {
            members
                .take()
                .map(Pending::ProtocolMembers)
                .ok_or(RunError::Contract(
                    "legacy protocol iterator transferred twice",
                ))
        })
        .await
    }

    async fn push_protocol_types<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        types: [Option<Type<'db>>; 6],
        index: usize,
    ) -> RunResult<()> {
        self.push_protocol_pending(pending, || Ok(Pending::ProtocolMemberTypes(types, index)))
            .await
    }

    async fn push_finish_fresh_visitor<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
    ) -> RunResult<()> {
        self.push_protocol_pending(pending, || Ok(Pending::FinishFreshVisitor))
            .await
    }

    async fn next_protocol_member(
        &self,
        members: &mut btree_map::Iter<'db, Name, ProtocolMemberData<'db>>,
    ) -> RunResult<Option<(&'db Name, &'db ProtocolMemberData<'db>)>> {
        let action = || members.next();
        let StorageQuote { work, bytes } =
            protocol_action_quote::<Option<(&'db Name, &'db ProtocolMemberData<'db>)>>(
                1,
                size_of::<Option<(&'db Name, &'db ProtocolMemberData<'db>)>>(),
                size_of_val(&action),
            )?;
        #[cfg(test)]
        observations::before_protocol(
            self.db(),
            observations::Boundary::ProtocolMember,
            work,
            bytes,
            None,
        );
        let member = self.local(work, bytes, action).await?;
        #[cfg(test)]
        observations::accepted_protocol(self.db(), observations::Boundary::ProtocolMember, None);
        Ok(member)
    }

    async fn protocol_member_types(
        &self,
        name: &'db Name,
        data: &'db ProtocolMemberData<'db>,
    ) -> RunResult<[Option<Type<'db>>; 6]> {
        let action = || ProtocolMember::from_stored(name, data, None).stored_types_for_visitor();
        let StorageQuote { work, bytes } = protocol_action_quote::<[Option<Type<'db>>; 6]>(
            16,
            size_of::<ProtocolMember<'db, 'db>>() + size_of::<[Option<Type<'db>>; 6]>(),
            size_of_val(&action),
        )?;
        #[cfg(test)]
        observations::before_protocol(
            self.db(),
            observations::Boundary::ProtocolTypes,
            work,
            bytes,
            None,
        );
        let types = self.local(work, bytes, action).await?;
        #[cfg(test)]
        observations::accepted_protocol(self.db(), observations::Boundary::ProtocolTypes, None);
        Ok(types)
    }

    async fn protocol_type_step(
        &self,
        types: &[Option<Type<'db>>; 6],
        index: usize,
    ) -> RunResult<Option<LegacyProtocolTypeStep<'db>>> {
        let action = || {
            types.get(index).map(|ty| LegacyProtocolTypeStep {
                ty: *ty,
                next: index + 1,
            })
        };
        let StorageQuote { work, bytes } =
            protocol_action_quote::<Option<LegacyProtocolTypeStep<'db>>>(
                2,
                size_of::<Option<LegacyProtocolTypeStep<'db>>>(),
                size_of_val(&action),
            )?;
        #[cfg(test)]
        observations::before_protocol(
            self.db(),
            observations::Boundary::ProtocolSlot,
            work,
            bytes,
            None,
        );
        let step = self.local(work, bytes, action).await?;
        #[cfg(test)]
        observations::accepted_protocol(self.db(), observations::Boundary::ProtocolSlot, None);
        Ok(step)
    }

    async fn protocol_members(
        &self,
        interface: ProtocolInterface<'db>,
    ) -> RunResult<btree_map::Iter<'db, Name, ProtocolMemberData<'db>>> {
        self.legacy_fields()
            .stored_protocol_members(interface)
            .await
    }

    async fn materialized_protocol_origin(
        &self,
        materialized: MaterializedProtocolType<'db>,
    ) -> RunResult<ProtocolClass<'db>> {
        self.legacy_fields()
            .materialized_protocol_origin(materialized)
            .await
    }

    async fn enqueue_protocol<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> RunResult<()> {
        enqueue_protocol_with_effects(pending, protocol, self).await
    }

    async fn protocol_inner(
        &self,
        protocol: ProtocolInstanceType<'db>,
    ) -> RunResult<Protocol<'db>> {
        self.protocol_local(1, size_of::<Protocol<'db>>(), || protocol.inner)
            .await
    }

    async fn protocol_class_type(&self, class: ProtocolClass<'db>) -> RunResult<Type<'db>> {
        self.protocol_local(1, size_of::<Type<'db>>(), || Type::from(class))
            .await
    }

    async fn synthesized_interface(
        &self,
        protocol: SynthesizedProtocolType<'db>,
    ) -> RunResult<ProtocolInterface<'db>> {
        self.protocol_local(1, size_of::<ProtocolInterface<'db>>(), || {
            protocol.interface()
        })
        .await
    }

    async fn push_receiver<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        constraints: &'walk OwnedConstraintSet<'db>,
    ) -> RunResult<()> {
        #[cfg(test)]
        {
            let StorageQuote { work, bytes } = legacy_pending_quote(pending)?;
            observations::before(
                self.db(),
                observations::Boundary::CursorCreate,
                work,
                bytes,
                Some(pending_state(pending)),
                None,
            );
        }
        self.push_legacy_pending(pending, || {
            Ok(Pending::ConstraintTypes(OwnedConstraintTypeCursor::new(
                constraints,
            )))
        })
        .await?;
        #[cfg(test)]
        observations::accepted(
            self.db(),
            observations::Boundary::CursorCreate,
            Some(pending_state(pending)),
            None,
        );
        Ok(())
    }

    async fn requeue_receiver<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
        cursor: OwnedConstraintTypeCursor<'walk, 'db>,
    ) -> RunResult<()> {
        // The cursor stays in this future while admission can fail. The factory below borrows
        // its slot, so rejecting or draining the local callback cannot retire the cursor early.
        self.local(
            1,
            size_of::<Option<OwnedConstraintTypeCursor<'walk, 'db>>>(),
            || (),
        )
        .await?;
        let mut cursor = Some(cursor);
        #[cfg(test)]
        {
            let StorageQuote { work, bytes } = legacy_pending_quote(pending)?;
            observations::before(
                self.db(),
                observations::Boundary::OwningEnqueue,
                work,
                bytes,
                Some(pending_state(pending)),
                cursor.as_ref().map(OwnedConstraintTypeCursor::state),
            );
        }
        self.push_legacy_pending(pending, || {
            cursor
                .take()
                .map(Pending::ConstraintTypes)
                .ok_or(RunError::Contract(
                    "legacy receiver cursor transferred twice",
                ))
        })
        .await?;
        #[cfg(test)]
        observations::accepted(
            self.db(),
            observations::Boundary::OwningEnqueue,
            Some(pending_state(pending)),
            None,
        );
        Ok(())
    }

    async fn next_pending<'walk>(
        &self,
        pending: &mut LegacyPendingStack<'walk, 'db>,
    ) -> RunResult<Option<Pending<'walk, 'db>>> {
        let bytes = size_of::<Option<Pending<'walk, 'db>>>();
        let work = 2;
        #[cfg(test)]
        observations::before(
            self.db(),
            observations::Boundary::Pop,
            work,
            bytes,
            Some(pending_state(pending)),
            pending_receiver_state(pending.last()),
        );
        let item = self.local(work, bytes, || pending.pop()).await?;
        #[cfg(test)]
        observations::accepted(
            self.db(),
            observations::Boundary::Pop,
            Some(pending_state(pending)),
            pending_receiver_state(item.as_ref()),
        );
        Ok(item)
    }

    async fn signature_children<'walk>(
        &self,
        signatures: &'walk [Signature<'db>],
    ) -> RunResult<Option<LegacySignatureChildren<'walk, 'db>>> {
        self.local(
            5,
            size_of::<Option<LegacySignatureChildren<'walk, 'db>>>(),
            || {
                signatures
                    .split_first()
                    .map(|(signature, remaining)| LegacySignatureChildren {
                        remaining,
                        receiver: signature.receiver_constraints(),
                        parameters: signature.parameters().as_slice(),
                        return_type: signature.return_ty,
                    })
            },
        )
        .await
    }

    async fn parameter_children<'walk>(
        &self,
        parameters: &'walk [Parameter<'db>],
    ) -> RunResult<Option<LegacyParameterChildren<'walk, 'db>>> {
        self.local(
            4,
            size_of::<Option<LegacyParameterChildren<'walk, 'db>>>(),
            || {
                parameters
                    .split_first()
                    .map(|(parameter, remaining)| LegacyParameterChildren {
                        remaining,
                        annotation: parameter.annotated_type(),
                        default: parameter.eager_default_type(),
                    })
            },
        )
        .await
    }

    async fn receiver_step<'walk>(
        &self,
        cursor: &mut OwnedConstraintTypeCursor<'walk, 'db>,
    ) -> RunResult<Option<Option<[Type<'db>; 2]>>> {
        let bytes = size_of::<Option<Option<[Type<'db>; 2]>>>();
        let work = 1;
        #[cfg(test)]
        observations::before(
            self.db(),
            observations::Boundary::ReceiverStep,
            work,
            bytes,
            None,
            Some(cursor.state()),
        );
        self.local(work, bytes, || ()).await?;
        let types = self
            .legacy_fields()
            .receiver_constraint_step(cursor)
            .await?;
        #[cfg(test)]
        observations::accepted(
            self.db(),
            observations::Boundary::ReceiverStep,
            None,
            Some(cursor.state()),
        );
        Ok(types)
    }

    async fn callable_signatures(
        &self,
        callable: CallableType<'db>,
    ) -> RunResult<&'db [Signature<'db>]> {
        let signatures =
            TypeWalkEffects::callable_signatures(&mut self.legacy_fields(), callable).await?;
        self.local(1, size_of::<&[Signature<'db>]>(), || &*signatures.overloads)
            .await
    }

    async fn insert_variable(
        &self,
        variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<()> {
        let quote =
            ordered_merge::<BoundTypeVarInstance<'db>>(variables.len(), variables.capacity(), 1)
                .ok_or(RunError::Contract(
                    "legacy-variable insertion quotation overflow",
                ))?;
        self.local(quote.work, quote.bytes, || {
            variables.insert(variable);
        })
        .await
    }

    async fn unbound_recursive(&self) -> RunResult<()> {
        Err(RunError::Contract(
            "semantic operation on an unbound recursive variable",
        ))
    }

    async fn variable_kind(&self, variable: BoundTypeVarInstance<'db>) -> RunResult<TypeVarKind> {
        let variable = TypeVarBindingEffects::bound_typevar(self, variable).await?;
        TypeVarBindingEffects::kind(self, variable).await
    }

    async fn variable_binding(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<BindingContext<'db>> {
        let identity = TypeVarBindingEffects::bound_identity(self, variable).await?;
        Ok(identity.binding_context)
    }

    async fn specialization_tuple(
        &self,
        ty: Specialization<'db>,
    ) -> RunResult<Option<&'db TupleSpec<'db>>> {
        TypeWalkEffects::specialization_tuple(&mut self.legacy_fields(), ty).await
    }
    async fn specialization_types(&self, ty: Specialization<'db>) -> RunResult<&'db [Type<'db>]> {
        TypeWalkEffects::specialization_types(&mut self.legacy_fields(), ty).await
    }
    async fn context_variable(
        &self,
        ty: GenericContext<'db>,
        index: usize,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        TypeWalkEffects::context_variable(&mut self.legacy_fields(), ty, index).await
    }
    async fn union_elements(&self, ty: UnionType<'db>) -> RunResult<&'db [Type<'db>]> {
        TypeWalkEffects::union_elements(&mut self.legacy_fields(), ty).await
    }
    async fn intersection_positive(
        &self,
        ty: IntersectionType<'db>,
    ) -> RunResult<&'db FxOrderSet<Type<'db>>> {
        TypeWalkEffects::intersection_positive(&mut self.legacy_fields(), ty).await
    }
    async fn intersection_negative(
        &self,
        ty: IntersectionType<'db>,
    ) -> RunResult<&'db NegativeIntersectionElements<'db>> {
        TypeWalkEffects::intersection_negative(&mut self.legacy_fields(), ty).await
    }
    async fn enum_rest(
        &self,
        ty: EnumComplementType<'db>,
    ) -> RunResult<&'db FxOrderSet<Type<'db>>> {
        TypeWalkEffects::enum_rest(&mut self.legacy_fields(), ty).await
    }
    async fn alias_specialization(&self, ty: GenericAlias<'db>) -> RunResult<Specialization<'db>> {
        TypeWalkEffects::alias_specialization(&mut self.legacy_fields(), ty).await
    }
    async fn nominal_children(
        &self,
        ty: NominalInstanceType<'db>,
    ) -> RunResult<NominalVisitorChildren<'db>> {
        TypeWalkEffects::nominal_children(&mut self.legacy_fields(), ty).await
    }
    async fn type_is_argument(&self, ty: TypeIsType<'db>) -> RunResult<Type<'db>> {
        TypeWalkEffects::type_is_argument(&mut self.legacy_fields(), ty).await
    }
    async fn type_guard_return(&self, ty: TypeGuardType<'db>) -> RunResult<Type<'db>> {
        TypeWalkEffects::type_guard_return(&mut self.legacy_fields(), ty).await
    }
    async fn type_form_argument(&self, ty: TypeFormType<'db>) -> RunResult<Type<'db>> {
        TypeWalkEffects::type_form_argument(&mut self.legacy_fields(), ty).await
    }
    async fn union_value(&self, ty: UnionTypeInstance<'db>) -> RunResult<Option<Type<'db>>> {
        TypeWalkEffects::union_value(&mut self.legacy_fields(), ty).await
    }
    async fn interned_type(&self, ty: InternedType<'db>) -> RunResult<Type<'db>> {
        TypeWalkEffects::interned_type(&mut self.legacy_fields(), ty).await
    }
    async fn method_wrapper_type(&self, ty: MethodWrapper<'db>) -> RunResult<Type<'db>> {
        TypeWalkEffects::method_wrapper_type(&mut self.legacy_fields(), ty).await
    }

    async fn normalize_paramspec(
        &self,
        _db: &'db dyn Db,
        _variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<BoundTypeVarInstance<'db>> {
        self.unavailable(SourceOperation::LegacyTypeVariables(
            LegacyTypeVarOperation::NormalizeParamSpec,
        ))
        .await
    }

    async fn collect_candidate(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
        binding_context: Option<Definition<'db>>,
        variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    ) -> RunResult<()> {
        collect_candidate_with_effects(
            db,
            variable,
            binding_context,
            variables,
            LegacyTypeVarFacts,
            self,
        )
        .await
    }

    async fn collect_with_visitor(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        binding_context: Option<Definition<'db>>,
        variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) -> RunResult<()> {
        self.environment_program(env).await?;
        collect_with_visitor_with_effects(
            db,
            env,
            ty,
            binding_context,
            variables,
            visitor,
            LegacyTypeVarFacts,
            self,
        )
        .await
    }

    async fn deferred(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _binding_context: Option<Definition<'db>>,
        dependency: LegacyTypeVarDependency<'db>,
        _variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        _visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) -> RunResult<()> {
        let operation = match dependency {
            LegacyTypeVarDependency::Guarded { operation, .. } => match operation {
                GuardedLegacyTypeVarDependency::Recursive(_) => LegacyTypeVarOperation::Recursive,
                GuardedLegacyTypeVarDependency::Function(_) => LegacyTypeVarOperation::Function,
                GuardedLegacyTypeVarDependency::BoundMethod(_) => {
                    LegacyTypeVarOperation::BoundMethod
                }
                GuardedLegacyTypeVarDependency::FunctionWrapper(_) => {
                    LegacyTypeVarOperation::FunctionWrapper
                }
                GuardedLegacyTypeVarDependency::BoundMethodWrapper(_) => {
                    LegacyTypeVarOperation::BoundMethodWrapper
                }
                GuardedLegacyTypeVarDependency::Property(_) => LegacyTypeVarOperation::Property,
                GuardedLegacyTypeVarDependency::Slot(_) => LegacyTypeVarOperation::Slot,
                GuardedLegacyTypeVarDependency::Alias(_) => LegacyTypeVarOperation::Alias,
            },
        };
        self.unavailable(SourceOperation::LegacyTypeVariables(operation))
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> DefaultSpecializationConstructionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    async fn checkpoint(&self, _work: DefaultSpecializationWork) -> RunResult<()> {
        self.work(1).await
    }
    async fn context_len(&self, context: GenericContext<'db>) -> RunResult<usize> {
        let variables = self
            .field(context.variables_request(self.access.endpoint().field_request_context()))
            .await?;
        self.local(1, 0, || variables.len()).await
    }
    async fn context_program(&self, context: GenericContext<'db>) -> RunResult<Program<'db>> {
        let program = self
            .field(
                context
                    .field_requests(self.access.endpoint().field_request_context())
                    .program(),
            )
            .await?;
        self.check_program(program)?;
        Ok(program)
    }
    async fn specialize_missing(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
        len: usize,
    ) -> RunResult<Specialization<'db>> {
        let input = self
            .local(1, 0, || {
                DefaultArguments::Missing(std::iter::repeat_n(None, len))
            })
            .await?;
        specialize_partial_with_effects(db, context, input, DefaultSpecializationFacts, self).await
    }
    async fn specialization_types(
        &self,
        specialization: Specialization<'db>,
    ) -> RunResult<&'db [Type<'db>]> {
        TypeWalkEffects::specialization_types(&mut self.legacy_fields(), specialization).await
    }

    /// Constructs the canonical homogeneous Unknown tuple in the supplied environment.
    async fn unknown_tuple(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<TupleType<'db>> {
        let program = self
            .type_parameter_future(|| self.environment_program(env))
            .await?
            .await?;
        let spec = self
            .type_parameter_future(|| TupleSpecEffects::unknown_tuple(self))
            .await?
            .await?;
        self.type_parameter_future(|| self.access.intern_tuple(program, spec))
            .await?
            .await
    }

    async fn intern_specialization(
        &self,
        _db: &'db dyn Db,
        context: GenericContext<'db>,
        types: Cow<'_, [Type<'db>]>,
        tuple: Option<TupleType<'db>>,
    ) -> RunResult<Specialization<'db>> {
        let types = match types {
            Cow::Owned(types) => {
                if types.len() == types.capacity() {
                    self.local(1, 0, || types.into_boxed_slice()).await?
                } else {
                    let quote = dense_finish::<Type<'db>>(types.len(), types.capacity()).ok_or(
                        RunError::Contract("default-specialization boxing quotation overflow"),
                    )?;
                    self.local(quote.work, quote.bytes, || types.into_boxed_slice())
                        .await?
                }
            }
            Cow::Borrowed(types) => {
                let quote = dense_finish::<Type<'db>>(types.len(), types.len()).ok_or(
                    RunError::Contract("default-specialization clone quotation overflow"),
                )?;
                self.local(quote.work, quote.bytes, || Box::<[Type<'db>]>::from(types))
                    .await?
            }
        };
        self.access
            .intern_specialization(context, types, None, tuple)
            .await
    }
    async fn owned_types(&self, types: Box<[Type<'db>]>) -> RunResult<Cow<'db, [Type<'db>]>> {
        self.local(1, 0, || Cow::Owned(types.into_vec())).await
    }
}

impl<'args, 'run, 'db: 'run, A: SourceAccess<'run, 'db>>
    DefaultArgumentEffects<'db, DefaultArguments<'args, 'db>> for SourceEffects<'_, 'run, 'db, A>
{
    type Cursor = DefaultArguments<'args, 'db>;
    type Buffer = DefaultArgumentBuffer<'run, 'db>;
    async fn arguments(&self, input: Self::Cursor) -> RunResult<Self::Cursor> {
        self.local(1, 0, || input).await
    }
    async fn context_variables(
        &self,
        context: GenericContext<'db>,
    ) -> RunResult<DefaultVariableCursor<'db>> {
        let variables = self
            .field(context.variables_request(self.access.endpoint().field_request_context()))
            .await?;
        self.local(1, 0, || variables.values().copied()).await
    }
    async fn input_len(&self, input: &Self::Cursor) -> RunResult<usize> {
        self.local(1, 0, || input.len()).await
    }
    async fn check_arity(
        &self,
        context: GenericContext<'db>,
        input: &Self::Cursor,
    ) -> RunResult<()> {
        let len = DefaultSpecializationConstructionEffects::context_len(self, context).await?;
        let matches = self.local(1, 0, || len == input.len()).await?;
        if matches {
            Ok(())
        } else {
            Err(RunError::Contract("default-specialization arity mismatch"))
        }
    }
    async fn new_buffer(&self, len: usize) -> RunResult<Self::Buffer> {
        self.access
            .resources()
            .default_arguments(self.access.endpoint(), len)
            .await
    }
    async fn buffer_len(&self, buffer: &Self::Buffer) -> RunResult<usize> {
        self.local(1, 0, || buffer.len()).await
    }
    async fn next_argument(
        &self,
        input: &mut Self::Cursor,
        variables: &mut DefaultVariableCursor<'db>,
    ) -> RunResult<Option<(Option<Type<'db>>, BoundTypeVarInstance<'db>)>> {
        self.local(2, 0, || {
            input
                .next()
                .and_then(|ty| variables.next().map(|variable| (ty, variable)))
        })
        .await
    }
    async fn variable_kind(&self, variable: BoundTypeVarInstance<'db>) -> RunResult<TypeVarKind> {
        LegacyTypeVarTraversalEffects::variable_kind(self, variable).await
    }
    async fn append(&self, types: &mut Self::Buffer, ty: Type<'db>) -> RunResult<()> {
        self.local(size_of::<Type<'db>>() * 2 + 4, 0, || {
            types.append(ty).map_err(|error| match error {
                PrefixWriteError::Full => RunError::Contract("default-argument buffer is full"),
                PrefixWriteError::Initialized => {
                    RunError::Contract("default-argument slot is already initialized")
                }
            })
        })
        .await?
    }
    async fn finish_buffer(&self, types: Self::Buffer) -> RunResult<Box<[Type<'db>]>> {
        let len = self
            .local(2, 0, || {
                if types.len() != types.capacity() {
                    return Err(RunError::Contract("default-argument buffer is incomplete"));
                }
                Ok(types.len())
            })
            .await??;
        let quote = dense_finish::<Type<'db>>(len, len).ok_or(
            RunError::Contract("default-specialization finish quotation overflow"),
        )?;
        let work = quote
            .work
            .checked_add(len.checked_mul(size_of::<Type<'db>>() * 2 + 4).ok_or(
                RunError::Contract("default-specialization copying work overflow"),
            )?)
            .ok_or(RunError::Contract(
                "default-specialization copying work overflow",
            ))?;
        self.local(work, quote.bytes, || {
            let prefix = types.prefix();
            let mut result = Vec::with_capacity(len);
            for index in 0..len {
                let Some(ty) = prefix.checked_get(index).map_err(|_| {
                    RunError::Contract("default-argument prefix contains an uninitialized slot")
                })?
                else {
                    return Err(RunError::Contract(
                        "default-argument prefix is shorter than its buffer",
                    ));
                };
                result.push(ty);
            }
            Ok(result.into_boxed_slice())
        })
        .await?
    }
    async fn fill_supplied(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
        input: Self::Cursor,
    ) -> RunResult<Box<[Type<'db>]>> {
        fill_in_defaults_with_effects(db, context, input, DefaultSpecializationFacts, self).await
    }
    async fn default_type(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.access.bound_typevar_default(variable).await
    }
    async fn map_default(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        default: Type<'db>,
        context: GenericContext<'db>,
        prefix: &Self::Buffer,
    ) -> RunResult<Type<'db>> {
        let prefix = self
            .local_with_fixed_transfers(
                12,
                size_of::<crate::types::generics::prefix::InitializedTypePrefix<'run, 'db>>() * 2
                    + size_of::<&Self::Buffer>() * 2
                    + size_of::<usize>() * 2,
                || prefix.prefix(),
            )
            .await?;
        self.type_parameter_future(|| {
            self.apply_partial_specialization(default, env, context, prefix, None)
        }).await?.await
    }
    async fn unknown_paramspec(&self, _db: &'db dyn Db) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::DefaultSpecialization(
            DefaultSpecializationOperation::UnknownParamSpec,
        ))
        .await
    }
}
