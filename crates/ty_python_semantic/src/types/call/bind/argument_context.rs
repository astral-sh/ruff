//! Parameter context and candidate selection shared by ordinary and controlled call inference.

use std::convert::Infallible;
use std::future::Future;

use smallvec::SmallVec;

#[cfg(feature = "experimental-analysis")]
use super::Bindings;
use super::{ArgumentTypeContext, Binding, CallableBinding, ParamSpecArgumentContext};
use crate::types::call::CallArguments;
use crate::types::callable::CallableTypeKind;
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::generics::Specialization;
use crate::types::{
    BoundTypeVarInstance, CallableType, Type, TypeContext, TypeVarBoundOrConstraints,
};
use crate::{Db, ProgramEnvironment};

pub(in crate::types) struct ArgumentContextRequest<'a, 'call, 'db> {
    pub(in crate::types) db: &'db dyn Db,
    pub(in crate::types) env: &'a ProgramEnvironment<'db>,
    pub(in crate::types) constraints: &'a ConstraintSetBuilder<'db>,
    pub(in crate::types) overload: &'a Binding<'db>,
    pub(in crate::types) binding: &'a CallableBinding<'db>,
    pub(in crate::types) arguments_types: &'a CallArguments<'call, 'db>,
    pub(in crate::types) argument_index: usize,
    pub(in crate::types) call_expression_tcx: TypeContext<'db>,
}

pub(in crate::types) trait ArgumentContextEffects<'db> {
    type Error;

    async fn local<T>(
        &self,
        work: Option<usize>,
        requested_bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;

    async fn upper_bound(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        typevar: BoundTypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error>;

    async fn without_paramspec_attr(
        &self,
        db: &'db dyn Db,
        typevar: BoundTypeVarInstance<'db>,
    ) -> Result<BoundTypeVarInstance<'db>, Self::Error>;

    async fn merged_specialization(
        &self,
        db: &'db dyn Db,
        overload: &Binding<'db>,
    ) -> Result<Option<Specialization<'db>>, Self::Error>;

    async fn specialization_binding(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
        paramspec: BoundTypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error>;

    async fn apply_specialization(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        specialization: Specialization<'db>,
    ) -> Result<Type<'db>, Self::Error>;

    async fn paramspec_context(
        &self,
        request: &ArgumentContextRequest<'_, '_, 'db>,
        callable: CallableType<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error>;

    async fn typevartuple_context(
        &self,
        request: &ArgumentContextRequest<'_, '_, 'db>,
        expected_return_ty: Type<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error>;

    async fn has_expandable_variadic(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        arguments: &CallArguments<'_, 'db>,
    ) -> Result<bool, Self::Error>;
}

pub(super) struct InlineArgumentContextEffects;

impl<'db> Binding<'db> {
    #[expect(clippy::too_many_arguments)]
    pub(in crate::types) async fn argument_type_context_with<E, F>(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        constraints: &ConstraintSetBuilder<'db>,
        binding: &CallableBinding<'db>,
        arguments_types: &CallArguments<'_, 'db>,
        argument_index: usize,
        call_expression_tcx: TypeContext<'db>,
        specialization: impl Fn() -> F,
        effects: &E,
    ) -> Result<Option<ArgumentTypeContext<'db>>, E::Error>
    where
        E: ArgumentContextEffects<'db>,
        F: Future<Output = Result<Option<Specialization<'db>>, E::Error>>,
    {
        let matched = effects
            .local(Some(6), Some(0), || {
                let argument_matches =
                    self.matched_argument_for_call_argument(binding, argument_index)?;
                let [matched_parameter] = argument_matches.parameters.as_slice() else {
                    return None;
                };
                let parameter = &self.signature.parameters()[matched_parameter.index];
                Some((parameter, matched_parameter.expected_type))
            })
            .await?;
        let Some((parameter, expected_type)) = matched else {
            return Ok(None);
        };
        let original_parameter_type = parameter.annotated_type();
        let mut parameter_type = expected_type.unwrap_or(original_parameter_type);

        // A non-ParamSpec type variable with an upper bound, such as `typing.Self`, uses that
        // bound directly. ParamSpec components need specialization from earlier arguments.
        if let Type::TypeVar(typevar) = parameter_type
            && !effects
                .local(Some(3), Some(0), || typevar.is_paramspec(db))
                .await?
            && let Some(bound) = effects.upper_bound(db, env, typevar).await?
        {
            return Ok(Some(ArgumentTypeContext::standard(
                original_parameter_type,
                bound,
            )));
        }

        // Generic calls specialize parameter types using constraints collected for the call.
        if self.signature.generic_context.is_some() {
            let paramspec_component = if let Type::TypeVar(typevar) = original_parameter_type
                && effects
                    .local(Some(5), Some(0), || {
                        typevar.is_paramspec(db) && typevar.paramspec_attr(db).is_some()
                    })
                    .await?
            {
                Some(effects.without_paramspec_attr(db, typevar).await?)
            } else {
                None
            };
            let request = ArgumentContextRequest {
                db,
                env,
                constraints,
                overload: self,
                binding,
                arguments_types,
                argument_index,
                call_expression_tcx,
            };

            // Specializing `P.args` or `P.kwargs` yields the entire parameter list. Infer this
            // argument against its matched parameter on the specialized callable instead.
            if let Some(paramspec) = paramspec_component {
                let mut specialized = None;
                if self.inference.is_some()
                    && let Some(merged) = effects.merged_specialization(db, self).await?
                {
                    specialized = effects
                        .specialization_binding(db, merged, paramspec)
                        .await?;
                }
                if specialized.is_none()
                    && let Some(specialization) = specialization().await?
                {
                    specialized = effects
                        .specialization_binding(db, specialization, paramspec)
                        .await?;
                }
                let Some(Type::Callable(callable)) = specialized else {
                    return Ok(None);
                };
                if !effects
                    .local(Some(1), Some(0), || {
                        callable.kind(db) == CallableTypeKind::ParamSpecValue
                    })
                    .await?
                {
                    return Ok(None);
                }
                let Some(parameter_type) = effects.paramspec_context(&request, callable).await?
                else {
                    return Ok(None);
                };
                return Ok(Some(ArgumentTypeContext::paramspec(
                    original_parameter_type,
                    parameter_type,
                )));
            }

            if let Some(specialization) = specialization().await? {
                parameter_type = effects
                    .apply_specialization(db, parameter_type, specialization)
                    .await?;
            }
            if let Some(expected_return_ty) = call_expression_tcx.annotation
                && let Type::TypeVar(typevartuple) = original_parameter_type
                && effects
                    .local(Some(5), Some(0), || {
                        parameter.is_variadic()
                            && parameter.has_starred_annotation()
                            && typevartuple.is_typevartuple(db)
                    })
                    .await?
                && let Some(expected) = effects
                    .typevartuple_context(&request, expected_return_ty)
                    .await?
            {
                parameter_type = expected;
            }
        }

        Ok(Some(ArgumentTypeContext::standard(
            original_parameter_type,
            parameter_type,
        )))
    }
}

impl<'db> CallableBinding<'db> {
    /// Work in the directly retained matching payload; does not follow semantic dependencies.
    /// Admit the outer overload scan before calling this method.
    pub(in crate::types) fn argument_context_metadata_work(&self) -> Option<usize> {
        self.overloads
            .iter()
            .try_fold(self.overloads.len(), |work, overload| {
                work.checked_add(overload.errors.len())?
                    .checked_add(overload.argument_matches.len())
            })
    }

    pub(in crate::types) async fn candidate_overload_indices_with<
        E: ArgumentContextEffects<'db>,
    >(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        call_arguments: &CallArguments<'_, 'db>,
        effects: &E,
    ) -> Result<SmallVec<[usize; 1]>, E::Error> {
        let work = effects
            .local(self.overloads.len().checked_add(1), Some(0), || {
                self.argument_context_metadata_work()
            })
            .await?;
        let matching_count = effects
            .local(work, Some(0), || self.matching_overloads().count())
            .await?;
        let expand = self.overloads.len() > 1
            && matching_count < self.overloads.len()
            && effects
                .has_expandable_variadic(db, env, call_arguments)
                .await?;
        let count = if expand {
            self.overloads.len()
        } else {
            matching_count
        };
        let bytes = if count > 1 {
            count.checked_mul(size_of::<usize>())
        } else {
            Some(0)
        };
        let mut indices = SmallVec::new();
        effects
            .local(work.and_then(|work| work.checked_add(count)), bytes, || {
                indices.reserve_exact(count);
                if expand {
                    indices.extend(0..self.overloads.len());
                } else {
                    indices.extend(self.matching_overloads().map(|(index, _)| index));
                }
            })
            .await?;
        Ok(indices)
    }
}

#[cfg(feature = "experimental-analysis")]
impl<'db> Bindings<'db> {
    pub(in crate::types) fn argument_context_root_len(&self) -> usize {
        self.elements.len()
    }

    /// Work for one level of callable traversal, excluding downstream constructor bindings.
    pub(in crate::types) fn direct_type_context_work(&self) -> Option<usize> {
        self.elements
            .iter()
            .try_fold(self.elements.len(), |work, element| {
                work.checked_add(element.items.len())
            })
    }

    pub(in crate::types) fn direct_type_context_callables(
        &self,
    ) -> impl Iterator<Item = (&CallableBinding<'db>, Option<&Bindings<'db>>)> {
        self.iter_callable_items().map(|item| {
            (
                item.callable(),
                item.as_constructor()
                    .and_then(|constructor| constructor.downstream_constructor.as_deref()),
            )
        })
    }
}

impl<'db> ArgumentContextEffects<'db> for InlineArgumentContextEffects {
    type Error = Infallible;

    async fn local<T>(
        &self,
        _work: Option<usize>,
        _requested_bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error> {
        Ok(action())
    }

    async fn upper_bound(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        typevar: BoundTypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(match typevar.typevar(db).bound_or_constraints(db, env) {
            Some(TypeVarBoundOrConstraints::UpperBound(bound)) => Some(bound),
            _ => None,
        })
    }

    async fn without_paramspec_attr(
        &self,
        db: &'db dyn Db,
        typevar: BoundTypeVarInstance<'db>,
    ) -> Result<BoundTypeVarInstance<'db>, Self::Error> {
        Ok(typevar.without_paramspec_attr(db))
    }

    async fn merged_specialization(
        &self,
        db: &'db dyn Db,
        overload: &Binding<'db>,
    ) -> Result<Option<Specialization<'db>>, Self::Error> {
        Ok(overload.merged_specialization(db))
    }

    async fn specialization_binding(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
        paramspec: BoundTypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(specialization.get(db, paramspec))
    }

    async fn apply_specialization(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        specialization: Specialization<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(ty.apply_specialization(db, specialization))
    }

    async fn paramspec_context(
        &self,
        request: &ArgumentContextRequest<'_, '_, 'db>,
        callable: CallableType<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(request
            .overload
            .paramspec_argument_context(&ParamSpecArgumentContext {
                db: request.db,
                env: request.env,
                constraints: request.constraints,
                binding: request.binding,
                callable,
                arguments_types: request.arguments_types,
                argument_index: request.argument_index,
                call_expression_tcx: request.call_expression_tcx,
            }))
    }

    async fn typevartuple_context(
        &self,
        request: &ArgumentContextRequest<'_, '_, 'db>,
        expected_return_ty: Type<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(request.overload.typevartuple_argument_context(
            request.db,
            request.env,
            request.binding,
            request.arguments_types,
            request.argument_index,
            expected_return_ty,
        ))
    }

    async fn has_expandable_variadic(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        arguments: &CallArguments<'_, 'db>,
    ) -> Result<bool, Self::Error> {
        Ok(arguments.expansions(db, env).has_expandable_variadic())
    }
}
