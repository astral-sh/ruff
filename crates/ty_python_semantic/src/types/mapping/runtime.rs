//! Canonical materialization roots with children sharing the original transformation visitor.

use std::cell::Cell;

use salsa::attempt_probe::Incomplete;
use salsa::execution_probe::{
    CallableRoute, CallableRouteProvider, ExecutionWork, InternedValues, NativeValueOperation,
    NativeValueQuote, QueryKeys, RetainedInput, RunError, RunResult, TaskEndpoint,
};

use super::effects::{
    MappingDispatchFacts, MappingTransformationScope, MappingWork, MaterializationEffects,
    NativeMappingLeaf, SharedMappingEffects, SharedMappingStartEffects, materialization_with,
};
pub(in crate::types) use super::materialization::{
    MaterializationConfiguration, MaterializationKeyProfile,
};
use super::{LegacyTypeMappingContinuation, MappingStart, TypeVarMappingContinuation};
use crate::types::constraints::control::GrowthPlan;
use crate::types::cyclic::{
    TypeIdentity, TypeTransformationControl, TypeTransformationGrowth, TypeTransformationWork,
    TypeTransformer, TypeTransformerVisit,
};
use crate::types::relation::runtime_resources::{CallEnvironments, CallMappingVisitors};
use crate::types::tuple::TupleType;
use crate::types::{
    ApplyTypeMappingTag, ApplyTypeMappingVisitor, BoundTypeVarInstance, DivergentType,
    IntersectionType, KnownClass, MaterializationKind, NominalInstanceType, Type, TypeContext,
    TypeFormType, TypeMapping, UnionType,
};
use crate::{Db, Program};

mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum UnavailableMaterialization {
    Mode,
    Leaf(super::effects::MappingOperation),
    TypeVar,
    LegacyContinuation,
    Identity,
    Recovery,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MaterializationStage {
    Mapping(MappingWork),
    Begin,
    ArgumentRead,
    Child,
    Intern,
    Finish,
    Transformation(TypeTransformationWork),
    Initial,
    Recovery,
}

#[derive(Default)]
pub(in crate::types) struct MaterializationObservations {
    pub(in crate::types) roots: Cell<usize>,
    stage: Cell<Option<MaterializationStage>>,
    root_visitor: Cell<Option<*const ()>>,
    last_child_visitor: Cell<Option<*const ()>>,
    deepest_active: Cell<usize>,
    completed_children: Cell<usize>,

    pub(in crate::types) children: Cell<usize>,
    pub(in crate::types) wrapper_interns: Cell<usize>,
    pub(in crate::types) unavailable: Cell<Option<UnavailableMaterialization>>,
}

pub(in crate::types) struct MaterializationQueries<'run, 'db: 'run, C: MaterializationConfiguration>
{
    pub(in crate::types) route: CallableRoute<'run, 'db, C>,
    pub(in crate::types) keys: &'run QueryKeys<'db, C, MaterializationKeyProfile>,
}

impl<'run, 'db: 'run, C: MaterializationConfiguration> MaterializationQueries<'run, 'db, C> {
    pub(in crate::types) async fn materialization(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        ty: Type<'db>,
        program: Program<'db>,
        kind: MaterializationKind,
    ) -> RunResult<Type<'db>> {
        materialization_with(
            db,
            ty,
            kind,
            &RootEffects {
                endpoint,
                queries: self,
                program,
            },
            MappingDispatchFacts,
        )
        .await
    }
}

struct RootEffects<'call, 'run, 'db: 'run, C: MaterializationConfiguration> {
    endpoint: &'call TaskEndpoint<'run, 'db>,
    queries: &'call MaterializationQueries<'run, 'db, C>,
    program: Program<'db>,
}

impl<'db, C: MaterializationConfiguration> MaterializationEffects<'db>
    for RootEffects<'_, '_, 'db, C>
{
    type Failure = RunError;
    async fn nominal_is_generic(
        &self,
        db: &'db dyn Db,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<bool> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                Ok(instance.is_definition_generic(db))
            })
            .await)
    }
    async fn cached_materialization(
        &self,
        _db: &'db dyn Db,
        ty: Type<'db>,
        kind: MaterializationKind,
    ) -> RunResult<Type<'db>> {
        let id = self
            .endpoint
            .intern_query_key(self.queries.keys, (ty, self.program, kind))
            .await;
        Ok(self
            .endpoint
            .child_call(|| async { Ok(*self.endpoint.fetch_ref(&self.queries.route, id)?.await?) })
            .await)
    }
}

pub(in crate::types) struct MaterializationProvider<'run, 'db> {
    pub(in crate::types) environments: &'run CallEnvironments<'db>,
    pub(in crate::types) visitors: &'run CallMappingVisitors<'run, 'db>,
    pub(in crate::types) forms: &'run InternedValues<'db, TypeFormType<'static>, ()>,
    pub(in crate::types) observations: &'run MaterializationObservations,
}

impl<'run, 'db: 'run, C: MaterializationConfiguration> CallableRouteProvider<'run, 'db, C>
    for MaterializationProvider<'run, 'db>
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        let work = match operation {
            // Type's derived Clone copies its enum and handles, including the borrowed Todo
            // label. Program is a generated handle; MaterializationKind is a fieldless enum.
            NativeValueOperation::InputConversion(RetainedInput::Interned(_)) => 4,
            NativeValueOperation::InputConversion(RetainedInput::SalsaStruct(_)) => {
                return Err(RunError::Contract(
                    "materialization input requires a retained argument tuple",
                ));
            }
            NativeValueOperation::Comparison { left, right } => {
                endpoint
                    .local_call(|| {
                        endpoint.admit_work(2)?;
                        endpoint.check_completion()?;
                        left.inline_payload_bytes()
                            .checked_add(right.inline_payload_bytes())
                            .and_then(|bytes| bytes.checked_add(2))
                            .ok_or(RunError::Contract(
                                "materialization comparison work overflow",
                            ))
                    })
                    .await
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
        (ty, program, kind): C::Input<'db>,
    ) -> RunResult<Type<'db>>
    where
        'run: 'call,
    {
        let env = self.environments.allocate(&endpoint, program).await;
        let visitor = self.visitors.allocate(&endpoint, env).await;
        endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                self.observations
                    .roots
                    .set(self.observations.roots.get() + 1);
                self.observations
                    .root_visitor
                    .set(Some(std::ptr::from_ref(visitor).cast()));
                Ok(())
            })
            .await;
        let effects = MaterializationMapping {
            endpoint: &endpoint,
            visitor,
            forms: self.forms,
            observations: self.observations,
        };
        ty.apply_type_mapping_with(
            db,
            &TypeMapping::Materialize(kind),
            TypeContext::default(),
            visitor,
            &effects,
        )
        .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        id: salsa::Id,
        (_, _, kind): C::Input<'db>,
    ) -> RunResult<Type<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                self.observations
                    .stage
                    .set(Some(MaterializationStage::Initial));
                endpoint.admit_work(1)?;
                Ok(Type::Divergent(DivergentType::new(id).materialized(kind)))
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call Type<'db>,
        _value: Type<'db>,
        _input: C::Input<'db>,
    ) -> RunResult<Type<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                self.observations
                    .stage
                    .set(Some(MaterializationStage::Recovery));
                endpoint.admit_work(1)?;
                self.observations
                    .unavailable
                    .set(Some(UnavailableMaterialization::Recovery));
                Err(RunError::Refused(Incomplete::Interrupted))
            })
            .await)
    }
}

struct MaterializationMapping<'call, 'run, 'db> {
    endpoint: &'call TaskEndpoint<'run, 'db>,
    visitor: &'run ApplyTypeMappingVisitor<'run, 'db>,
    forms: &'run InternedValues<'db, TypeFormType<'static>, ()>,
    observations: &'run MaterializationObservations,
}

impl MaterializationMapping<'_, '_, '_> {
    async fn unavailable<T>(&self, operation: UnavailableMaterialization) -> RunResult<T> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.observations.unavailable.set(Some(operation));
                Err(RunError::Refused(Incomplete::Interrupted))
            })
            .await)
    }
}

struct TransformationControl<'call, 'run, 'db> {
    endpoint: &'call TaskEndpoint<'run, 'db>,
    observations: &'call MaterializationObservations,
}

impl TypeTransformationControl for TransformationControl<'_, '_, '_> {
    type Error = RunError;
    fn checkpoint(&self, work: TypeTransformationWork) -> RunResult<()> {
        self.observations
            .stage
            .set(Some(MaterializationStage::Transformation(work)));
        if let TypeTransformationWork::ActiveStorage { len, .. } = work {
            self.observations
                .deepest_active
                .set(self.observations.deepest_active.get().max(len + 1));
        }

        let quote = work
            .quote()
            .ok_or(RunError::Contract("transformation work quote overflow"))?;
        self.endpoint.admit_work(quote.work_units)?;
        if quote.requested_payload_bytes != 0 {
            self.endpoint.admit(ExecutionWork::Resource {
                requested_bytes: quote.requested_payload_bytes,
            })?;
        }
        Ok(())
    }
    fn prepare_growth(&self, request: TypeTransformationGrowth) -> RunResult<Option<GrowthPlan>> {
        let plan = request
            .checked_plan()
            .ok_or(RunError::Contract("transformation work quote overflow"))?;
        self.checkpoint(request.work(plan))?;
        Ok(Some(plan))
    }
    fn identity<'db>(&self, db: &'db dyn Db, ty: Type<'db>) -> RunResult<TypeIdentity<'db>> {
        self.endpoint.admit_work(1)?;
        match ty {
            Type::TypeForm(_) => Ok(ty.to_type_identity(db)),
            _ => {
                self.observations
                    .unavailable
                    .set(Some(UnavailableMaterialization::Identity));
                Err(RunError::Refused(Incomplete::Interrupted))
            }
        }
    }
}

impl<'db> SharedMappingStartEffects<'db> for MaterializationMapping<'_, '_, 'db> {
    type Failure = RunError;
    async fn checkpoint(&self, work: MappingWork) -> RunResult<()> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.observations
                    .stage
                    .set(Some(MaterializationStage::Mapping(work)));
                let units = match work {
                    MappingWork::ArgumentCapacity { width }
                    | MappingWork::SpecializationPayload { width } => {
                        width.checked_mul(8).and_then(|n| n.checked_add(1))
                    }
                    MappingWork::ArgumentPrefixCopy { len } => len.checked_add(1),
                    MappingWork::GenericAliasIntern
                    | MappingWork::ExplicitAnyIntern
                    | MappingWork::WrapperIntern => Some(8),
                    _ => Some(1),
                }
                .ok_or(RunError::Contract("mapping work quote overflow"))?;
                self.endpoint.admit_work(units)
            })
            .await)
    }
    async fn admit_mode(&self, mapping: &TypeMapping<'_, 'db>) -> RunResult<()> {
        if matches!(mapping, TypeMapping::Materialize(_)) {
            Ok(self
                .endpoint
                .local_call(|| self.endpoint.admit_work(1))
                .await)
        } else {
            self.unavailable(UnavailableMaterialization::Mode).await
        }
    }
    async fn expand_paramspecs(
        &self,
        _db: &'db dyn Db,
        _ty: Type<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(UnavailableMaterialization::Leaf(
            super::effects::MappingOperation::FunctionParamSpecPrelude,
        ))
        .await
    }
    async fn nominal_known_class(
        &self,
        db: &'db dyn Db,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<Option<KnownClass>> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                Ok(instance.known_class(db))
            })
            .await)
    }
    async fn start_typevar(
        &self,
        _db: &'db dyn Db,
        _variable: BoundTypeVarInstance<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<MappingStart<Type<'db>, TypeVarMappingContinuation<'db>>> {
        self.unavailable(UnavailableMaterialization::TypeVar).await
    }
    async fn begin_transformation<'v>(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &'v ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<TypeTransformerVisit<'db, MappingTransformationScope<'v, 'db>>> {
        let unsupported = match ty {
            Type::FunctionLiteral(_) => Some(super::effects::MappingOperation::Function),
            Type::Callable(_) => Some(super::effects::MappingOperation::Callable),
            _ => None,
        };
        if let Some(operation) = unsupported {
            return self.unavailable(UnavailableMaterialization::Leaf(operation)).await;
        }
        Ok(self
            .endpoint
            .local_call(|| {
                self.observations
                    .stage
                    .set(Some(MaterializationStage::Begin));
                self.endpoint.admit_work(1)?;
                if !std::ptr::eq(visitor, self.visitor) {
                    return Err(RunError::Contract("materialization visitor changed"));
                }
                if visitor.transformer_cell(mapping).get().is_none() {
                    self.endpoint.admit(ExecutionWork::Resource {
                        requested_bytes: size_of::<TypeTransformer<'db, ApplyTypeMappingTag>>(),
                    })?;
                }
                visitor.transformer(mapping).begin_visit_with(
                    db,
                    ty,
                    &TransformationControl {
                        endpoint: self.endpoint,
                        observations: self.observations,
                    },
                )
            })
            .await)
    }
    async fn legacy_leaf(
        &self,
        _db: &'db dyn Db,
        _ty: Type<'db>,
        leaf: NativeMappingLeaf<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(UnavailableMaterialization::Leaf(leaf.operation()))
            .await
    }
}

impl<'db> SharedMappingEffects<'db> for MaterializationMapping<'_, '_, 'db> {
    async fn map_union(
        &self,
        _db: &'db dyn Db,
        _union: UnionType<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(UnavailableMaterialization::Leaf(
            super::effects::MappingOperation::Union,
        ))
        .await
    }
    async fn map_intersection(
        &self,
        _db: &'db dyn Db,
        _intersection: IntersectionType<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(UnavailableMaterialization::Leaf(
            super::effects::MappingOperation::Intersection,
        ))
        .await
    }
    async fn map_tuple(
        &self,
        _db: &'db dyn Db,
        _tuple: TupleType<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(UnavailableMaterialization::Leaf(
            super::effects::MappingOperation::Tuple,
        ))
        .await
    }
    async fn typeform_argument(
        &self,
        db: &'db dyn Db,
        typeform: TypeFormType<'db>,
    ) -> RunResult<Type<'db>> {
        Ok(self
            .endpoint
            .local_call(|| {
                self.observations
                    .stage
                    .set(Some(MaterializationStage::ArgumentRead));
                self.endpoint.admit_work(1)?;
                Ok(*typeform
                    .read_fields(salsa::FieldReads::new(db))
                    .type_argument())
            })
            .await)
    }
    async fn intern_typeform(&self, _db: &'db dyn Db, argument: Type<'db>) -> RunResult<Type<'db>> {
        self.observations
            .stage
            .set(Some(MaterializationStage::Intern));
        let form = self.endpoint.intern_value(self.forms, (argument,)).await;
        self.observations
            .wrapper_interns
            .set(self.observations.wrapper_interns.get() + 1);
        Ok(Type::TypeForm(form))
    }
    async fn finish_transformation(
        &self,
        mut scope: MappingTransformationScope<'_, 'db>,
        result: Type<'db>,
    ) -> RunResult<Type<'db>> {
        // The future owns the active scope while a rejected callback drains its children.
        let prepared = self
            .endpoint
            .local_call(|| {
                self.observations
                    .stage
                    .set(Some(MaterializationStage::Finish));
                self.endpoint.admit_work(1)?;
                scope.prepare_finish_with(
                    result,
                    &TransformationControl {
                        endpoint: self.endpoint,
                        observations: self.observations,
                    },
                )
            })
            .await;
        match prepared.try_commit() {
            Ok(result) => Ok(result),
            Err(rejected) => {
                let result = self
                    .endpoint
                    .local_call(|| {
                        let _held = &rejected;
                        Err::<Type<'db>, _>(RunError::Contract(
                            "transformation finish preparation became stale",
                        ))
                    })
                    .await;
                drop(rejected);
                Ok(result)
            }
        }
    }
    async fn map_type(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Type<'db>> {
        let kind = self
            .endpoint
            .local_call(|| {
                self.observations
                    .stage
                    .set(Some(MaterializationStage::Child));
                self.endpoint.admit_work(1)?;
                if !std::ptr::eq(visitor, self.visitor) {
                    return Err(RunError::Contract("materialization child changed visitor"));
                }
                let TypeMapping::Materialize(kind) = mapping else {
                    self.observations
                        .unavailable
                        .set(Some(UnavailableMaterialization::Mode));
                    return Err(RunError::Refused(Incomplete::Interrupted));
                };
                self.observations
                    .children
                    .set(self.observations.children.get() + 1);
                self.observations
                    .last_child_visitor
                    .set(Some(std::ptr::from_ref(visitor).cast()));
                Ok(*kind)
            })
            .await;
        let child_endpoint = self.endpoint.clone();
        let visitor = self.visitor;
        let forms = self.forms;
        let observations = self.observations;
        let result = self
            .endpoint
            .child_call(|| async {
                self.endpoint
                    .demand(move || async move {
                        let effects = MaterializationMapping {
                            endpoint: &child_endpoint,
                            visitor,
                            forms,
                            observations,
                        };
                        ty.apply_type_mapping_with(
                            db,
                            &TypeMapping::Materialize(kind),
                            tcx,
                            visitor,
                            &effects,
                        )
                        .await
                    })?
                    .await
            })
            .await;
        self.observations
            .completed_children
            .set(self.observations.completed_children.get() + 1);
        Ok(result)
    }
    async fn resume_legacy(
        &self,
        _db: &'db dyn Db,
        _continuation: LegacyTypeMappingContinuation<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(UnavailableMaterialization::LegacyContinuation)
            .await
    }
}
