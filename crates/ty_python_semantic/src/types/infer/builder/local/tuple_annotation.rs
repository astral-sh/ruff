//! Tuple annotations suspend their children in the existing local inference invocation.

use super::*;
use crate::types::SubclassOfType;
use crate::types::tuple::{TupleSpec, TupleSpecBuilder, TupleType};

/// Selects the outer result while the inner slice always describes tuple instances.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types::infer::builder) enum ResultMode {
    Instance,
    Class,
    Subclass,
}

/// A tuple subscript together with the type its enclosing expression needs.
#[derive(Clone, Copy, Debug)]
pub(in crate::types::infer) struct Request<'expr> {
    pub(in crate::types::infer::builder) subscript: &'expr ast::ExprSubscript,
    pub(in crate::types::infer::builder) mode: ResultMode,
}

/// Retains the growing specification while exactly one element is inferred.
pub(super) struct Elements<'db, 'expr> {
    pub(super) request: Request<'expr>,
    pub(super) remaining: &'expr [ast::Expr],
    pub(super) types: TupleSpecBuilder<'db>,
    pub(super) first_variadic: Option<&'expr ast::Expr>,
}

pub(super) enum State<'db, 'expr> {
    Start(Request<'expr>),
    Elements(Elements<'db, 'expr>),
    Homogeneous {
        request: Request<'expr>,
        element: &'expr ast::Expr,
        ellipsis: &'expr ast::Expr,
    },
    Complete(Completed<'db, 'expr>),
}

pub(super) enum Target<'db, 'expr> {
    Elements(Elements<'db, 'expr>),
    Homogeneous {
        request: Request<'expr>,
        ellipsis: &'expr ast::Expr,
    },
    Single(Request<'expr>),
}

pub(super) enum Pending<'db, 'expr> {
    Ellipsis {
        request: Request<'expr>,
        element: &'expr ast::Expr,
        ellipsis: &'expr ast::Expr,
    },
    Element {
        target: Target<'db, 'expr>,
        expression: &'expr ast::Expr,
        saved_unpack: bool,
    },
}

/// Single and homogeneous annotations need no temporary fixed-element buffer.
pub(super) enum Specification<'db> {
    Builder(TupleSpecBuilder<'db>),
    Borrowed(&'db TupleSpec<'db>),
    Homogeneous(Type<'db>),
    Single(Type<'db>),
    TypeVarTuple(BoundTypeVarInstance<'db>),
}

pub(super) struct Completed<'db, 'expr> {
    pub(super) request: Request<'expr>,
    pub(super) specification: Specification<'db>,
}

pub(super) enum Action<'db, 'expr> {
    InferValue(Pending<'db, 'expr>, &'expr ast::Expr),
    InferType(Pending<'db, 'expr>, &'expr ast::Expr),
    Continue(State<'db, 'expr>),
    Complete(Completed<'db, 'expr>),
}

#[derive(Debug)]
pub(super) struct Active(pub(super) usize);
#[derive(Debug)]
pub(super) struct Waiting(pub(super) usize);
#[derive(Debug)]
pub(super) struct Finished(pub(super) usize);

pub(super) enum Phase<'db, 'expr> {
    Active(State<'db, 'expr>),
    Pending(Pending<'db, 'expr>),
    Completed(Completed<'db, 'expr>),
}

pub(super) struct Payload<P> {
    pub(super) builder: BuilderId,
    pub(super) phase: P,
    #[cfg(test)]
    pub(super) lifetime: Option<OwnerLifetime>,
}

impl<P> Payload<P> {
    pub(super) fn map<Q>(self, transition: impl FnOnce(P) -> Q) -> Payload<Q> {
        Payload {
            builder: self.builder,
            phase: transition(self.phase),
            #[cfg(test)]
            lifetime: self.lifetime,
        }
    }
}

pub(super) struct Taken<P> {
    pub(super) index: usize,
    pub(super) payload: Payload<P>,
}

impl<P> Taken<P> {
    pub(super) fn map<Q>(self, transition: impl FnOnce(P) -> Q) -> Taken<Q> {
        Taken {
            index: self.index,
            payload: self.payload.map(transition),
        }
    }
}

pub(super) enum Step<'expr> {
    InferType {
        owner: Waiting,
        builder: BuilderId,
        expression: &'expr ast::Expr,
    },
    InferValue {
        owner: Waiting,
        builder: BuilderId,
        expression: &'expr ast::Expr,
    },
    Continue(Active),
    Complete(Finished),
}

/// Keeps completed storage in its slot while construction suspends; dropping the lease retires it.
pub(super) struct CompletionLease<'owner, 'db, 'expr, B, T> {
    pub(super) index: usize,
    slot: &'owner mut OwnerSlot<'db, 'expr, B, T>,
}

impl<'db, 'expr, B, T> CompletionLease<'_, 'db, 'expr, B, T> {
    pub(super) fn parts_mut(&mut self) -> (BuilderId, &mut Completed<'db, 'expr>) {
        let OwnerSlot::TupleAnnotation(Payload {
            builder,
            phase: Phase::Completed(completed),
            ..
        }) = self.slot
        else {
            panic!("completed tuple annotation token does not identify a completed owner");
        };
        (*builder, completed)
    }
}

impl<B, T> Drop for CompletionLease<'_, '_, '_, B, T> {
    fn drop(&mut self) {
        *self.slot = OwnerSlot::Taken;
    }
}

impl<'db, 'expr, B, T> LocalOwners<'db, 'expr, B, T> {
    pub(super) fn push_tuple_annotation(
        &mut self,
        builder: BuilderId,
        request: Request<'expr>,
    ) -> Active {
        let index = self.slots.len();
        #[cfg(test)]
        let identity = self.next_identity;
        #[cfg(test)]
        {
            self.next_identity += 1;
        }
        self.slots.push(OwnerSlot::TupleAnnotation(Payload {
            builder,
            phase: Phase::Active(State::Start(request)),
            #[cfg(test)]
            lifetime: self.observer.as_ref().map(|observer| OwnerLifetime {
                identity,
                observer: observer.clone(),
            }),
        }));
        #[cfg(test)]
        if let Some(observer) = &self.observer {
            observer(OwnershipEvent::Created {
                kind: OwnerKind::TupleAnnotation,
                call: request.subscript.range(),
                index,
                identity,
                len: self.slots.len(),
                capacity: self.slots.capacity(),
            });
        }
        Active(index)
    }

    fn take_tuple_annotation(&mut self, index: usize) -> Payload<Phase<'db, 'expr>> {
        assert_eq!(index + 1, self.slots.len());
        let OwnerSlot::TupleAnnotation(payload) =
            std::mem::replace(&mut self.slots[index], OwnerSlot::Taken)
        else {
            panic!("tuple annotation token does not identify an initialized owner");
        };
        #[cfg(test)]
        if let Some(lifetime) = &payload.lifetime {
            (lifetime.observer)(OwnershipEvent::Taken {
                index,
                identity: lifetime.identity,
            });
        }
        payload
    }

    pub(super) fn take_tuple_active(&mut self, owner: Active) -> Taken<State<'db, 'expr>> {
        Taken {
            index: owner.0,
            payload: self.take_tuple_annotation(owner.0).map(|phase| {
                let Phase::Active(state) = phase else {
                    panic!("active tuple annotation token does not identify an active owner");
                };
                state
            }),
        }
    }

    pub(super) fn take_tuple_pending(&mut self, owner: Waiting) -> Taken<Pending<'db, 'expr>> {
        Taken {
            index: owner.0,
            payload: self.take_tuple_annotation(owner.0).map(|phase| {
                let Phase::Pending(pending) = phase else {
                    panic!("pending tuple annotation token does not identify a pending owner");
                };
                pending
            }),
        }
    }

    fn install_tuple_annotation(&mut self, taken: Taken<Phase<'db, 'expr>>) {
        assert_eq!(taken.index + 1, self.slots.len());
        assert!(matches!(self.slots[taken.index], OwnerSlot::Taken));
        #[cfg(test)]
        let observation = taken.payload.lifetime.as_ref().map(|lifetime| {
            (
                lifetime.identity,
                match &taken.payload.phase {
                    Phase::Active(_) => ObservedOwnerPhase::Active,
                    Phase::Pending(_) => ObservedOwnerPhase::Pending,
                    Phase::Completed(_) => ObservedOwnerPhase::Completed,
                },
            )
        });
        self.slots[taken.index] = OwnerSlot::TupleAnnotation(taken.payload);
        #[cfg(test)]
        if let Some(observer) = &self.observer
            && let Some((identity, phase)) = observation
        {
            observer(OwnershipEvent::Installed {
                index: taken.index,
                identity,
                phase,
            });
        }
    }

    pub(super) fn install_tuple_active(&mut self, taken: Taken<State<'db, 'expr>>) -> Active {
        let owner = Active(taken.index);
        self.install_tuple_annotation(taken.map(Phase::Active));
        owner
    }

    pub(super) fn install_tuple_action(&mut self, taken: Taken<Action<'db, 'expr>>) -> Step<'expr> {
        let Taken { index, payload } = taken;
        let Payload {
            builder,
            phase,
            #[cfg(test)]
            lifetime,
        } = payload;
        let (phase, step) = match phase {
            Action::InferType(pending, expression) => (
                Phase::Pending(pending),
                Step::InferType {
                    owner: Waiting(index),
                    builder,
                    expression,
                },
            ),
            Action::InferValue(pending, expression) => (
                Phase::Pending(pending),
                Step::InferValue {
                    owner: Waiting(index),
                    builder,
                    expression,
                },
            ),
            Action::Continue(state) => (Phase::Active(state), Step::Continue(Active(index))),
            Action::Complete(completed) => {
                (Phase::Completed(completed), Step::Complete(Finished(index)))
            }
        };
        self.install_tuple_annotation(Taken {
            index,
            payload: Payload {
                builder,
                phase,
                #[cfg(test)]
                lifetime,
            },
        });
        step
    }

    pub(super) fn tuple_completion(
        &mut self,
        owner: Finished,
    ) -> CompletionLease<'_, 'db, 'expr, B, T> {
        assert_eq!(owner.0 + 1, self.slots.len());
        let slot = &mut self.slots[owner.0];
        #[cfg(test)]
        if let OwnerSlot::TupleAnnotation(Payload {
            lifetime: Some(lifetime),
            ..
        }) = &slot
        {
            (lifetime.observer)(OwnershipEvent::Taken {
                index: owner.0,
                identity: lifetime.identity,
            });
        }
        CompletionLease {
            index: owner.0,
            slot,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Facts;
#[derive(Clone, Copy, Debug)]
pub(super) struct OrdinaryEffects;

shared_semantic_family! {
    #[synchronous(SynchronousTupleAnnotationEffects)]
    pub(super) trait TupleAnnotationEffects<'db, 'ast> {
        type Error;
        #[operation(local)]
        async fn elements<'expr>(&self, request: Request<'expr>, elements: &'expr [ast::Expr]) -> Result<Elements<'db, 'expr>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_element<'expr>(&self, state: &mut Elements<'db, 'expr>) -> Result<Option<&'expr ast::Expr>, Self::Error>;
        #[operation(local)]
        async fn enter_unpack(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn restore_unpack(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, previous: bool) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn flags(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<TypeExpressionFlags, Self::Error>;
        #[operation(local)]
        async fn is_unpack(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn exact_spec(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<Option<&'db TupleSpec<'db>>, Self::Error>;
        #[operation(source)]
        async fn typevartuple(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(source)]
        async fn invalid_ellipsis(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn unpack_before_ellipsis(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ellipsis: &ast::Expr) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn duplicate_unpack(&self, builder: &TypeInferenceBuilder<'db, 'ast>, subscript: &ast::ExprSubscript, first: &ast::Expr, later: &ast::Expr) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn remember_variadic<'expr>(&self, state: &mut Elements<'db, 'expr>, expression: &'expr ast::Expr) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn push(&self, state: &mut Elements<'db, '_>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn concat(&self, builder: &TypeInferenceBuilder<'db, 'ast>, state: &mut Elements<'db, '_>, spec: &TupleSpec<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn concat_typevar(&self, builder: &TypeInferenceBuilder<'db, 'ast>, state: &mut Elements<'db, '_>, typevar: BoundTypeVarInstance<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn construct(&self, builder: &TypeInferenceBuilder<'db, 'ast>, specification: &mut Specification<'db>) -> Result<TupleType<'db>, Self::Error>;
        #[operation(local)]
        async fn instance(&self, tuple: TupleType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn convert(&self, builder: &TypeInferenceBuilder<'db, 'ast>, tuple: TupleType<'db>, mode: ResultMode) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn store(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, ty: Type<'db>) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl Facts {
        fn slice<'expr>(&self, request: Request<'expr>) -> &'expr ast::Expr { &request.subscript.slice }
        fn elements<'expr>(&self, tuple: &'expr ast::ExprTuple) -> &'expr [ast::Expr] { &tuple.elts }
        fn unknown<'db>(&self) -> Type<'db> { Type::unknown() }
        fn bare_typevartuple(&self, flags: TypeExpressionFlags) -> bool { flags.contains(TypeExpressionFlags::INVALID_BARE_TYPE_VAR_TUPLE) }
        fn unpack(&self, flags: TypeExpressionFlags) -> bool { flags.contains(TypeExpressionFlags::UNPACK) }
        fn ellipsis(&self, expression: &ast::Expr) -> bool { expression.is_ellipsis_literal_expr() }
        fn variadic(&self, spec: &TupleSpec<'_>) -> bool { spec.is_variadic() }
        fn first_variadic<'expr>(&self, state: &Elements<'_, 'expr>) -> Option<&'expr ast::Expr> { state.first_variadic }
        fn subscript<'expr>(&self, state: &Elements<'_, 'expr>) -> &'expr ast::ExprSubscript { state.request.subscript }
        fn homogeneous_unknown<'db>(&self) -> TupleSpec<'db> { TupleSpec::homogeneous(Type::unknown()) }
        fn stores_slice(&self, request: Request<'_>) -> bool { matches!(request.subscript.slice.as_ref(), ast::Expr::Tuple(_)) }
    }

    /// Select the next child, or finish after every source element has been handled.
    #[synchronous(advance_sync)]
    #[capabilities(effects = TupleAnnotationEffects, facts = Facts)]
    #[passive_values(Action::InferValue, Action::InferType, Action::Continue, Action::Complete, State::Elements, Pending::Ellipsis, Pending::Element, Target::Single, Target::Elements, Target::Homogeneous, Specification::Single, Specification::Builder, Completed)]
    pub(super) async fn advance_with<'db, 'ast, 'expr, E: TupleAnnotationEffects<'db, 'ast>>(
        state: State<'db, 'expr>, builder: &mut TypeInferenceBuilder<'db, 'ast>, facts: Facts, effects: &E,
    ) -> Result<Action<'db, 'expr>, E::Error> {
        match state {
            State::Start(request) => {
                match facts.slice(request) {
                    ast::Expr::Tuple(elements) => {
                        if let [element, ellipsis @ ast::Expr::EllipsisLiteral(_)] = facts.elements(elements) {
                            return Ok(Action::InferValue(Pending::Ellipsis { request, element, ellipsis }, ellipsis));
                        }
                        let state = effects.elements(request, facts.elements(elements)).await?;
                        Ok(Action::Continue(State::Elements(state)))
                    }
                    expression => {
                        if facts.ellipsis(expression) {
                            effects.invalid_ellipsis(builder, expression).await?;
                            effects.store(builder, expression, facts.unknown()).await?;
                            return Ok(Action::Complete(Completed { request, specification: Specification::Single(facts.unknown()) }));
                        }
                        let saved_unpack = effects.enter_unpack(builder).await?;
                        Ok(Action::InferType(Pending::Element { target: Target::Single(request), expression, saved_unpack }, expression))
                    }
                }
            }
            State::Homogeneous { request, element, ellipsis } => {
                let saved_unpack = effects.enter_unpack(builder).await?;
                Ok(Action::InferType(Pending::Element { target: Target::Homogeneous { request, ellipsis }, expression: element, saved_unpack }, element))
            }
            State::Elements(mut state) => {
                if let Some(expression) = effects.next_element(&mut state).await? {
                    if facts.ellipsis(expression) {
                        effects.invalid_ellipsis(builder, expression).await?;
                        effects.store(builder, expression, facts.unknown()).await?;
                        effects.push(&mut state, facts.unknown()).await?;
                        return Ok(Action::Continue(State::Elements(state)));
                    }
                    let saved_unpack = effects.enter_unpack(builder).await?;
                    return Ok(Action::InferType(Pending::Element { target: Target::Elements(state), expression, saved_unpack }, expression));
                }
                let Elements { request, types, .. } = state;
                Ok(Action::Complete(Completed { request, specification: Specification::Builder(types) }))
            }
            State::Complete(completed) => Ok(Action::Complete(completed)),
        }
    }

    /// Incorporate a child, restoring an element's unpack context before examining its result.
    #[synchronous(resume_sync)]
    #[capabilities(effects = TupleAnnotationEffects, facts = Facts)]
    #[passive_values(State::Homogeneous, State::Elements, State::Complete, Completed, Specification::Homogeneous, Specification::Single, Specification::Borrowed, Specification::TypeVarTuple)]
    pub(super) async fn resume_with<'db, 'ast, 'expr, E: TupleAnnotationEffects<'db, 'ast>>(
        pending: Pending<'db, 'expr>, ty: Type<'db>, builder: &mut TypeInferenceBuilder<'db, 'ast>, facts: Facts, effects: &E,
    ) -> Result<State<'db, 'expr>, E::Error> {
        match pending {
            Pending::Ellipsis { request, element, ellipsis } => Ok(State::Homogeneous { request, element, ellipsis }),
            Pending::Element { target, expression, saved_unpack } => {
                effects.restore_unpack(builder, saved_unpack).await?;
                match target {
                    Target::Homogeneous { request, ellipsis } => {
                        let flags = effects.flags(builder, expression).await?;
                        if facts.unpack(flags) { effects.unpack_before_ellipsis(builder, ellipsis).await?; }
                        Ok(State::Complete(Completed { request, specification: Specification::Homogeneous(ty) }))
                    }
                    Target::Single(request) => {
                        let flags = effects.flags(builder, expression).await?;
                        if facts.bare_typevartuple(flags) {
                            return Ok(State::Complete(Completed { request, specification: Specification::Homogeneous(facts.unknown()) }));
                        }
                        if effects.is_unpack(builder, expression).await? {
                            if let Some(spec) = effects.exact_spec(builder, ty).await? {
                                return Ok(State::Complete(Completed { request, specification: Specification::Borrowed(spec) }));
                            }
                            if let Some(typevar) = effects.typevartuple(builder, ty).await? {
                                return Ok(State::Complete(Completed { request, specification: Specification::TypeVarTuple(typevar) }));
                            }
                        }
                        Ok(State::Complete(Completed { request, specification: Specification::Single(ty) }))
                    }
                    Target::Elements(mut state) => {
                        if effects.is_unpack(builder, expression).await? {
                            if let Some(spec) = effects.exact_spec(builder, ty).await? {
                                effects.concat(builder, &mut state, spec).await?;
                                if facts.variadic(spec) {
                                    if let Some(first) = facts.first_variadic(&state) {
                                        effects.duplicate_unpack(builder, facts.subscript(&state), first, expression).await?;
                                    } else { effects.remember_variadic(&mut state, expression).await?; }
                                }
                            } else if let Some(typevar) = effects.typevartuple(builder, ty).await? {
                                if let Some(first) = facts.first_variadic(&state) {
                                    effects.duplicate_unpack(builder, facts.subscript(&state), first, expression).await?;
                                } else { effects.remember_variadic(&mut state, expression).await?; }
                                effects.concat_typevar(builder, &mut state, typevar).await?;
                            }
                            // TODO: emit a diagnostic
                        } else {
                            let flags = effects.flags(builder, expression).await?;
                            if facts.bare_typevartuple(flags) {
                                // Do not count recovery as another explicit unpack.
                                effects.concat(builder, &mut state, &facts.homogeneous_unknown()).await?;
                            } else { effects.push(&mut state, ty).await?; }
                        }
                        Ok(State::Elements(state))
                    }
                }
            }
        }
    }

    /// Construct the canonical tuple, store any inner tuple slice, then convert the outer result.
    #[synchronous(finish_sync)]
    #[capabilities(effects = TupleAnnotationEffects, facts = Facts)]
    #[passive_values()]
    pub(super) async fn finish_with<'db, 'ast, E: TupleAnnotationEffects<'db, 'ast>>(
        completed: &mut Completed<'db, '_>, builder: &mut TypeInferenceBuilder<'db, 'ast>, facts: Facts, effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        let tuple = effects.construct(builder, &mut completed.specification).await?;
        if facts.stores_slice(completed.request) {
            let instance = effects.instance(tuple).await?;
            // Store the inner `int, str` slice as an instance even when the outer result is a class.
            effects.store(builder, facts.slice(completed.request), instance).await?;
        }
        effects.convert(builder, tuple, completed.request.mode).await
    }
}

pub(super) fn next_element<'expr>(state: &mut Elements<'_, 'expr>) -> Option<&'expr ast::Expr> {
    let (element, remaining) = state.remaining.split_first()?;
    state.remaining = remaining;
    Some(element)
}

pub(super) fn is_unpack(builder: &TypeInferenceBuilder<'_, '_>, expression: &ast::Expr) -> bool {
    matches!(expression, ast::Expr::Starred(_))
        || matches!(expression, ast::Expr::Subscript(ast::ExprSubscript { value, .. })
            if builder.expression_type(value) == Type::SpecialForm(SpecialFormType::Unpack))
}

impl<'db, 'ast> SynchronousTupleAnnotationEffects<'db, 'ast> for OrdinaryEffects {
    type Error = Infallible;

    fn elements<'expr>(
        &self,
        request: Request<'expr>,
        elements: &'expr [ast::Expr],
    ) -> Result<Elements<'db, 'expr>, Infallible> {
        Ok(Elements {
            request,
            remaining: elements,
            types: TupleSpecBuilder::with_capacity(elements.len()),
            first_variadic: None,
        })
    }

    fn next_element<'expr>(
        &self,
        state: &mut Elements<'db, 'expr>,
    ) -> Result<Option<&'expr ast::Expr>, Infallible> {
        Ok(next_element(state))
    }

    fn enter_unpack(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<bool, Infallible> {
        Ok(builder
            .context
            .inference_flags
            .replace(InferenceFlags::IN_VALID_UNPACK_CONTEXT, true))
    }

    fn restore_unpack(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        previous: bool,
    ) -> Result<(), Infallible> {
        builder
            .context
            .inference_flags
            .set(InferenceFlags::IN_VALID_UNPACK_CONTEXT, previous);
        Ok(())
    }

    fn flags(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> Result<TypeExpressionFlags, Infallible> {
        Ok(builder.type_expression_flags(expression))
    }

    fn is_unpack(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> Result<bool, Infallible> {
        Ok(is_unpack(builder, expression))
    }

    fn exact_spec(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<Option<&'db TupleSpec<'db>>, Infallible> {
        Ok(ty
            .as_nominal_instance()
            .and_then(|instance| instance.exact_tuple())
            .map(|tuple| tuple.tuple(builder.db())))
    }

    fn typevartuple(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Infallible> {
        Ok(match ty {
            Type::TypeVar(typevar) if typevar.is_typevartuple(builder.db()) => Some(typevar),
            _ => None,
        })
    }

    fn invalid_ellipsis(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> Result<(), Infallible> {
        if let Some(diagnostic) = builder.context.report_lint(&INVALID_TYPE_FORM, expression) {
            let mut diagnostic = diagnostic.into_diagnostic("Invalid `tuple` specialization");
            diagnostic.set_primary_annotation_message("`...` can only be used as the second element in a two-element `tuple` specialization");
        }
        Ok(())
    }

    fn unpack_before_ellipsis(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ellipsis: &ast::Expr,
    ) -> Result<(), Infallible> {
        if let Some(diagnostic) = builder.context.report_lint(&INVALID_TYPE_FORM, ellipsis) {
            let mut diagnostic = diagnostic.into_diagnostic("Invalid `tuple` specialization");
            diagnostic
                .set_primary_annotation_message("`...` cannot be used after an unpacked element");
        }
        Ok(())
    }

    fn duplicate_unpack(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        subscript: &ast::ExprSubscript,
        first: &ast::Expr,
        later: &ast::Expr,
    ) -> Result<(), Infallible> {
        if let Some(diagnostic) = builder.context.report_lint(&INVALID_TYPE_FORM, subscript) {
            let mut diagnostic = diagnostic.into_diagnostic(
                "Multiple unpacked variadic tuples are not allowed in a `tuple` specialization",
            );
            diagnostic.annotate(
                builder
                    .context
                    .secondary(first)
                    .message("First unpacked variadic tuple"),
            );
            diagnostic.annotate(
                builder
                    .context
                    .secondary(later)
                    .message("Later unpacked variadic tuple"),
            );
        }
        Ok(())
    }

    fn remember_variadic<'expr>(
        &self,
        state: &mut Elements<'db, 'expr>,
        expression: &'expr ast::Expr,
    ) -> Result<(), Infallible> {
        state.first_variadic = Some(expression);
        Ok(())
    }

    fn push(&self, state: &mut Elements<'db, '_>, ty: Type<'db>) -> Result<(), Infallible> {
        state.types.push(ty);
        Ok(())
    }

    fn concat(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        state: &mut Elements<'db, '_>,
        spec: &TupleSpec<'db>,
    ) -> Result<(), Infallible> {
        let types = std::mem::replace(&mut state.types, TupleSpecBuilder::with_capacity(0));
        state.types = types.concat(builder.db(), builder.program_environment(), spec);
        Ok(())
    }

    fn concat_typevar(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        state: &mut Elements<'db, '_>,
        typevar: BoundTypeVarInstance<'db>,
    ) -> Result<(), Infallible> {
        let types = std::mem::replace(&mut state.types, TupleSpecBuilder::with_capacity(0));
        state.types =
            types.concat_variadic_typevar(builder.db(), builder.program_environment(), typevar);
        Ok(())
    }

    fn construct(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        specification: &mut Specification<'db>,
    ) -> Result<TupleType<'db>, Infallible> {
        let db = builder.db();
        let env = builder.program_environment();
        Ok(match specification {
            Specification::Builder(types) => TupleType::new(
                db,
                env,
                &std::mem::replace(types, TupleSpecBuilder::with_capacity(0)).build(),
            ),
            Specification::Borrowed(spec) => TupleType::new(db, env, spec),
            Specification::Homogeneous(ty) => TupleType::homogeneous(db, env, *ty),
            Specification::Single(ty) => TupleType::heterogeneous(db, env, [*ty]),
            Specification::TypeVarTuple(typevar) => {
                TupleType::unpacked_typevartuple(db, env, *typevar)
            }
        })
    }

    fn instance(&self, tuple: TupleType<'db>) -> Result<Type<'db>, Infallible> {
        Ok(Type::tuple(tuple))
    }

    fn convert(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        tuple: TupleType<'db>,
        mode: ResultMode,
    ) -> Result<Type<'db>, Infallible> {
        Ok(match mode {
            ResultMode::Instance => Type::tuple(tuple),
            ResultMode::Class => Type::from(tuple.to_class_type(builder.db())),
            ResultMode::Subclass => SubclassOfType::from(
                builder.db(),
                builder.program_environment(),
                tuple.to_class_type(builder.db()),
            ),
        })
    }

    fn store(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        ty: Type<'db>,
    ) -> Result<(), Infallible> {
        builder.store_expression_type(expression, ty);
        Ok(())
    }
}
