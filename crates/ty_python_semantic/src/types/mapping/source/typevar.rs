use salsa::execution_probe::{BorrowOrCopy, FieldRequest, FieldRequestContext, RunError, RunResult};

use super::{MappingSourceEffects, RetainedMappingSource, SourceMapping};
use crate::{Db, ProgramEnvironment};
use crate::types::constraints::control::hash_slots;
use crate::types::generics::context_construction::ContextVariables;
use crate::types::generics::mapping::{SpecializationLookupEffects, lookup_specialization_with};
use crate::types::generics::prefix::TypeArgumentPrefix;
use crate::types::local_transfer::{
    generated_field_quote, boxed_future_with_fixed_transfers_at,
};
use crate::types::mapping::MaterializationOperation;
use crate::types::mapping::return_callables::ReturnTypevarReplacements;
use crate::types::mapping::effects::{MappingOperation, SharedMappingEffects};
use crate::types::typevar::specialization::{
    TypeVarSpecialization, TypeVarSpecializationEffects, TypeVarSpecializationFacts,
    specialize_bound_typevar_with,
};
use crate::types::typevar::{
    BoundTypeVarIdentity, BoundTypeVarInstance, ParamSpecAttrKind, TypeVarKind,
    TypeVarBoundOrConstraints, TypeVarBoundOrConstraintsEvaluation, TypeVarConstraints, TypeVarDefaultEvaluation, TypeVarIdentity, TypeVarInstance,
};
use crate::types::{ApplySpecialization, GenericContext, Specialization, Type, TypeContext, TypeVarVariance};
use crate::types::typevar::retained_self::{RetainedSelfEffects, RetainedBoundsEffects, retain_self_domain_with};
use crate::types::visitor::runtime::TypeSliceDeref;

/// A stored or single-variable substitution that needs no borrowed collection owner.
/// Raw lookup is shared by type-variable mapping and signature-context filtering; the former
/// separately strips and reapplies ParamSpec attributes in its shared algorithm.
#[derive(Clone, Copy, Debug)]
pub(super) enum SourceSpecialization<'db> {
    Stored {
        specialization: Specialization<'db>,
        specialize_self_domain: bool,
    },
    Single {
        variable: BoundTypeVarInstance<'db>,
        replacement: Type<'db>,
    },
}

impl<'db> SourceSpecialization<'db> {
    /// Captures the ordinary `Specialization` and `Single` variants of `ApplySpecialization`.
    /// Other modes return `None`; borrowed prefixes and return maps keep their existing owners.
    pub(super) const fn from_specialization(specialization: &ApplySpecialization<'_, 'db>) -> Option<Self> {
        match specialization {
            ApplySpecialization::Specialization { specialization, specialize_self_domain } => Some(Self::Stored {
                specialization: *specialization,
                specialize_self_domain: *specialize_self_domain,
            }),
            ApplySpecialization::Single(variable, replacement) => Some(Self::Single {
                variable: *variable,
                replacement: *replacement,
            }),
            ApplySpecialization::TypeAlias(_)
            | ApplySpecialization::Partial { .. }
            | ApplySpecialization::ReturnCallables(_)
            | ApplySpecialization::WithBindings { .. } => None,
        }
    }

    /// Reconstructs the ordinary substitution without borrowing a temporary mapping descriptor.
    pub(super) const fn into_specialization(self) -> ApplySpecialization<'db, 'db> {
        match self {
            Self::Stored { specialization, specialize_self_domain } => ApplySpecialization::Specialization {
                specialization,
                specialize_self_domain,
            },
            Self::Single { variable, replacement } => ApplySpecialization::Single(variable, replacement),
        }
    }
}

/// Stores the constraint-buffer length and checked work/byte quote used to admit allocation and
/// eventual retirement before constructing the buffer.
#[derive(Clone, Copy, Debug)]
struct RetainedConstraintStorage {
    len: usize,
    work: usize,
    bytes: usize,
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>> SourceMapping<'_, 'owner, 'env, 'run, 'db, R> {
    pub(super) async fn specialize_typevar(
        &self,
        _db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
        specialization: &ApplySpecialization<'_, 'db>,
    ) -> RunResult<Type<'db>> {
        let supported = self
            .local(2, 0, || {
                Ok(matches!(
                    specialization,
                    ApplySpecialization::Partial { .. } | ApplySpecialization::Specialization { .. } | ApplySpecialization::ReturnCallables(_) | ApplySpecialization::Single(..)
                ))
            })
            .await?;
        if !supported {
            return self
                .unavailable(MaterializationOperation::Leaf(
                    MappingOperation::MappingMode,
                ))
                .await;
        }
        match specialize_bound_typevar_with(
            variable,
            specialization,
            TypeVarSpecializationFacts,
            self,
        )
        .await?
        {
            TypeVarSpecialization::Type(ty) => Ok(ty),
            TypeVarSpecialization::RetainedSelf => {
                let stored = self.function_local(Some(2), Some(0), || match specialization {
                    ApplySpecialization::Specialization { specialization, .. } => Some(*specialization),
                    _ => None,
                }).await?;
                let Some(stored) = stored else {
                    return self.unavailable(MaterializationOperation::Leaf(MappingOperation::RetainedSelf)).await;
                };
                let mapped = self.function_child(|| retain_self_domain_with(variable, stored, self.visitor.env, self)).await?;
                self.function_local(Some(1), Some(0), || Type::TypeVar(mapped)).await
            }
        }
    }

    /// Looks up a raw replacement using stored specialization order or complete occurrence identity.
    /// Single-variable lookup reads the candidate identity before the selected identity, as ordinary
    /// `ApplySpecialization::get` does; bounds and defaults do not change occurrence identity.
    pub(super) async fn source_specialization_lookup(
        &self,
        specialization: SourceSpecialization<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        match specialization {
            SourceSpecialization::Stored { specialization, .. } => {
                lookup_specialization_with(specialization, variable, self).await
            }
            SourceSpecialization::Single { variable: selected, replacement } => {
                let candidate = TypeVarSpecializationEffects::identity(self, variable).await?;
                let selected = TypeVarSpecializationEffects::identity(self, selected).await?;
                self.function_local(Some(16), Some(8 * size_of::<bool>()), || {
                    if candidate == selected { Some(replacement) } else { None }
                }).await
            }
        }
    }

    /// Looks up one renamed variable after admitting the complete insert-only table probe.
    pub(super) async fn return_typevar_lookup(
        &self,
        replacements: ReturnTypevarReplacements<'_, 'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        let (len, capacity) = self.function_local(Some(2), Some(0), || {
            (replacements.len(), replacements.capacity())
        }).await?;
        let slots = hash_slots::<RunError>(capacity).ok();
        let work = slots.and_then(|slots| slots.checked_mul(4))
            .and_then(|work| work.checked_add(len.checked_mul(4)?))
            .and_then(|work| work.checked_add(16));
        self.function_local(work, Some(size_of::<Option<&BoundTypeVarInstance<'db>>>() * 2 + size_of::<u64>() * 8), || {
            replacements.get(variable)
        }).await
    }

    /// Finds a bound identity's ordered index in a specialization context after admitting the
    /// complete table probe. Returns `None` when the identity is absent; quotation overflow refuses
    /// before the lookup.
    async fn specialization_index(
        &self,
        variables: &'db ContextVariables<'db>,
        identity: BoundTypeVarIdentity<'db>,
    ) -> RunResult<Option<usize>> {
        let quote = self.function_local(Some(20), Some(12 * size_of::<usize>() + 12 * size_of::<Option<usize>>()), || {
            let slots = hash_slots::<RunError>(variables.capacity()).map_err(|_| RunError::Contract("specialization context lookup quotation overflow"))?;
            let work = variables.len().checked_mul(12)
                .and_then(|work| work.checked_add(slots.checked_mul(4)?))
                .and_then(|work| work.checked_add(24))
                .ok_or(RunError::Contract("specialization context lookup quotation overflow"))?;
            Ok::<_, RunError>((work, 8 * size_of::<u64>() + 2 * size_of::<BoundTypeVarIdentity<'db>>() + 2 * size_of::<Option<&BoundTypeVarIdentity<'db>>>()))
        }).await??;
        // Four logical identity fields determine hashing/comparison; representation widths are bytes.
        self.function_local(Some(quote.0), Some(quote.1), || variables.get_index_of(&identity)).await
    }
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>> SpecializationLookupEffects<'db>
    for SourceMapping<'_, 'owner, 'env, 'run, 'db, R>
{
    type Error = RunError;

    async fn generic_context(
        &self,
        specialization: Specialization<'db>,
    ) -> RunResult<GenericContext<'db>> {
        self.retained_field(specialization,
            |value, context| value.field_requests(context),
            |value, context| value.field_requests(context).generic_context(),
        ).await
    }

    async fn variables(
        &self,
        context: GenericContext<'db>,
    ) -> RunResult<&'db ContextVariables<'db>> {
        self.retained_field(context,
            |value, context| value.field_requests(context),
            |value, context| value.variables_request(context),
        ).await
    }

    async fn identity(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<BoundTypeVarIdentity<'db>> {
        TypeVarSpecializationEffects::identity(self, variable).await
    }

    async fn index(
        &self,
        variables: &'db ContextVariables<'db>,
        identity: BoundTypeVarIdentity<'db>,
    ) -> RunResult<Option<usize>> {
        self.specialization_index(variables, identity).await
    }

    async fn types(&self, specialization: Specialization<'db>) -> RunResult<&'db [Type<'db>]> {
        let quote = generated_field_quote(
            |value: Specialization<'db>, context| value.field_requests(context),
            |value: Specialization<'db>, context| value.field_requests(context).types(),
        );
        let read = boxed_future_with_fixed_transfers_at(self.endpoint, quote, || {
            let request = specialization.field_requests(self.endpoint.field_request_context()).types();
            self.endpoint.read_field(request, &TypeSliceDeref)
        }).await?;
        Ok(read.await)
    }

    async fn type_at(&self, types: &'db [Type<'db>], index: usize) -> RunResult<Option<Type<'db>>> {
        self.local(3, size_of::<Type<'db>>() * 2, || {
            Ok(types.get(index).copied())
        })
        .await
    }
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>> SourceMapping<'_, 'owner, 'env, 'run, 'db, R> {
    /// Admits generated request construction and its native borrowed/copied read future together.
    async fn retained_field<H: Copy, A, Q: FieldRequest<'db>>(
        &self,
        handle: H,
        accessor: impl FnOnce(H, FieldRequestContext<'db>) -> A,
        request: impl Fn(H, FieldRequestContext<'db>) -> Q,
    ) -> RunResult<Q::Output> {
        let quote = generated_field_quote(accessor, |value, context| request(value, context));
        let read = boxed_future_with_fixed_transfers_at(self.endpoint, quote, move || {
            self.endpoint.read_field(request(handle, self.endpoint.field_request_context()), &BorrowOrCopy)
        }).await?;
        Ok(read.await)
    }
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>>
    RetainedSelfEffects<'db> for SourceMapping<'_, 'owner, 'env, 'run, 'db, R>
{
    type Error = RunError;

    async fn prepare(&self) -> RunResult<()> {
        self.function_local(Some(8), Some(4 * size_of::<Option<TypeVarBoundOrConstraintsEvaluation<'db>>>()), || ()).await
    }

    async fn binding_environment(&self, bound: BoundTypeVarInstance<'db>) -> RunResult<ProgramEnvironment<'db>> {
        let identity = TypeVarSpecializationEffects::identity(self, bound).await?;
        self.function_child(|| async { self.source.effects().retained_self_environment(identity.binding_context).await }).await
    }

    async fn typevar(&self, bound: BoundTypeVarInstance<'db>) -> RunResult<TypeVarInstance<'db>> {
        self.retained_field(bound, |value, context| value.field_requests(context), |value, context| value.field_requests(context).typevar()).await
    }

    async fn bounds(&self, variable: TypeVarInstance<'db>, env: &ProgramEnvironment<'db>) -> RunResult<Option<TypeVarBoundOrConstraints<'db>>> {
        self.function_child(|| async { self.source.effects().freshening_bounds(variable, env).await }).await
    }

    async fn map_bounds(&self, bounds: TypeVarBoundOrConstraints<'db>, specialization: Specialization<'db>, _env: &ProgramEnvironment<'db>) -> RunResult<TypeVarBoundOrConstraints<'db>> {
        self.function_child(|| async { self.source.effects().retained_self_bounds(bounds, specialization).await }).await
    }

    async fn identity(&self, variable: TypeVarInstance<'db>) -> RunResult<TypeVarIdentity<'db>> {
        self.retained_field(variable, |value, context| value.field_requests(context), |value, context| value.field_requests(context).identity()).await
    }

    async fn variance(&self, variable: TypeVarInstance<'db>) -> RunResult<Option<TypeVarVariance>> {
        self.retained_field(variable, |value, context| value.field_requests(context), |value, context| value.field_requests(context).explicit_variance()).await
    }

    async fn stored_default(&self, variable: TypeVarInstance<'db>) -> RunResult<Option<TypeVarDefaultEvaluation<'db>>> {
        self.retained_field(variable, |value, context| value.field_requests(context), |value, context| value.default_request(context)).await
    }

    async fn intern_variable(&self, identity: TypeVarIdentity<'db>, bounds: Option<TypeVarBoundOrConstraintsEvaluation<'db>>, variance: Option<TypeVarVariance>, default: Option<TypeVarDefaultEvaluation<'db>>) -> RunResult<TypeVarInstance<'db>> {
        self.function_child(|| async { self.source.effects().intern_freshening_variable(identity, bounds, variance, default).await }).await
    }

    async fn rebind(&self, variable: TypeVarInstance<'db>, original: BoundTypeVarInstance<'db>) -> RunResult<BoundTypeVarInstance<'db>> {
        let identity = TypeVarSpecializationEffects::identity(self, original).await?;
        self.function_child(|| async { self.source.effects().intern_freshening_bound(variable, identity).await }).await
    }
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>>
    RetainedBoundsEffects<'db> for SourceMapping<'_, 'owner, 'env, 'run, 'db, R>
{
    type Error = RunError;

    async fn prepare(&self) -> RunResult<()> {
        self.function_local(Some(5), Some(size_of::<usize>() + 2 * size_of::<TypeVarBoundOrConstraints<'db>>()), || ()).await
    }

    async fn map_type(&self, db: &'db dyn Db, ty: Type<'db>) -> RunResult<Type<'db>> {
        let descriptor = self.function_local(Some(2), Some(0), || self.mapping.into_mapping()).await?;
        self.function_child(|| SharedMappingEffects::map_type(self, db, ty, &descriptor, TypeContext::default(), self.visitor)).await
    }

    async fn elements(&self, constraints: TypeVarConstraints<'db>) -> RunResult<&'db [Type<'db>]> {
        let elements = self.retained_field(constraints, |value, context| value.field_requests(context), |value, context| value.field_requests(context).elements()).await?;
        self.function_local(Some(1), Some(0), || &**elements).await
    }

    async fn new_elements(&self, elements: &[Type<'db>]) -> RunResult<Vec<Type<'db>>> {
        let quote = self.function_local(Some(16), Some(8 * size_of::<usize>() + 8 * size_of::<Option<usize>>()), || {
            let len = elements.len();
            let bytes = std::alloc::Layout::array::<Type<'db>>(len).ok().map(|layout| layout.size());
            let work = len.checked_mul(2).and_then(|work| work.checked_add(4));
            work.zip(bytes).map(|(work, bytes)| RetainedConstraintStorage { len, work, bytes })
                .ok_or(RunError::Contract("retained Self constraint allocation quotation overflow"))
        }).await??;
        self.function_local(Some(quote.work), Some(quote.bytes), || Vec::with_capacity(quote.len)).await
    }

    async fn next(&self, elements: &[Type<'db>], cursor: &mut usize) -> RunResult<Option<Type<'db>>> {
        self.function_local(Some(5), Some(size_of::<Option<&Type<'db>>>()), || {
            let value = elements.get(*cursor).copied();
            if value.is_some() {
                *cursor += 1;
            }
            value
        }).await
    }

    async fn push(&self, elements: &mut Vec<Type<'db>>, ty: Type<'db>) -> RunResult<()> {
        self.function_local(Some(5), Some(size_of::<Type<'db>>()), || {
            if elements.len() == elements.capacity() {
                return Err(RunError::Contract("retained Self constraints exceeded their canonical length"));
            }
            elements.push(ty);
            Ok(())
        }).await?
    }

    async fn intern(&self, elements: &mut Vec<Type<'db>>) -> RunResult<TypeVarConstraints<'db>> {
        self.function_child(|| async { self.source.effects().intern_freshening_constraints(elements).await }).await
    }
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>> TypeVarSpecializationEffects<'db>
    for SourceMapping<'_, 'owner, 'env, 'run, 'db, R>
{
    type Error = RunError;

    async fn identity(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<BoundTypeVarIdentity<'db>> {
        let quote = generated_field_quote(
            |variable: BoundTypeVarInstance<'db>, context| variable.field_requests(context),
            |variable: BoundTypeVarInstance<'db>, context| variable.identity_request(context),
        );
        // Construct the native read future after admission. If field execution later refuses,
        // its request stays inside the future through child drainage. The box and concrete
        // future storage are admitted together.
        let read = boxed_future_with_fixed_transfers_at(self.endpoint, quote, || {
            let context = self.endpoint.field_request_context();
            let request = variable.identity_request(context);
            self.endpoint.read_field(request, &BorrowOrCopy)
        })
        .await?;
        Ok(read.await)
    }

    async fn kind(&self, identity: BoundTypeVarIdentity<'db>) -> RunResult<TypeVarKind> {
        let quote = generated_field_quote(
            |identity: BoundTypeVarIdentity<'db>, context| identity.identity.field_requests(context),
            |identity: BoundTypeVarIdentity<'db>, context| {
                identity.identity.field_requests(context).kind()
            },
        );
        let read = boxed_future_with_fixed_transfers_at(self.endpoint, quote, || {
            let context = self.endpoint.field_request_context();
            let fields = identity.identity.field_requests(context);
            let request = fields.kind();
            self.endpoint.read_field(request, &BorrowOrCopy)
        })
        .await?;
        Ok(read.await)
    }

    async fn without_paramspec_attr(
        &self,
        _variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<BoundTypeVarInstance<'db>> {
        self.unavailable(MaterializationOperation::Leaf(MappingOperation::ParamSpec))
            .await
    }

    async fn context_index(
        &self,
        context: GenericContext<'db>,
        identity: BoundTypeVarIdentity<'db>,
    ) -> RunResult<Option<usize>> {
        let variables = self
            .endpoint
            .read_field(
                context.variables_request(self.endpoint.field_request_context()),
                &BorrowOrCopy,
            )
            .await;
        self.specialization_index(variables, identity).await
    }

    async fn prefix_type(
        &self,
        types: TypeArgumentPrefix<'_, 'db>,
        index: usize,
    ) -> RunResult<Option<Type<'db>>> {
        self.local(size_of::<Type<'db>>() * 2 + 3, 0, || {
            types.checked_get(index).map_err(|_| {
                RunError::Contract("specialization prefix contains an uninitialized slot")
            })
        })
        .await
    }

    async fn other_lookup(
        &self,
        specialization: &ApplySpecialization<'_, 'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        if let ApplySpecialization::ReturnCallables(replacements) = specialization {
            let value = self.return_typevar_lookup(*replacements, variable).await?;
            return self.function_local(Some(2), Some(0), || value.map(Type::TypeVar)).await;
        }
        let source = self.function_local(Some(5), Some(0), || {
            SourceSpecialization::from_specialization(specialization)
        }).await?;
        match source {
            Some(source) => self.source_specialization_lookup(source, variable).await,
            None => self.unavailable(MaterializationOperation::Leaf(MappingOperation::MappingMode)).await,
        }
    }

    async fn with_paramspec_attr(
        &self,
        _variable: BoundTypeVarInstance<'db>,
        _attr: ParamSpecAttrKind,
    ) -> RunResult<BoundTypeVarInstance<'db>> {
        self.unavailable(MaterializationOperation::Leaf(MappingOperation::ParamSpec))
            .await
    }

    async fn specialize_self_domain(
        &self,
        specialization: &ApplySpecialization<'_, 'db>,
    ) -> RunResult<bool> {
        self.local(6, size_of::<ApplySpecialization<'_, 'db>>(), || match specialization {
            ApplySpecialization::Partial { .. }
            | ApplySpecialization::ReturnCallables(_)
            | ApplySpecialization::Specialization { .. }
            | ApplySpecialization::Single(..) => Ok(specialization.specialize_self_domain()),
            ApplySpecialization::TypeAlias(_)
            | ApplySpecialization::WithBindings { .. } => Err(RunError::Contract(
                "specialization mode changed during typevar mapping",
            )),
        })
        .await
    }

    async fn finish(
        &self,
        result: TypeVarSpecialization<'db>,
    ) -> RunResult<TypeVarSpecialization<'db>> {
        self.local(1, size_of::<TypeVarSpecialization<'db>>() * 2, || {
            Ok(result)
        })
        .await
    }
}
