//! Mapping dependencies and work reservations within the current evaluation session.

use crate::types::mapping::MappingStart;
use crate::types::mapping::effects::{
    NativeMappingLeaf, SharedMappingEffects, SharedMappingStartEffects,
};
use crate::types::mapping::{LegacyTypeMappingContinuation, TypeVarMappingContinuation};
use crate::types::tuple::TupleType;
use crate::types::{IntersectionType, NominalInstanceType, TypeFormType, UnionType};

use crate::types::cyclic::TypeTransformerVisit;
use std::cell::Cell;
use std::future::{Future, ready};
use std::task::Poll;

use super::mro::{MroNodeId, PreparedMroWork, StaticMroRequest};
use super::{Boundary, Effect, Key, Output, Router, Task};
use crate::types::generics::{ApplySpecialization, Specialization};
use crate::types::mapping::effects::{
    MappingEffects, MappingFacts, MappingOperation, MappingStartEffects,
    MappingTransformationScope, MappingWork, sealed,
};
use crate::types::{
    ApplyTypeMappingVisitor, BoundTypeVarInstance, ClassLiteral, KnownClass, PromotionKind,
    PromotionMode, SelfBinding, Type, TypeContext, TypeMapping, TypeVarVariance,
};
use crate::{Db, ProgramEnvironment};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct Domain<'db> {
    evaluation: usize,
    sequence: usize,
    owner: MappingRootOwner<'db>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum MappingRootOwner<'db> {
    Consumer {
        evaluation: usize,
    },
    StaticMro {
        request: StaticMroRequest<'db>,
        node: MroNodeId,
    },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum RequestedMapping<'db> {
    Specialization {
        specialization: Specialization<'db>,
        owner: bool,
    },
    RawSpecialization {
        specialization: Specialization<'db>,
    },
    RegularPromotion(PromotionMode),
}

impl RequestedMapping<'_> {
    fn permits_child(self, child: Self) -> bool {
        match (self, child) {
            (Self::Specialization { .. }, Self::Specialization { .. }) => self == child,
            (Self::RawSpecialization { .. }, Self::RawSpecialization { .. }) => self == child,
            (Self::RegularPromotion(_), Self::RegularPromotion(_)) => true,
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum PromotionFactKey<'db> {
    Variance(BoundTypeVarInstance<'db>),
    EnumSingleton(ClassLiteral<'db>),
    ScalarFallback(KnownClass),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MappingFailure<'db> {
    Boundary(Boundary),
    MissingPromotionFact(PromotionFactKey<'db>),
}

impl From<Boundary> for MappingFailure<'_> {
    fn from(boundary: Boundary) -> Self {
        Self::Boundary(boundary)
    }
}

pub(super) type MappingAnswer<'db> = Result<Type<'db>, MappingFailure<'db>>;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct MappingRequest<'db> {
    domain: Domain<'db>,
    operation: RequestedMapping<'db>,
    ty: Type<'db>,
    tcx: TypeContext<'db>,
    root: bool,
}

impl<'db> MappingRequest<'db> {
    fn child(
        self,
        operation: RequestedMapping<'db>,
        ty: Type<'db>,
        tcx: TypeContext<'db>,
    ) -> Result<Self, MappingFailure<'db>> {
        if !self.operation.permits_child(operation) {
            return Err(Boundary::MappingDomain.into());
        }
        Ok(Self {
            domain: self.domain,
            operation,
            ty,
            tcx,
            root: false,
        })
    }

    fn accepts_child(self, child: Self) -> bool {
        self.domain == child.domain && !child.root && self.operation.permits_child(child.operation)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum SemanticOwner<'db> {
    Mapping(MappingRequest<'db>),
    Consumer {
        evaluation: usize,
    },
    StaticMro {
        request: StaticMroRequest<'db>,
        node: MroNodeId,
    },
}

impl<'db> SemanticOwner<'db> {
    pub(super) fn key(self) -> Key<'db> {
        match self {
            Self::Mapping(request) => Key::Mapping(request),
            Self::Consumer { .. } => Key::Consumer,
            Self::StaticMro { request, .. } => Key::StaticMro(request),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct SemanticWork<'db> {
    pub(super) owner: SemanticOwner<'db>,
    pub(super) sequence: usize,
    pub(super) units: usize,
}

impl<'db, 'c> Router<'db, 'c> {
    pub(super) fn mapping_root(
        &self,
        ty: Type<'db>,
        specialization: Specialization<'db>,
        owner: bool,
    ) -> Result<MappingRequest<'db>, MappingFailure<'db>> {
        self.mapping_root_for(
            ty,
            RequestedMapping::Specialization {
                specialization,
                owner,
            },
        )
    }

    pub(super) fn promotion_root(
        &self,
        ty: Type<'db>,
    ) -> Result<MappingRequest<'db>, MappingFailure<'db>> {
        self.mapping_root_for(ty, RequestedMapping::RegularPromotion(PromotionMode::On))
    }

    #[cfg(test)]
    pub(super) fn promotion_root_with_mode(
        &self,
        ty: Type<'db>,
        mode: PromotionMode,
    ) -> Result<MappingRequest<'db>, MappingFailure<'db>> {
        self.mapping_root_for(ty, RequestedMapping::RegularPromotion(mode))
    }

    fn mapping_root_for(
        &self,
        ty: Type<'db>,
        operation: RequestedMapping<'db>,
    ) -> Result<MappingRequest<'db>, MappingFailure<'db>> {
        let evaluation = self.evaluation_domain.0.ok_or(Boundary::MappingDomain)?;
        self.mapping_root_with_owner(
            ty,
            operation,
            TypeContext::default(),
            MappingRootOwner::Consumer { evaluation },
        )
    }

    fn mapping_root_with_owner(
        &self,
        ty: Type<'db>,
        operation: RequestedMapping<'db>,
        tcx: TypeContext<'db>,
        owner: MappingRootOwner<'db>,
    ) -> Result<MappingRequest<'db>, MappingFailure<'db>> {
        let evaluation = self.evaluation_domain.0.ok_or(Boundary::MappingDomain)?;
        let sequence = self.mapping_sequence.get();
        self.mapping_sequence
            .set(sequence.checked_add(1).ok_or(Boundary::CostOverflow)?);
        Ok(MappingRequest {
            domain: Domain {
                evaluation,
                sequence,
                owner,
            },
            operation,
            ty,
            tcx,
            root: true,
        })
    }

    fn validate_mapping_owner(&self, owner: MappingRootOwner<'db>) -> Result<(), Boundary> {
        if !self.driver_is_live() {
            return Err(Boundary::MappingDomain);
        }
        match owner {
            MappingRootOwner::Consumer { evaluation }
                if self.evaluation_domain.0 == Some(evaluation) && self.consumer_active.get() =>
            {
                Ok(())
            }
            MappingRootOwner::StaticMro { request, node } => {
                self.validate_static_mro_owner(request, node)
            }
            MappingRootOwner::Consumer { .. } => Err(Boundary::MappingDomain),
        }
    }

    fn raw_mapping_root(
        &self,
        db: &'db dyn Db,
        work: &PreparedMroWork<'_, 'db, 'c>,
        ty: Type<'db>,
        specialization: Specialization<'db>,
        tcx: TypeContext<'db>,
    ) -> Result<MappingRequest<'db>, MappingFailure<'db>> {
        if !std::ptr::eq(self, work.router()) {
            return Err(Boundary::MappingDomain.into());
        }
        let owner = work.mapping_root_owner()?;
        self.validate_mapping_owner(owner)?;
        let program = specialization.generic_context(db).program(db);
        let declarations = self
            .declarations
            .as_deref()
            .ok_or(Boundary::SourcePreparation)?;
        if declarations.program() != program {
            return Err(Boundary::ProgramDomain.into());
        }
        self.validate_declarations(db, &ProgramEnvironment::from_program(program))?;
        self.mapping_root_with_owner(
            ty,
            RequestedMapping::RawSpecialization { specialization },
            tcx,
            owner,
        )
    }

    async fn raw_mapping_demand(
        &self,
        work: &PreparedMroWork<'_, 'db, 'c>,
        request: MappingRequest<'db>,
    ) -> MappingAnswer<'db> {
        if !std::ptr::eq(self, work.router())
            || request.domain.owner != work.mapping_root_owner()?
            || !matches!(
                request.operation,
                RequestedMapping::RawSpecialization { .. }
            )
        {
            return Err(Boundary::MappingDomain.into());
        }
        self.mapping_demand(work.parent_key(), request).await
    }

    pub(super) fn consumer_mapping_demand(
        &self,
        request: MappingRequest<'db>,
    ) -> impl Future<Output = MappingAnswer<'db>> + '_ {
        self.mapping_demand(Key::Consumer, request)
    }

    fn mapping_demand(
        &self,
        parent: Key<'db>,
        child: MappingRequest<'db>,
    ) -> impl Future<Output = MappingAnswer<'db>> + '_ {
        let mut registered = false;
        std::future::poll_fn(move |_| {
            if let Err(error) = self.validate_mapping_owner(child.domain.owner) {
                return Poll::Ready(Err(error.into()));
            }
            let valid_parent = match parent {
                Key::Consumer => {
                    child.root && matches!(child.domain.owner, MappingRootOwner::Consumer { .. })
                }
                Key::Mapping(parent) => {
                    parent.accepts_child(child)
                        && self
                            .mappings
                            .borrow()
                            .get(&parent)
                            .is_some_and(|entry| entry.answer.is_none())
                }
                Key::StaticMro(parent) => {
                    child.root
                        && matches!(
                            child.domain.owner,
                            MappingRootOwner::StaticMro { request, .. } if request == parent
                        )
                }
                _ => false,
            };
            if self.evaluation_domain.0 != Some(child.domain.evaluation) || !valid_parent {
                return Poll::Ready(Err(Boundary::MappingDomain.into()));
            }
            if !registered {
                self.effects
                    .borrow_mut()
                    .push(Effect::DemandMapping { parent, child });
                registered = true;
                return Poll::Pending;
            }
            self.mappings
                .borrow()
                .get(&child)
                .and_then(|entry| entry.answer)
                .map_or(Poll::Pending, Poll::Ready)
        })
    }

    pub(super) async fn consumer_checkpoint(&self, units: usize) -> Result<(), Boundary> {
        let evaluation = self.evaluation_domain.0.ok_or(Boundary::MappingDomain)?;
        let sequence = self.consumer_work_sequence.get();
        self.consumer_work_sequence
            .set(sequence.checked_add(1).ok_or(Boundary::CostOverflow)?);
        self.semantic_checkpoint(SemanticWork {
            owner: SemanticOwner::Consumer { evaluation },
            sequence,
            units,
        })
        .await
    }

    pub(super) async fn semantic_checkpoint(
        &self,
        request: SemanticWork<'db>,
    ) -> Result<(), Boundary> {
        let mut registered = false;
        std::future::poll_fn(move |_| {
            let active = match request.owner {
                SemanticOwner::Mapping(owner) => {
                    self.validate_mapping_owner(owner.domain.owner).is_ok()
                        && self
                            .mappings
                            .borrow()
                            .get(&owner)
                            .is_some_and(|entry| entry.answer.is_none())
                }
                SemanticOwner::Consumer { evaluation } => self
                    .validate_mapping_owner(MappingRootOwner::Consumer { evaluation })
                    .is_ok(),
                SemanticOwner::StaticMro { request, node } => {
                    self.driver_is_live() && self.validate_static_mro_owner(request, node).is_ok()
                }
            };
            if !active {
                return Poll::Ready(Err(match request.owner {
                    SemanticOwner::StaticMro { .. } => Boundary::MroDomain,
                    _ => Boundary::MappingDomain,
                }));
            }
            if !registered {
                self.effects
                    .borrow_mut()
                    .push(Effect::DemandSemanticWork(request));
                registered = true;
                return Poll::Pending;
            }
            self.semantic_work
                .borrow()
                .get(&request)
                .and_then(|entry| entry.answer)
                .map_or(Poll::Pending, |()| Poll::Ready(Ok(())))
        })
        .await
    }
}

struct QueuedMappingEffects<'eval, 'db, 'c> {
    router: &'eval Router<'db, 'c>,
    parent: MappingRequest<'db>,
    sequence: Cell<usize>,
}

fn prepared_variance<'db>(
    router: &Router<'db, '_>,
    variable: BoundTypeVarInstance<'db>,
) -> Result<TypeVarVariance, MappingFailure<'db>> {
    router
        .declarations
        .as_deref()
        .ok_or(MappingFailure::MissingPromotionFact(
            PromotionFactKey::Variance(variable),
        ))?
        .promotion_variance(variable)
        .map_err(MappingFailure::MissingPromotionFact)
}

fn prepared_scalar_fallback<'db>(
    router: &Router<'db, '_>,
    class: KnownClass,
) -> MappingAnswer<'db> {
    router
        .declarations
        .as_deref()
        .ok_or(MappingFailure::MissingPromotionFact(
            PromotionFactKey::ScalarFallback(class),
        ))?
        .promotion_scalar_fallback(class)
        .map_err(MappingFailure::MissingPromotionFact)
}

fn mapping_work_units(work: MappingWork) -> Result<usize, Boundary> {
    Ok(match work {
        MappingWork::RootAdmission
        | MappingWork::TypeVarLookup
        | MappingWork::ChildRequest
        | MappingWork::NominalPromotionClass
        | MappingWork::VarianceLookup
        | MappingWork::ScalarFallbackLookup => 4,
        MappingWork::TypeDispatch => 3,
        MappingWork::ArgumentAdvance
        | MappingWork::ArgumentAppend
        | MappingWork::ResultPublication => 1,
        MappingWork::ArgumentCapacity { width } => {
            width.checked_add(1).ok_or(Boundary::CostOverflow)?
        }
        MappingWork::ArgumentPrefixCopy { len } => {
            len.checked_add(1).ok_or(Boundary::CostOverflow)?
        }
        // Rebuilding the interned payload visits its immediate arguments for copying,
        // conversion, hashing and equality. Nested types are fixed-size handles here.
        MappingWork::SpecializationPayload { width } => width
            .checked_mul(8)
            .and_then(|n| n.checked_add(8))
            .ok_or(Boundary::CostOverflow)?,
        MappingWork::GenericAliasIntern
        | MappingWork::ExplicitAnyIntern
        | MappingWork::WrapperIntern => 8,
    })
}

impl sealed::Sealed for QueuedMappingEffects<'_, '_, '_> {}

impl<'db> MappingFacts<'db> for QueuedMappingEffects<'_, 'db, '_> {
    type Error = MappingFailure<'db>;

    fn variance(
        &self,
        _db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<TypeVarVariance, Self::Error> {
        prepared_variance(self.router, variable)
    }

    fn scalar_fallback(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        class: KnownClass,
    ) -> Result<Type<'db>, Self::Error> {
        prepared_scalar_fallback(self.router, class)
    }
}

impl<'db> MappingStartEffects<'db> for QueuedMappingEffects<'_, 'db, '_> {
    fn legacy<T>(
        &self,
        operation: MappingOperation,
        _thunk: impl FnOnce() -> T,
    ) -> Result<T, Self::Error> {
        Err(Boundary::MappingOperation(operation).into())
    }
}

impl<'db> MappingEffects<'db> for QueuedMappingEffects<'_, 'db, '_> {
    fn should_bind_self(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _binding: &SelfBinding<'db>,
        _variable: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Err(Boundary::MappingOperation(
            MappingOperation::MappingMode,
        )
        .into()))
    }
}

pub(super) struct PreparedMroMappingEffects<'eval, 'db, 'c> {
    db: &'db dyn Db,
    work: &'eval PreparedMroWork<'eval, 'db, 'c>,
}

impl<'eval, 'db, 'c> PreparedMroMappingEffects<'eval, 'db, 'c> {
    pub(super) fn new(db: &'db dyn Db, work: &'eval PreparedMroWork<'eval, 'db, 'c>) -> Self {
        Self { db, work }
    }
}

impl sealed::Sealed for PreparedMroMappingEffects<'_, '_, '_> {}

impl<'db> MappingFacts<'db> for PreparedMroMappingEffects<'_, 'db, '_> {
    type Error = MappingFailure<'db>;

    fn variance(
        &self,
        _db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<TypeVarVariance, Self::Error> {
        prepared_variance(self.work.router(), variable)
    }

    fn scalar_fallback(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        class: KnownClass,
    ) -> MappingAnswer<'db> {
        prepared_scalar_fallback(self.work.router(), class)
    }
}

impl<'db> MappingStartEffects<'db> for PreparedMroMappingEffects<'_, 'db, '_> {
    fn legacy<T>(
        &self,
        operation: MappingOperation,
        _thunk: impl FnOnce() -> T,
    ) -> Result<T, Self::Error> {
        Err(Boundary::MappingOperation(operation).into())
    }
}

impl<'db> MappingEffects<'db> for PreparedMroMappingEffects<'_, 'db, '_> {
    fn should_bind_self(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _binding: &SelfBinding<'db>,
        _variable: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Err(Boundary::MappingOperation(
            MappingOperation::MappingMode,
        )
        .into()))
    }
}

impl<'db> SharedMappingStartEffects<'db> for QueuedMappingEffects<'_, 'db, '_> {
    type Failure = MappingFailure<'db>;
    async fn checkpoint(&self, work: MappingWork) -> Result<(), Self::Failure> {
        let units = mapping_work_units(work)?;
        let sequence = self.sequence.get();
        self.sequence
            .set(sequence.checked_add(1).ok_or(Boundary::CostOverflow)?);
        self.router
            .semantic_checkpoint(SemanticWork {
                owner: SemanticOwner::Mapping(self.parent),
                sequence,
                units,
            })
            .await
            .map_err(Into::into)
    }
    async fn begin_transformation<'a>(
        &self,
        _db: &'db dyn Db,
        _ty: Type<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _visitor: &'a ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<TypeTransformerVisit<'db, MappingTransformationScope<'a, 'db>>, Self::Failure> {
        let operation = match _ty {
            Type::FunctionLiteral(_) => MappingOperation::Function,
            Type::Callable(_) => MappingOperation::Callable,
            _ => MappingOperation::TypeForm,
        };
        Err(Boundary::MappingOperation(operation).into())
    }
    async fn admit_mode(&self, _mapping: &TypeMapping<'_, 'db>) -> Result<(), Self::Failure> {
        Err(Boundary::MappingOperation(MappingOperation::MappingMode).into())
    }
    async fn expand_paramspecs(
        &self,
        _db: &'db dyn Db,
        _ty: Type<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Option<Type<'db>>, Self::Failure> {
        Err(Boundary::MappingOperation(MappingOperation::FunctionParamSpecPrelude).into())
    }
    async fn nominal_known_class(
        &self,
        db: &'db dyn Db,
        instance: NominalInstanceType<'db>,
    ) -> Result<Option<KnownClass>, Self::Failure> {
        Ok(instance.known_class(db))
    }
    async fn start_typevar(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<MappingStart<Type<'db>, TypeVarMappingContinuation<'db>>, Self::Failure> {
        variable
            .mapping_start_with(db, mapping, visitor, self)
            .await
    }
    async fn legacy_leaf(
        &self,
        _db: &'db dyn Db,
        _ty: Type<'db>,
        leaf: NativeMappingLeaf<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Self::Failure> {
        Err(Boundary::MappingOperation(leaf.operation()).into())
    }
}
impl<'db> SharedMappingEffects<'db> for QueuedMappingEffects<'_, 'db, '_> {
    async fn map_union(
        &self,
        _db: &'db dyn Db,
        _union: UnionType<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Self::Failure> {
        Err(Boundary::MappingOperation(MappingOperation::Union).into())
    }
    async fn map_intersection(
        &self,
        _db: &'db dyn Db,
        _intersection: IntersectionType<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Self::Failure> {
        Err(Boundary::MappingOperation(MappingOperation::Intersection).into())
    }
    async fn map_tuple(
        &self,
        _db: &'db dyn Db,
        _tuple: TupleType<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Self::Failure> {
        Err(Boundary::MappingOperation(MappingOperation::Tuple).into())
    }
    async fn map_type(
        &self,
        _db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> MappingAnswer<'db> {
        let operation = match mapping {
            TypeMapping::ApplySpecialization(ApplySpecialization::Specialization {
                specialization,
                specialize_self_domain,
            }) => {
                if matches!(
                    self.parent.operation,
                    RequestedMapping::RawSpecialization { .. }
                ) && !*specialize_self_domain
                {
                    RequestedMapping::RawSpecialization {
                        specialization: *specialization,
                    }
                } else {
                    RequestedMapping::Specialization {
                        specialization: *specialization,
                        owner: *specialize_self_domain,
                    }
                }
            }
            TypeMapping::Promote(mode, PromotionKind::Regular) => {
                RequestedMapping::RegularPromotion(*mode)
            }
            _ => return Err(Boundary::MappingOperation(MappingOperation::MappingMode).into()),
        };
        let child = self.parent.child(operation, ty, tcx)?;
        self.router
            .mapping_demand(Key::Mapping(self.parent), child)
            .await
    }
    async fn finish_transformation(
        &self,
        _scope: MappingTransformationScope<'_, 'db>,
        _result: Type<'db>,
    ) -> Result<Type<'db>, Self::Failure> {
        Err(Boundary::MappingOperation(MappingOperation::TypeForm).into())
    }
    async fn typeform_argument(
        &self,
        db: &'db dyn Db,
        typeform: TypeFormType<'db>,
    ) -> Result<Type<'db>, Self::Failure> {
        Ok(typeform.type_argument(db))
    }
    async fn intern_typeform(
        &self,
        db: &'db dyn Db,
        argument: Type<'db>,
    ) -> Result<Type<'db>, Self::Failure> {
        Ok(TypeFormType::from_type_expression(db, argument))
    }
    fn resume_legacy(
        &self,
        db: &'db dyn Db,
        continuation: LegacyTypeMappingContinuation<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Failure>> {
        continuation.resume_mapping_with(db, mapping, tcx, visitor, self)
    }
}

impl<'db> SharedMappingStartEffects<'db> for PreparedMroMappingEffects<'_, 'db, '_> {
    type Failure = MappingFailure<'db>;
    async fn checkpoint(&self, work: MappingWork) -> Result<(), Self::Failure> {
        self.work
            .checkpoint(mapping_work_units(work)?)
            .await
            .map_err(Into::into)
    }
    async fn begin_transformation<'a>(
        &self,
        _db: &'db dyn Db,
        _ty: Type<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _visitor: &'a ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<TypeTransformerVisit<'db, MappingTransformationScope<'a, 'db>>, Self::Failure> {
        let operation = match _ty {
            Type::FunctionLiteral(_) => MappingOperation::Function,
            Type::Callable(_) => MappingOperation::Callable,
            _ => MappingOperation::TypeForm,
        };
        Err(Boundary::MappingOperation(operation).into())
    }
    async fn admit_mode(&self, _mapping: &TypeMapping<'_, 'db>) -> Result<(), Self::Failure> {
        Err(Boundary::MappingOperation(MappingOperation::MappingMode).into())
    }
    async fn expand_paramspecs(
        &self,
        _db: &'db dyn Db,
        _ty: Type<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Option<Type<'db>>, Self::Failure> {
        Err(Boundary::MappingOperation(MappingOperation::FunctionParamSpecPrelude).into())
    }
    async fn nominal_known_class(
        &self,
        db: &'db dyn Db,
        instance: NominalInstanceType<'db>,
    ) -> Result<Option<KnownClass>, Self::Failure> {
        Ok(instance.known_class(db))
    }
    async fn start_typevar(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<MappingStart<Type<'db>, TypeVarMappingContinuation<'db>>, Self::Failure> {
        variable
            .mapping_start_with(db, mapping, visitor, self)
            .await
    }
    async fn legacy_leaf(
        &self,
        _db: &'db dyn Db,
        _ty: Type<'db>,
        leaf: NativeMappingLeaf<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Self::Failure> {
        Err(Boundary::MappingOperation(leaf.operation()).into())
    }
}
impl<'db> SharedMappingEffects<'db> for PreparedMroMappingEffects<'_, 'db, '_> {
    async fn map_union(
        &self,
        _db: &'db dyn Db,
        _union: UnionType<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Self::Failure> {
        Err(Boundary::MappingOperation(MappingOperation::Union).into())
    }
    async fn map_intersection(
        &self,
        _db: &'db dyn Db,
        _intersection: IntersectionType<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Self::Failure> {
        Err(Boundary::MappingOperation(MappingOperation::Intersection).into())
    }
    async fn map_tuple(
        &self,
        _db: &'db dyn Db,
        _tuple: TupleType<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Self::Failure> {
        Err(Boundary::MappingOperation(MappingOperation::Tuple).into())
    }
    async fn map_type(
        &self,
        _db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> MappingAnswer<'db> {
        let TypeMapping::ApplySpecialization(ApplySpecialization::Specialization {
            specialization,
            specialize_self_domain: false,
        }) = mapping
        else {
            return Err(Boundary::MappingOperation(MappingOperation::MappingMode).into());
        };
        let router = self.work.router();
        let request = router.raw_mapping_root(self.db, self.work, ty, *specialization, tcx)?;
        router.raw_mapping_demand(self.work, request).await
    }
    async fn finish_transformation(
        &self,
        _scope: MappingTransformationScope<'_, 'db>,
        _result: Type<'db>,
    ) -> Result<Type<'db>, Self::Failure> {
        Err(Boundary::MappingOperation(MappingOperation::TypeForm).into())
    }
    async fn typeform_argument(
        &self,
        db: &'db dyn Db,
        typeform: TypeFormType<'db>,
    ) -> Result<Type<'db>, Self::Failure> {
        Ok(typeform.type_argument(db))
    }
    async fn intern_typeform(
        &self,
        db: &'db dyn Db,
        argument: Type<'db>,
    ) -> Result<Type<'db>, Self::Failure> {
        Ok(TypeFormType::from_type_expression(db, argument))
    }
    fn resume_legacy(
        &self,
        db: &'db dyn Db,
        continuation: LegacyTypeMappingContinuation<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Failure>> {
        continuation.resume_mapping_with(db, mapping, tcx, visitor, self)
    }
}

pub(super) fn task<'eval, 'db: 'eval, 'c: 'eval, R: 'eval>(
    db: &'db dyn Db,
    env: &'eval ProgramEnvironment<'db>,
    router: &'eval Router<'db, 'c>,
    request: MappingRequest<'db>,
) -> Task<'eval, 'db, 'c, R> {
    Box::pin(async move {
        let effects = QueuedMappingEffects {
            router,
            parent: request,
            sequence: Cell::new(0),
        };
        let result = async {
            let mapping = match request.operation {
                RequestedMapping::Specialization {
                    specialization,
                    owner,
                } => {
                    if request.root
                        && let Some(ty) = request
                            .ty
                            .specialization_start_with(db, specialization, owner, &effects)
                            .await?
                    {
                        return Ok(ty);
                    }
                    TypeMapping::for_specialization(db, specialization, owner)
                }
                RequestedMapping::RegularPromotion(mode) => {
                    if request.root {
                        effects.checkpoint(MappingWork::RootAdmission).await?;
                    }
                    TypeMapping::Promote(mode, PromotionKind::Regular)
                }
                RequestedMapping::RawSpecialization { specialization } => {
                    TypeMapping::ApplySpecialization(ApplySpecialization::specialization(
                        specialization,
                    ))
                }
            };
            let visitor = ApplyTypeMappingVisitor::new(env);
            request
                .ty
                .apply_type_mapping_with(db, &mapping, request.tcx, &visitor, &effects)
                .await
        }
        .await;
        Output::Mapping(result)
    })
}

#[cfg(test)]
pub(in crate::types::callable::scheduled_probe) mod tests {
    use std::task::{Context, Waker};

    use ruff_python_ast::name::Name;
    use salsa::prepared_source_probe as probe;

    use super::*;
    use crate::db::tests::{TestDb, setup_db};
    use crate::types::callable::scheduled_probe::{Entry, run_with};
    use crate::types::generics::GenericContext;
    use crate::types::tuple::TupleType;
    use crate::types::{LiteralValueType, MaterializationKind, UnionType};

    pub(in crate::types::callable::scheduled_probe) struct RawMroOwnerProbe<'db> {
        root: MappingRequest<'db>,
        descendant: MappingRequest<'db>,
        specialization: Specialization<'db>,
    }

    impl<'db> RawMroOwnerProbe<'db> {
        pub(in crate::types::callable::scheduled_probe) fn new(
            db: &'db dyn Db,
            work: &PreparedMroWork<'_, 'db, '_>,
            specialization: Specialization<'db>,
            other_specialization: Specialization<'db>,
        ) -> Result<Self, MappingFailure<'db>> {
            let router = work.router();
            let ty = Type::int_literal(1);
            let tcx = TypeContext::default();
            let owner = work.mapping_root_owner()?;
            assert!(matches!(owner, MappingRootOwner::StaticMro { .. }));
            let root = router.raw_mapping_root(db, work, ty, specialization, tcx)?;
            let descendant = root.child(root.operation, Type::int_literal(2), tcx)?;
            assert_eq!(root.domain.owner, owner);
            assert_eq!(Some(root.domain.evaluation), router.evaluation_domain.0);
            assert_eq!(descendant.domain, root.domain);
            assert_eq!(descendant.operation, root.operation);
            assert!(!descendant.root);
            assert!(root.accepts_child(descendant));
            without_mapping_mutation(router, || {
                for operation in [
                    RequestedMapping::RawSpecialization {
                        specialization: other_specialization,
                    },
                    RequestedMapping::Specialization {
                        specialization,
                        owner: false,
                    },
                    RequestedMapping::Specialization {
                        specialization,
                        owner: true,
                    },
                    RequestedMapping::RegularPromotion(PromotionMode::On),
                ] {
                    assert_eq!(
                        descendant.child(operation, ty, tcx),
                        Err(MappingFailure::Boundary(Boundary::MappingDomain))
                    );
                }
            });

            // These pending entries are controlled fixture storage, not computed mappings.
            // Their presence must not preserve authority after the originating task retires.
            let mut mappings = router.mappings.borrow_mut();
            assert!(mappings.insert(root, Entry::default()).is_none());
            assert!(mappings.insert(descendant, Entry::default()).is_none());
            Ok(Self {
                root,
                descendant,
                specialization,
            })
        }

        pub(in crate::types::callable::scheduled_probe) fn assert_wrong_owner(
            &self,
            work: &PreparedMroWork<'_, 'db, '_>,
        ) {
            let router = work.router();
            assert!(router.driver_is_live());
            assert_ne!(work.mapping_root_owner(), Ok(self.root.domain.owner));
            without_mapping_mutation(router, || {
                let mut cx = Context::from_waker(Waker::noop());
                let mut demand = std::pin::pin!(router.raw_mapping_demand(work, self.root));
                assert_eq!(
                    demand.as_mut().poll(&mut cx),
                    Poll::Ready(Err(MappingFailure::Boundary(Boundary::MappingDomain)))
                );
            });
        }

        pub(in crate::types::callable::scheduled_probe) fn assert_retired(
            &self,
            db: &'db dyn Db,
            work: &PreparedMroWork<'_, 'db, '_>,
        ) {
            let router = work.router();
            assert!(router.driver_is_live());
            assert_eq!(work.mapping_root_owner(), Err(Boundary::MroDomain));
            for request in [self.root, self.descendant] {
                assert!(router.mappings.borrow()[&request].answer.is_none());
            }
            without_mapping_mutation(router, || {
                assert_eq!(
                    router.raw_mapping_root(
                        db,
                        work,
                        self.root.ty,
                        self.specialization,
                        self.root.tcx,
                    ),
                    Err(MappingFailure::Boundary(Boundary::MroDomain))
                );
                let mut cx = Context::from_waker(Waker::noop());
                let mut root = std::pin::pin!(router.raw_mapping_demand(work, self.root));
                assert_eq!(
                    root.as_mut().poll(&mut cx),
                    Poll::Ready(Err(MappingFailure::Boundary(Boundary::MroDomain)))
                );
                let mut descendant =
                    std::pin::pin!(router.mapping_demand(Key::Mapping(self.root), self.descendant));
                assert_eq!(
                    descendant.as_mut().poll(&mut cx),
                    Poll::Ready(Err(MappingFailure::Boundary(Boundary::MroDomain)))
                );
                let mut checkpoint = std::pin::pin!(router.semantic_checkpoint(SemanticWork {
                    owner: SemanticOwner::Mapping(self.descendant),
                    sequence: 0,
                    units: 1,
                }));
                assert_eq!(
                    checkpoint.as_mut().poll(&mut cx),
                    Poll::Ready(Err(Boundary::MappingDomain))
                );
            });
        }
    }

    fn without_mapping_mutation(router: &Router<'_, '_>, assertion: impl FnOnce()) {
        let snapshot = || {
            (
                router.effects.borrow().len(),
                router.mapping_sequence.get(),
                router.consumer_work_sequence.get(),
                router
                    .mappings
                    .borrow()
                    .iter()
                    .map(|(key, entry)| (*key, (entry.answer, entry.dependents.clone())))
                    .collect::<rustc_hash::FxHashMap<_, _>>(),
                router
                    .semantic_work
                    .borrow()
                    .iter()
                    .map(|(key, entry)| (*key, (entry.answer, entry.dependents.clone())))
                    .collect::<rustc_hash::FxHashMap<_, _>>(),
            )
        };
        let before = snapshot();
        assertion();
        assert_eq!(snapshot(), before);
    }

    fn inputs<'db>(
        db: &'db TestDb,
        env: &ProgramEnvironment<'db>,
    ) -> (
        BoundTypeVarInstance<'db>,
        Specialization<'db>,
        Specialization<'db>,
    ) {
        let variable = BoundTypeVarInstance::synthetic(
            db,
            env,
            Name::new_static("T"),
            TypeVarVariance::Invariant,
        );
        let context = GenericContext::from_typevar_instances(db, env, [variable]);
        (
            variable,
            context.specialize(db, [Type::int_literal(1)].as_slice()),
            context.specialize(db, [Type::int_literal(2)].as_slice()),
        )
    }

    #[test]
    fn mapping_requests_preserve_domain_and_operation_family() {
        let db = setup_db();
        let env = db.program_environment();
        let (variable, specialization, other_specialization) = inputs(&db, &env);
        let ty = Type::TypeVar(variable);
        let tcx = TypeContext::default();
        let router = Router::default();
        let root = router.mapping_root(ty, specialization, true).unwrap();
        let next_root = router.mapping_root(ty, specialization, true).unwrap();
        let child = root.child(root.operation, ty, tcx).unwrap();
        assert_ne!(root, next_root);
        assert_ne!(root, child);
        assert_eq!(child, root.child(root.operation, ty, tcx).unwrap());
        assert!(root.accepts_child(child));
        assert!(!next_root.accepts_child(child));
        assert_ne!(
            child,
            root.child(root.operation, Type::int_literal(1), tcx)
                .unwrap()
        );
        assert_ne!(
            child,
            root.child(root.operation, ty, TypeContext::new(Some(ty)))
                .unwrap()
        );

        for operation in [
            RequestedMapping::Specialization {
                specialization,
                owner: false,
            },
            RequestedMapping::Specialization {
                specialization: other_specialization,
                owner: true,
            },
            RequestedMapping::RegularPromotion(PromotionMode::On),
        ] {
            assert_eq!(
                root.child(operation, ty, tcx),
                Err(MappingFailure::Boundary(Boundary::MappingDomain))
            );
        }

        let promotion = router
            .mapping_root_for(ty, RequestedMapping::RegularPromotion(PromotionMode::On))
            .unwrap();
        let on = promotion.child(promotion.operation, ty, tcx).unwrap();
        let off = promotion
            .child(
                RequestedMapping::RegularPromotion(PromotionMode::Off),
                ty,
                tcx,
            )
            .unwrap();
        assert_ne!(on, off);
        assert_eq!(on.domain, off.domain);
        assert_eq!(off.child(on.operation, ty, tcx), Ok(on));
        assert_eq!(
            promotion.child(root.operation, ty, tcx),
            Err(MappingFailure::Boundary(Boundary::MappingDomain))
        );

        // Even a request bypassing the child constructor must satisfy the parent's identity.
        let mut cx = Context::from_waker(Waker::noop());
        for invalid in [
            root,
            next_root.child(root.operation, ty, tcx).unwrap(),
            MappingRequest {
                operation: promotion.operation,
                ..child
            },
        ] {
            let mut demand = std::pin::pin!(router.mapping_demand(Key::Mapping(root), invalid));
            assert_eq!(
                demand.as_mut().poll(&mut cx),
                Poll::Ready(Err(MappingFailure::Boundary(Boundary::MappingDomain)))
            );
        }
        assert!(router.effects.borrow().is_empty());
    }

    #[test]
    fn mapping_provider_rejects_family_changes_and_retains_polarity() {
        let db = setup_db();
        let env = db.program_environment();
        let (variable, specialization, _) = inputs(&db, &env);
        let ty = Type::TypeVar(variable);
        let router = Router::default();
        let specialization_root = router.mapping_root(ty, specialization, true).unwrap();
        let promotion_root = router
            .mapping_root_for(ty, RequestedMapping::RegularPromotion(PromotionMode::On))
            .unwrap();
        let visitor = ApplyTypeMappingVisitor::new(&env);
        let mut cx = Context::from_waker(Waker::noop());
        for (parent, mapping) in [
            (
                specialization_root,
                TypeMapping::Promote(PromotionMode::On, PromotionKind::Regular),
            ),
            (
                promotion_root,
                TypeMapping::for_specialization(&db, specialization, true),
            ),
        ] {
            let effects = QueuedMappingEffects {
                router: &router,
                parent,
                sequence: Cell::new(0),
            };
            let mut mapped = std::pin::pin!(effects.map_type(
                &db,
                ty,
                &mapping,
                TypeContext::default(),
                &visitor
            ));
            assert_eq!(
                mapped.as_mut().poll(&mut cx),
                Poll::Ready(Err(MappingFailure::Boundary(Boundary::MappingDomain)))
            );
        }
        assert!(router.effects.borrow().is_empty());

        let result = run_with(&db, &env, &router, 1_000, false, false, |router| async {
            router
                .mappings
                .borrow_mut()
                .insert(promotion_root, Entry::default());
            let effects = QueuedMappingEffects {
                router,
                parent: promotion_root,
                sequence: Cell::new(0),
            };
            let mapping = TypeMapping::Promote(PromotionMode::Off, PromotionKind::Regular);
            let mut mapped = std::pin::pin!(effects.map_type(
                &db,
                ty,
                &mapping,
                TypeContext::default(),
                &visitor
            ));
            assert!(mapped.as_mut().poll(&mut cx).is_pending());
            let queued = router.effects.take();
            let [Effect::DemandMapping { parent, child }] = queued.as_slice() else {
                panic!("expected a single mapping dependency");
            };
            assert_eq!(*parent, Key::Mapping(promotion_root));
            assert_eq!(child.domain, promotion_root.domain);
            assert_eq!(
                child.operation,
                RequestedMapping::RegularPromotion(PromotionMode::Off)
            );
            router.mappings.borrow_mut().remove(&promotion_root);
        })
        .unwrap();
        assert_eq!(result.consumer, Some(()));
    }

    #[test]
    fn raw_specialization_preserves_replacements_before_materialization() {
        let db = setup_db();
        let env = db.program_environment();
        let (first, _, _) = inputs(&db, &env);
        let second = BoundTypeVarInstance::synthetic(
            &db,
            &env,
            Name::new_static("U"),
            TypeVarVariance::Invariant,
        );
        let context = GenericContext::from_typevar_instances(&db, &env, [first, second]);
        let input = KnownClass::List.to_specialized_instance(&db, &env, &[Type::TypeVar(first)]);
        for (replacement, materialization) in [
            (Type::TypeVar(second), None),
            (Type::any(), Some(MaterializationKind::Top)),
        ] {
            let specialization = context
                .specialize(&db, &[replacement, Type::int_literal(1)])
                .with_materialization_kind(&db, materialization);
            let router = Router::default();
            let owner = MappingRootOwner::Consumer {
                evaluation: router.evaluation_domain.0.unwrap(),
            };
            let request = router
                .mapping_root_with_owner(
                    input,
                    RequestedMapping::RawSpecialization { specialization },
                    TypeContext::default(),
                    owner,
                )
                .unwrap();
            let observed = probe::capture(&db, || {
                run_with(&db, &env, &router, 10_000, false, false, |router| {
                    router.consumer_mapping_demand(request)
                })
                .unwrap()
            })
            .unwrap();
            assert!(observed.reads.is_empty());
            let expected = input.apply_type_mapping_impl(
                &db,
                &TypeMapping::ApplySpecialization(ApplySpecialization::specialization(
                    specialization,
                )),
                TypeContext::default(),
                &ApplyTypeMappingVisitor::new(&env),
            );
            assert_eq!(observed.value.consumer, Some(Ok(expected)));
            assert!(observed.value.mapping_polls.len() > 1);
            assert!(observed.value.mapping_polls.keys().all(|child| {
                child.domain.owner == owner && child.operation == request.operation
            }));
            let mut after_retirement = std::pin::pin!(router.consumer_mapping_demand(request));
            let mut cx = Context::from_waker(Waker::noop());
            assert_eq!(
                after_retirement.as_mut().poll(&mut cx),
                Poll::Ready(Err(MappingFailure::Boundary(Boundary::MappingDomain))),
            );
            let work = PreparedMroWork::consumer(&router);
            assert_eq!(
                router.raw_mapping_root(&db, &work, input, specialization, TypeContext::default(),),
                Err(MappingFailure::Boundary(Boundary::MroDomain)),
            );
            let mut raw_demand = std::pin::pin!(router.raw_mapping_demand(&work, request));
            assert_eq!(
                raw_demand.as_mut().poll(&mut cx),
                Poll::Ready(Err(MappingFailure::Boundary(Boundary::MroDomain))),
            );
            assert!(router.effects.borrow().is_empty());
        }
    }

    #[test]
    fn promotion_retains_variables_and_missing_facts_keep_their_keys() {
        let db = setup_db();
        let env = db.program_environment();
        let (variable, _, _) = inputs(&db, &env);
        let ty = Type::TypeVar(variable);
        let router = Router::default();
        let request = router
            .mapping_root_for(ty, RequestedMapping::RegularPromotion(PromotionMode::On))
            .unwrap();
        let result = run_with(&db, &env, &router, 1_000, false, false, |router| {
            router.consumer_mapping_demand(request)
        })
        .unwrap();
        assert_eq!(result.consumer, Some(Ok(ty)));
        assert_eq!(result.graph.mapping_values.get(&request), Some(&Ok(ty)));
        let mut reservations: Vec<_> = result.semantic_work_polls.keys().copied().collect();
        reservations.sort_by_key(|work| work.sequence);
        assert_eq!(
            reservations
                .iter()
                .map(|work| work.units)
                .collect::<Vec<_>>(),
            [4, 3, 1],
        );

        let class = KnownClass::Int
            .to_class_literal(&db, &env)
            .as_class_literal()
            .unwrap();
        for key in [
            PromotionFactKey::Variance(variable),
            PromotionFactKey::EnumSingleton(class),
            PromotionFactKey::ScalarFallback(KnownClass::Int),
        ] {
            let router = Router::default();
            let request = router
                .mapping_root_for(ty, RequestedMapping::RegularPromotion(PromotionMode::On))
                .unwrap();
            let missing = Err(MappingFailure::MissingPromotionFact(key));
            router.mappings.borrow_mut().insert(
                request,
                Entry {
                    answer: Some(missing),
                    ..Entry::default()
                },
            );
            let result = run_with(&db, &env, &router, 1_000, false, false, |router| {
                router.consumer_mapping_demand(request)
            })
            .unwrap();
            assert_eq!(result.consumer, Some(missing));
            assert_eq!(result.graph.mapping_values.get(&request), Some(&missing));
            assert!(result.mapping_polls.is_empty());
        }
    }

    #[test]
    fn promotion_leaf_boundaries_reserve_only_reached_work() {
        let db = setup_db();
        let env = db.program_environment();
        let literal = Type::int_literal(1);
        let fixed_literal = Type::LiteralValue(LiteralValueType::unpromotable(1_i64));
        let float = KnownClass::Float.to_instance(&db, &env);
        let complex = KnownClass::Complex.to_instance(&db, &env);
        let tuple = Type::tuple(TupleType::empty(&db, &env));
        let union = UnionType::from_two_elements(&db, &env, literal, Type::int_literal(2));
        let promotion_boundary = Err(MappingFailure::Boundary(Boundary::MappingOperation(
            MappingOperation::Promotion,
        )));
        for (ty, mode, expected, work) in [
            (
                literal,
                PromotionMode::On,
                Err(MappingFailure::MissingPromotionFact(
                    PromotionFactKey::ScalarFallback(KnownClass::Int),
                )),
                vec![4, 3, 4],
            ),
            (literal, PromotionMode::Off, Ok(literal), vec![4, 3, 1]),
            (
                fixed_literal,
                PromotionMode::On,
                Ok(fixed_literal),
                vec![4, 3, 1],
            ),
            (float, PromotionMode::On, promotion_boundary, vec![4, 3, 4]),
            (
                complex,
                PromotionMode::On,
                promotion_boundary,
                vec![4, 3, 4],
            ),
            (float, PromotionMode::Off, Ok(float), vec![4, 3, 1]),
            (
                tuple,
                PromotionMode::Off,
                Err(MappingFailure::Boundary(Boundary::MappingOperation(
                    MappingOperation::Tuple,
                ))),
                vec![4, 3],
            ),
            (
                union,
                PromotionMode::Off,
                Err(MappingFailure::Boundary(Boundary::MappingOperation(
                    MappingOperation::Union,
                ))),
                vec![4, 3],
            ),
        ] {
            let router = Router::default();
            let request = router.promotion_root_with_mode(ty, mode).unwrap();
            let observed = probe::capture(&db, || {
                run_with(&db, &env, &router, 1_000, false, false, |router| {
                    router.consumer_mapping_demand(request)
                })
                .unwrap()
            })
            .unwrap();
            assert!(observed.reads.is_empty());
            let result = observed.value;
            assert_eq!(result.consumer, Some(expected));
            assert_eq!(result.graph.mapping_values.get(&request), Some(&expected));
            assert_eq!(result.mapping_polls.len(), 1);
            let mut reservations: Vec<_> = result.semantic_work_polls.keys().copied().collect();
            reservations.sort_by_key(|work| work.sequence);
            assert_eq!(
                reservations
                    .iter()
                    .map(|work| work.units)
                    .collect::<Vec<_>>(),
                work
            );
        }
    }

    #[test]
    fn promotion_fact_reservations_reject_sequence_overflow() {
        let router = Router::default();
        let parent = router.promotion_root(Type::int_literal(1)).unwrap();
        for work in [
            MappingWork::VarianceLookup,
            MappingWork::ScalarFallbackLookup,
        ] {
            let effects = QueuedMappingEffects {
                router: &router,
                parent,
                sequence: Cell::new(usize::MAX),
            };
            let mut checkpoint = std::pin::pin!(effects.checkpoint(work));
            let mut context = Context::from_waker(Waker::noop());
            assert_eq!(
                checkpoint.as_mut().poll(&mut context),
                Poll::Ready(Err(MappingFailure::Boundary(Boundary::CostOverflow))),
            );
            assert!(router.effects.borrow().is_empty());
            assert_eq!(effects.sequence.get(), usize::MAX);
        }
    }
}

impl<'db> crate::types::mapping::specialization_start::SpecializationStartEffects<'db>
    for QueuedMappingEffects<'_, 'db, '_>
{
    type Error = MappingFailure<'db>;
    async fn checkpoint(&self, work: MappingWork) -> Result<(), Self::Error> {
        SharedMappingStartEffects::checkpoint(self, work).await
    }
    async fn nominal_is_definition_generic(
        &self,
        db: &'db dyn Db,
        instance: NominalInstanceType<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(instance.is_definition_generic(db))
    }
    async fn typevar_is_paramspec(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(variable.is_paramspec(db))
    }
    async fn lookup_typevar(
        &self,
        db: &'db dyn Db,
        specialization: crate::types::Specialization<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(specialization.get(db, variable))
    }
    async fn materialization_kind(
        &self,
        db: &'db dyn Db,
        specialization: crate::types::Specialization<'db>,
    ) -> Result<Option<crate::types::MaterializationKind>, Self::Error> {
        Ok(specialization.materialization_kind(db))
    }
    async fn typevar_is_self(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(variable.typevar(db).is_self(db))
    }
}
