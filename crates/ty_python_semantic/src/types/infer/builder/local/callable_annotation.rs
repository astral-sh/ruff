//! Callable annotations retain their parameters while the return annotation is inferred.

use super::*;

/// Syntax retained until the callable type can be stored on both argument expressions.
#[derive(Clone, Copy, Debug)]
pub(in crate::types::infer) struct Request<'expr> {
    pub(super) subscript: &'expr ast::ExprSubscript,
    pub(super) parameter_expression: &'expr ast::Expr,
    parameters: ParameterSyntax<'expr>,
    pub(super) returns: &'expr ast::Expr,
}

/// Distinguishes explicit parameters from the gradual `*args: Any, **kwargs: Any` form.
#[derive(Clone, Copy, Debug)]
enum ParameterSyntax<'expr> {
    List(&'expr [ast::Expr]),
    Gradual,
}

/// Recognizes callable forms handled by the shared continuation, leaving other syntax to its caller.
pub(in crate::types::infer::builder) fn request(
    subscript: &ast::ExprSubscript,
) -> Option<Request<'_>> {
    let ast::Expr::Tuple(arguments) = subscript.slice.as_ref() else {
        return None;
    };
    let [parameter_expression, returns] = arguments.elts.as_slice() else {
        return None;
    };
    let parameters = match parameter_expression {
        ast::Expr::List(parameters)
            if !matches!(parameters.elts.as_slice(), [ast::Expr::EllipsisLiteral(_)]) =>
        {
            ParameterSyntax::List(&parameters.elts)
        }
        ast::Expr::EllipsisLiteral(_) => ParameterSyntax::Gradual,
        _ => return None,
    };
    Some(Request {
        subscript,
        parameter_expression,
        parameters,
        returns,
    })
}

#[derive(Clone, Copy, Debug)]
pub(super) struct SavedFlags {
    check_unbound: bool,
    concatenate: bool,
    unpack: Option<bool>,
}

impl SavedFlags {
    fn enter(builder: &mut TypeInferenceBuilder<'_, '_>, syntax: ParameterSyntax<'_>) -> Self {
        let flags = &mut builder.context.inference_flags;
        Self {
            check_unbound: flags.replace(InferenceFlags::CHECK_UNBOUND_TYPEVARS, false),
            concatenate: flags.replace(InferenceFlags::IN_VALID_CONCATENATE_CONTEXT, true),
            unpack: match syntax {
                ParameterSyntax::List(_) => {
                    Some(flags.replace(InferenceFlags::IN_VALID_UNPACK_CONTEXT, true))
                }
                ParameterSyntax::Gradual => None,
            },
        }
    }

    pub(super) fn restore_unpack(self, builder: &mut TypeInferenceBuilder<'_, '_>) {
        if let Some(unpack) = self.unpack {
            builder
                .context
                .inference_flags
                .set(InferenceFlags::IN_VALID_UNPACK_CONTEXT, unpack);
        }
    }

    pub(super) fn restore_concatenate(self, builder: &mut TypeInferenceBuilder<'_, '_>) {
        builder.context.inference_flags.set(
            InferenceFlags::IN_VALID_CONCATENATE_CONTEXT,
            self.concatenate,
        );
    }

    pub(super) fn restore_unbound(self, builder: &mut TypeInferenceBuilder<'_, '_>) {
        builder
            .context
            .inference_flags
            .set(InferenceFlags::CHECK_UNBOUND_TYPEVARS, self.check_unbound);
    }
}

pub(super) struct ParameterState<'db, 'expr> {
    pub(super) request: Request<'expr>,
    pub(super) remaining: &'expr [ast::Expr],
    pub(super) parameters: Vec<Parameter<'db>>,
    pub(super) saved: SavedFlags,
}

pub(super) enum State<'db, 'expr> {
    Gradual {
        request: Request<'expr>,
        saved: SavedFlags,
    },
    Parameters(ParameterState<'db, 'expr>),
    Complete(Completed<'db, 'expr>),
}

pub(super) enum Pending<'db, 'expr> {
    Parameter(ParameterState<'db, 'expr>, &'expr ast::Expr),
    Return {
        request: Request<'expr>,
        parameters: Parameters<'db>,
        saved: SavedFlags,
    },
}

pub(super) struct Completed<'db, 'expr> {
    pub(super) request: Request<'expr>,
    pub(super) parameters: Parameters<'db>,
    pub(super) return_type: Type<'db>,
    pub(super) saved: SavedFlags,
}

pub(super) enum Action<'db, 'expr> {
    Infer(Pending<'db, 'expr>, &'expr ast::Expr),
    Complete(Completed<'db, 'expr>),
}

pub(super) struct Active(pub(super) usize);
pub(super) struct Waiting(pub(super) usize);
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
    Infer {
        owner: Waiting,
        builder: BuilderId,
        expression: &'expr ast::Expr,
    },
    Complete(Finished),
}

pub(super) struct CompletionLease<'owner, 'db, 'expr, B, T> {
    pub(super) index: usize,
    slot: &'owner mut OwnerSlot<'db, 'expr, B, T>,
}

impl<'db, 'expr, B, T> CompletionLease<'_, 'db, 'expr, B, T> {
    pub(super) fn parts_mut(&mut self) -> (BuilderId, &mut Completed<'db, 'expr>) {
        let OwnerSlot::CallableAnnotation(Payload {
            builder,
            phase: Phase::Completed(completed),
            ..
        }) = self.slot
        else {
            panic!("completed callable annotation token does not identify a completed owner");
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
    pub(super) fn push_callable_annotation(
        &mut self,
        builder: BuilderId,
        request: Request<'expr>,
        inference: &mut TypeInferenceBuilder<'db, '_>,
    ) -> Active {
        let index = self.slots.len();
        #[cfg(test)]
        let identity = self.next_identity;
        #[cfg(test)]
        {
            self.next_identity += 1;
        }
        let saved = SavedFlags::enter(inference, request.parameters);
        let state = match request.parameters {
            ParameterSyntax::List(remaining) => State::Parameters(ParameterState {
                request,
                remaining,
                parameters: Vec::new(),
                saved,
            }),
            ParameterSyntax::Gradual => State::Gradual { request, saved },
        };
        self.slots.push(OwnerSlot::CallableAnnotation(Payload {
            builder,
            phase: Phase::Active(state),
            #[cfg(test)]
            lifetime: self.observer.as_ref().map(|observer| OwnerLifetime {
                identity,
                observer: observer.clone(),
            }),
        }));
        #[cfg(test)]
        if let Some(observer) = &self.observer {
            observer(OwnershipEvent::Created {
                kind: OwnerKind::CallableAnnotation,
                call: request.subscript.range(),
                index,
                identity,
                len: self.slots.len(),
                capacity: self.slots.capacity(),
            });
        }
        Active(index)
    }

    fn take_callable_annotation(&mut self, index: usize) -> Payload<Phase<'db, 'expr>> {
        assert_eq!(index + 1, self.slots.len());
        let OwnerSlot::CallableAnnotation(payload) =
            std::mem::replace(&mut self.slots[index], OwnerSlot::Taken)
        else {
            panic!("callable annotation token does not identify an initialized owner");
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

    pub(super) fn take_callable_active(&mut self, owner: Active) -> Taken<State<'db, 'expr>> {
        Taken {
            index: owner.0,
            payload: self.take_callable_annotation(owner.0).map(|phase| {
                let Phase::Active(state) = phase else {
                    panic!("active callable annotation token does not identify an active owner");
                };
                state
            }),
        }
    }

    pub(super) fn take_callable_pending(&mut self, owner: Waiting) -> Taken<Pending<'db, 'expr>> {
        Taken {
            index: owner.0,
            payload: self.take_callable_annotation(owner.0).map(|phase| {
                let Phase::Pending(pending) = phase else {
                    panic!("pending callable annotation token does not identify a pending owner");
                };
                pending
            }),
        }
    }

    fn install_callable_annotation(&mut self, taken: Taken<Phase<'db, 'expr>>) {
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
        self.slots[taken.index] = OwnerSlot::CallableAnnotation(taken.payload);
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

    pub(super) fn install_callable_active(&mut self, taken: Taken<State<'db, 'expr>>) -> Active {
        let owner = Active(taken.index);
        self.install_callable_annotation(taken.map(Phase::Active));
        owner
    }

    pub(super) fn install_callable_action(
        &mut self,
        taken: Taken<Action<'db, 'expr>>,
    ) -> Step<'expr> {
        let Taken { index, payload } = taken;
        let Payload {
            builder,
            phase,
            #[cfg(test)]
            lifetime,
        } = payload;
        let (phase, step) = match phase {
            Action::Infer(pending, expression) => (
                Phase::Pending(pending),
                Step::Infer {
                    owner: Waiting(index),
                    builder,
                    expression,
                },
            ),
            Action::Complete(completed) => {
                (Phase::Completed(completed), Step::Complete(Finished(index)))
            }
        };
        self.install_callable_annotation(Taken {
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

    pub(super) fn callable_completion(
        &mut self,
        owner: Finished,
    ) -> CompletionLease<'_, 'db, 'expr, B, T> {
        assert_eq!(owner.0 + 1, self.slots.len());
        let slot = &mut self.slots[owner.0];
        #[cfg(test)]
        if let OwnerSlot::CallableAnnotation(Payload {
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

pub(super) struct Facts;
pub(super) struct OrdinaryEffects;

shared_semantic_family! {
    #[synchronous(SynchronousCallableAnnotationEffects)]
    pub(super) trait CallableAnnotationEffects<'db, 'ast> {
        type Error;
        #[operation(child)]
        async fn gradual_parameters(&self) -> Result<Parameters<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_parameter<'expr>(&self, state: &mut ParameterState<'db, 'expr>) -> Result<Option<&'expr ast::Expr>, Self::Error>;
        #[operation(local)]
        async fn restore_unpack(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, saved: SavedFlags) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn normalize(&self, builder: &TypeInferenceBuilder<'db, 'ast>, state: &mut ParameterState<'db, '_>) -> Result<Parameters<'db>, Self::Error>;
        #[operation(local)]
        async fn restore_concatenate(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, saved: SavedFlags) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn parameter(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, ty: Type<'db>) -> Result<Parameter<'db>, Self::Error>;
        #[operation(local)]
        async fn append(&self, state: &mut ParameterState<'db, '_>, parameter: Parameter<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn callable(&self, builder: &TypeInferenceBuilder<'db, 'ast>, completed: &Completed<'db, '_>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn store(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn restore_unbound(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, saved: SavedFlags) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl Facts {
        fn saved(&self, state: &ParameterState<'_, '_>) -> SavedFlags { state.saved }
        fn request<'expr>(&self, state: &ParameterState<'_, 'expr>) -> Request<'expr> { state.request }
        fn returns<'expr>(&self, request: Request<'expr>) -> &'expr ast::Expr { request.returns }
        fn arguments<'expr>(&self, completed: &Completed<'_, 'expr>) -> &'expr ast::Expr { &completed.request.subscript.slice }
        fn parameter_expression<'expr>(&self, completed: &Completed<'_, 'expr>) -> &'expr ast::Expr { completed.request.parameter_expression }
        fn completed_saved(&self, completed: &Completed<'_, '_>) -> SavedFlags { completed.saved }
    }

    #[synchronous(advance_sync)]
    #[capabilities(effects = CallableAnnotationEffects, facts = Facts)]
    #[passive_values(Action::Infer, Action::Complete, Pending::Parameter, Pending::Return)]
    pub(super) async fn advance_with<'db, 'ast, 'expr, E: CallableAnnotationEffects<'db, 'ast>>(
        state: State<'db, 'expr>, builder: &mut TypeInferenceBuilder<'db, 'ast>, facts: Facts, effects: &E,
    ) -> Result<Action<'db, 'expr>, E::Error> {
        match state {
            State::Gradual { request, saved } => {
                let parameters = effects.gradual_parameters().await?;
                effects.restore_concatenate(builder, saved).await?;
                Ok(Action::Infer(Pending::Return { request, parameters, saved }, facts.returns(request)))
            }
            State::Complete(completed) => Ok(Action::Complete(completed)),
            State::Parameters(mut state) => {
                if let Some(expression) = effects.next_parameter(&mut state).await? {
                    return Ok(Action::Infer(Pending::Parameter(state, expression), expression));
                }
                let saved = facts.saved(&state);
                let request = facts.request(&state);
                effects.restore_unpack(builder, saved).await?;
                let parameters = effects.normalize(builder, &mut state).await?;
                effects.restore_concatenate(builder, saved).await?;
                Ok(Action::Infer(Pending::Return { request, parameters, saved }, facts.returns(request)))
            }
        }
    }

    #[synchronous(resume_sync)]
    #[capabilities(effects = CallableAnnotationEffects)]
    #[passive_values(State::Parameters, State::Complete, Completed)]
    pub(super) async fn resume_with<'db, 'ast, 'expr, E: CallableAnnotationEffects<'db, 'ast>>(
        pending: Pending<'db, 'expr>, ty: Type<'db>, builder: &TypeInferenceBuilder<'db, 'ast>, effects: &E,
    ) -> Result<State<'db, 'expr>, E::Error> {
        match pending {
            Pending::Parameter(mut state, expression) => {
                let parameter = effects.parameter(builder, expression, ty).await?;
                effects.append(&mut state, parameter).await?;
                Ok(State::Parameters(state))
            }
            Pending::Return { request, parameters, saved } => {
                Ok(State::Complete(Completed { request, parameters, return_type: ty, saved }))
            }
        }
    }

    #[synchronous(finish_sync)]
    #[capabilities(effects = CallableAnnotationEffects, facts = Facts)]
    #[passive_values()]
    pub(super) async fn finish_with<'db, 'ast, E: CallableAnnotationEffects<'db, 'ast>>(
        completed: &mut Completed<'db, '_>, builder: &mut TypeInferenceBuilder<'db, 'ast>, facts: Facts, effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        let ty = effects.callable(builder, completed).await?;
        effects.store(builder, facts.arguments(completed), ty).await?;
        effects.store(builder, facts.parameter_expression(completed), ty).await?;
        effects.restore_unbound(builder, facts.completed_saved(completed)).await?;
        Ok(ty)
    }
}

pub(super) fn next_parameter<'expr>(
    state: &mut ParameterState<'_, 'expr>,
) -> Option<&'expr ast::Expr> {
    let (expression, remaining) = state.remaining.split_first()?;
    state.remaining = remaining;
    Some(expression)
}

impl<'db, 'ast> SynchronousCallableAnnotationEffects<'db, 'ast> for OrdinaryEffects {
    type Error = Infallible;

    fn gradual_parameters(&self) -> Result<Parameters<'db>, Infallible> {
        Ok(Parameters::gradual_form())
    }

    fn next_parameter<'expr>(
        &self,
        state: &mut ParameterState<'db, 'expr>,
    ) -> Result<Option<&'expr ast::Expr>, Infallible> {
        Ok(next_parameter(state))
    }
    fn restore_unpack(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        saved: SavedFlags,
    ) -> Result<(), Infallible> {
        saved.restore_unpack(builder);
        Ok(())
    }
    fn normalize(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        state: &mut ParameterState<'db, '_>,
    ) -> Result<Parameters<'db>, Infallible> {
        Ok(Parameters::from_annotation(
            builder.db(),
            std::mem::take(&mut state.parameters),
        ))
    }
    fn restore_concatenate(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        saved: SavedFlags,
    ) -> Result<(), Infallible> {
        saved.restore_concatenate(builder);
        Ok(())
    }
    fn parameter(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        ty: Type<'db>,
    ) -> Result<Parameter<'db>, Infallible> {
        Ok(builder.callable_parameter_from_annotation(expression, ty))
    }
    fn append(
        &self,
        state: &mut ParameterState<'db, '_>,
        parameter: Parameter<'db>,
    ) -> Result<(), Infallible> {
        state.parameters.push(parameter);
        Ok(())
    }
    fn callable(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        completed: &Completed<'db, '_>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(Type::single_callable(
            builder.db(),
            Signature::new(completed.parameters.clone(), completed.return_type),
        ))
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
    fn restore_unbound(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        saved: SavedFlags,
    ) -> Result<(), Infallible> {
        saved.restore_unbound(builder);
        Ok(())
    }
}
