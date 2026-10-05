//! Freshening reuses a retained mapping visitor for bounds, constraints, and bound defaults.

use std::slice;

use salsa::execution_probe::{
    FieldRequest, FieldRequestContext, RunError, RunResult, TaskEndpoint,
};

use super::{FixedMappingField, MappingSourceEffects, RetainedMappingSource, SourceMapping};
use crate::Db;
use crate::types::constraints::control::hash_slots;
use crate::types::mapping::effects::SharedMappingEffects;
use crate::types::typevar::freshening::{
    TypeVarFresheningEffects, TypeVarFresheningFacts, freshen_bound_typevar_with,
};
use crate::types::typevar::{
    BoundTypeVarIdentity, BoundTypeVarInstance, TypeVarBoundOrConstraints,
    TypeVarBoundOrConstraintsEvaluation, TypeVarConstraints, TypeVarDefaultEvaluation,
    TypeVarIdentity, TypeVarInstance, TypeVarKind,
};
use crate::types::{GenericContext, Type, TypeContext, TypeVarVariance};

/// Borrows the current root while supplying the database to recursive mapping children.
struct SourceTypeVarFreshening<'a, 'db, M> {
    db: &'db dyn Db,
    mapping: &'a M,
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>>
    SourceMapping<'_, 'owner, 'env, 'run, 'db, R>
{
    /// Freshens an occurrence using the root's descriptor and retained recursive visitor.
    pub(super) async fn freshen_typevar(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
        generic_context: GenericContext<'db>,
        delta: u32,
    ) -> RunResult<BoundTypeVarInstance<'db>> {
        let effects = self
            .function_local(Some(2), Some(0), || SourceTypeVarFreshening {
                db,
                mapping: self,
            })
            .await?;
        self.function_child(|| {
            freshen_bound_typevar_with(
                variable,
                generic_context,
                delta,
                TypeVarFresheningFacts,
                &effects,
            )
        })
        .await
    }
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>>
    SourceTypeVarFreshening<'_, 'db, SourceMapping<'_, 'owner, 'env, 'run, 'db, R>>
{
    /// Admits generated accessors and request carriers before constructing a canonical field read.
    /// The unused accessor factory identifies the generated accessor type whose representation is charged;
    /// request construction runs only after admission. The inner field-read future retains the request
    /// through child drainage before this helper returns the field value.
    async fn field<H: Copy, A, Q: FieldRequest<'db>>(
        &self,
        handle: H,
        _accessor: impl FnOnce(H, FieldRequestContext<'db>) -> A,
        request: impl FnOnce(H, FieldRequestContext<'db>) -> Q,
    ) -> RunResult<Q::Output> {
        let bytes = size_of::<A>()
            .checked_mul(3)
            .and_then(|bytes| bytes.checked_add(size_of::<FieldRequestContext<'db>>() * 8))
            .and_then(|bytes| bytes.checked_add(size_of::<H>().checked_mul(2)?))
            .and_then(|bytes| bytes.checked_add(size_of::<Q>().checked_mul(8)?))
            .and_then(|bytes| bytes.checked_add(size_of::<Q::Output>().checked_mul(2)?))
            .and_then(|bytes| bytes.checked_add(size_of::<RunResult<Q::Output>>().checked_mul(2)?))
            .and_then(|bytes| {
                bytes.checked_add(
                    size_of::<usize>() * 32
                        + size_of::<Option<usize>>() * 32
                        + size_of::<&TaskEndpoint<'run, 'db>>() * 4,
                )
            });
        let read = self
            .mapping
            .function_local(Some(58), bytes, || {
                let context = self.mapping.endpoint.field_request_context();
                self.mapping
                    .endpoint
                    .read_field(request(handle, context), &FixedMappingField)
            })
            .await?;
        Ok(read.await)
    }
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>>
    TypeVarFresheningEffects<'db>
    for SourceTypeVarFreshening<'_, 'db, SourceMapping<'_, 'owner, 'env, 'run, 'db, R>>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        // These are fixed decisions: identity extraction, ParamSpec normalization and skip,
        // nonce addition, bounds/default dispatch, eager wrappers, and cursor construction.
        self.mapping
            .function_local(
                Some(32),
                Some(
                    size_of::<BoundTypeVarIdentity<'db>>() * 3
                        + size_of::<slice::Iter<'db, Type<'db>>>() * 2,
                ),
                || (),
            )
            .await
    }

    async fn bound_identity(
        &self,
        bound: BoundTypeVarInstance<'db>,
    ) -> RunResult<BoundTypeVarIdentity<'db>> {
        self.field(
            bound,
            |bound, context| bound.field_requests(context),
            |bound, context| bound.identity_request(context),
        )
        .await
    }

    async fn kind(&self, identity: TypeVarIdentity<'db>) -> RunResult<TypeVarKind> {
        self.field(
            identity,
            |identity, context| identity.field_requests(context),
            |identity, context| identity.field_requests(context).kind(),
        )
        .await
    }

    async fn contains(
        &self,
        context: GenericContext<'db>,
        identity: BoundTypeVarIdentity<'db>,
    ) -> RunResult<bool> {
        let variables = self
            .field(
                context,
                |context, fields| context.field_requests(fields),
                |context, fields| context.variables_request(fields),
            )
            .await?;
        let (len, capacity) = self
            .mapping
            .function_local(Some(2), Some(0), || (variables.len(), variables.capacity()))
            .await?;
        let work = hash_slots::<RunError>(capacity)
            .ok()
            .and_then(|slots| slots.checked_mul(4))
            .and_then(|work| work.checked_add(len.checked_mul(12)?))
            .and_then(|work| work.checked_add(24));
        self.mapping
            .function_local(work, Some(size_of::<u64>() * 8), || {
                variables.contains_key(&identity)
            })
            .await
    }

    async fn typevar(&self, bound: BoundTypeVarInstance<'db>) -> RunResult<TypeVarInstance<'db>> {
        self.field(
            bound,
            |bound, context| bound.field_requests(context),
            |bound, context| bound.field_requests(context).typevar(),
        )
        .await
    }

    async fn bounds(
        &self,
        typevar: TypeVarInstance<'db>,
    ) -> RunResult<Option<TypeVarBoundOrConstraints<'db>>> {
        self.mapping
            .function_child(|| async {
                self.mapping
                    .source
                    .effects()
                    .freshening_bounds(typevar, self.mapping.visitor.env)
                    .await
            })
            .await
    }

    async fn bound_default(
        &self,
        bound: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.mapping
            .function_child(|| async {
                self.mapping
                    .source
                    .effects()
                    .freshening_bound_default(bound)
                    .await
            })
            .await
    }

    async fn identity(&self, typevar: TypeVarInstance<'db>) -> RunResult<TypeVarIdentity<'db>> {
        self.field(
            typevar,
            |typevar, context| typevar.field_requests(context),
            |typevar, context| typevar.field_requests(context).identity(),
        )
        .await
    }

    async fn variance(&self, typevar: TypeVarInstance<'db>) -> RunResult<Option<TypeVarVariance>> {
        self.field(
            typevar,
            |typevar, context| typevar.field_requests(context),
            |typevar, context| typevar.field_requests(context).explicit_variance(),
        )
        .await
    }

    async fn map_type(&self, ty: Type<'db>) -> RunResult<Type<'db>> {
        let descriptor = self
            .mapping
            .function_local(Some(2), Some(0), || self.mapping.mapping.into_mapping())
            .await?;
        self.mapping
            .function_child(|| {
                SharedMappingEffects::map_type(
                    self.mapping,
                    self.db,
                    ty,
                    &descriptor,
                    TypeContext::default(),
                    self.mapping.visitor,
                )
            })
            .await
    }

    async fn constraint_elements(
        &self,
        constraints: TypeVarConstraints<'db>,
    ) -> RunResult<&'db [Type<'db>]> {
        let elements = self
            .field(
                constraints,
                |constraints, context| constraints.field_requests(context),
                |constraints, context| constraints.field_requests(context).elements(),
            )
            .await?;
        self.mapping
            .function_local(Some(1), Some(0), || &**elements)
            .await
    }

    async fn new_constraints(&self, elements: &[Type<'db>]) -> RunResult<Vec<Type<'db>>> {
        let len = self
            .mapping
            .function_local(Some(1), Some(0), || elements.len())
            .await?;
        let bytes = std::alloc::Layout::array::<Type<'db>>(len)
            .ok()
            .map(|layout| layout.size());
        let work = len.checked_mul(2).and_then(|work| work.checked_add(4));
        self.mapping
            .function_local(work, bytes, || Vec::with_capacity(len))
            .await
    }

    async fn next_constraint(
        &self,
        elements: &mut slice::Iter<'db, Type<'db>>,
    ) -> RunResult<Option<Type<'db>>> {
        self.mapping
            .function_local(Some(3), Some(0), || elements.next().copied())
            .await
    }

    async fn push_constraint(&self, elements: &mut Vec<Type<'db>>, ty: Type<'db>) -> RunResult<()> {
        self.mapping
            .function_local(Some(3), Some(0), || {
                if elements.len() == elements.capacity() {
                    return Err(RunError::Contract(
                        "freshening constraint buffer exceeded its canonical length",
                    ));
                }
                elements.push(ty);
                Ok(())
            })
            .await?
    }

    async fn intern_constraints(
        &self,
        elements: &mut Vec<Type<'db>>,
    ) -> RunResult<TypeVarConstraints<'db>> {
        self.mapping
            .function_child(|| async {
                self.mapping
                    .source
                    .effects()
                    .intern_freshening_constraints(elements)
                    .await
            })
            .await
    }

    async fn intern_variable(
        &self,
        identity: TypeVarIdentity<'db>,
        bounds: Option<TypeVarBoundOrConstraintsEvaluation<'db>>,
        variance: Option<TypeVarVariance>,
        default: Option<TypeVarDefaultEvaluation<'db>>,
    ) -> RunResult<TypeVarInstance<'db>> {
        self.mapping
            .function_child(|| async {
                self.mapping
                    .source
                    .effects()
                    .intern_freshening_variable(identity, bounds, variance, default)
                    .await
            })
            .await
    }

    async fn intern_bound(
        &self,
        typevar: TypeVarInstance<'db>,
        identity: BoundTypeVarIdentity<'db>,
    ) -> RunResult<BoundTypeVarInstance<'db>> {
        self.mapping
            .function_child(|| async {
                self.mapping
                    .source
                    .effects()
                    .intern_freshening_bound(typevar, identity)
                    .await
            })
            .await
    }

    async fn finish(
        &self,
        bound: BoundTypeVarInstance<'db>,
    ) -> RunResult<BoundTypeVarInstance<'db>> {
        self.mapping
            .function_local(Some(2), Some(0), || bound)
            .await
    }
}
