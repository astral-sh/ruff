//! Admitted tuple state stays in its local owner until a child resumes or completion retires it.

use super::*;
use crate::types::infer::builder::source_expression::SourceExpressionEffects;
use crate::types::tuple::construction::tuple_type;
use crate::types::tuple::{TupleSpec, TupleSpecBuilder, TupleType};
use salsa::execution_probe::{
    ExecutionWork, FieldReadProfile, FieldReturnMode, NativeValueQuote, TaskEndpoint,
};
use tuple_annotation::Specification;

/// The exact tuple field is borrowed; quoting it never traverses or clones its elements.
#[derive(Debug)]
struct TupleSpecBorrow;

impl<'spec> FieldReadProfile<TupleSpec<'spec>> for TupleSpecBorrow {
    async fn quote<'call, 'run: 'call, 'db: 'run>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        _stored: &'call TupleSpec<'spec>,
        mode: FieldReturnMode,
    ) -> RunResult<NativeValueQuote> {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(2)?;
                endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: size_of::<NativeValueQuote>()
                        + size_of::<RunResult<NativeValueQuote>>(),
                })?;
                endpoint.check_completion()?;
                if mode != FieldReturnMode::Ref {
                    return Err(RunError::Contract(
                        "tuple specification field requires a borrowed result",
                    ));
                }
                Ok(NativeValueQuote {
                    work: 1,
                    requested_bytes: size_of::<&TupleSpec<'_>>(),
                    cleanup_work: 0,
                })
            })
            .await)
    }
}

/// Reserves a fixed tail and quotes relocation and eventual buffer retirement before allocation.
async fn reserve_elements<'run, 'db: 'run, A: SourceAccess<'run, 'db>>(
    effects: &SourceEffects<'_, 'run, 'db, A>,
    elements: &mut Vec<Type<'db>>,
    additional: usize,
) -> RunResult<()> {
    let needed =
        SourceEffects::<'_, 'run, 'db, A>::checked(elements.len().checked_add(additional))?;
    let growth = needed > elements.capacity();
    let relocated = if growth { elements.len() } else { 0 };
    let allocated = if growth { needed } else { 0 };
    let retired = if growth { elements.capacity() } else { 0 };
    let bytes = SourceEffects::<'_, 'run, 'db, A>::checked(
        allocated
            .checked_add(relocated)
            .and_then(|count| count.checked_mul(size_of::<Type<'db>>()))
            .and_then(|bytes| bytes.checked_add(size_of::<Vec<Type<'db>>>())),
    )?;
    let work = SourceEffects::<'_, 'run, 'db, A>::checked(
        relocated
            .checked_add(retired)
            .and_then(|work| work.checked_add(allocated))
            .and_then(|work| work.checked_add(4)),
    )?;
    effects
        .local(work, bytes, || {
            if growth {
                elements.reserve_exact(additional);
            }
        })
        .await
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Reads an expression's stored type-expression flags from the caller's inference builder.
    pub(in crate::types::infer::builder) async fn source_type_expression_flags(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        expression: &ast::Expr,
    ) -> RunResult<TypeExpressionFlags> {
        let work = Self::checked(
            builder
                .type_expression_flags
                .capacity()
                .checked_mul(4)
                .and_then(|work| work.checked_add(4)),
        )?;
        self.local(work, size_of::<TypeExpressionFlags>(), || {
            builder.type_expression_flags(expression)
        })
        .await
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>>
    tuple_annotation::TupleAnnotationEffects<'db, 'ast> for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn elements<'expr>(
        &self,
        request: tuple_annotation::Request<'expr>,
        elements: &'expr [ast::Expr],
    ) -> RunResult<tuple_annotation::Elements<'db, 'expr>> {
        let bytes = Self::checked(elements.len().checked_mul(size_of::<Type<'db>>()).and_then(
            |bytes| bytes.checked_add(size_of::<tuple_annotation::Elements<'db, 'expr>>()),
        ))?;
        let work = Self::checked(elements.len().checked_add(4))?;
        self.local(work, bytes, || tuple_annotation::Elements {
            request,
            remaining: elements,
            types: TupleSpecBuilder::with_capacity(elements.len()),
            first_variadic: None,
        })
        .await
    }

    async fn next_element<'expr>(
        &self,
        state: &mut tuple_annotation::Elements<'db, 'expr>,
    ) -> RunResult<Option<&'expr ast::Expr>> {
        self.local(
            3,
            size_of::<Option<&ast::Expr>>() + size_of::<&[ast::Expr]>(),
            || tuple_annotation::next_element(state),
        )
        .await
    }

    async fn enter_unpack(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>) -> RunResult<bool> {
        self.local(2, size_of::<bool>() + size_of::<InferenceFlags>(), || {
            builder
                .context
                .inference_flags
                .replace(InferenceFlags::IN_VALID_UNPACK_CONTEXT, true)
        })
        .await
    }

    async fn restore_unpack(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        previous: bool,
    ) -> RunResult<()> {
        self.local(2, size_of::<InferenceFlags>(), || {
            builder
                .context
                .inference_flags
                .set(InferenceFlags::IN_VALID_UNPACK_CONTEXT, previous)
        })
        .await
    }

    async fn flags(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> RunResult<TypeExpressionFlags> {
        self.boxed_future_with_fixed_transfers(
            Ok((
                11,
                size_of::<[(&Self, &TypeInferenceBuilder<'db, 'ast>, &ast::Expr); 2]>(),
            )),
            || self.source_type_expression_flags(builder, expression),
        )
        .await?
        .await
    }

    async fn is_unpack(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> RunResult<bool> {
        let work = Self::checked(
            builder
                .expressions
                .capacity()
                .checked_mul(4)
                .and_then(|work| work.checked_add(4)),
        )?;
        self.local(work, size_of::<Type<'db>>() + size_of::<bool>(), || {
            tuple_annotation::is_unpack(builder, expression)
        })
        .await
    }

    async fn exact_spec(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> RunResult<Option<&'db TupleSpec<'db>>> {
        let tuple = self
            .initialize_value(|| {
                ty.as_nominal_instance()
                    .and_then(|instance| instance.exact_tuple())
            })
            .await?;
        let Some(tuple) = tuple else {
            return self.initialize_value(|| None).await;
        };
        let spec = self
            .field_with_profile(
                tuple.field_requests(builder.db()).tuple(),
                &TupleSpecBorrow,
            )
            .await?;
        self.initialize_value(|| Some(spec)).await
    }

    async fn typevartuple(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        if let Type::TypeVar(_) = ty {
            return self
                .unavailable(SourceOperation::TypeExpressionSubscript)
                .await;
        }
        self.initialize_value(|| None).await
    }

    async fn invalid_ellipsis(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _expression: &ast::Expr,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::TypeExpressionInvalid)
            .await
    }

    async fn unpack_before_ellipsis(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _ellipsis: &ast::Expr,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::TypeExpressionInvalid)
            .await
    }

    async fn duplicate_unpack(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _subscript: &ast::ExprSubscript,
        _first: &ast::Expr,
        _later: &ast::Expr,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::TypeExpressionInvalid)
            .await
    }

    async fn remember_variadic<'expr>(
        &self,
        state: &mut tuple_annotation::Elements<'db, 'expr>,
        expression: &'expr ast::Expr,
    ) -> RunResult<()> {
        self.local(1, size_of::<Option<&ast::Expr>>(), || {
            state.first_variadic = Some(expression)
        })
        .await
    }

    async fn push(
        &self,
        state: &mut tuple_annotation::Elements<'db, '_>,
        ty: Type<'db>,
    ) -> RunResult<()> {
        let elements = match &mut state.types {
            TupleSpecBuilder::Fixed(elements) => elements,
            TupleSpecBuilder::Variable { suffix, .. } => suffix,
        };
        reserve_elements(self, elements, 1).await?;
        self.local(1, size_of::<Type<'db>>(), || elements.push(ty))
            .await
    }

    async fn concat(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        state: &mut tuple_annotation::Elements<'db, '_>,
        spec: &TupleSpec<'db>,
    ) -> RunResult<()> {
        let TupleSpec::Fixed(right) = spec else {
            // Variable specifications remain unavailable; merging two requires alias-preserving union construction.
            return self
                .unavailable(SourceOperation::TypeExpressionSubscript)
                .await;
        };
        let elements = match &mut state.types {
            TupleSpecBuilder::Fixed(elements) => elements,
            TupleSpecBuilder::Variable { suffix, .. } => suffix,
        };
        let right = right.elements_slice();
        reserve_elements(self, elements, right.len()).await?;
        let bytes = Self::checked(right.len().checked_mul(size_of::<Type<'db>>()))?;
        let work = Self::checked(right.len().checked_add(1))?;
        self.local(work, bytes, || elements.extend_from_slice(right))
            .await
    }

    async fn concat_typevar(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _state: &mut tuple_annotation::Elements<'db, '_>,
        _typevar: BoundTypeVarInstance<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::TypeExpressionSubscript)
            .await
    }

    async fn construct(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        specification: &mut Specification<'db>,
    ) -> RunResult<TupleType<'db>> {
        let spec = match specification {
            Specification::Borrowed(spec) => {
                return tuple_type(builder.db(), builder.program_environment(), spec, self).await;
            }
            Specification::Builder(types) => {
                let (length, capacity) = match types {
                    TupleSpecBuilder::Fixed(elements) => (elements.len(), elements.capacity()),
                    TupleSpecBuilder::Variable { .. } => {
                        return self
                            .unavailable(SourceOperation::TypeExpressionSubscript)
                            .await;
                    }
                };
                let replaces_buffer = length != capacity;
                let copied = if replaces_buffer { length } else { 0 };
                let retired = if replaces_buffer { capacity } else { 0 };
                let bytes = Self::checked(
                    copied
                        .checked_mul(2)
                        .and_then(|count| count.checked_mul(size_of::<Type<'db>>()))
                        .and_then(|bytes| {
                            bytes.checked_add(
                                size_of::<TupleSpec<'db>>() + size_of::<TupleSpecBuilder<'db>>(),
                            )
                        }),
                )?;
                let work = Self::checked(
                    copied
                        .checked_mul(2)
                        .and_then(|work| work.checked_add(retired))
                        .and_then(|work| work.checked_add(5)),
                )?;
                self.local(work, bytes, || {
                    std::mem::replace(types, TupleSpecBuilder::with_capacity(0)).build()
                })
                .await?
            }
            Specification::Homogeneous(ty) => {
                self.initialize_value(|| TupleSpec::homogeneous(*ty))
                    .await?
            }
            Specification::Single(ty) => {
                self.local(
                    5,
                    size_of::<TupleSpec<'db>>() + 2 * size_of::<Type<'db>>(),
                    || TupleSpec::heterogeneous([*ty]),
                )
                .await?
            }
            Specification::TypeVarTuple(_) => {
                return self
                    .unavailable(SourceOperation::TypeExpressionSubscript)
                    .await;
            }
        };
        tuple_type(builder.db(), builder.program_environment(), &spec, self).await
    }

    async fn instance(&self, tuple: TupleType<'db>) -> RunResult<Type<'db>> {
        self.initialize_value(|| Type::tuple(tuple)).await
    }

    async fn convert(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        tuple: TupleType<'db>,
        mode: tuple_annotation::ResultMode,
    ) -> RunResult<Type<'db>> {
        match mode {
            tuple_annotation::ResultMode::Instance => {
                self.initialize_value(|| Type::tuple(tuple)).await
            }
            tuple_annotation::ResultMode::Class | tuple_annotation::ResultMode::Subclass => {
                self.unavailable(SourceOperation::TypeExpressionSubscript)
                    .await
            }
        }
    }

    async fn store(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        ty: Type<'db>,
    ) -> RunResult<()> {
        self.store_expression(builder, expression, ty).await
    }
}

/// Fixed state transformations are quoted before a payload leaves its owner slot.
const fn transition_bytes() -> usize {
    size_of::<tuple_annotation::Phase<'_, '_>>()
        + size_of::<tuple_annotation::State<'_, '_>>()
        + size_of::<tuple_annotation::Pending<'_, '_>>()
        + size_of::<tuple_annotation::Action<'_, '_>>()
        + size_of::<tuple_annotation::Specification<'_>>()
        + size_of::<TupleSpec<'_>>()
        + size_of::<Type<'_>>()
}

pub(super) async fn start<'run, 'db: 'run, 'ast, 'expr, A: SourceAccess<'run, 'db>>(
    effects: &SourceEffects<'_, 'run, 'db, A>,
    owners: &mut LocalOwners<
        'db,
        'expr,
        <A::Resources as RelationResourceAccess<'run, 'db>>::Builder,
    >,
    _builders: &mut BuilderStore<'_, 'db, 'ast>,
    builder: BuilderId,
    request: tuple_annotation::Request<'expr>,
) -> RunResult<tuple_annotation::Active> {
    let grows = owners.slots.len() == owners.slots.capacity();
    let slot_size = size_of::<
        OwnerSlot<'db, 'expr, <A::Resources as RelationResourceAccess<'run, 'db>>::Builder>,
    >();
    let allocation = if grows {
        SourceEffects::<'_, 'run, 'db, A>::checked(owners.slots.len().checked_add(1))?
    } else {
        0
    };
    let relocated = if grows { owners.slots.len() } else { 0 };
    let bytes = SourceEffects::<'_, 'run, 'db, A>::checked(
        allocation
            .checked_add(relocated)
            .and_then(|count| count.checked_add(1))
            .and_then(|count| count.checked_mul(slot_size))
            .and_then(|bytes| bytes.checked_add(size_of::<tuple_annotation::Active>())),
    )?;
    let work = SourceEffects::<'_, 'run, 'db, A>::checked(if grows {
        owners.slots.len().checked_add(16)
    } else {
        Some(16)
    })?;
    effects
        .local(work, bytes, || {
            if grows {
                owners.slots.reserve_exact(1);
            }
            owners.push_tuple_annotation(builder, request)
        })
        .await
}

pub(super) async fn step<'run, 'db: 'run, 'ast, 'expr, A: SourceAccess<'run, 'db>>(
    effects: &SourceEffects<'_, 'run, 'db, A>,
    owner: tuple_annotation::Active,
    owners: &mut LocalOwners<
        'db,
        'expr,
        <A::Resources as RelationResourceAccess<'run, 'db>>::Builder,
    >,
    builders: &mut BuilderStore<'_, 'db, 'ast>,
) -> RunResult<tuple_annotation::Step<'expr>> {
    #[cfg(test)]
    tests::tuple_annotations::before_step(builders, owners, &owner);
    effects
        .allocate_future(|| async move {
            let taken = effects
                .local(
                    8,
                    size_of::<tuple_annotation::Taken<tuple_annotation::State<'db, 'expr>>>()
                        + transition_bytes(),
                    || owners.take_tuple_active(owner),
                )
                .await?;
            let tuple_annotation::Taken { index, payload } = taken;
            let tuple_annotation::Payload {
                builder,
                phase,
                #[cfg(test)]
                lifetime,
            } = payload;
            let phase = tuple_annotation::advance_with(
                phase,
                builders.get_mut(builder),
                tuple_annotation::Facts,
                effects,
            )
            .await?;
            let mut taken = Some(tuple_annotation::Taken {
                index,
                payload: tuple_annotation::Payload {
                    builder,
                    phase,
                    #[cfg(test)]
                    lifetime,
                },
            });
            effects
                .local(
                    4,
                    size_of::<
                        OwnerSlot<
                            'db,
                            'expr,
                            <A::Resources as RelationResourceAccess<'run, 'db>>::Builder,
                        >,
                    >() + size_of::<tuple_annotation::Step<'expr>>(),
                    || {
                        taken
                            .take()
                            .map(|taken| owners.install_tuple_action(taken))
                            .ok_or(RunError::Contract(
                                "tuple annotation action owner was consumed",
                            ))
                    },
                )
                .await?
        })
        .await?
        .await
}

pub(super) async fn resume<'run, 'db: 'run, 'ast, 'expr, A: SourceAccess<'run, 'db>>(
    effects: &SourceEffects<'_, 'run, 'db, A>,
    owner: tuple_annotation::Waiting,
    ty: Type<'db>,
    owners: &mut LocalOwners<
        'db,
        'expr,
        <A::Resources as RelationResourceAccess<'run, 'db>>::Builder,
    >,
    builders: &mut BuilderStore<'_, 'db, 'ast>,
) -> RunResult<tuple_annotation::Active> {
    effects
        .allocate_future(|| async move {
            let taken = effects
                .local(
                    8,
                    size_of::<tuple_annotation::Taken<tuple_annotation::Pending<'db, 'expr>>>()
                        + transition_bytes(),
                    || owners.take_tuple_pending(owner),
                )
                .await?;
            let tuple_annotation::Taken { index, payload } = taken;
            let tuple_annotation::Payload {
                builder,
                phase,
                #[cfg(test)]
                lifetime,
            } = payload;
            let phase = tuple_annotation::resume_with(
                phase,
                ty,
                builders.get_mut(builder),
                tuple_annotation::Facts,
                effects,
            )
            .await?;
            let mut taken = Some(tuple_annotation::Taken {
                index,
                payload: tuple_annotation::Payload {
                    builder,
                    phase,
                    #[cfg(test)]
                    lifetime,
                },
            });
            effects
                .local(
                    4,
                    size_of::<
                        OwnerSlot<
                            'db,
                            'expr,
                            <A::Resources as RelationResourceAccess<'run, 'db>>::Builder,
                        >,
                    >() + size_of::<tuple_annotation::Step<'expr>>(),
                    || {
                        taken
                            .take()
                            .map(|taken| owners.install_tuple_active(taken))
                            .ok_or(RunError::Contract(
                                "resumed tuple annotation owner was consumed",
                            ))
                    },
                )
                .await?
        })
        .await?
        .await
}

pub(super) async fn finish<'run, 'db: 'run, 'ast, 'expr, A: SourceAccess<'run, 'db>>(
    effects: &SourceEffects<'_, 'run, 'db, A>,
    builders: &mut BuilderStore<'_, 'db, 'ast>,
    owners: &mut LocalOwners<
        'db,
        'expr,
        <A::Resources as RelationResourceAccess<'run, 'db>>::Builder,
    >,
    owner: tuple_annotation::Finished,
) -> RunResult<Type<'db>> {
    effects
        .allocate_future(|| async move {
            let mut lease = effects
                .local(
                    4,
                    size_of::<
                        tuple_annotation::CompletionLease<
                            '_,
                            'db,
                            'expr,
                            <A::Resources as RelationResourceAccess<'run, 'db>>::Builder,
                            Infallible,
                        >,
                    >(),
                    || owners.tuple_completion(owner),
                )
                .await?;
            let index = lease.index;
            let (builder, completed) = lease.parts_mut();
            let ty = tuple_annotation::finish_with(
                completed,
                builders.get_mut(builder),
                tuple_annotation::Facts,
                effects,
            )
            .await?;
            drop(lease);
            effects
                .local(
                    4,
                    size_of::<FinishedOwner<'db>>()
                        + size_of::<Type<'db>>()
                        + size_of::<
                            OwnerSlot<
                                'db,
                                'expr,
                                <A::Resources as RelationResourceAccess<'run, 'db>>::Builder,
                            >,
                        >(),
                    || owners.retire(FinishedOwner { index, ty }),
                )
                .await
        })
        .await?
        .await
}
