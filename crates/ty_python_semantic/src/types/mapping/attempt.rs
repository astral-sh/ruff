//! Mapping tasks whose children are driven on a flat stack under the installed attempt.

use crate::types::mapping::effects::{
    NativeMappingLeaf, SharedMappingEffects, SharedMappingStartEffects,
};
use crate::types::mapping::effects::{
    SynchronousSharedMappingEffects, SynchronousSharedMappingStartEffects,
};
use crate::types::mapping::{LegacyTypeMappingContinuation, TypeVarMappingContinuation};
use crate::types::{IntersectionType, NominalInstanceType, TypeFormType, UnionType};

use std::cell::{Cell, RefCell};
use std::future::{Future, poll_fn, ready};
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use super::MappingStart;
use super::effects::{
    MappingEffects, MappingFacts, MappingOperation, MappingStartEffects,
    MappingTransformationScope, MappingWork, SynchronousMappingEffects,
    SynchronousMappingStartEffects, sealed,
};
use super::self_binding::attempt::AttemptSelfBindingEffects;
use super::self_binding::{prepare_with, should_bind_with};
use crate::types::class_base::ClassBase;
use crate::types::constraints::control::GrowthPlan;
use crate::types::constructor::expansion_probe::{self, Incomplete};
use crate::types::cyclic::{
    TypeIdentity, TypeTransformationControl, TypeTransformationGrowth, TypeTransformationWork,
    TypeTransformerVisit,
};
use crate::types::generics::{ApplySpecialization, Specialization};
use crate::types::relation::execution::attempt::AttemptAdmission;
use crate::types::relation::execution::{
    ExecutionAdmission as _, ExecutionWork as MappingExecutionWork,
};
use crate::types::signatures::effects::try_poll_immediate;
use crate::types::tuple::TupleType;
use crate::types::typevar::BindingContext;
use crate::types::{
    ApplyTypeMappingVisitor, BoundTypeVarInstance, KnownClass, MaterializationKind, SelfBinding,
    Type, TypeContext, TypeMapping, TypeVarVariance,
};
use crate::{Db, ProgramEnvironment};

#[cfg(test)]
pub(in crate::types) mod frame_observation;

#[cfg(test)]
mod layout_probe;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum UnsupportedMappingOperation {
    Legacy(MappingOperation),
    Identity,
    Variance,
    ScalarFallback,
    ChildMapping,
    InvalidSuspension,
    UncontrolledMro,
}

fn unsupported<T>(db: &dyn Db, operation: UnsupportedMappingOperation) -> Result<T, Incomplete> {
    expansion_probe::continue_work(db)?;
    Err(expansion_probe::refuse(
        db,
        Incomplete::UnsupportedMappingOperation(operation),
    ))
}

#[cfg_attr(test, track_caller)]
fn admit_width(db: &dyn Db, width: usize, multiplier: usize) -> Result<(), Incomplete> {
    let Some(units) = width
        .checked_mul(multiplier)
        .and_then(|units| units.checked_add(1))
    else {
        #[cfg(test)]
        expansion_probe::charge_ledger::overflow(width, multiplier);
        return Err(expansion_probe::refuse(db, Incomplete::Allowance));
    };
    expansion_probe::charge_work(db, units)
}

enum OwnedMapping<'db> {
    Specialize {
        specialization: Specialization<'db>,
        specialize_self_domain: bool,
    },
    BindSelf(SelfBinding<'db>),
    Materialize(MaterializationKind),
}

impl<'db> OwnedMapping<'db> {
    fn from_mapping(db: &'db dyn Db, mapping: &TypeMapping<'_, 'db>) -> Result<Self, Incomplete> {
        match mapping {
            TypeMapping::ApplySpecialization(ApplySpecialization::Specialization {
                specialization,
                specialize_self_domain,
            }) => Ok(Self::Specialize {
                specialization: *specialization,
                specialize_self_domain: *specialize_self_domain,
            }),
            TypeMapping::BindSelf(binding) => Ok(Self::BindSelf(*binding)),
            TypeMapping::Materialize(kind) => Ok(Self::Materialize(*kind)),
            _ => unsupported(db, UnsupportedMappingOperation::ChildMapping),
        }
    }

    fn into_mapping(self) -> TypeMapping<'db, 'db> {
        match self {
            Self::Specialize {
                specialization,
                specialize_self_domain,
            } => TypeMapping::ApplySpecialization(ApplySpecialization::Specialization {
                specialization,
                specialize_self_domain,
            }),
            Self::BindSelf(binding) => TypeMapping::BindSelf(binding),
            Self::Materialize(kind) => TypeMapping::Materialize(kind),
        }
    }
}

struct MappingRequest<'db> {
    ty: Type<'db>,
    tcx: TypeContext<'db>,
    mapping: OwnedMapping<'db>,
}

#[derive(Default)]
struct Mailbox<'db> {
    request: Option<MappingRequest<'db>>,
    answer: Option<Type<'db>>,
}

/// The caller owns the visitor until this driver's tasks have all completed or been dropped.
/// Child requests retain operation handles, so they never borrow a suspended parent's mapping.
struct Queued<'a, 'env, 'db> {
    visitor: &'a ApplyTypeMappingVisitor<'env, 'db>,
    mailbox: &'a RefCell<Mailbox<'db>>,
}

struct AttemptMappingEffects<'a, 'db, Mode = ()> {
    db: &'db dyn Db,
    last_work: &'a Cell<Option<MappingWork>>,
    prefix_copies_requested: &'a Cell<usize>,
    mode: Mode,
}

impl<Mode> sealed::Sealed for AttemptMappingEffects<'_, '_, Mode> {}

impl<Mode> TypeTransformationControl for AttemptMappingEffects<'_, '_, Mode> {
    type Error = Incomplete;

    fn checkpoint(&self, work: TypeTransformationWork) -> Result<(), Incomplete> {
        #[cfg(test)]
        let _charge = expansion_probe::charge_ledger::scope(&work);
        let quote = work
            .quote()
            .ok_or_else(|| expansion_probe::refuse(self.db, Incomplete::Allowance))?;
        expansion_probe::charge_work(self.db, quote.work_units)?;
        if quote.requested_payload_bytes != 0 {
            AttemptAdmission { db: self.db }.admit(MappingExecutionWork::Allocation {
                requested_payload_bytes: quote.requested_payload_bytes,
            })?;
        }
        Ok(())
    }

    fn prepare_growth(
        &self,
        request: TypeTransformationGrowth,
    ) -> Result<Option<GrowthPlan>, Incomplete> {
        let plan = request
            .checked_plan()
            .ok_or_else(|| expansion_probe::refuse(self.db, Incomplete::Allowance))?;
        TypeTransformationControl::checkpoint(self, request.work(plan))?;
        Ok(Some(plan))
    }

    fn identity<'db>(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
    ) -> Result<TypeIdentity<'db>, Incomplete> {
        expansion_probe::charge_work(db, 1)?;
        match ty {
            Type::TypeAlias(_)
            | Type::ProtocolInstance(_)
            | Type::TypedDict(_)
            | Type::Recursive(_) => unsupported(db, UnsupportedMappingOperation::Identity),
            _ => Ok(ty.to_type_identity(db)),
        }
    }
}

impl<'db, Mode> MappingFacts<'db> for AttemptMappingEffects<'_, 'db, Mode> {
    type Error = Incomplete;

    fn variance(
        &self,
        _db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<TypeVarVariance, Incomplete> {
        expansion_probe::charge_work(self.db, 1)?;
        variable
            .typevar(self.db)
            .explicit_variance(self.db)
            .ok_or_else(|| {
                expansion_probe::refuse(
                    self.db,
                    Incomplete::UnsupportedMappingOperation(UnsupportedMappingOperation::Variance),
                )
            })
    }

    fn scalar_fallback(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _class: KnownClass,
    ) -> Result<Type<'db>, Incomplete> {
        unsupported(self.db, UnsupportedMappingOperation::ScalarFallback)
    }
}

impl<Mode> AttemptMappingEffects<'_, '_, Mode> {
    fn admit(&self, work: MappingWork) -> Result<(), Incomplete> {
        #[cfg(test)]
        let _charge = expansion_probe::charge_ledger::scope(&work);
        self.last_work.set(Some(work));
        if matches!(work, MappingWork::ArgumentPrefixCopy { .. }) {
            self.prefix_copies_requested
                .set(self.prefix_copies_requested.get() + 1);
        }
        match work {
            MappingWork::ArgumentCapacity { width }
            | MappingWork::SpecializationPayload { width } => admit_width(self.db, width, 8),
            MappingWork::ArgumentPrefixCopy { len } => admit_width(self.db, len, 1),
            MappingWork::WrapperIntern
            | MappingWork::GenericAliasIntern
            | MappingWork::ExplicitAnyIntern => expansion_probe::charge_work(self.db, 8),
            _ => expansion_probe::charge_work(self.db, 1),
        }
    }
}

impl<'db> MappingStartEffects<'db> for AttemptMappingEffects<'_, 'db, Queued<'_, '_, 'db>> {
    fn legacy<T>(
        &self,
        operation: MappingOperation,
        _thunk: impl FnOnce() -> T,
    ) -> Result<T, Incomplete> {
        unsupported(self.db, UnsupportedMappingOperation::Legacy(operation))
    }
}

impl<'db> MappingEffects<'db> for AttemptMappingEffects<'_, 'db, Queued<'_, '_, 'db>> {
    async fn should_bind_self(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        binding: &SelfBinding<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<bool, Incomplete> {
        should_bind_with(
            db,
            env,
            binding,
            variable,
            &AttemptSelfBindingEffects::new(db),
        )
        .await
    }
}

impl<'db> SynchronousMappingStartEffects<'db> for AttemptMappingEffects<'_, 'db> {
    fn legacy<T>(
        &self,
        operation: MappingOperation,
        _thunk: impl FnOnce() -> T,
    ) -> Result<T, Incomplete> {
        unsupported(self.db, UnsupportedMappingOperation::Legacy(operation))
    }
}

impl<'db> SynchronousMappingEffects<'db> for AttemptMappingEffects<'_, 'db> {
    fn should_bind_self(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        binding: &SelfBinding<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<bool, Incomplete> {
        match try_poll_immediate(should_bind_with(
            db,
            env,
            binding,
            variable,
            &AttemptSelfBindingEffects::new(db),
        )) {
            Poll::Ready(result) => result,
            Poll::Pending => unsupported(db, UnsupportedMappingOperation::InvalidSuspension),
        }
    }
}

impl<'db> SharedMappingStartEffects<'db> for AttemptMappingEffects<'_, 'db, Queued<'_, '_, 'db>> {
    type Failure = Incomplete;
    fn checkpoint(&self, work: MappingWork) -> impl Future<Output = Result<(), Incomplete>> {
        ready(self.admit(work))
    }
    async fn begin_transformation<'a>(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &'a ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<TypeTransformerVisit<'db, MappingTransformationScope<'a, 'db>>, Incomplete> {
        AttemptMappingEffects::begin_transformation(self, db, ty, mapping, visitor)
    }
    async fn admit_mode(&self, _mapping: &TypeMapping<'_, 'db>) -> Result<(), Self::Failure> {
        unsupported(
            self.db,
            UnsupportedMappingOperation::Legacy(MappingOperation::MappingMode),
        )
    }
    async fn expand_paramspecs(
        &self,
        _db: &'db dyn Db,
        _ty: Type<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Option<Type<'db>>, Self::Failure> {
        unsupported(
            self.db,
            UnsupportedMappingOperation::Legacy(MappingOperation::FunctionParamSpecPrelude),
        )
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
        unsupported(
            self.db,
            UnsupportedMappingOperation::Legacy(leaf.operation()),
        )
    }
}
impl<'db> SharedMappingEffects<'db> for AttemptMappingEffects<'_, 'db, Queued<'_, '_, 'db>> {
    async fn map_union(
        &self,
        _db: &'db dyn Db,
        _union: UnionType<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Self::Failure> {
        unsupported(
            self.db,
            UnsupportedMappingOperation::Legacy(MappingOperation::Union),
        )
    }
    async fn map_intersection(
        &self,
        _db: &'db dyn Db,
        _intersection: IntersectionType<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Self::Failure> {
        unsupported(
            self.db,
            UnsupportedMappingOperation::Legacy(MappingOperation::Intersection),
        )
    }
    async fn map_tuple(
        &self,
        _db: &'db dyn Db,
        _tuple: TupleType<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Self::Failure> {
        unsupported(
            self.db,
            UnsupportedMappingOperation::Legacy(MappingOperation::Tuple),
        )
    }
    async fn map_type(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Incomplete> {
        expansion_probe::charge_work(db, 1)?;
        if !std::ptr::eq(visitor, self.mode.visitor) {
            return unsupported(db, UnsupportedMappingOperation::ChildMapping);
        }
        let mapping = OwnedMapping::from_mapping(db, mapping)?;
        {
            let mut mailbox = self.mode.mailbox.borrow_mut();
            if mailbox.request.is_some() || mailbox.answer.is_some() {
                return unsupported(db, UnsupportedMappingOperation::InvalidSuspension);
            }
            mailbox.request = Some(MappingRequest { ty, tcx, mapping });
        }
        poll_fn(|_| {
            if let Err(error) = expansion_probe::continue_work(db) {
                return Poll::Ready(Err(error));
            }
            match self.mode.mailbox.borrow_mut().answer.take() {
                Some(answer) => Poll::Ready(Ok(answer)),
                None => Poll::Pending,
            }
        })
        .await
    }
    async fn finish_transformation(
        &self,
        scope: MappingTransformationScope<'_, 'db>,
        result: Type<'db>,
    ) -> Result<Type<'db>, Incomplete> {
        AttemptMappingEffects::finish_transformation(self, scope, result)
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

impl<'db> SynchronousSharedMappingStartEffects<'db> for AttemptMappingEffects<'_, 'db> {
    type Failure = Incomplete;
    fn checkpoint(&self, work: MappingWork) -> Result<(), Incomplete> {
        self.admit(work)
    }
    fn begin_transformation<'a>(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &'a ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<TypeTransformerVisit<'db, MappingTransformationScope<'a, 'db>>, Incomplete> {
        AttemptMappingEffects::begin_transformation(self, db, ty, mapping, visitor)
    }
    fn admit_mode(&self, _mapping: &TypeMapping<'_, 'db>) -> Result<(), Self::Failure> {
        unsupported(
            self.db,
            UnsupportedMappingOperation::Legacy(MappingOperation::MappingMode),
        )
    }
    fn expand_paramspecs(
        &self,
        _db: &'db dyn Db,
        _ty: Type<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Option<Type<'db>>, Self::Failure> {
        unsupported(
            self.db,
            UnsupportedMappingOperation::Legacy(MappingOperation::FunctionParamSpecPrelude),
        )
    }
    fn nominal_known_class(
        &self,
        db: &'db dyn Db,
        instance: NominalInstanceType<'db>,
    ) -> Result<Option<KnownClass>, Self::Failure> {
        Ok(instance.known_class(db))
    }
    fn start_typevar(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<MappingStart<Type<'db>, TypeVarMappingContinuation<'db>>, Self::Failure> {
        variable.mapping_start_sync(db, mapping, visitor, self)
    }
    fn legacy_leaf(
        &self,
        _db: &'db dyn Db,
        _ty: Type<'db>,
        leaf: NativeMappingLeaf<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Self::Failure> {
        unsupported(
            self.db,
            UnsupportedMappingOperation::Legacy(leaf.operation()),
        )
    }
}
impl<'db> SynchronousSharedMappingEffects<'db> for AttemptMappingEffects<'_, 'db> {
    fn map_union(
        &self,
        _db: &'db dyn Db,
        _union: UnionType<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Self::Failure> {
        unsupported(
            self.db,
            UnsupportedMappingOperation::Legacy(MappingOperation::Union),
        )
    }
    fn map_intersection(
        &self,
        _db: &'db dyn Db,
        _intersection: IntersectionType<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Self::Failure> {
        unsupported(
            self.db,
            UnsupportedMappingOperation::Legacy(MappingOperation::Intersection),
        )
    }
    fn map_tuple(
        &self,
        _db: &'db dyn Db,
        _tuple: TupleType<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _tcx: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Self::Failure> {
        unsupported(
            self.db,
            UnsupportedMappingOperation::Legacy(MappingOperation::Tuple),
        )
    }
    fn map_type(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Incomplete> {
        expansion_probe::charge_work(db, 1)?;
        let request = MappingRequest {
            ty,
            tcx,
            mapping: OwnedMapping::from_mapping(db, mapping)?,
        };
        drive_type(
            db,
            request,
            visitor,
            self,
            &mut MappingStatistics::default(),
        )
    }
    fn finish_transformation(
        &self,
        scope: MappingTransformationScope<'_, 'db>,
        result: Type<'db>,
    ) -> Result<Type<'db>, Incomplete> {
        AttemptMappingEffects::finish_transformation(self, scope, result)
    }
    fn typeform_argument(
        &self,
        db: &'db dyn Db,
        typeform: TypeFormType<'db>,
    ) -> Result<Type<'db>, Self::Failure> {
        Ok(typeform.type_argument(db))
    }
    fn intern_typeform(
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
    ) -> Result<Type<'db>, Self::Failure> {
        continuation.resume_mapping_sync(db, mapping, tcx, visitor, self)
    }
}

impl<'db, Mode> AttemptMappingEffects<'_, 'db, Mode> {
    fn begin_transformation<'a>(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &'a ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<TypeTransformerVisit<'db, MappingTransformationScope<'a, 'db>>, Incomplete> {
        let unsupported_operation = match ty {
            Type::FunctionLiteral(_) => Some(MappingOperation::Function),
            Type::Callable(_) => Some(MappingOperation::Callable),
            _ => None,
        };
        if let Some(operation) = unsupported_operation {
            return unsupported(db, UnsupportedMappingOperation::Legacy(operation));
        }
        expansion_probe::charge_work(db, 1)?;
        visitor.transformer(mapping).begin_visit_with(db, ty, self)
    }
    fn finish_transformation(
        &self,
        scope: MappingTransformationScope<'_, 'db>,
        result: Type<'db>,
    ) -> Result<Type<'db>, Incomplete> {
        expansion_probe::charge_work(self.db, 1)?;
        scope.finish_with(result, self)
    }
}
impl<'db> AttemptMappingEffects<'_, 'db, Queued<'_, '_, 'db>> {
    fn map_type(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> impl Future<Output = Result<Type<'db>, Incomplete>> {
        SharedMappingEffects::map_type(self, db, ty, mapping, tcx, visitor)
    }
}

type Task<'a, 'db> = Pin<Box<dyn Future<Output = Result<Type<'db>, Incomplete>> + 'a>>;

#[derive(Clone, Copy, Default)]
struct NativeDepth {
    roots: usize,
    drivers: usize,
    polls: usize,
    drops: usize,
    max_polls: usize,
    max_drops: usize,
    max_drivers: usize,
}

thread_local! {
    static NATIVE_DEPTH: Cell<NativeDepth> = Cell::new(NativeDepth::default());
}

enum Activity {
    Root,
    Driver,
    Poll,
    Drop,
}

struct ActivityScope(Activity);

impl ActivityScope {
    fn enter(activity: Activity) -> Self {
        NATIVE_DEPTH.with(|state| {
            let mut depth = state.get();
            match activity {
                Activity::Root => {
                    if depth.roots == 0 {
                        depth = NativeDepth::default();
                    }
                    depth.roots += 1;
                }
                Activity::Poll => {
                    depth.polls += 1;
                    depth.max_polls = depth.max_polls.max(depth.polls);
                }
                Activity::Driver => {
                    depth.drivers += 1;
                    depth.max_drivers = depth.max_drivers.max(depth.drivers);
                }
                Activity::Drop => {
                    depth.drops += 1;
                    depth.max_drops = depth.max_drops.max(depth.drops);
                }
            }
            state.set(depth);
        });
        Self(activity)
    }
}

impl Drop for ActivityScope {
    fn drop(&mut self) {
        NATIVE_DEPTH.with(|state| {
            let mut depth = state.get();
            match self.0 {
                Activity::Root => depth.roots -= 1,
                Activity::Driver => depth.drivers -= 1,
                Activity::Poll => depth.polls -= 1,
                Activity::Drop => depth.drops -= 1,
            }
            state.set(depth);
        });
    }
}

#[derive(Debug, Default)]
pub(in crate::types) struct MappingStatistics {
    pub frames: usize,
    pub polls: usize,
    pub max_pending: usize,
    pub dropped_frames: usize,
    pub last_work: Option<MappingWork>,
    pub prefix_copies_requested: usize,
    pub max_task_poll_depth: usize,
    pub max_task_drop_depth: usize,
    pub max_driver_depth: usize,
}

/// A suspended parent never owns its child. Dropping this stack removes innermost scopes first.
#[derive(Default)]
struct Tasks<'a, 'db> {
    pending: Vec<Task<'a, 'db>>,
}

impl Tasks<'_, '_> {
    fn drop_last(&mut self) -> bool {
        let Some(task) = self.pending.pop() else {
            return false;
        };
        let _drop = ActivityScope::enter(Activity::Drop);
        drop(task);
        #[cfg(test)]
        frame_observation::dropped();
        true
    }
}

impl Drop for Tasks<'_, '_> {
    fn drop(&mut self) {
        while self.drop_last() {}
    }
}

fn start_task<'a, 'db>(
    db: &'db dyn Db,
    request: MappingRequest<'db>,
    control: &AttemptMappingEffects<'_, 'db>,
    effects: &'a AttemptMappingEffects<'_, 'db, Queued<'_, '_, 'db>>,
    tasks: &mut Tasks<'a, 'db>,
    statistics: &mut MappingStatistics,
) -> Result<Option<Type<'db>>, Incomplete> {
    expansion_probe::charge_work(db, 1)?;
    let mapping = request.mapping.into_mapping();
    let continuation = match request.ty.mapping_start_sync(
        db,
        &mapping,
        request.tcx,
        effects.mode.visitor,
        control,
    )? {
        MappingStart::Complete(result) => return Ok(Some(result)),
        MappingStart::Continue(continuation) => continuation,
    };
    if tasks.pending.len() == tasks.pending.capacity() {
        let Some(capacity) = tasks.pending.capacity().max(2).checked_mul(2) else {
            #[cfg(test)]
            expansion_probe::charge_ledger::overflow(tasks.pending.capacity().max(2), 2);
            return Err(expansion_probe::refuse(db, Incomplete::Allowance));
        };
        {
            #[cfg(test)]
            let _charge = expansion_probe::charge_ledger::scope(&"MappingTaskCapacity");
            admit_width(db, capacity, 2)?;
        }
        tasks.pending.reserve_exact(capacity - tasks.pending.len());
    }
    let task = async move {
        continuation
            .resume_mapping_with(db, &mapping, request.tcx, effects.mode.visitor, effects)
            .await
    };
    {
        #[cfg(test)]
        let _charge = expansion_probe::charge_ledger::scope(&"MappingFrame");
        expansion_probe::charge_work(db, std::mem::size_of_val(&task))?;
    }
    #[cfg(test)]
    let frame_bytes = std::mem::size_of_val(&task);
    tasks.pending.push(Box::pin(task));
    #[cfg(test)]
    frame_observation::allocated(frame_bytes);
    statistics.frames += 1;
    statistics.max_pending = statistics.max_pending.max(tasks.pending.len());
    Ok(None)
}

/// Drives recursive Type children while the synchronous outer operation retains its visitor.
/// Source inference can still call back into a constructor and enter a separate driver.
fn drive_type<'db>(
    db: &'db dyn Db,
    request: MappingRequest<'db>,
    visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    control: &AttemptMappingEffects<'_, 'db>,
    statistics: &mut MappingStatistics,
) -> Result<Type<'db>, Incomplete> {
    let _driver = ActivityScope::enter(Activity::Driver);
    #[cfg(test)]
    let _observer_driver = frame_observation::DriverScope::enter();
    let mailbox = RefCell::new(Mailbox::default());
    let effects = AttemptMappingEffects {
        db,
        last_work: control.last_work,
        prefix_copies_requested: control.prefix_copies_requested,
        mode: Queued {
            visitor,
            mailbox: &mailbox,
        },
    };
    let mut tasks = Tasks::default();
    let mut context = Context::from_waker(Waker::noop());
    let answer = (|| {
        let mut next_request = Some(request);
        loop {
            if let Some(request) = next_request.take()
                && let Some(result) =
                    start_task(db, request, control, &effects, &mut tasks, statistics)?
            {
                expansion_probe::charge_work(db, 1)?;
                if tasks.pending.is_empty() {
                    break Ok(result);
                }
                mailbox.borrow_mut().answer = Some(result);
            }
            expansion_probe::charge_work(db, 1)?;
            let Some(task) = tasks.pending.last_mut() else {
                break unsupported(db, UnsupportedMappingOperation::InvalidSuspension);
            };
            statistics.polls += 1;
            let polled = {
                let _poll = ActivityScope::enter(Activity::Poll);
                task.as_mut().poll(&mut context)
            };
            #[cfg(test)]
            frame_observation::polled(&polled);
            match polled {
                Poll::Ready(result) => {
                    tasks.drop_last();
                    statistics.dropped_frames += 1;
                    let result = result?;
                    expansion_probe::charge_work(db, 1)?;
                    if tasks.pending.is_empty() {
                        break Ok(result);
                    }
                    mailbox.borrow_mut().answer = Some(result);
                }
                Poll::Pending => {
                    let request = mailbox.borrow_mut().request.take();
                    let Some(request) = request else {
                        break unsupported(db, UnsupportedMappingOperation::InvalidSuspension);
                    };
                    next_request = Some(request);
                }
            }
        }
    })();
    statistics.dropped_frames += tasks.pending.len();
    answer
}

fn with_boundary<'db, T>(
    db: &'db dyn Db,
    operation: impl FnOnce(&AttemptMappingEffects<'_, 'db>) -> Result<T, Incomplete>,
) -> Result<T, Incomplete> {
    let _root = ActivityScope::enter(Activity::Root);
    if !expansion_probe::mro_effects_enabled() {
        return unsupported(db, UnsupportedMappingOperation::UncontrolledMro);
    }
    let last_work = Cell::new(None);
    let prefix_copies_requested = Cell::new(0);
    let effects = AttemptMappingEffects {
        db,
        last_work: &last_work,
        prefix_copies_requested: &prefix_copies_requested,
        mode: (),
    };
    effects.admit(MappingWork::RootAdmission)?;
    let result = operation(&effects)?;
    effects.admit(MappingWork::ResultPublication)?;
    Ok(result)
}

pub(in crate::types) fn compose_specialization<'db>(
    db: &'db dyn Db,
    base: Specialization<'db>,
    additional: Specialization<'db>,
) -> Result<Specialization<'db>, Incomplete> {
    with_boundary(db, |effects| {
        let env = ProgramEnvironment::from_program(additional.generic_context(db).program(db));
        let visitor = ApplyTypeMappingVisitor::new(&env);
        base.apply_specialization_sync(db, additional, &visitor, effects)
    })
}

pub(in crate::types) fn specialize_base<'db>(
    db: &'db dyn Db,
    base: ClassBase<'db>,
    specialization: Option<Specialization<'db>>,
) -> Result<ClassBase<'db>, Incomplete> {
    with_boundary(db, |effects| {
        base.apply_optional_specialization_sync(db, specialization, effects)
    })
}

pub(in crate::types) fn bind_self<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    receiver: Type<'db>,
    binding_context: Option<BindingContext<'db>>,
) -> (Result<Type<'db>, Incomplete>, MappingStatistics) {
    let _root = ActivityScope::enter(Activity::Root);
    let mut statistics = MappingStatistics::default();
    let last_work = Cell::new(None);
    let prefix_copies_requested = Cell::new(0);
    let result = (|| {
        if !expansion_probe::mro_effects_enabled() {
            return unsupported(db, UnsupportedMappingOperation::UncontrolledMro);
        }
        let binding = match try_poll_immediate(prepare_with(
            db,
            env,
            receiver,
            binding_context,
            &AttemptSelfBindingEffects::new(db),
        )) {
            Poll::Ready(binding) => binding?,
            Poll::Pending => {
                return unsupported(db, UnsupportedMappingOperation::InvalidSuspension);
            }
        };
        let visitor = ApplyTypeMappingVisitor::new(env);
        let effects = AttemptMappingEffects {
            db,
            last_work: &last_work,
            prefix_copies_requested: &prefix_copies_requested,
            mode: (),
        };
        drive_type(
            db,
            MappingRequest {
                ty,
                tcx: TypeContext::default(),
                mapping: OwnedMapping::BindSelf(binding),
            },
            &visitor,
            &effects,
            &mut statistics,
        )
    })();
    statistics.last_work = last_work.get();
    statistics.prefix_copies_requested = prefix_copies_requested.get();
    let depth = NATIVE_DEPTH.with(Cell::get);
    statistics.max_task_poll_depth = depth.max_polls;
    statistics.max_task_drop_depth = depth.max_drops;
    statistics.max_driver_depth = depth.max_drivers;
    (result, statistics)
}
