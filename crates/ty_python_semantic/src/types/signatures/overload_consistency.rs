use itertools::Itertools;
use smallvec::{SmallVec, smallvec_inline};

use super::{ConcatenateTail, ParameterForm, ParameterKind, Parameters, Signature};
use crate::types::call::{Argument, CallArguments};
use crate::types::constraints::{ConstraintSet, ConstraintSetBuilder};
use crate::types::generics::{ApplySpecialization, InferableTypeVars};
use crate::types::relation::{HasRelationToVisitor, IsDisjointVisitor, TypeRelationChecker};
use crate::types::variance::VarianceInferable;
use crate::types::{
    ApplyTypeMappingVisitor, BoundTypeVarInstance, Type, TypeContext, TypeMapping,
    TypeVarBoundOrConstraints, TypeVarVariance,
};
use crate::{Db, FxOrderSet};

impl<'db> Signature<'db> {
    /// Return the type variables that can be inferred during implementation-consistency checks.
    ///
    /// Function-local type variables and all `ParamSpec`s are inferable. Class-level type variables
    /// remain universally quantified so that every specialization accepted by an overload is
    /// accepted by the implementation.
    ///
    /// ```python
    /// from typing import overload
    ///
    /// class C[T]:
    ///     @overload
    ///     def f[U](self, value: U) -> U: ...
    ///     def f[U](self, value: U) -> U:
    ///         return value
    /// ```
    fn implementation_consistency_inferable_typevars(
        &self,
        db: &'db dyn Db,
    ) -> InferableTypeVars<'db> {
        let Some(generic_context) = self.generic_context else {
            return InferableTypeVars::None;
        };

        let Some(definition) = self.definition else {
            return generic_context.inferable_typevars(db);
        };

        let typevars = generic_context
            .variables(db)
            .filter(|bound_typevar| {
                bound_typevar.is_paramspec(db)
                    || bound_typevar.binding_context(db).definition() == Some(definition)
            })
            .map(|bound_typevar| bound_typevar.identity(db))
            .collect::<FxOrderSet<_>>();
        InferableTypeVars::from_typevars(db, typevars)
    }

    /// Return the `ParamSpec` variables in this signature's generic context.
    ///
    /// Implementation-consistency checks allow overload `ParamSpec`s to be inferred against the
    /// implementation's parameter list while still treating non-`ParamSpec` overload type
    /// variables as part of the overload's parameter domain.
    ///
    /// ```python
    /// from collections.abc import Callable
    /// from typing import ParamSpec, TypeVar, overload
    ///
    /// P = ParamSpec("P")
    /// R = TypeVar("R")
    ///
    /// @overload
    /// def decorate(func: Callable[P, R]) -> Callable[P, R]: ...
    /// def decorate(func: Callable[..., object]) -> Callable[..., object]:
    ///     return func
    /// ```
    fn paramspec_typevars(&self, db: &'db dyn Db) -> InferableTypeVars<'db> {
        let Some(generic_context) = self.generic_context else {
            return InferableTypeVars::None;
        };

        let typevars = generic_context
            .variables(db)
            .filter(|bound_typevar| bound_typevar.is_paramspec(db))
            .map(|bound_typevar| bound_typevar.identity(db))
            .collect::<FxOrderSet<_>>();
        InferableTypeVars::from_typevars(db, typevars)
    }

    /// Build the constraint set for this implementation accepting one overload parameter domain.
    ///
    /// The comparison ignores return types and normalizes an implicit or gradual receiver so a
    /// method overload and its implementation are compared on their explicit call arguments.
    ///
    /// ```python
    /// from typing import overload
    ///
    /// class C:
    ///     @overload
    ///     def f(self, value: int) -> int: ...
    ///     def f(self, value: object) -> object:
    ///         return value
    /// ```
    fn when_implementation_parameters_compatible_with<'c>(
        &self,
        db: &'db dyn Db,
        other: &Self,
        constraints: &'c ConstraintSetBuilder<'db>,
        has_implicit_receiver: bool,
    ) -> ConstraintSet<'db, 'c> {
        let relation_visitor = HasRelationToVisitor::default(constraints);
        let disjointness_visitor = IsDisjointVisitor::default(constraints);
        let materialization_visitor = ApplyTypeMappingVisitor::default();
        let checker = TypeRelationChecker::implementation_compatibility(
            constraints,
            &relation_visitor,
            &disjointness_visitor,
            &materialization_visitor,
        );
        let (self_signature, other_signature);
        let (self_, other) =
            if has_implicit_receiver && self.has_gradual_or_implicit_self_or_cls_parameter() {
                self_signature = self
                    .clone()
                    .with_first_parameter_type_and_positional_only(Type::unknown());
                other_signature = other
                    .clone()
                    .with_first_parameter_type_and_positional_only(Type::unknown());
                (&self_signature, &other_signature)
            } else {
                (self, other)
            };
        checker.check_signature_pair_with_typevars(
            db,
            self_,
            other,
            self_.implementation_consistency_inferable_typevars(db),
            other.paramspec_typevars(db),
        )
    }

    /// Return `true` if this implementation accepts every argument shape accepted by an overload.
    ///
    /// ```python
    /// from typing import overload
    ///
    /// @overload
    /// def f(x: int) -> int: ...
    /// def f(x: object) -> object:
    ///     return x
    /// ```
    fn are_implementation_parameters_compatible_with(
        &self,
        db: &'db dyn Db,
        other: &Self,
        has_implicit_receiver: bool,
    ) -> bool {
        let self_ = self.clone().with_return_type(Type::unknown());
        let inferable = self_
            .implementation_consistency_inferable_typevars(db)
            .merge(db, other.paramspec_typevars(db));

        other
            .parameter_domain_variants(db, self_.parameters.is_gradual())
            .iter()
            .all(|other| {
                let constraints = ConstraintSetBuilder::new();
                self_
                    .when_implementation_parameters_compatible_with(
                        db,
                        other,
                        &constraints,
                        has_implicit_receiver,
                    )
                    .satisfied_by_all_typevars(db, &constraints, inferable)
            })
    }

    /// Return concrete signatures that cover the parameter domain accepted by this overload.
    ///
    /// Bounded type variables use their upper bound, constrained type variables expand to one
    /// variant per constraint while preserving repeated-use correlations, and `ParamSpec`s stay
    /// correlated unless the implementation itself has gradual parameters.
    ///
    /// ```python
    /// from typing import TypeVar, overload
    ///
    /// AnyStr = TypeVar("AnyStr", str, bytes)
    ///
    /// @overload
    /// def concat(value: AnyStr, other: AnyStr) -> AnyStr: ...
    /// def concat(value: str | bytes, other: str | bytes) -> str | bytes:
    ///     return value + other
    /// ```
    fn parameter_domain_variants(
        &self,
        db: &'db dyn Db,
        gradualize_paramspec: bool,
    ) -> SmallVec<[Self; 1]> {
        let signature = self.clone().with_return_type(Type::unknown());

        let Some(generic_context) = self.generic_context else {
            return smallvec_inline![signature];
        };

        let mut typevar_choices = Vec::with_capacity(generic_context.len(db));
        let mut has_typevar_domain = false;

        for bound_typevar in generic_context.variables(db) {
            let choices: SmallVec<[Type<'db>; 2]> = if bound_typevar.is_paramspec(db) {
                if gradualize_paramspec {
                    has_typevar_domain = true;
                    std::iter::once(Type::paramspec_value_callable(
                        db,
                        Parameters::gradual_form(),
                    ))
                    .collect()
                } else {
                    std::iter::once(Type::TypeVar(bound_typevar)).collect()
                }
            } else {
                has_typevar_domain = true;
                match bound_typevar.typevar(db).require_bound_or_constraints(db) {
                    TypeVarBoundOrConstraints::UpperBound(bound) => {
                        if self.parameter_domain_variance_of(db, bound_typevar)
                            == TypeVarVariance::Invariant
                        {
                            std::iter::once(Type::TypeVar(bound_typevar)).collect()
                        } else {
                            std::iter::once(bound).collect()
                        }
                    }
                    TypeVarBoundOrConstraints::Constraints(constraints) => {
                        constraints.elements(db).iter().copied().collect()
                    }
                }
            };
            typevar_choices.push(choices);
        }

        if !has_typevar_domain {
            return smallvec_inline![signature];
        }

        typevar_choices
            .into_iter()
            .multi_cartesian_product()
            .map(|types| {
                let mapping = TypeMapping::ApplySpecialization(ApplySpecialization::Partial {
                    generic_context,
                    types: &types,
                    skip: None,
                });
                let visitor = ApplyTypeMappingVisitor::default();
                let parameters = if gradualize_paramspec
                    && let Some((prefix, _)) = self.parameters.as_paramspec_with_prefix()
                {
                    let prefix = prefix
                        .iter()
                        .map(|param| {
                            param.apply_type_mapping_impl(
                                db,
                                &mapping,
                                TypeContext::default(),
                                &visitor,
                            )
                        })
                        .collect::<Vec<_>>();
                    if prefix.is_empty() {
                        Parameters::gradual_form()
                    } else {
                        Parameters::concatenate(db, prefix, ConcatenateTail::Gradual)
                    }
                } else {
                    self.parameters.apply_type_mapping_impl(
                        db,
                        &mapping,
                        TypeContext::default(),
                        &visitor,
                    )
                };

                Self {
                    generic_context: Some(
                        mapping.update_signature_generic_context(db, generic_context),
                    ),
                    definition: self.definition,
                    parameters,
                    return_ty: Type::unknown(),
                }
            })
            .collect()
    }

    /// Return the variance of a type variable across this signature's parameter domain.
    ///
    /// A bounded type variable in an invariant parameter position cannot be replaced by its bound
    /// when expanding overload domains, because repeated uses must stay correlated.
    ///
    /// ```python
    /// from typing import overload
    ///
    /// class Box[T]: ...
    ///
    /// @overload
    /// def f[T](left: Box[T], right: Box[T]) -> T: ...
    /// def f(left: Box[object], right: Box[object]) -> object: ...
    /// ```
    fn parameter_domain_variance_of(
        &self,
        db: &'db dyn Db,
        typevar: BoundTypeVarInstance<'db>,
    ) -> TypeVarVariance {
        let mut variance = TypeVarVariance::Bivariant;

        let mut visit_parameter = |parameter: &super::Parameter<'db>| {
            if parameter.form == ParameterForm::Value {
                variance = variance.join(
                    parameter
                        .annotated_type()
                        .with_polarity(TypeVarVariance::Contravariant)
                        .variance_of(db, typevar),
                );
            }
        };

        if let Some((prefix_parameters, paramspec)) = self.parameters.as_paramspec_with_prefix() {
            for parameter in prefix_parameters {
                visit_parameter(parameter);
            }
            variance = variance.join(
                Type::TypeVar(paramspec)
                    .with_polarity(TypeVarVariance::Contravariant)
                    .variance_of(db, typevar),
            );
        } else {
            for parameter in &self.parameters {
                visit_parameter(parameter);
            }
        }

        variance
    }

    /// Return `true` if this implementation signature accepts every argument shape accepted by
    /// one overload signature.
    ///
    /// ```python
    /// from typing import overload
    ///
    /// @overload
    /// def f(x: int) -> int: ...
    /// def f(x: object) -> object:
    ///     return x
    /// ```
    pub(crate) fn is_overload_implementation_parameters_consistent_with(
        &self,
        db: &'db dyn Db,
        overload: &Self,
        has_implicit_receiver: bool,
    ) -> bool {
        self.are_implementation_parameters_compatible_with(db, overload, has_implicit_receiver)
    }

    /// Return `true` if one overload return type is assignable to this implementation return type.
    ///
    /// ```python
    /// from typing import overload
    ///
    /// @overload
    /// def f(x: int) -> int: ...
    /// def f(x: int) -> object:
    ///     return x
    /// ```
    pub(crate) fn is_overload_implementation_return_consistent_with(
        &self,
        db: &'db dyn Db,
        overload: &Self,
        has_implicit_receiver: bool,
    ) -> bool {
        let implementation_return_ty = self
            .return_type_for_argument_types_of(db, overload, has_implicit_receiver)
            .unwrap_or(self.return_ty);

        self.is_overload_return_type_assignable_for_implementation_parameters(
            db,
            overload,
            implementation_return_ty,
            has_implicit_receiver,
        )
    }

    /// Return whether an overload return is assignable to the implementation return.
    ///
    /// The synthetic call can still leave equivalent overload and implementation type variables
    /// with different identities. When the parameter relation is satisfiable, use those constraints
    /// to relate generic return types while checking the synthetic-call return type.
    ///
    /// ```python
    /// from typing import overload
    ///
    /// @overload
    /// def f[T](x: T) -> T: ...
    /// def f[T](x: T) -> T:
    ///     return x
    /// ```
    fn is_overload_return_type_assignable_for_implementation_parameters(
        &self,
        db: &'db dyn Db,
        overload: &Self,
        implementation_return_ty: Type<'db>,
        has_implicit_receiver: bool,
    ) -> bool {
        let constraints = ConstraintSetBuilder::new();
        let inferable = self.implementation_consistency_inferable_typevars(db);

        let parameter_constraints = self
            .clone()
            .with_return_type(Type::unknown())
            .when_implementation_parameters_compatible_with(
                db,
                &overload.clone().with_return_type(Type::unknown()),
                &constraints,
                has_implicit_receiver,
            );

        if !parameter_constraints.satisfied_by_all_typevars(db, &constraints, inferable) {
            // Keep return diagnostics independent when parameter compatibility has already failed.
            return overload
                .return_ty
                .when_assignable_to(db, implementation_return_ty, &constraints, inferable)
                .satisfied_by_all_typevars(db, &constraints, inferable);
        }

        // Only infer overload return type variables when both returns are generic. If the
        // implementation return is concrete, overload type variables must stay universal.
        let return_inferable = if overload.return_ty.has_typevar_or_typevar_instance(db)
            && implementation_return_ty.has_typevar_or_typevar_instance(db)
        {
            inferable.merge(
                db,
                overload.implementation_consistency_inferable_typevars(db),
            )
        } else {
            inferable
        };
        let return_constraints = overload.return_ty.when_assignable_to(
            db,
            implementation_return_ty,
            &constraints,
            return_inferable,
        );

        parameter_constraints
            .and(db, &constraints, || return_constraints)
            .satisfied_by_all_typevars(db, &constraints, inferable)
    }

    /// Return whether the first parameter is an implicit or gradual `self`/`cls` receiver.
    ///
    /// Such receivers are ignored when comparing overload and implementation call shapes because
    /// users do not pass them explicitly.
    ///
    /// ```python
    /// from typing import overload
    ///
    /// class C:
    ///     @overload
    ///     def f(self, x: int) -> int: ...
    ///     def f(self, x: object) -> object:
    ///         return x
    /// ```
    fn has_gradual_or_implicit_self_or_cls_parameter(&self) -> bool {
        self.parameters().iter().next().is_some_and(|parameter| {
            parameter
                .name()
                .is_some_and(|name| matches!(name.as_str(), "self" | "cls"))
                && (parameter.inferred_annotation || parameter.annotated_type().is_dynamic())
        })
    }

    /// Return the implementation return type for a synthetic call shaped like the overload.
    ///
    /// This lets overload consistency compare the actual return type selected by a generic
    /// implementation for the overload's argument types.
    ///
    /// ```python
    /// from typing import overload
    ///
    /// @overload
    /// def f(x: int) -> int: ...
    /// def f[T](x: T) -> T:
    ///     return x
    /// ```
    fn return_type_for_argument_types_of(
        &self,
        db: &'db dyn Db,
        overload: &Self,
        has_implicit_receiver: bool,
    ) -> Option<Type<'db>> {
        let gradual_or_implicit_receiver =
            has_implicit_receiver && self.has_gradual_or_implicit_self_or_cls_parameter();
        let arguments = overload.call_arguments_for_parameters(
            gradual_or_implicit_receiver,
            overload.positional_or_keyword_parameter_count(gradual_or_implicit_receiver),
        );

        Type::single_callable(db, self.clone())
            .try_call(db, &arguments)
            .ok()
            .map(|bindings| bindings.return_type(db))
    }

    /// Build synthetic call arguments from this signature's parameters.
    ///
    /// Positional-or-keyword parameters are emitted as both positional and keyword forms across
    /// repeated calls so the implementation is checked against every accepted argument shape.
    ///
    /// ```python
    /// from typing import overload
    ///
    /// @overload
    /// def f(x: int, y: int) -> int: ...
    /// def f(x: object, y: object) -> object:
    ///     return x
    /// ```
    fn call_arguments_for_parameters<'a>(
        &'a self,
        gradual_or_implicit_receiver: bool,
        positional_or_keyword_as_positional: usize,
    ) -> CallArguments<'a, 'db> {
        let mut positional_or_keyword_parameters_seen = 0;
        let include_variadic_parameters = positional_or_keyword_as_positional
            == self.positional_or_keyword_parameter_count(gradual_or_implicit_receiver);

        self.parameters()
            .iter()
            .enumerate()
            .filter_map(|(index, parameter)| {
                let argument = match parameter.kind() {
                    ParameterKind::PositionalOnly { .. } => Argument::Positional,
                    ParameterKind::PositionalOrKeyword { name, .. } => {
                        if index == 0 && gradual_or_implicit_receiver {
                            Argument::Positional
                        } else {
                            let argument = if positional_or_keyword_parameters_seen
                                < positional_or_keyword_as_positional
                            {
                                Argument::Positional
                            } else {
                                Argument::Keyword(name.as_str())
                            };
                            positional_or_keyword_parameters_seen += 1;
                            argument
                        }
                    }
                    ParameterKind::Variadic { .. } => {
                        if include_variadic_parameters {
                            Argument::Positional
                        } else {
                            return None;
                        }
                    }
                    ParameterKind::KeywordOnly { name, .. } => Argument::Keyword(name.as_str()),
                    ParameterKind::KeywordVariadic { .. } => {
                        Argument::Keyword("__ty_synthetic_keyword")
                    }
                };
                let annotated_type = if index == 0 && gradual_or_implicit_receiver {
                    Type::unknown()
                } else {
                    parameter.annotated_type()
                };

                Some((argument, Some(annotated_type)))
            })
            .collect()
    }

    /// Count positional-or-keyword parameters after excluding an implicit receiver.
    ///
    /// The count controls how many synthetic call variants are needed to cover the overload's
    /// positional and keyword call forms.
    ///
    /// ```python
    /// from typing import overload
    ///
    /// @overload
    /// def f(x: int, y: int) -> int: ...
    /// def f(x: object, y: object) -> object:
    ///     return x
    /// ```
    fn positional_or_keyword_parameter_count(&self, gradual_or_implicit_receiver: bool) -> usize {
        self.parameters()
            .iter()
            .enumerate()
            .filter(|(index, parameter)| {
                !(*index == 0 && gradual_or_implicit_receiver)
                    && matches!(parameter.kind(), ParameterKind::PositionalOrKeyword { .. })
            })
            .count()
    }

    /// Return a copy with the first parameter rewritten to a positional-only parameter of `ty`.
    ///
    /// This normalizes implicit or gradual method receivers before comparing an overload with its
    /// implementation.
    ///
    /// ```python
    /// from typing import overload
    ///
    /// class C:
    ///     @overload
    ///     def f(self, x: int) -> int: ...
    ///     def f(self, x: object) -> object:
    ///         return x
    /// ```
    fn with_first_parameter_type_and_positional_only(mut self, ty: Type<'db>) -> Self {
        if let Some(first) = self.parameters.value.first_mut() {
            let mut normalized = first.clone().with_annotated_type(ty);
            if let ParameterKind::PositionalOrKeyword { name, default_type } = normalized.kind {
                normalized.kind = ParameterKind::PositionalOnly {
                    name: Some(name),
                    default_type,
                };
            }
            *first = normalized;
        }
        self
    }
}
