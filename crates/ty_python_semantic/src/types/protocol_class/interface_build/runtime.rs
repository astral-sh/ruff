//! Canonical cold interface construction from final declaration sources.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use ruff_index::IndexSlice;
use ruff_python_ast::name::Name;
use salsa::execution_probe::{
    BorrowOrCopy, CallableRoute, CallableRouteProvider, FinalSourceRoute, FiniteInternedValues,
    NativeValueOperation, NativeValueQuote, RetainedInput, RunError, RunResult, TaskEndpoint,
};
use salsa::plumbing::AsId;
use salsa::plumbing::function::Configuration;
use ty_module_resolver::KnownModule;
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::narrowing_constraints::{NarrowingConstraints, ScopedNarrowingConstraint};
use ty_python_core::place::ScopedPlaceId;
use ty_python_core::predicate::{Predicate, ScopedPredicateId};
use ty_python_core::reachability_constraints::{
    ReachabilityConstraints, ScopedReachabilityConstraintId,
};
use ty_python_core::scope::ScopeId;
use ty_python_core::symbol::ScopedSymbolId;
use ty_python_core::{
    BindingWithConstraintsIterator, DeclarationsIterator, ImportedFinalCandidatesIterator,
    PlaceTable, PredicateNarrowingTargets, ProgramFile, Truthiness, UseDefMap,
};

use super::super::{ProtocolInterface, ProtocolMemberCandidate, ProtocolMemberData};
use super::{
    ProtocolCandidateEffects, ProtocolInterfaceBuild, ProtocolInterfaceEffects,
    ProtocolInterfaceNormalizationEffects, ProtocolInterfaceWork, ProtocolNormalizationWork,
    protocol_interface_build_with, protocol_interface_candidate_with,
    protocol_interface_normalize_with,
};
use crate::place::source_effects::{
    self, PublicLookupEffects, SourcePlaceEffects, SourcePlaceWork,
};
use crate::place::{
    ConsideredDefinitions, LoopHeaderReachability, PlaceAndQualifiers, PlaceFromDeclarationsResult,
    PlaceWithDefinition, RequiresExplicitReExport, place_from_bindings_with,
    place_from_declarations_with,
};
use crate::reachability::{NarrowingProjector, ReachabilityEvaluationCache};
use crate::types::class::{KnownClassLookupError, interpret_class_literal_lookup};
use crate::types::class_base::{ClassBaseConversion, ClassBaseDependency};
use crate::types::constructor::expansion_probe::{self, Incomplete};
use crate::types::context::ProgramEnvironmentSource;
use crate::types::generics::{GenericContext, Specialization};
use crate::types::infer::DefinitionInference;
use crate::types::mro::field_reads::{MroFieldReads, MroIdentity};
use crate::types::mro::{
    Mro, StaticMroError, StaticMroErrorKind, base, c3, collection, construction, iteration, root,
};
use crate::types::protocol_class::interner::intern_protocol_interface;
use crate::types::{
    CallableType, ClassBase, ClassLiteral, ClassType, FunctionType, GenericAlias,
    PropertyInstanceType, StaticClassLiteral, Type, TypeAndQualifiers, UnionBuilder,
};
use crate::{Db, Program, ProgramEnvironment};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum UnsupportedProtocolInterfaceOperation {
    Reexport,
    Union,
    Narrowing,
    Reachability,
    LoopHeader,
    DiscardedBinding,
    Function,
    Lookup,
    DefinitionInference,
    GenericSpecialization,
    DynamicMro,
    BaseConversion,
    MroDiagnostic,
    MissingObject,
    Property,
    Callable,
    Descriptor,
    MemberNormalization,
}

pub(in crate::types) struct ProtocolSources<'db, PT, UD, DI, GC, EB, PC, KC>
where
    PT: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<PlaceTable>,
        >,
    UD: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<UseDefMap<'a>>,
        >,
    DI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Definition<'a>,
            Output<'a> = DefinitionInference<'a>,
        >,
    GC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    EB: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    PC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    KC: for<'a> Configuration<
            DbView = dyn Db,
            Output<'a> = Result<Option<StaticClassLiteral<'a>>, KnownClassLookupError<'a>>,
        >,
{
    pub(in crate::types) places: FinalSourceRoute<'db, PT>,
    pub(in crate::types) uses: FinalSourceRoute<'db, UD>,
    pub(in crate::types) definitions: Option<FinalSourceRoute<'db, DI>>,
    pub(in crate::types) contexts: FinalSourceRoute<'db, GC>,
    pub(in crate::types) bases: FinalSourceRoute<'db, EB>,
    pub(in crate::types) pep695: Option<FinalSourceRoute<'db, PC>>,
    pub(in crate::types) object: FinalSourceRoute<'db, KC>,
    pub(in crate::types) object_program: Program<'db>,
    pub(in crate::types) object_id: salsa::Id,
}

pub(in crate::types) struct ProtocolMroProvider<'run, 'db: 'run, CM, PT, UD, DI, GC, EB, PC, KC>
where
    CM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Result<Mro<'a>, Box<StaticMroError<'a>>>,
        >,
    PT: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<PlaceTable>,
        >,
    UD: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<UseDefMap<'a>>,
        >,
    DI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Definition<'a>,
            Output<'a> = DefinitionInference<'a>,
        >,
    GC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    EB: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    PC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    KC: for<'a> Configuration<
            DbView = dyn Db,
            Output<'a> = Result<Option<StaticClassLiteral<'a>>, KnownClassLookupError<'a>>,
        >,
{
    pub(in crate::types) route: CallableRoute<'run, 'db, CM>,
    pub(in crate::types) sources: &'run ProtocolSources<'db, PT, UD, DI, GC, EB, PC, KC>,
}

pub(in crate::types) struct ProtocolInterfaceProvider<
    'run,
    'db: 'run,
    CM,
    PT,
    UD,
    DI,
    GC,
    EB,
    PC,
    KC,
    MI,
> where
    CM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Result<Mro<'a>, Box<StaticMroError<'a>>>,
        >,
    PT: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<PlaceTable>,
        >,
    UD: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<UseDefMap<'a>>,
        >,
    DI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Definition<'a>,
            Output<'a> = DefinitionInference<'a>,
        >,
    GC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    EB: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    PC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    KC: for<'a> Configuration<
            DbView = dyn Db,
            Output<'a> = Result<Option<StaticClassLiteral<'a>>, KnownClassLookupError<'a>>,
        >,
    MI: for<'a> Configuration<
            DbView = dyn Db,
            SalsaStruct<'a> = ProtocolInterface<'a>,
            Output<'a> = usize,
        >,
{
    pub(in crate::types) sources: &'run ProtocolSources<'db, PT, UD, DI, GC, EB, PC, KC>,
    pub(in crate::types) mro_route: CallableRoute<'run, 'db, CM>,
    pub(in crate::types) values: FiniteInternedValues<'db, ProtocolInterface<'static>, MI>,
}

struct RuntimeProtocolEffects<'call, 'run: 'call, 'db: 'run, CM, PT, UD, DI, GC, EB, PC, KC>
where
    CM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Result<Mro<'a>, Box<StaticMroError<'a>>>,
        >,
    PT: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<PlaceTable>,
        >,
    UD: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<UseDefMap<'a>>,
        >,
    DI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Definition<'a>,
            Output<'a> = DefinitionInference<'a>,
        >,
    GC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    EB: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    PC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    KC: for<'a> Configuration<
            DbView = dyn Db,
            Output<'a> = Result<Option<StaticClassLiteral<'a>>, KnownClassLookupError<'a>>,
        >,
{
    db: &'db dyn Db,
    fields: MroFieldReads<'db>,
    endpoint: &'call TaskEndpoint<'run, 'db>,
    route: &'call CallableRoute<'run, 'db, CM>,
    sources: &'run ProtocolSources<'db, PT, UD, DI, GC, EB, PC, KC>,
}

impl<'call, 'run: 'call, 'db: 'run, CM, PT, UD, DI, GC, EB, PC, KC>
    RuntimeProtocolEffects<'call, 'run, 'db, CM, PT, UD, DI, GC, EB, PC, KC>
where
    CM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Result<Mro<'a>, Box<StaticMroError<'a>>>,
        >,
    PT: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<PlaceTable>,
        >,
    UD: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<UseDefMap<'a>>,
        >,
    DI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Definition<'a>,
            Output<'a> = DefinitionInference<'a>,
        >,
    GC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    EB: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    PC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    KC: for<'a> Configuration<
            DbView = dyn Db,
            Output<'a> = Result<Option<StaticClassLiteral<'a>>, KnownClassLookupError<'a>>,
        >,
{
    fn endpoint(&self) -> &TaskEndpoint<'run, 'db> {
        self.endpoint
    }

    fn unsupported(&self, reason: UnsupportedProtocolInterfaceOperation) -> RunError {
        let reason = expansion_probe::refuse(
            self.db,
            Incomplete::UnsupportedProtocolInterfaceOperation(reason),
        );
        RunError::Refused(match reason {
            Incomplete::Allowance => salsa::attempt_probe::Incomplete::Allowance,
            Incomplete::RequestedAllocation => salsa::attempt_probe::Incomplete::RequestedAllocation,
            _ => salsa::attempt_probe::Incomplete::Interrupted,
        })
    }

    async fn refuse<T>(&self, reason: UnsupportedProtocolInterfaceOperation) -> RunResult<T> {
        Ok(self
            .endpoint()
            .local_call(|| {
                self.endpoint().admit_work(1)?;
                Err(self.unsupported(reason))
            })
            .await)
    }

    async fn work(&self, extra: usize) -> RunResult<()> {
        self.endpoint()
            .local_call(|| {
                let units = extra.checked_add(1).ok_or(RunError::Refused(
                    salsa::attempt_probe::Incomplete::Allowance,
                ))?;
                self.endpoint().admit_work(units)
            })
            .await;
        Ok(())
    }

    async fn object(&self, env: &ProgramEnvironment<'db>) -> RunResult<ClassBase<'db>> {
        self.endpoint()
            .local_call(|| {
                self.endpoint().admit_work(1)?;
                if env.program(self.db) != self.sources.object_program {
                    return Err(
                        self.unsupported(UnsupportedProtocolInterfaceOperation::MissingObject)
                    );
                }
                Ok(())
            })
            .await;
        let result = self
            .endpoint()
            .read_final_source(&self.sources.object, self.sources.object_id)
            .await;
        Ok(self
            .endpoint()
            .local_call(|| {
                self.endpoint().admit_work(1)?;
                interpret_class_literal_lookup(*result)
                    .map(|class| ClassBase::Class(ClassType::NonGeneric(class.into())))
                    .ok_or_else(|| {
                        self.unsupported(UnsupportedProtocolInterfaceOperation::MissingObject)
                    })
            })
            .await)
    }

    async fn stored_mro(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<&'db Result<Mro<'db>, Box<StaticMroError<'db>>>> {
        self.work(0).await?;
        Ok(self
            .endpoint()
            .child_call(|| async { self.endpoint().fetch_ref(self.route, class.as_id())?.await })
            .await)
    }

    async fn env(&self, class: ClassType<'db>) -> RunResult<ProgramEnvironment<'db>> {
        Ok(self
            .endpoint()
            .local_call(|| {
                self.endpoint().admit_work(1)?;
                let Some((class, _)) = self.fields.static_class_literal(class) else {
                    return Err(self.unsupported(UnsupportedProtocolInterfaceOperation::DynamicMro));
                };
                Ok(ProgramEnvironment::from_file(class.program_file(self.db)))
            })
            .await)
    }

    async fn program(&self, env: &ProgramEnvironment<'db>) -> RunResult<Program<'db>> {
        Ok(self
            .endpoint()
            .local_call(|| {
                self.endpoint().admit_work(1)?;
                Ok(env.program(self.db))
            })
            .await)
    }
}

impl<'call, 'run: 'call, 'db: 'run, CM, PT, UD, DI, GC, EB, PC, KC> root::sealed::Sealed
    for RuntimeProtocolEffects<'call, 'run, 'db, CM, PT, UD, DI, GC, EB, PC, KC>
where
    CM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Result<Mro<'a>, Box<StaticMroError<'a>>>,
        >,
    PT: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<PlaceTable>,
        >,
    UD: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<UseDefMap<'a>>,
        >,
    DI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Definition<'a>,
            Output<'a> = DefinitionInference<'a>,
        >,
    GC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    EB: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    PC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    KC: for<'a> Configuration<
            DbView = dyn Db,
            Output<'a> = Result<Option<StaticClassLiteral<'a>>, KnownClassLookupError<'a>>,
        >,
{
}

impl<'call, 'run: 'call, 'db: 'run, CM, PT, UD, DI, GC, EB, PC, KC> base::sealed::Sealed
    for RuntimeProtocolEffects<'call, 'run, 'db, CM, PT, UD, DI, GC, EB, PC, KC>
where
    CM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Result<Mro<'a>, Box<StaticMroError<'a>>>,
        >,
    PT: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<PlaceTable>,
        >,
    UD: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<UseDefMap<'a>>,
        >,
    DI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Definition<'a>,
            Output<'a> = DefinitionInference<'a>,
        >,
    GC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    EB: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    PC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    KC: for<'a> Configuration<
            DbView = dyn Db,
            Output<'a> = Result<Option<StaticClassLiteral<'a>>, KnownClassLookupError<'a>>,
        >,
{
}

impl<'call, 'run: 'call, 'db: 'run, CM, PT, UD, DI, GC, EB, PC, KC> construction::sealed::Sealed
    for RuntimeProtocolEffects<'call, 'run, 'db, CM, PT, UD, DI, GC, EB, PC, KC>
where
    CM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Result<Mro<'a>, Box<StaticMroError<'a>>>,
        >,
    PT: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<PlaceTable>,
        >,
    UD: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<UseDefMap<'a>>,
        >,
    DI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Definition<'a>,
            Output<'a> = DefinitionInference<'a>,
        >,
    GC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    EB: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    PC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    KC: for<'a> Configuration<
            DbView = dyn Db,
            Output<'a> = Result<Option<StaticClassLiteral<'a>>, KnownClassLookupError<'a>>,
        >,
{
}

impl<'call, 'run: 'call, 'db: 'run, CM, PT, UD, DI, GC, EB, PC, KC> c3::sealed::Sealed
    for RuntimeProtocolEffects<'call, 'run, 'db, CM, PT, UD, DI, GC, EB, PC, KC>
where
    CM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Result<Mro<'a>, Box<StaticMroError<'a>>>,
        >,
    PT: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<PlaceTable>,
        >,
    UD: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<UseDefMap<'a>>,
        >,
    DI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Definition<'a>,
            Output<'a> = DefinitionInference<'a>,
        >,
    GC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    EB: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    PC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    KC: for<'a> Configuration<
            DbView = dyn Db,
            Output<'a> = Result<Option<StaticClassLiteral<'a>>, KnownClassLookupError<'a>>,
        >,
{
}

impl<'call, 'run: 'call, 'db: 'run, CM, PT, UD, DI, GC, EB, PC, KC> source_effects::sealed::Sealed
    for RuntimeProtocolEffects<'call, 'run, 'db, CM, PT, UD, DI, GC, EB, PC, KC>
where
    CM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Result<Mro<'a>, Box<StaticMroError<'a>>>,
        >,
    PT: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<PlaceTable>,
        >,
    UD: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<UseDefMap<'a>>,
        >,
    DI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Definition<'a>,
            Output<'a> = DefinitionInference<'a>,
        >,
    GC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    EB: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    PC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    KC: for<'a> Configuration<
            DbView = dyn Db,
            Output<'a> = Result<Option<StaticClassLiteral<'a>>, KnownClassLookupError<'a>>,
        >,
{
}

impl<'call, 'run: 'call, 'db: 'run, CM, PT, UD, DI, GC, EB, PC, KC> root::MroRootFacts<'db>
    for RuntimeProtocolEffects<'call, 'run, 'db, CM, PT, UD, DI, GC, EB, PC, KC>
where
    CM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Result<Mro<'a>, Box<StaticMroError<'a>>>,
        >,
    PT: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<PlaceTable>,
        >,
    UD: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<UseDefMap<'a>>,
        >,
    DI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Definition<'a>,
            Output<'a> = DefinitionInference<'a>,
        >,
    GC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    EB: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    PC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    KC: for<'a> Configuration<
            DbView = dyn Db,
            Output<'a> = Result<Option<StaticClassLiteral<'a>>, KnownClassLookupError<'a>>,
        >,
{
    type Error = RunError;
}

impl<'call, 'run: 'call, 'db: 'run, CM, PT, UD, DI, GC, EB, PC, KC> base::BaseMroFacts<'db>
    for RuntimeProtocolEffects<'call, 'run, 'db, CM, PT, UD, DI, GC, EB, PC, KC>
where
    CM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Result<Mro<'a>, Box<StaticMroError<'a>>>,
        >,
    PT: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<PlaceTable>,
        >,
    UD: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<UseDefMap<'a>>,
        >,
    DI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Definition<'a>,
            Output<'a> = DefinitionInference<'a>,
        >,
    GC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    EB: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    PC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    KC: for<'a> Configuration<
            DbView = dyn Db,
            Output<'a> = Result<Option<StaticClassLiteral<'a>>, KnownClassLookupError<'a>>,
        >,
{
    type Error = RunError;
}

impl<'call, 'run: 'call, 'db: 'run, CM, PT, UD, DI, GC, EB, PC, KC>
    construction::StaticMroFacts<'db>
    for RuntimeProtocolEffects<'call, 'run, 'db, CM, PT, UD, DI, GC, EB, PC, KC>
where
    CM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Result<Mro<'a>, Box<StaticMroError<'a>>>,
        >,
    PT: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<PlaceTable>,
        >,
    UD: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<UseDefMap<'a>>,
        >,
    DI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Definition<'a>,
            Output<'a> = DefinitionInference<'a>,
        >,
    GC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    EB: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    PC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    KC: for<'a> Configuration<
            DbView = dyn Db,
            Output<'a> = Result<Option<StaticClassLiteral<'a>>, KnownClassLookupError<'a>>,
        >,
{
    type Error = RunError;
}

impl<'call, 'run: 'call, 'db: 'run, CM, PT, UD, DI, GC, EB, PC, KC> root::MroRootEffects<'db>
    for RuntimeProtocolEffects<'call, 'run, 'db, CM, PT, UD, DI, GC, EB, PC, KC>
where
    CM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Result<Mro<'a>, Box<StaticMroError<'a>>>,
        >,
    PT: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<PlaceTable>,
        >,
    UD: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<UseDefMap<'a>>,
        >,
    DI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Definition<'a>,
            Output<'a> = DefinitionInference<'a>,
        >,
    GC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    EB: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    PC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    KC: for<'a> Configuration<
            DbView = dyn Db,
            Output<'a> = Result<Option<StaticClassLiteral<'a>>, KnownClassLookupError<'a>>,
        >,
{
    async fn generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        Ok(*self
            .endpoint()
            .read_final_source(&self.sources.contexts, class.as_id())
            .await)
    }
    async fn generic_alias(
        &self,
        _class: StaticClassLiteral<'db>,
        _specialization: Specialization<'db>,
    ) -> RunResult<ClassType<'db>> {
        self.refuse(UnsupportedProtocolInterfaceOperation::GenericSpecialization)
            .await
    }
    async fn checkpoint(&self, _work: root::MroRootWork) -> RunResult<()> {
        self.work(0).await
    }
    async fn default_class_specialization(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<ClassType<'db>> {
        if root::MroRootEffects::generic_context(self, class)
            .await?
            .is_some()
        {
            return self
                .refuse(UnsupportedProtocolInterfaceOperation::GenericSpecialization)
                .await;
        }
        Ok(ClassType::NonGeneric(class.into()))
    }
    async fn tuple_runtime_specialization(
        &self,
        _specialization: Specialization<'db>,
    ) -> RunResult<Specialization<'db>> {
        self.refuse(UnsupportedProtocolInterfaceOperation::GenericSpecialization)
            .await
    }
}

impl<'call, 'run: 'call, 'db: 'run, CM, PT, UD, DI, GC, EB, PC, KC>
    iteration::MroIterationEffects<'db>
    for RuntimeProtocolEffects<'call, 'run, 'db, CM, PT, UD, DI, GC, EB, PC, KC>
where
    CM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Result<Mro<'a>, Box<StaticMroError<'a>>>,
        >,
    PT: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<PlaceTable>,
        >,
    UD: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<UseDefMap<'a>>,
        >,
    DI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Definition<'a>,
            Output<'a> = DefinitionInference<'a>,
        >,
    GC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    EB: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    PC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    KC: for<'a> Configuration<
            DbView = dyn Db,
            Output<'a> = Result<Option<StaticClassLiteral<'a>>, KnownClassLookupError<'a>>,
        >,
{
    async fn iteration_checkpoint(&self, _work: iteration::MroIterationWork) -> RunResult<()> {
        self.work(0).await
    }
    async fn full_mro(&self, request: root::MroTailRequest<'db>) -> RunResult<&'db Mro<'db>> {
        match request {
            root::MroTailRequest::Static(class, None) => Ok(self
                .stored_mro(class)
                .await?
                .as_ref()
                .unwrap_or_else(|error| error.fallback_mro())),
            root::MroTailRequest::Static(_, Some(_)) => {
                self.refuse(UnsupportedProtocolInterfaceOperation::GenericSpecialization)
                    .await
            }
            _ => {
                self.refuse(UnsupportedProtocolInterfaceOperation::DynamicMro)
                    .await
            }
        }
    }
}

impl<'call, 'run: 'call, 'db: 'run, CM, PT, UD, DI, GC, EB, PC, KC> base::BaseMroEffects<'db>
    for RuntimeProtocolEffects<'call, 'run, 'db, CM, PT, UD, DI, GC, EB, PC, KC>
where
    CM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Result<Mro<'a>, Box<StaticMroError<'a>>>,
        >,
    PT: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<PlaceTable>,
        >,
    UD: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<UseDefMap<'a>>,
        >,
    DI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Definition<'a>,
            Output<'a> = DefinitionInference<'a>,
        >,
    GC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    EB: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    PC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    KC: for<'a> Configuration<
            DbView = dyn Db,
            Output<'a> = Result<Option<StaticClassLiteral<'a>>, KnownClassLookupError<'a>>,
        >,
{
    async fn alias_origin(&self, alias: GenericAlias<'db>) -> RunResult<StaticClassLiteral<'db>> {
        Ok(self.fields.alias_origin(alias))
    }
    async fn alias_specialization(
        &self,
        alias: GenericAlias<'db>,
    ) -> RunResult<Specialization<'db>> {
        Ok(self.fields.alias_specialization(alias))
    }
    async fn object_base(&self, env: &ProgramEnvironment<'db>) -> RunResult<ClassBase<'db>> {
        self.object(env).await
    }
    async fn checkpoint(&self, _work: base::BaseMroWork) -> RunResult<()> {
        self.work(0).await
    }
    async fn compose_specialization(
        &self,
        _base: Specialization<'db>,
        _additional: Specialization<'db>,
    ) -> RunResult<Specialization<'db>> {
        self.refuse(UnsupportedProtocolInterfaceOperation::GenericSpecialization)
            .await
    }
    async fn collect_start(
        &self,
        start: base::BaseMroStart<'db>,
    ) -> RunResult<VecDeque<ClassBase<'db>>> {
        collection::base::collect_start_with(self.fields, start, self).await
    }
    async fn collect_start_with_root(
        &self,
        root: ClassType<'db>,
        start: base::BaseMroStart<'db>,
    ) -> RunResult<Mro<'db>> {
        collection::base::collect_start_with_root_with(self.fields, root, start, self).await
    }
}

impl<'call, 'run: 'call, 'db: 'run, CM, PT, UD, DI, GC, EB, PC, KC>
    collection::MroCollectionEffects<'db>
    for RuntimeProtocolEffects<'call, 'run, 'db, CM, PT, UD, DI, GC, EB, PC, KC>
where
    CM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Result<Mro<'a>, Box<StaticMroError<'a>>>,
        >,
    PT: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<PlaceTable>,
        >,
    UD: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<UseDefMap<'a>>,
        >,
    DI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Definition<'a>,
            Output<'a> = DefinitionInference<'a>,
        >,
    GC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    EB: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    PC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    KC: for<'a> Configuration<
            DbView = dyn Db,
            Output<'a> = Result<Option<StaticClassLiteral<'a>>, KnownClassLookupError<'a>>,
        >,
{
    async fn collection_checkpoint(&self, work: collection::MroCollectionWork) -> RunResult<()> {
        self.work(match work {
            collection::MroCollectionWork::BoxOutput { len, .. } => len,
            _ => 0,
        })
        .await
    }
}

impl<'call, 'run: 'call, 'db: 'run, CM, PT, UD, DI, GC, EB, PC, KC> c3::C3Effects<'db>
    for RuntimeProtocolEffects<'call, 'run, 'db, CM, PT, UD, DI, GC, EB, PC, KC>
where
    CM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Result<Mro<'a>, Box<StaticMroError<'a>>>,
        >,
    PT: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<PlaceTable>,
        >,
    UD: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<UseDefMap<'a>>,
        >,
    DI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Definition<'a>,
            Output<'a> = DefinitionInference<'a>,
        >,
    GC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    EB: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    PC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    KC: for<'a> Configuration<
            DbView = dyn Db,
            Output<'a> = Result<Option<StaticClassLiteral<'a>>, KnownClassLookupError<'a>>,
        >,
{
    type Error = RunError;
    async fn checkpoint(&self, work: c3::C3Work) -> RunResult<()> {
        self.work(match work {
            c3::C3Work::OutputCapacity { entries } => entries,
            c3::C3Work::RetainSequences { len } | c3::C3Work::BoxOutput { len, .. } => len,
            c3::C3Work::IdentityComparison { todo_bytes }
            | c3::C3Work::RemoveHead { todo_bytes } => todo_bytes,
            _ => 0,
        })
        .await
    }

    async fn mro_identity(
        &self,
        _fields: MroFieldReads<'db>,
        base: ClassBase<'db>,
    ) -> RunResult<Type<'db>> {
        match MroIdentity::of(base) {
            MroIdentity::Type(ty) => Ok(ty),
            MroIdentity::GenericAlias(alias) => {
                let endpoint = self.endpoint();
                let origin = endpoint
                    .read_field(
                        alias
                            .field_requests(endpoint.field_request_context())
                            .origin(),
                        &BorrowOrCopy,
                    )
                    .await;
                Ok(Type::ClassLiteral(origin.into()))
            }
        }
    }
}

impl<'call, 'run: 'call, 'db: 'run, CM, PT, UD, DI, GC, EB, PC, KC>
    construction::StaticMroEffects<'db>
    for RuntimeProtocolEffects<'call, 'run, 'db, CM, PT, UD, DI, GC, EB, PC, KC>
where
    CM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Result<Mro<'a>, Box<StaticMroError<'a>>>,
        >,
    PT: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<PlaceTable>,
        >,
    UD: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<UseDefMap<'a>>,
        >,
    DI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Definition<'a>,
            Output<'a> = DefinitionInference<'a>,
        >,
    GC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    EB: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    PC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    KC: for<'a> Configuration<
            DbView = dyn Db,
            Output<'a> = Result<Option<StaticClassLiteral<'a>>, KnownClassLookupError<'a>>,
        >,
{
    async fn body_scope(&self, class: StaticClassLiteral<'db>) -> RunResult<ScopeId<'db>> {
        Ok(self.fields.body_scope(class))
    }

    async fn is_object(&self, class: ClassType<'db>) -> RunResult<bool> {
        Ok(self.fields.is_object(class))
    }

    async fn static_class_literal(
        &self,
        class: ClassType<'db>,
    ) -> RunResult<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>> {
        Ok(self.fields.static_class_literal(class))
    }

    async fn explicit_bases<'borrow>(
        &'borrow self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<&'borrow [Type<'db>]>
    where
        'db: 'borrow,
    {
        let has_bases = self
            .endpoint()
            .local_call(|| {
                self.endpoint().admit_work(1)?;
                Ok(class.has_explicit_bases(self.db))
            })
            .await;
        if !has_bases {
            return Ok(&[]);
        }
        Ok(self
            .endpoint()
            .read_final_source(&self.sources.bases, class.as_id())
            .await)
    }
    async fn has_pep_695_type_params(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        let has_params = self
            .endpoint()
            .local_call(|| {
                self.endpoint().admit_work(1)?;
                Ok(class.has_type_params(self.db))
            })
            .await;
        if !has_params {
            return Ok(false);
        }
        let Some(route) = &self.sources.pep695 else {
            return self
                .refuse(UnsupportedProtocolInterfaceOperation::GenericSpecialization)
                .await;
        };
        Ok(self
            .endpoint()
            .read_final_source(route, class.as_id())
            .await
            .is_some())
    }
    async fn converted_explicit_base(
        &self,
        _env: &ProgramEnvironment<'db>,
        _class: StaticClassLiteral<'db>,
        _index: usize,
        ty: Type<'db>,
    ) -> RunResult<Option<ClassBase<'db>>> {
        let conversion = self
            .endpoint()
            .local_call(|| {
                self.endpoint().admit_work(1)?;
                Ok(ClassBaseConversion::from_explicit_type(ty))
            })
            .await;
        match conversion {
            ClassBaseConversion::Ready(base) => Ok(base),
            ClassBaseConversion::Dependency(ClassBaseDependency::DefaultSpecialization(
                ClassLiteral::Static(class),
            )) => Ok(Some(
                root::mro_first_with(self.fields, class.into(), None, self).await?,
            )),
            _ => {
                self.refuse(UnsupportedProtocolInterfaceOperation::BaseConversion)
                    .await
            }
        }
    }
    async fn object_base(&self, env: &ProgramEnvironment<'db>) -> RunResult<ClassBase<'db>> {
        self.object(env).await
    }
    async fn checkpoint(&self, work: construction::StaticMroWork) -> RunResult<()> {
        self.work(match work {
            construction::StaticMroWork::GenericProtocolScan { len }
            | construction::StaticMroWork::GenericAliasScan { len }
            | construction::StaticMroWork::InvalidBasesBox { len, .. }
            | construction::StaticMroWork::DirectSequenceCapacity { len } => len,
            construction::StaticMroWork::FixedMro { entries } => entries,
            _ => 0,
        })
        .await
    }
    async fn root_class(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> RunResult<ClassType<'db>> {
        root::apply_optional_class_specialization_with(self.fields, class, specialization, self)
            .await
    }
    async fn static_mro_is_cycle(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> RunResult<bool> {
        if specialization.is_some() {
            return self
                .refuse(UnsupportedProtocolInterfaceOperation::GenericSpecialization)
                .await;
        }
        Ok(self
            .stored_mro(class)
            .await?
            .as_ref()
            .is_err_and(|error| error.is_cycle()))
    }
    async fn collect_single_base_mro(
        &self,
        env: &ProgramEnvironment<'db>,
        root: ClassType<'db>,
        base: ClassBase<'db>,
        additional: Option<Specialization<'db>>,
    ) -> RunResult<Mro<'db>> {
        base::collect_single_base_mro_with(self.fields, env, root, base, additional, self).await
    }
    async fn collect_base_mro(
        &self,
        env: &ProgramEnvironment<'db>,
        base: ClassBase<'db>,
        additional: Option<Specialization<'db>>,
    ) -> RunResult<VecDeque<ClassBase<'db>>> {
        base::collect_base_mro_with(self.fields, env, base, additional, self).await
    }
    async fn specialize_base(
        &self,
        base: ClassBase<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> RunResult<ClassBase<'db>> {
        self.work(0).await?;
        if specialization.is_some() {
            return self
                .refuse(UnsupportedProtocolInterfaceOperation::GenericSpecialization)
                .await;
        }
        Ok(base)
    }
    async fn c3_merge(
        &self,
        sequences: Vec<VecDeque<ClassBase<'db>>>,
    ) -> RunResult<Option<Mro<'db>>> {
        c3::c3_merge_with(self.fields, sequences, self).await
    }
    async fn make_error(
        &self,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
        kind: StaticMroErrorKind<'db>,
    ) -> RunResult<StaticMroError<'db>> {
        let object = self.object(env).await?;
        self.work(0).await?;
        Ok(kind.into_mro_error_with_object(class, object))
    }
    async fn failed_c3(
        &self,
        _env: &ProgramEnvironment<'db>,
        _class_literal: StaticClassLiteral<'db>,
        _class: ClassType<'db>,
        _original_bases: &[Type<'db>],
        _resolved_bases: &[ClassBase<'db>],
    ) -> RunResult<Result<Mro<'db>, StaticMroError<'db>>> {
        self.refuse(UnsupportedProtocolInterfaceOperation::MroDiagnostic)
            .await
    }
}

impl<'call, 'run: 'call, 'db: 'run, CM, PT, UD, DI, GC, EB, PC, KC>
    ProtocolCandidateEffects<'db, ProtocolInterfaceBuild<'db>>
    for RuntimeProtocolEffects<'call, 'run, 'db, CM, PT, UD, DI, GC, EB, PC, KC>
where
    CM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Result<Mro<'a>, Box<StaticMroError<'a>>>,
        >,
    PT: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<PlaceTable>,
        >,
    UD: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<UseDefMap<'a>>,
        >,
    DI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Definition<'a>,
            Output<'a> = DefinitionInference<'a>,
        >,
    GC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    EB: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    PC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    KC: for<'a> Configuration<
            DbView = dyn Db,
            Output<'a> = Result<Option<StaticClassLiteral<'a>>, KnownClassLookupError<'a>>,
        >,
{
    type Error = RunError;
    async fn mro_start(&self, class: ClassType<'db>) -> RunResult<iteration::MroCursor<'db>> {
        let start = base::class_mro_start_with(self.fields, class, None, self).await?;
        Ok(iteration::MroCursor::new(start.class, start.specialization))
    }
    async fn mro_next(
        &self,
        cursor: &mut iteration::MroCursor<'db>,
    ) -> RunResult<Option<ClassBase<'db>>> {
        iteration::mro_next_with(self.fields, cursor, iteration::MroDirection::Forward, self).await
    }
    async fn protocol_scope(
        &self,
        class: ClassType<'db>,
    ) -> RunResult<Option<(ScopeId<'db>, Option<Specialization<'db>>)>> {
        let Some((class, specialization)) = self.fields.static_class_literal(class) else {
            return Ok(None);
        };
        let known = self
            .endpoint()
            .local_call(|| {
                self.endpoint().admit_work(1)?;
                Ok(class.is_protocol_without_inference(self.db))
            })
            .await;
        let is_protocol = match known {
            Some(value) => value,
            None => {
                let bases = construction::StaticMroEffects::explicit_bases(self, class).await?;
                self.endpoint()
                    .local_call(|| {
                        self.endpoint().admit_work(1)?;
                        Ok(StaticClassLiteral::protocol_explicit_bases(bases))
                    })
                    .await
            }
        };
        Ok(is_protocol.then(|| (self.fields.body_scope(class), specialization)))
    }
    async fn use_def_map(&self, scope: ScopeId<'db>) -> RunResult<&'db UseDefMap<'db>> {
        Ok(self
            .endpoint()
            .read_final_source(&self.sources.uses, scope.as_id())
            .await)
    }
    async fn place_table(&self, scope: ScopeId<'db>) -> RunResult<&'db PlaceTable> {
        Ok(self
            .endpoint()
            .read_final_source(&self.sources.places, scope.as_id())
            .await)
    }
    async fn binding_place<'map>(
        &self,
        env: &ProgramEnvironment<'db>,
        bindings: BindingWithConstraintsIterator<'map, 'db>,
    ) -> RunResult<PlaceWithDefinition<'db>> {
        place_from_bindings_with(env, self, bindings, RequiresExplicitReExport::No, None).await
    }
    async fn declaration_place<'map>(
        &self,
        env: &ProgramEnvironment<'db>,
        declarations: DeclarationsIterator<'map, 'db>,
        imported: ImportedFinalCandidatesIterator<'map, 'db>,
    ) -> RunResult<PlaceFromDeclarationsResult<'db>> {
        let mut result = place_from_declarations_with(
            env,
            self,
            declarations,
            RequiresExplicitReExport::No,
            None,
        )
        .await?;
        result
            .apply_imported_final_with(
                env,
                self,
                imported,
                RequiresExplicitReExport::No,
                None,
                false,
            )
            .await?;
        self.work(0).await?;
        Ok(result)
    }
    async fn checkpoint(&self, work: ProtocolInterfaceWork) -> RunResult<()> {
        let extra = match work {
            ProtocolInterfaceWork::Bindings { entries } => entries,
            ProtocolInterfaceWork::Declarations {
                entries,
                imported_entries,
            } => entries.saturating_add(imported_entries),
            ProtocolInterfaceWork::CandidateName { bytes } => bytes,
            ProtocolInterfaceWork::MemberLookup { name_bytes }
            | ProtocolInterfaceWork::MemberInsert { name_bytes } => name_bytes,
            _ => 0,
        };
        self.work(extra).await
    }
    async fn visit_candidate(
        &self,
        env: &ProgramEnvironment<'db>,
        consumer: &mut ProtocolInterfaceBuild<'db>,
        name: &Name,
        candidate: ProtocolMemberCandidate<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> RunResult<()> {
        protocol_interface_candidate_with(env, consumer, name, candidate, specialization, self)
            .await
    }
}

impl<'call, 'run: 'call, 'db: 'run, CM, PT, UD, DI, GC, EB, PC, KC> ProtocolInterfaceEffects<'db>
    for RuntimeProtocolEffects<'call, 'run, 'db, CM, PT, UD, DI, GC, EB, PC, KC>
where
    CM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Result<Mro<'a>, Box<StaticMroError<'a>>>,
        >,
    PT: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<PlaceTable>,
        >,
    UD: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<UseDefMap<'a>>,
        >,
    DI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Definition<'a>,
            Output<'a> = DefinitionInference<'a>,
        >,
    GC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    EB: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    PC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    KC: for<'a> Configuration<
            DbView = dyn Db,
            Output<'a> = Result<Option<StaticClassLiteral<'a>>, KnownClassLookupError<'a>>,
        >,
{
    async fn environment(&self, class: ClassType<'db>) -> RunResult<ProgramEnvironment<'db>> {
        self.env(class).await
    }
    async fn with_typevar_bounds(
        &self,
        _specialization: Specialization<'db>,
    ) -> RunResult<Specialization<'db>> {
        self.refuse(UnsupportedProtocolInterfaceOperation::GenericSpecialization)
            .await
    }
    async fn specialize_candidate_type(
        &self,
        ty: Type<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> RunResult<Type<'db>> {
        self.work(0).await?;
        if specialization.is_some() {
            return self
                .refuse(UnsupportedProtocolInterfaceOperation::GenericSpecialization)
                .await;
        }
        Ok(ty)
    }
    async fn property_accessors(
        &self,
        _property: PropertyInstanceType<'db>,
    ) -> RunResult<(Option<Type<'db>>, Option<Type<'db>>)> {
        self.refuse(UnsupportedProtocolInterfaceOperation::Property)
            .await
    }
    async fn callable_is_method_like(&self, _callable: CallableType<'db>) -> RunResult<bool> {
        self.refuse(UnsupportedProtocolInterfaceOperation::Callable)
            .await
    }
    async fn function_is_staticmethod(&self, _function: FunctionType<'db>) -> RunResult<bool> {
        self.refuse(UnsupportedProtocolInterfaceOperation::Function)
            .await
    }
    async fn function_is_classmethod(&self, _function: FunctionType<'db>) -> RunResult<bool> {
        self.refuse(UnsupportedProtocolInterfaceOperation::Function)
            .await
    }
    async fn function_callable(
        &self,
        _function: FunctionType<'db>,
    ) -> RunResult<CallableType<'db>> {
        self.refuse(UnsupportedProtocolInterfaceOperation::Function)
            .await
    }
    async fn definition_is_function(&self, definition: Definition<'db>) -> RunResult<bool> {
        Ok(self
            .endpoint()
            .local_call(|| {
                self.endpoint().admit_work(1)?;
                Ok(definition.kind(self.db).is_function_def())
            })
            .await)
    }
    async fn method_member(
        &self,
        _env: &ProgramEnvironment<'db>,
        _callable: CallableType<'db>,
        _definition: Option<Definition<'db>>,
    ) -> RunResult<ProtocolMemberData<'db>> {
        self.refuse(UnsupportedProtocolInterfaceOperation::Callable)
            .await
    }
    async fn descriptor_member(
        &self,
        _env: &ProgramEnvironment<'db>,
        _ty: Type<'db>,
        _class: ClassType<'db>,
        _definition: Option<Definition<'db>>,
    ) -> RunResult<Option<ProtocolMemberData<'db>>> {
        self.refuse(UnsupportedProtocolInterfaceOperation::Descriptor)
            .await
    }
}

impl<'call, 'run: 'call, 'db: 'run, CM, PT, UD, DI, GC, EB, PC, KC>
    ProtocolInterfaceNormalizationEffects<'db>
    for RuntimeProtocolEffects<'call, 'run, 'db, CM, PT, UD, DI, GC, EB, PC, KC>
where
    CM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Result<Mro<'a>, Box<StaticMroError<'a>>>,
        >,
    PT: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<PlaceTable>,
        >,
    UD: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<UseDefMap<'a>>,
        >,
    DI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Definition<'a>,
            Output<'a> = DefinitionInference<'a>,
        >,
    GC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    EB: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    PC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    KC: for<'a> Configuration<
            DbView = dyn Db,
            Output<'a> = Result<Option<StaticClassLiteral<'a>>, KnownClassLookupError<'a>>,
        >,
{
    type Error = RunError;
    async fn checkpoint(&self, work: ProtocolNormalizationWork) -> RunResult<()> {
        self.work(match work {
            ProtocolNormalizationWork::Member { name_bytes } => name_bytes,
            _ => 0,
        })
        .await
    }
    async fn normalize_member(
        &self,
        _env: &ProgramEnvironment<'db>,
        _current: &ProtocolMemberData<'db>,
        _previous: &ProtocolMemberData<'db>,
        _cycle: &salsa::Cycle<'_>,
    ) -> RunResult<ProtocolMemberData<'db>> {
        self.refuse(UnsupportedProtocolInterfaceOperation::MemberNormalization)
            .await
    }
}

impl<'call, 'run: 'call, 'db: 'run, CM, PT, UD, DI, GC, EB, PC, KC> PublicLookupEffects<'db>
    for RuntimeProtocolEffects<'call, 'run, 'db, CM, PT, UD, DI, GC, EB, PC, KC>
where
    CM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Result<Mro<'a>, Box<StaticMroError<'a>>>,
        >,
    PT: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<PlaceTable>,
        >,
    UD: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<UseDefMap<'a>>,
        >,
    DI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Definition<'a>,
            Output<'a> = DefinitionInference<'a>,
        >,
    GC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    EB: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    PC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    KC: for<'a> Configuration<
            DbView = dyn Db,
            Output<'a> = Result<Option<StaticClassLiteral<'a>>, KnownClassLookupError<'a>>,
        >,
{
    type Error = RunError;
    async fn promote_public_type(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.refuse(UnsupportedProtocolInterfaceOperation::Lookup)
            .await
    }
    async fn union_two(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _first: Type<'db>,
        _second: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.refuse(UnsupportedProtocolInterfaceOperation::Union)
            .await
    }
}

impl<'call, 'run: 'call, 'db: 'run, CM, PT, UD, DI, GC, EB, PC, KC> SourcePlaceEffects<'db>
    for RuntimeProtocolEffects<'call, 'run, 'db, CM, PT, UD, DI, GC, EB, PC, KC>
where
    CM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Result<Mro<'a>, Box<StaticMroError<'a>>>,
        >,
    PT: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<PlaceTable>,
        >,
    UD: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<UseDefMap<'a>>,
        >,
    DI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Definition<'a>,
            Output<'a> = DefinitionInference<'a>,
        >,
    GC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    EB: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    PC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    KC: for<'a> Configuration<
            DbView = dyn Db,
            Output<'a> = Result<Option<StaticClassLiteral<'a>>, KnownClassLookupError<'a>>,
        >,
{
    async fn check_imported_file(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        file: ProgramFile<'db>,
    ) -> RunResult<()> {
        let endpoint = self.endpoint();
        let fields = endpoint.field_request_context();
        let program = endpoint
            .read_field(file.read_fields(fields).program(), &BorrowOrCopy)
            .await;
        let env_program = match env.source() {
            ProgramEnvironmentSource::Program(program) => program,
            ProgramEnvironmentSource::File(file) => {
                endpoint
                    .read_field(file.read_fields(fields).program(), &BorrowOrCopy)
                    .await
            }
            ProgramEnvironmentSource::Definition(definition) => {
                let scope = endpoint
                    .read_field(definition.read_fields(fields).scope_id(), &BorrowOrCopy)
                    .await;
                let file = endpoint
                    .read_field(scope.read_fields(fields).program_file(), &BorrowOrCopy)
                    .await;
                endpoint
                    .read_field(file.read_fields(fields).program(), &BorrowOrCopy)
                    .await
            }
            ProgramEnvironmentSource::Scope(scope) => {
                let file = endpoint
                    .read_field(scope.read_fields(fields).program_file(), &BorrowOrCopy)
                    .await;
                endpoint
                    .read_field(file.read_fields(fields).program(), &BorrowOrCopy)
                    .await
            }
        };
        endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                debug_assert_eq!(program, env_program);
                Ok(())
            })
            .await;
        Ok(())
    }
    async fn file_is_stub(&self, _db: &'db dyn Db, file: ProgramFile<'db>) -> RunResult<bool> {
        let endpoint = self.endpoint();
        let fields = endpoint.field_request_context();
        let python_file = endpoint
            .read_field(file.read_fields(fields).python_file(), &BorrowOrCopy)
            .await;
        let file = endpoint
            .read_field(python_file.read_fields(fields).file(), &BorrowOrCopy)
            .await;
        let path = endpoint
            .read_field(file.read_fields(fields).path(), &BorrowOrCopy)
            .await;
        Ok(endpoint
            .local_call(|| {
                let work = path
                    .as_str()
                    .len()
                    .checked_add(4)
                    .ok_or(RunError::Contract("file path work overflow"))?;
                endpoint.admit_work(work)?;
                Ok(path.source_type().is_stub())
            })
            .await)
    }
    async fn reduction_checkpoint(&self, _work: SourcePlaceWork) -> RunResult<()> {
        self.work(0).await
    }
    async fn definition_kind(
        &self,
        definition: Definition<'db>,
    ) -> RunResult<&'db DefinitionKind<'db>> {
        Ok(self
            .endpoint()
            .local_call(|| {
                self.endpoint().admit_work(1)?;
                Ok(definition.kind(self.db))
            })
            .await)
    }
    async fn definition_is_reexported(&self, definition: Definition<'db>) -> RunResult<bool> {
        Ok(self
            .endpoint()
            .local_call(|| {
                self.endpoint().admit_work(1)?;
                Ok(definition.is_reexported(self.db))
            })
            .await)
    }
    async fn function_is_overload(&self, _function: FunctionType<'db>) -> RunResult<bool> {
        self.refuse(UnsupportedProtocolInterfaceOperation::Function)
            .await
    }
    async fn union_builder(&self, _env: &ProgramEnvironment<'db>) -> RunResult<UnionBuilder<'db>> {
        self.refuse(UnsupportedProtocolInterfaceOperation::Union)
            .await
    }
    async fn narrowing_projector<'map>(
        &self,
        _env: &'map ProgramEnvironment<'db>,
        _constraints: &'map NarrowingConstraints,
        _predicates: &'map IndexSlice<ScopedPredicateId, Predicate<'db>>,
        _targets: &'map PredicateNarrowingTargets,
        _binding: Definition<'db>,
        _base_ty: Type<'db>,
    ) -> RunResult<NarrowingProjector<'map, 'db>>
    where
        'db: 'map,
    {
        self.refuse(UnsupportedProtocolInterfaceOperation::Narrowing)
            .await
    }
    async fn symbol_id(
        &self,
        _db: &'db dyn Db,
        _scope: ScopeId<'db>,
        _name: &str,
    ) -> RunResult<Option<ScopedSymbolId>> {
        self.refuse(UnsupportedProtocolInterfaceOperation::Lookup)
            .await
    }
    async fn is_known_module(
        &self,
        _db: &'db dyn Db,
        _scope: ScopeId<'db>,
        _module: KnownModule,
    ) -> RunResult<bool> {
        self.refuse(UnsupportedProtocolInterfaceOperation::Lookup)
            .await
    }
    async fn place_by_id(
        &self,
        _db: &'db dyn Db,
        _scope: ScopeId<'db>,
        _place: ScopedPlaceId,
        _reexport: RequiresExplicitReExport,
        _considered: ConsideredDefinitions,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.refuse(UnsupportedProtocolInterfaceOperation::Lookup)
            .await
    }
    async fn global_scope(
        &self,
        _db: &'db dyn Db,
        _file: ProgramFile<'db>,
    ) -> RunResult<ScopeId<'db>> {
        self.refuse(UnsupportedProtocolInterfaceOperation::Lookup)
            .await
    }
    async fn resolve_known_module(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _module: KnownModule,
    ) -> RunResult<Option<ProgramFile<'db>>> {
        self.refuse(UnsupportedProtocolInterfaceOperation::Lookup)
            .await
    }
    async fn imported_fallback(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _prior: PlaceAndQualifiers<'db>,
        _file: Option<ProgramFile<'db>>,
        _name: &str,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.refuse(UnsupportedProtocolInterfaceOperation::Lookup)
            .await
    }
    async fn is_reexported(&self, _definition: Definition<'db>) -> RunResult<bool> {
        self.refuse(UnsupportedProtocolInterfaceOperation::Reexport)
            .await
    }
    async fn inferred_declaration(
        &self,
        definition: Definition<'db>,
    ) -> RunResult<Option<TypeAndQualifiers<'db>>> {
        let Some(route) = &self.sources.definitions else {
            return self
                .refuse(UnsupportedProtocolInterfaceOperation::DefinitionInference)
                .await;
        };
        let inference = self
            .endpoint()
            .read_final_source(route, definition.as_id())
            .await;
        self.work(inference.declaration_scan_len(definition))
            .await?;
        Ok(inference.inferred_declaration(definition).declared())
    }
    async fn binding_type(&self, definition: Definition<'db>) -> RunResult<Type<'db>> {
        let Some(route) = &self.sources.definitions else {
            return self
                .refuse(UnsupportedProtocolInterfaceOperation::DefinitionInference)
                .await;
        };
        let inference = self
            .endpoint()
            .read_final_source(route, definition.as_id())
            .await;
        self.work(inference.binding_scan_len(definition)).await?;
        Ok(inference.binding_type(definition))
    }
    async fn is_discarded_dict_key_assignment(
        &self,
        _definition: Definition<'db>,
    ) -> RunResult<bool> {
        self.refuse(UnsupportedProtocolInterfaceOperation::DiscardedBinding)
            .await
    }
    async fn loop_header_reachability(
        &self,
        _definition: Definition<'db>,
    ) -> RunResult<LoopHeaderReachability<'db>> {
        self.refuse(UnsupportedProtocolInterfaceOperation::LoopHeader)
            .await
    }
    async fn reachability(
        &self,
        _cache: Option<&ReachabilityEvaluationCache<'db>>,
        _constraints: &ReachabilityConstraints,
        _predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
        _constraint: ScopedReachabilityConstraintId,
    ) -> RunResult<Truthiness> {
        self.refuse(UnsupportedProtocolInterfaceOperation::Reachability)
            .await
    }
    async fn narrow(
        &self,
        _projector: &mut NarrowingProjector<'_, 'db>,
        _constraint: ScopedNarrowingConstraint,
        _ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.refuse(UnsupportedProtocolInterfaceOperation::Narrowing)
            .await
    }
    async fn union_add(&self, _builder: &mut UnionBuilder<'db>, _ty: Type<'db>) -> RunResult<()> {
        self.refuse(UnsupportedProtocolInterfaceOperation::Union)
            .await
    }
    async fn union_build(&self, builder: UnionBuilder<'db>) -> RunResult<Type<'db>> {
        let result = self
            .refuse(UnsupportedProtocolInterfaceOperation::Union)
            .await;
        drop(builder);
        result
    }
    async fn function_same_place(
        &self,
        _function: FunctionType<'db>,
        _other: FunctionType<'db>,
    ) -> RunResult<bool> {
        self.refuse(UnsupportedProtocolInterfaceOperation::Function)
            .await
    }
    async fn function_contains(
        &self,
        _function: FunctionType<'db>,
        _other: FunctionType<'db>,
    ) -> RunResult<bool> {
        self.refuse(UnsupportedProtocolInterfaceOperation::Function)
            .await
    }
    async fn equivalent(
        &self,
        _env: &ProgramEnvironment<'db>,
        _first: Type<'db>,
        _second: Type<'db>,
    ) -> RunResult<bool> {
        self.refuse(UnsupportedProtocolInterfaceOperation::Lookup)
            .await
    }
    async fn preserve_raw_public_type(
        &self,
        _db: &'db dyn Db,
        _scope: ScopeId<'db>,
        _place: ScopedPlaceId,
    ) -> RunResult<bool> {
        self.refuse(UnsupportedProtocolInterfaceOperation::Lookup)
            .await
    }
}

struct MroNativeComparisonQuote<'a, 'run, 'db> {
    endpoint: &'a TaskEndpoint<'run, 'db>,
    work: usize,
}

impl<'run, 'db: 'run> MroNativeComparisonQuote<'_, 'run, 'db> {
    fn checked(work: Option<usize>) -> RunResult<usize> {
        work.ok_or(RunError::Contract(
            "protocol MRO native value quotation overflow",
        ))
    }

    async fn scan(&self, work: usize) -> RunResult<()> {
        self.endpoint
            .local_call(|| self.endpoint.admit_work(work))
            .await;
        self.endpoint.checkpoint()?.await
    }

    fn add(&mut self, work: usize) -> RunResult<()> {
        self.work = Self::checked(self.work.checked_add(work))?;
        Ok(())
    }

    fn ty(&mut self, ty: Type<'db>) -> RunResult<()> {
        self.add(Self::checked(ty.inline_payload_bytes().checked_add(1))?)
    }

    async fn types(
        &mut self,
        mut types: impl ExactSizeIterator<Item = Type<'db>>,
    ) -> RunResult<()> {
        while types.len() != 0 {
            let chunk = types.len().min(64);
            self.scan(chunk).await?;
            for ty in types.by_ref().take(chunk) {
                self.ty(ty)?;
            }
        }
        Ok(())
    }

    async fn mro(&mut self, mro: &Mro<'db>) -> RunResult<()> {
        self.scan(1).await?;
        self.add(1)?;
        self.types(mro.iter().map(Type::from)).await
    }

    async fn error(&mut self, error: &StaticMroError<'db>) -> RunResult<()> {
        self.scan(8).await?;
        self.add(8)?;
        match error.reason() {
            StaticMroErrorKind::InvalidBases(bases) => {
                self.add(bases.len())?;
                self.types(bases.iter().map(|(_, ty)| *ty)).await?;
            }
            StaticMroErrorKind::DuplicateBases(bases) => {
                let mut bases = bases.iter();
                while bases.len() != 0 {
                    let chunk = bases.len().min(64);
                    self.scan(Self::checked(chunk.checked_mul(4))?).await?;
                    for base in bases.by_ref().take(chunk) {
                        self.add(Self::checked(base.later_indices.len().checked_add(4))?)?;
                        self.ty(Type::from(base.duplicate_base))?;
                    }
                }
            }
            StaticMroErrorKind::UnresolvableMro { bases_list, .. } => {
                self.types(bases_list.iter().copied()).await?;
            }
            StaticMroErrorKind::Pep695ClassWithGenericInheritance
            | StaticMroErrorKind::InheritanceCycle => {}
        }
        self.mro(error.fallback_mro()).await
    }
}

impl<'run, 'db: 'run, CM, PT, UD, DI, GC, EB, PC, KC> CallableRouteProvider<'run, 'db, CM>
    for ProtocolMroProvider<'run, 'db, CM, PT, UD, DI, GC, EB, PC, KC>
where
    CM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Result<Mro<'a>, Box<StaticMroError<'a>>>,
        >,
    PT: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<PlaceTable>,
        >,
    UD: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<UseDefMap<'a>>,
        >,
    DI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Definition<'a>,
            Output<'a> = DefinitionInference<'a>,
        >,
    GC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    EB: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    PC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    KC: for<'a> Configuration<
            DbView = dyn Db,
            Output<'a> = Result<Option<StaticClassLiteral<'a>>, KnownClassLookupError<'a>>,
        >,
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, CM>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        let work = match operation {
            NativeValueOperation::InputConversion(RetainedInput::SalsaStruct(_)) => 1,
            NativeValueOperation::InputConversion(RetainedInput::Interned(_)) => {
                return Err(RunError::Contract(
                    "protocol MRO input requires generated handle conversion",
                ));
            }
            NativeValueOperation::Comparison { left, right } => {
                let mut quote = MroNativeComparisonQuote {
                    endpoint: &endpoint,
                    work: 0,
                };
                for value in [left, right] {
                    quote.scan(1).await?;
                    quote.add(1)?;
                    match value {
                        Ok(mro) => quote.mro(mro).await?,
                        Err(error) => quote.error(error).await?,
                    }
                }
                quote.work
            }
        };
        Ok(NativeValueQuote {
            work,
            requested_bytes: 0,
            cleanup_work: 0,
        })
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Result<Mro<'db>, Box<StaticMroError<'db>>>>
    where
        'run: 'call,
    {
        let effects = RuntimeProtocolEffects {
            db,
            fields: MroFieldReads::new(db),
            endpoint: &endpoint,
            route: &self.route,
            sources: self.sources,
        };
        effects.work(0).await?;
        let result = construction::static_mro_with(effects.fields, class, None, &effects).await?;
        effects.work(0).await?;
        Ok(result.map_err(Box::new))
    }
    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        _id: salsa::Id,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Result<Mro<'db>, Box<StaticMroError<'db>>>>
    where
        'run: 'call,
    {
        let effects = RuntimeProtocolEffects {
            db,
            fields: MroFieldReads::new(db),
            endpoint: &endpoint,
            route: &self.route,
            sources: self.sources,
        };
        effects.work(0).await?;
        let error =
            construction::static_mro_cycle_with(effects.fields, class, None, &effects).await?;
        effects.work(0).await?;
        Ok(Err(Box::new(error)))
    }
    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call Result<Mro<'db>, Box<StaticMroError<'db>>>,
        value: Result<Mro<'db>, Box<StaticMroError<'db>>>,
        _class: StaticClassLiteral<'db>,
    ) -> RunResult<Result<Mro<'db>, Box<StaticMroError<'db>>>>
    where
        'run: 'call,
    {
        endpoint.local_call(|| endpoint.admit_work(1)).await;
        Ok(value)
    }
}

impl<'run, 'db: 'run, CI, CM, PT, UD, DI, GC, EB, PC, KC, MI> CallableRouteProvider<'run, 'db, CI>
    for ProtocolInterfaceProvider<'run, 'db, CM, PT, UD, DI, GC, EB, PC, KC, MI>
where
    CM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Result<Mro<'a>, Box<StaticMroError<'a>>>,
        >,
    PT: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<PlaceTable>,
        >,
    UD: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<UseDefMap<'a>>,
        >,
    DI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Definition<'a>,
            Output<'a> = DefinitionInference<'a>,
        >,
    GC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    EB: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    PC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    KC: for<'a> Configuration<
            DbView = dyn Db,
            Output<'a> = Result<Option<StaticClassLiteral<'a>>, KnownClassLookupError<'a>>,
        >,
    CI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = ClassType<'a>,
            Output<'a> = ProtocolInterface<'a>,
        >,
    MI: for<'a> Configuration<
            DbView = dyn Db,
            SalsaStruct<'a> = ProtocolInterface<'a>,
            Output<'a> = usize,
        >,
{
    async fn native_value<'call>(
        &'call self,
        _endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, CI>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        let work = match operation {
            // ClassType selects between ClassLiteral's five variants and GenericAlias.
            NativeValueOperation::InputConversion(RetainedInput::SalsaStruct(_)) => 8,
            NativeValueOperation::InputConversion(RetainedInput::Interned(_)) => {
                return Err(RunError::Contract(
                    "protocol interface input requires generated supertype conversion",
                ));
            }
            // ProtocolInterface equality compares its generated interned identity.
            NativeValueOperation::Comparison { .. } => 1,
        };
        Ok(NativeValueQuote {
            work,
            requested_bytes: 0,
            cleanup_work: 0,
        })
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        class: ClassType<'db>,
    ) -> RunResult<ProtocolInterface<'db>>
    where
        'run: 'call,
    {
        endpoint.local_call(|| endpoint.admit_work(1)).await;
        let effects = RuntimeProtocolEffects {
            db,
            fields: MroFieldReads::new(db),
            endpoint: &endpoint,
            route: &self.mro_route,
            sources: self.sources,
        };
        let prepared = protocol_interface_build_with(class, &effects).await?;
        let program = effects.program(&prepared.env).await?;
        Ok(intern_protocol_interface(&endpoint, &self.values, program, prepared.members).await)
    }
    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        _id: salsa::Id,
        class: ClassType<'db>,
    ) -> RunResult<ProtocolInterface<'db>>
    where
        'run: 'call,
    {
        endpoint.local_call(|| endpoint.admit_work(1)).await;
        let effects = RuntimeProtocolEffects {
            db,
            fields: MroFieldReads::new(db),
            endpoint: &endpoint,
            route: &self.mro_route,
            sources: self.sources,
        };
        let env = effects.env(class).await?;
        let program = effects.program(&env).await?;
        Ok(intern_protocol_interface(&endpoint, &self.values, program, BTreeMap::new()).await)
    }
    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        cycle: &'call salsa::Cycle<'call>,
        last: &'call ProtocolInterface<'db>,
        value: ProtocolInterface<'db>,
        class: ClassType<'db>,
    ) -> RunResult<ProtocolInterface<'db>>
    where
        'run: 'call,
    {
        endpoint.local_call(|| endpoint.admit_work(1)).await;
        let effects = RuntimeProtocolEffects {
            db,
            fields: MroFieldReads::new(db),
            endpoint: &endpoint,
            route: &self.mro_route,
            sources: self.sources,
        };
        let env = effects.env(class).await?;
        let (previous, current) = endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                Ok((last.inner(db), value.inner(db)))
            })
            .await;
        let members =
            protocol_interface_normalize_with(&env, previous, current, cycle, &effects).await?;
        let program = effects.program(&env).await?;
        Ok(intern_protocol_interface(&endpoint, &self.values, program, members).await)
    }
}

/// Declaration sources shared by interface construction and member lookup.
pub(in crate::types) struct LookupDeclarationSources<
    'run,
    'db: 'run,
    CM,
    PT,
    UD,
    DI,
    GC,
    EB,
    PC,
    KC,
    EM,
    EC,
    DE,
    CG,
    IA,
    KI,
> where
    CM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Result<Mro<'a>, Box<StaticMroError<'a>>>,
        >,
    PT: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<PlaceTable>,
        >,
    UD: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<UseDefMap<'a>>,
        >,
    DI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Definition<'a>,
            Output<'a> = DefinitionInference<'a>,
        >,
    GC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    EB: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    PC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    KC: for<'a> Configuration<
            DbView = dyn Db,
            Output<'a> = Result<Option<StaticClassLiteral<'a>>, KnownClassLookupError<'a>>,
        >,
    EM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = ClassLiteral<'a>,
            Output<'a> = Option<crate::types::enums::EnumMetadata<'a>>,
        >,
    EC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = ClassLiteral<'a>,
            Output<'a> = Option<crate::types::enums::EnumClassLiteral<'a>>,
        >,
    DE: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    CG: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<crate::types::class::CodeGeneratorKind<'a>>,
        >,
    IA: for<'a> Configuration<DbView = dyn Db, Input<'a> = ScopeId<'a>, Output<'a> = Box<[Name]>>,
    KI: for<'a> Configuration<DbView = dyn Db, Output<'a> = Type<'a>>,
{
    pub(in crate::types) unsupported: &'run std::cell::RefCell<Vec<&'static str>>,
    pub(in crate::types) db: &'db dyn Db,
    pub(in crate::types) base: &'run ProtocolSources<'db, PT, UD, DI, GC, EB, PC, KC>,
    pub(in crate::types) mro: CallableRoute<'run, 'db, CM>,
    pub(in crate::types) enum_metadata: FinalSourceRoute<'db, EM>,
    pub(in crate::types) enum_classes: FinalSourceRoute<'db, EC>,
    pub(in crate::types) decorators: FinalSourceRoute<'db, DE>,
    pub(in crate::types) generators: FinalSourceRoute<'db, CG>,
    pub(in crate::types) implicit_names: FinalSourceRoute<'db, IA>,
    pub(in crate::types) instances: FinalSourceRoute<'db, KI>,
    pub(in crate::types) known_keys: &'run [(crate::types::KnownClass, Program<'db>, salsa::Id)],
}

impl<'run, 'db: 'run, CM, PT, UD, DI, GC, EB, PC, KC, EM, EC, DE, CG, IA, KI>
    LookupDeclarationSources<'run, 'db, CM, PT, UD, DI, GC, EB, PC, KC, EM, EC, DE, CG, IA, KI>
where
    CM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Result<Mro<'a>, Box<StaticMroError<'a>>>,
        >,
    PT: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<PlaceTable>,
        >,
    UD: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<UseDefMap<'a>>,
        >,
    DI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Definition<'a>,
            Output<'a> = DefinitionInference<'a>,
        >,
    GC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    EB: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    PC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    KC: for<'a> Configuration<
            DbView = dyn Db,
            Output<'a> = Result<Option<StaticClassLiteral<'a>>, KnownClassLookupError<'a>>,
        >,
    EM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = ClassLiteral<'a>,
            Output<'a> = Option<crate::types::enums::EnumMetadata<'a>>,
        >,
    EC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = ClassLiteral<'a>,
            Output<'a> = Option<crate::types::enums::EnumClassLiteral<'a>>,
        >,
    DE: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    CG: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<crate::types::class::CodeGeneratorKind<'a>>,
        >,
    IA: for<'a> Configuration<DbView = dyn Db, Input<'a> = ScopeId<'a>, Output<'a> = Box<[Name]>>,
    KI: for<'a> Configuration<DbView = dyn Db, Output<'a> = Type<'a>>,
{
    fn effects<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
    ) -> RuntimeProtocolEffects<'call, 'run, 'db, CM, PT, UD, DI, GC, EB, PC, KC> {
        RuntimeProtocolEffects {
            db: self.db,
            fields: MroFieldReads::new(self.db),
            endpoint,
            route: &self.mro,
            sources: self.base,
        }
    }

    async fn known_key(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        program: Program<'db>,
        known: crate::types::KnownClass,
    ) -> RunResult<salsa::Id> {
        for &(candidate, candidate_program, id) in self.known_keys {
            let matches = endpoint
                .local_call(|| {
                    endpoint.admit_work(1)?;
                    Ok(candidate == known && candidate_program == program)
                })
                .await;
            if matches {
                return Ok(id);
            }
        }
        self.effects(endpoint)
            .refuse(UnsupportedProtocolInterfaceOperation::MissingObject)
            .await
    }
}

impl<'run, 'db: 'run, CM, PT, UD, DI, GC, EB, PC, KC, EM, EC, DE, CG, IA, KI>
    crate::types::member_lookup::runtime::MemberSourcesAccess<'run, 'db>
    for LookupDeclarationSources<'run, 'db, CM, PT, UD, DI, GC, EB, PC, KC, EM, EC, DE, CG, IA, KI>
where
    CM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Result<Mro<'a>, Box<StaticMroError<'a>>>,
        >,
    PT: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<PlaceTable>,
        >,
    UD: for<'a> Configuration<
            DbView = dyn ty_python_core::Db,
            Input<'a> = ScopeId<'a>,
            Output<'a> = Arc<UseDefMap<'a>>,
        >,
    DI: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Definition<'a>,
            Output<'a> = DefinitionInference<'a>,
        >,
    GC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    EB: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    PC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<GenericContext<'a>>,
        >,
    KC: for<'a> Configuration<
            DbView = dyn Db,
            Output<'a> = Result<Option<StaticClassLiteral<'a>>, KnownClassLookupError<'a>>,
        >,
    EM: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = ClassLiteral<'a>,
            Output<'a> = Option<crate::types::enums::EnumMetadata<'a>>,
        >,
    EC: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = ClassLiteral<'a>,
            Output<'a> = Option<crate::types::enums::EnumClassLiteral<'a>>,
        >,
    DE: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Box<[Type<'a>]>,
        >,
    CG: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<crate::types::class::CodeGeneratorKind<'a>>,
        >,
    IA: for<'a> Configuration<DbView = dyn Db, Input<'a> = ScopeId<'a>, Output<'a> = Box<[Name]>>,
    KI: for<'a> Configuration<DbView = dyn Db, Output<'a> = Type<'a>>,
{
    fn record_unsupported(&self, operation: &'static str) {
        self.unsupported.borrow_mut().push(operation);
    }
    fn db(&self) -> &'db dyn Db {
        self.db
    }
    async fn places(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        scope: ScopeId<'db>,
    ) -> RunResult<&'db PlaceTable> {
        Ok(endpoint
            .read_final_source(&self.base.places, scope.as_id())
            .await)
    }
    async fn uses(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        scope: ScopeId<'db>,
    ) -> RunResult<&'db UseDefMap<'db>> {
        Ok(endpoint
            .read_final_source(&self.base.uses, scope.as_id())
            .await)
    }
    async fn public_place(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        scope: ScopeId<'db>,
        place: ScopedPlaceId,
        reexport: RequiresExplicitReExport,
        definitions: ConsideredDefinitions,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        let uses = endpoint
            .read_final_source(&self.base.uses, scope.as_id())
            .await;
        crate::place::place_by_id_with(
            self.db,
            &self.effects(endpoint),
            scope,
            place,
            reexport,
            definitions,
            uses,
        )
        .await
    }
    async fn binding_place<'map>(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        env: &ProgramEnvironment<'db>,
        bindings: BindingWithConstraintsIterator<'map, 'db>,
    ) -> RunResult<PlaceWithDefinition<'db>> {
        place_from_bindings_with(
            env,
            &self.effects(endpoint),
            bindings,
            RequiresExplicitReExport::No,
            None,
        )
        .await
    }
    async fn declaration_place<'map>(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        env: &ProgramEnvironment<'db>,
        declarations: DeclarationsIterator<'map, 'db>,
    ) -> RunResult<PlaceFromDeclarationsResult<'db>> {
        place_from_declarations_with(
            env,
            &self.effects(endpoint),
            declarations,
            RequiresExplicitReExport::No,
            None,
        )
        .await
    }
    async fn imported_final<'map>(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        env: &ProgramEnvironment<'db>,
        result: PlaceFromDeclarationsResult<'db>,
        imported: ImportedFinalCandidatesIterator<'map, 'db>,
    ) -> RunResult<PlaceFromDeclarationsResult<'db>> {
        result
            .with_imported_final_with(
                env,
                &self.effects(endpoint),
                imported,
                RequiresExplicitReExport::No,
                None,
                false,
            )
            .await
    }
    async fn explicit_bases(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<&'db [Type<'db>]> {
        if !endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                Ok(class.has_explicit_bases(self.db))
            })
            .await
        {
            return Ok(&[]);
        }
        Ok(endpoint
            .read_final_source(&self.base.bases, class.as_id())
            .await)
    }
    async fn context(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        Ok(*endpoint
            .read_final_source(&self.base.contexts, class.as_id())
            .await)
    }
    async fn mro_start(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        class: ClassType<'db>,
    ) -> RunResult<iteration::MroCursor<'db>> {
        ProtocolCandidateEffects::mro_start(&self.effects(endpoint), class).await
    }
    async fn mro_next(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        cursor: &mut iteration::MroCursor<'db>,
    ) -> RunResult<Option<ClassBase<'db>>> {
        ProtocolCandidateEffects::mro_next(&self.effects(endpoint), cursor).await
    }
    async fn known_class(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        program: Program<'db>,
        known: crate::types::KnownClass,
    ) -> RunResult<Type<'db>> {
        let key = self.known_key(endpoint, program, known).await?;
        let result = endpoint.read_final_source(&self.base.object, key).await;
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                Ok(interpret_class_literal_lookup(*result)
                    .map(Type::from)
                    .unwrap_or_else(Type::unknown))
            })
            .await)
    }
    async fn known_instance(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        program: Program<'db>,
        known: crate::types::KnownClass,
    ) -> RunResult<Type<'db>> {
        let key = self.known_key(endpoint, program, known).await?;
        Ok(*endpoint.read_final_source(&self.instances, key).await)
    }
    async fn enum_metadata(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        class: ClassLiteral<'db>,
    ) -> RunResult<Option<&'db crate::types::enums::EnumMetadata<'db>>> {
        Ok(endpoint
            .read_final_source(&self.enum_metadata, class.as_id())
            .await
            .as_ref())
    }
    async fn enum_class(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        class: ClassLiteral<'db>,
    ) -> RunResult<Option<crate::types::enums::EnumClassLiteral<'db>>> {
        Ok(*endpoint
            .read_final_source(&self.enum_classes, class.as_id())
            .await)
    }
    async fn decorators(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<&'db [Type<'db>]> {
        Ok(endpoint
            .read_final_source(&self.decorators, class.as_id())
            .await)
    }
    async fn code_generator(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<crate::types::class::CodeGeneratorKind<'db>>> {
        Ok(*endpoint
            .read_final_source(&self.generators, class.as_id())
            .await)
    }
    async fn implicit_names(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        scope: ScopeId<'db>,
    ) -> RunResult<&'db [Name]> {
        Ok(endpoint
            .read_final_source(&self.implicit_names, scope.as_id())
            .await)
    }
}
