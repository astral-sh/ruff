use std::cell::RefCell;

use super::{ArgumentForms, Binding, Bindings, CallableBinding, CallableItem};
use crate::FxOrderSet;
use crate::db::Db;
use crate::types::call::arguments::CallArguments;
use crate::types::callable::CallableTypeKind;
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::generics::{GenericContext, Specialization};
use crate::types::signatures::{
    CallableSignature, Parameter, ParameterKind, Parameters, Signature,
};
use crate::types::tuple::{TupleSpecBuilder, TupleType};
use crate::types::visitor::{TypeCollector, TypeVisitor, walk_type_with_recursion_guard};
use crate::types::{
    BoundTypeVarInstance, CallableType, ClassLiteral, DynamicType, IntersectionType, Type,
    TypeContext,
};
use rustc_hash::{FxHashMap, FxHashSet};

/// Bindings for a constructor call.
///
/// The `entry` is the first-called constructor method (could be a metaclass `__call__`, a
/// `__new__`, or an `__init__`, depending what is present on the constructed class). Its
/// `downstream_constructor` may link to the next downstream constructor, if present (e.g.
/// metaclass `__call__` could have `__new__` or `__init__` as downstream; `__new__` could have
/// `__init__` as downstream; `__init__` cannot have a downstream). The downstream constructor is
/// only checked if the upstream returns an instance of the class being constructed. (A downstream
/// constructor may itself have a downstream constructor, in the case where metaclass `__call__`,
/// `__new__`, and `__init__` are all present.)
#[derive(Debug, Clone)]
pub(super) struct ConstructorBinding<'db> {
    /// The `CallableBinding` for this individual constructor method.
    pub(super) entry: CallableBinding<'db>,
    /// Context for the constructor callable: the instance type being constructed and the kind of
    /// constructor method.
    pub(super) constructor_context: ConstructorContext<'db>,
    /// The next downstream constructor method, if any, to be (conditionally) checked after this
    /// one.
    pub(super) downstream_constructor: Option<Box<Bindings<'db>>>,
}

impl<'db> ConstructorBinding<'db> {
    pub(super) fn new(
        entry: CallableBinding<'db>,
        constructor_context: ConstructorContext<'db>,
    ) -> Self {
        Self {
            entry,
            constructor_context,
            downstream_constructor: None,
        }
    }

    pub(super) fn context(&self) -> ConstructorContext<'db> {
        self.constructor_context
    }

    pub(super) fn constructed_instance_type(&self) -> Type<'db> {
        self.constructor_context.instance_type()
    }

    pub(super) fn callable(&self) -> &CallableBinding<'db> {
        &self.entry
    }

    pub(super) fn callable_mut(&mut self) -> &mut CallableBinding<'db> {
        &mut self.entry
    }

    pub(super) fn set_constructed_instance_type(&mut self, instance_type: Type<'db>) {
        self.constructor_context = self.constructor_context.with_instance_type(instance_type);
    }

    pub(super) fn set_downstream_constructor(&mut self, bindings: Bindings<'db>) {
        self.downstream_constructor = Some(Box::new(bindings));
    }

    /// Match parameters for this constructor method and downstream constructors.
    pub(super) fn match_parameters(
        &mut self,
        db: &'db dyn Db,
        arguments: &CallArguments<'_, 'db>,
        argument_forms: &mut ArgumentForms,
    ) {
        self.entry.match_parameters(db, arguments, argument_forms);

        // We don't know at this point whether we'll need to check downstream constructors or not
        // (since we can't resolve return types yet), so we match parameters for all downstream
        // constructors; this may be needed for argument type contexts.
        if let Some(downstream) = self.downstream_constructor.as_mut() {
            downstream.match_parameters_in_place(db, arguments);
        }
    }

    /// Check types for this constructor method, and then decide (based on the resolved return
    /// types) whether we should continue considering downstream constructors or discard them.
    pub(super) fn check_types(
        &mut self,
        db: &'db dyn Db,
        constraints: &ConstraintSetBuilder<'db>,
        argument_types: &CallArguments<'_, 'db>,
        call_expression_tcx: TypeContext<'db>,
    ) -> Option<ArgumentForms> {
        /// For constructors which may have downstreams (that is, metaclass `__call__` or `__new__`),
        /// analyze their overloads to determine whether to check downstream constructors.
        ///
        /// We analyze overloads individually rather than just relying on the resolved return type of
        /// the overall callable, because in multiple-matching-overload cases where the overload
        /// resolution algorithm might just collapse to `Unknown`, we want to make a more informed
        /// decision based on whether all overloads return instance types, or not.
        fn should_check_downstream<'db>(
            binding: &ConstructorBinding<'db>,
            db: &'db dyn Db,
        ) -> bool {
            let constructor_kind = binding.constructor_kind();
            if constructor_kind.is_init() || binding.downstream_constructor().is_none() {
                return false;
            }

            let callable = binding.callable();

            if callable.as_result().is_err() {
                return false;
            }

            let constructed_instance_type = binding.constructed_instance_type();
            let constructor_class_literal = binding.constructed_class_literal(db);

            // If any matching overload returns the constructed instance type itself, or an instance of
            // the constructed class, we need to check downstream constructors.
            callable.matching_overloads().any(|(_, overload)| {
                overload.return_ty == constructed_instance_type
                    || constructor_class_literal.is_some_and(|class_literal| {
                        constructor_returns_instance(db, class_literal, overload.return_ty)
                    })
            })
        }

        let forms = self
            .entry
            .check_types(db, constraints, argument_types, call_expression_tcx);

        // Now that we've fully checked our own callable, we can determine whether downstream
        // constructors should be checked or not.
        if !should_check_downstream(self, db) {
            // If not, we can discard the downstream constructor bindings entirely.
            self.downstream_constructor = None;
        }

        forms
    }

    /// Check types for downstream constructors, if any.
    pub(super) fn check_downstream_constructor(
        &mut self,
        db: &'db dyn Db,
        constraints: &ConstraintSetBuilder<'db>,
        argument_types: &CallArguments<'_, 'db>,
        call_expression_tcx: TypeContext<'db>,
        dataclass_field_specifiers: &[Type<'db>],
    ) {
        if let Some(downstream) = self.downstream_constructor_mut() {
            // We discard the result here, but that's fine; it's `report_diagnostics` and
            // `as_result` that ultimately matter.
            let _ = downstream.check_types_impl(
                db,
                constraints,
                argument_types,
                call_expression_tcx,
                dataclass_field_specifiers,
            );
        }
    }

    pub(super) fn downstream_constructor(&self) -> Option<&Bindings<'db>> {
        self.downstream_constructor.as_deref()
    }

    pub(super) fn downstream_constructor_mut(&mut self) -> Option<&mut Bindings<'db>> {
        self.downstream_constructor.as_deref_mut()
    }

    /// Builds the reduced callable for this `functools.partial(...)` constructor binding.
    ///
    /// This merges the entry constructor callable with any deferred downstream constructor
    /// checking so the reduced partial signature reflects both `__new__` and `__init__`.
    pub(super) fn functools_partial_callable<'a>(
        &self,
        db: &'db dyn Db,
        wrapped_callable_ty: Type<'db>,
        partial_overload: &mut Binding<'db>,
        bound_call_arguments: &CallArguments<'a, 'db>,
    ) -> Option<CallableType<'db>> {
        let entry_callable =
            self.entry
                .functools_partial_callable(db, partial_overload, bound_call_arguments)?;
        let Some(downstream) = self.downstream_constructor() else {
            return Some(entry_callable);
        };

        let Some(downstream_item) = downstream.single_item() else {
            return Some(entry_callable);
        };
        let Some(downstream_callable) = downstream_item.functools_partial_callable(
            db,
            wrapped_callable_ty,
            partial_overload,
            bound_call_arguments,
        ) else {
            return Some(entry_callable);
        };

        let Some(constructor_class_literal) = self.constructed_class_literal(db) else {
            return Some(merge_constructor_partial_callables(
                db,
                entry_callable,
                downstream_callable,
            ));
        };

        let (instance_signatures, non_instance_signatures): (Vec<_>, Vec<_>) = entry_callable
            .signatures(db)
            .iter()
            .cloned()
            .partition(|signature| {
                constructor_returns_instance(db, constructor_class_literal, signature.return_ty)
            });

        let mut merged_signatures = non_instance_signatures;

        if !instance_signatures.is_empty()
            && let Some(merged_instance_callable) = merged_bound_constructor_partial_callables(
                db,
                wrapped_callable_ty,
                &self.entry,
                downstream_item.callable(),
                constructor_class_literal,
                bound_call_arguments,
            )
        {
            merged_signatures.extend(merged_instance_callable.signatures(db).iter().cloned());
        } else if !instance_signatures.is_empty() {
            let instance_entry_callable = CallableType::new(
                db,
                CallableSignature::from_overloads(instance_signatures),
                CallableTypeKind::Regular,
            );
            merged_signatures.extend(
                merge_constructor_partial_callables(
                    db,
                    instance_entry_callable,
                    downstream_callable,
                )
                .signatures(db)
                .iter()
                .cloned(),
            );
        }

        if merged_signatures.is_empty() {
            Some(entry_callable)
        } else {
            Some(CallableType::new(
                db,
                CallableSignature::from_overloads(merged_signatures),
                CallableTypeKind::Regular,
            ))
        }
    }

    pub(super) fn map<F>(self, f: &F) -> ConstructorBinding<'db>
    where
        F: Fn(CallableBinding<'db>) -> CallableBinding<'db>,
    {
        // We only ever map constructor bindings before we set their downstream constructor; don't
        // spend complexity on dead code.
        assert!(
            self.downstream_constructor.is_none(),
            "map should not be used on a ConstructorBinding with downstream constructor"
        );
        ConstructorBinding {
            entry: f(self.entry),
            constructor_context: self.constructor_context,
            downstream_constructor: None,
        }
    }

    /// Compute the overall effective return type of this `ConstructorBinding`.
    pub(super) fn return_type(&self, db: &'db dyn Db) -> Type<'db> {
        let constructed_instance_type = self.constructed_instance_type();

        // If we are checking downstream constructors, and the downstream constructor resolves to a
        // non-instance return, that becomes the effective constructor return. This can only happen
        // if we are a metaclass `__call__` returning an instance of the constructed class, but
        // that class has a downstream `__new__` that does not.
        //
        // TODO: If the metaclass `__call__` return type in this scenario is explicitly annotated
        // with e.g. `-> T` where `cls: type[T]` (not just un-annotated), should this actually be
        // an error? It seems to imply that the metaclass `__call__` is violating its own return
        // annotation. But no other type checker considers it an error, and it probably rarely if
        // ever comes up.)
        if let Some(downstream) = self.downstream_constructor()
            && let Some(constructor_class_literal) = self.constructed_class_literal(db)
        {
            let downstream_return = downstream.return_type(db);
            if !constructor_returns_instance(db, constructor_class_literal, downstream_return) {
                return downstream_return;
            }
        }

        // If `__new__` or metaclass `__call__` produced an explicit return type, use it
        // directly rather than building an instance of the constructed class.
        if let Some(return_ty) = self.explicit_return_type(db) {
            return return_ty;
        }

        constructed_instance_type
            .apply_optional_specialization(db, self.instance_return_specialization(db))
    }

    fn first_matching_overload(&self) -> Option<&Binding<'db>> {
        self.callable()
            .matching_overloads()
            .map(|(_, overload)| overload)
            .next()
    }

    /// Combine inferred specializations from this constructor and downstream constructors. The
    /// resulting specialization can be applied either to the constructed instance type or to an
    /// explicit `__new__` / `__call__` return annotation that is an instance of the constructed
    /// type or a subclass.
    fn instance_return_specialization(&self, db: &'db dyn Db) -> Option<Specialization<'db>> {
        let constructed_instance_type = self.constructed_instance_type();
        // This will be `None` if we're constructing a non-generic class. If we're constructing a
        // non-specialized generic class (`C(...)`), it'll be the identity specialization. If we're
        // constructing an already-specialized generic alias (`C[str](...)`), it'll be the
        // specialization of that alias.
        let class_specialization = constructed_instance_type.class_specialization(db)?;
        let static_class_literal = self
            .constructed_class_literal(db)
            .and_then(ClassLiteral::as_static);
        let class_context = class_specialization.generic_context(db);

        let mut combined: Option<Specialization<'db>> = None;
        let mut combine_binding_specialization = |binding: &ConstructorBinding<'db>| {
            let Some(overload) = binding.first_matching_overload() else {
                return;
            };
            let return_specialization = static_class_literal
                // Use the already-resolved overload return type when possible.
                .and_then(|lit| overload.return_ty.specialization_of(db, lit));

            // TODO All this handling of return-specialization vs self-specialization is a hacky
            // work-around to a situation that can occur with a case like `def __init__(self:
            // "Class6[V1, V2]", v1: V1, v2: V2)`, where we don't currently solve across the entire
            // call, so the self annotation gives us `V1 = T1`, `V2 = T2` (where `T1` and `T2` are
            // the class typevars), and we consider T1 and T2 as unknowns. This will be fixed when
            // we start building up constraint sets across the full call. We should be able to just
            // use the return specialization and eliminate all this.
            let return_specialization_is_informative =
                return_specialization.is_some_and(|specialization| {
                    class_context.variables(db).any(|class_typevar| {
                        specialization
                            .get(db, class_typevar)
                            .is_some_and(|mapped_ty| !mapped_ty.is_unknown())
                    })
                });
            let self_parameter_specialization = static_class_literal.and_then(|lit| {
                let self_param_ty = overload.signature.parameters().get(0)?.annotated_type();
                let resolved_self_param_ty = overload
                    .specialization
                    .map(|specialization| self_param_ty.apply_specialization(db, specialization))
                    .unwrap_or(self_param_ty);
                resolved_self_param_ty.specialization_of(db, lit)
            });
            let refined_self_parameter_specialization =
                self_parameter_specialization.map(|specialization| {
                    let types: Box<[_]> = specialization
                        .types(db)
                        .iter()
                        .copied()
                        .map(|mapped_ty| {
                            let without_unknown =
                                mapped_ty.filter_union(db, |element| !element.is_unknown());
                            let mapped_ty = if without_unknown.is_never() {
                                mapped_ty
                            } else {
                                without_unknown
                            };
                            mapped_ty.promote(db)
                        })
                        .collect();
                    Specialization::new(
                        db,
                        specialization.generic_context(db),
                        types,
                        specialization.materialization_kind(db),
                        None,
                    )
                });
            let specialization = if return_specialization_is_informative {
                return_specialization
            } else {
                refined_self_parameter_specialization
                    .or(return_specialization)
                    .or_else(|| {
                        overload
                            .specialization
                            .and_then(|s| s.restrict(db, class_context))
                    })
            };
            // end TODO

            let Some(specialization) = specialization else {
                return;
            };
            combined = Some(match combined {
                None => specialization,
                Some(previous) => previous.combine(db, specialization),
            });
        };

        combine_binding_specialization(self);

        if let Some(downstream) = self.downstream_constructor() {
            for downstream_binding in downstream
                .iter_callable_items()
                .filter_map(CallableItem::as_constructor)
            {
                combine_binding_specialization(downstream_binding);
            }
        }

        combined.map(|specialization| {
            specialization.apply_optional_specialization(db, Some(class_specialization))
        })
    }

    /// Compute the explicit return type from a `__new__` or metaclass `__call__`.
    ///
    /// This method is only used for `__new__` and metaclass `__call__`, which (unlike `__init__`)
    /// can have explicit return types that determine the result of the constructor call.
    ///
    /// Returning `None` means "no explicit return type override, just construct an instance of the
    /// constructed class; default constructor behavior."
    ///
    /// This must be called only after downstream constructor bindings have been type-checked,
    /// because instance-returning constructor paths may incorporate downstream specializations.
    fn explicit_return_type(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        if self.constructor_kind().is_init() || self.constructed_class_literal(db).is_none() {
            return None;
        }

        let matching_overloads = self
            .callable()
            .matching_overloads()
            .map(|(_, overload)| overload);

        // If we have matching overloads, only those are candidates. If all overloads failed,
        // consider all overloads' return types. (This increases the chances of an `Unknown`
        // return, but still preserves more precise returns in unambiguous cases.)
        if matching_overloads.clone().next().is_none() {
            self.analyze_overload_returns(db, self.callable().overloads().iter())
        } else {
            self.analyze_overload_returns(db, matching_overloads)
        }
    }

    /// Combine return types from an iterator of overloads to determine the effective explicit
    /// return type of the constructor call. See `explicit_return_type` for details.
    fn analyze_overload_returns<'a>(
        &self,
        db: &'db dyn Db,
        overloads: impl IntoIterator<Item = &'a Binding<'db>>,
    ) -> Option<Type<'db>>
    where
        'db: 'a,
    {
        // If we see both instance and non-instance returns, we return Unknown.
        // If we see multiple different non-instance returns, we also return Unknown.
        // If we see multiple instance returns, we return `None` (we know we are constructing an
        // instance of the constructed class, but we don't have more precise information.)
        // Otherwise, we return the single non-instance return if present, or the single
        // instance return we saw (this is different from simply returning `None` since it
        // could be a specific subclass of the constructed class.)
        let mut sole_instance_return = None;
        let mut saw_instance_return = false;
        let mut non_instance_return = None;
        for overload in overloads {
            let (return_ty, is_instance_return) = self.single_overload_return(db, overload);
            if is_instance_return {
                if saw_instance_return {
                    sole_instance_return = None;
                } else {
                    sole_instance_return = Some(return_ty);
                    saw_instance_return = true;
                }
            } else {
                non_instance_return = Some(match non_instance_return {
                    None => return_ty,
                    Some(previous) if previous == return_ty => return_ty,
                    Some(_) => Type::unknown(),
                });
            }
        }
        if let Some(non_instance_return) = non_instance_return {
            if saw_instance_return {
                Some(Type::unknown())
            } else {
                Some(non_instance_return)
            }
        } else {
            sole_instance_return
        }
    }

    /// Compute the effective return type for the given constructor overload. This differs from the
    /// ordinary return type in that, if the overload returns an instance type, we apply a broader
    /// specialization derived (possibly) also from downstream constructors.
    ///
    /// Return a tuple of `(return_type, is_instance_return)`.
    fn single_overload_return(
        &self,
        db: &'db dyn Db,
        overload: &Binding<'db>,
    ) -> (Type<'db>, bool) {
        let return_ty = overload
            .unspecialized_return_type(db)
            .apply_optional_specialization(
                db,
                overload.specialization.map(|specialization| {
                    self.unspecialize_class_type_variables(db, specialization)
                }),
            );
        if self
            .constructed_class_literal(db)
            .is_some_and(|class_literal| constructor_returns_instance(db, class_literal, return_ty))
        {
            return (
                return_ty
                    .apply_optional_specialization(db, self.instance_return_specialization(db)),
                true,
            );
        }

        (overload.return_ty, false)
    }

    /// "Un-specialize" class-level type variables in an overload specialization.
    ///
    /// Per-overload specialization may contain defaulted (typically `Unknown`) solutions for the
    /// constructed class's own type variables. That is fine for parameter checking, but when
    /// inferring a type for a constructed instance, we need to also consider other sources of
    /// specialization, such as downstream constructors, but we lose the class type variables
    /// before the constructor-wide specialization can refine them. To avoid that, this helper
    /// identity-specializes any type variables belonging to the constructed class, while
    /// preserving specializations of method-level type parameters.
    ///
    /// TODO: This could be made simpler if we more clearly marked unsolved typevars in a
    /// specialization; we could probably avoid this entirely and just combine the specializations.
    fn unspecialize_class_type_variables(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Specialization<'db> {
        let Some(class_context) = self
            .constructed_instance_type()
            .class_specialization(db)
            .map(|specialization| specialization.generic_context(db))
        else {
            return specialization;
        };

        let class_variables: Vec<_> = class_context
            .variables(db)
            .map(|typevar| typevar.identity(db))
            .collect();
        let types: Box<[_]> = specialization
            .types(db)
            .iter()
            .copied()
            .zip(specialization.generic_context(db).variables(db))
            .map(|(mapped_ty, typevar)| {
                if class_variables.contains(&typevar.identity(db)) {
                    Type::TypeVar(typevar)
                } else {
                    mapped_ty
                }
            })
            .collect();

        Specialization::new(
            db,
            specialization.generic_context(db),
            types,
            specialization.materialization_kind(db),
            None,
        )
    }

    fn constructed_class_literal(&self, db: &'db dyn Db) -> Option<ClassLiteral<'db>> {
        self.constructed_instance_type()
            .as_nominal_instance()
            // TODO may need to handle `Type::KnownInstance` here as well?
            .map(|instance| instance.class(db).class_literal(db))
    }

    fn constructor_kind(&self) -> ConstructorCallableKind {
        self.constructor_context.kind()
    }
}

/// Merges the entry and downstream constructor-reduced callables into one callable.
///
/// We prefer merged signatures that still admit at least one satisfiable call, but keep
/// unsatisfiable fallbacks when that is all the constructor pair can express.
fn merge_constructor_partial_callables<'db>(
    db: &'db dyn Db,
    entry: CallableType<'db>,
    downstream: CallableType<'db>,
) -> CallableType<'db> {
    let mut satisfiable = Vec::new();
    let mut fallback = Vec::new();
    let mut seen_overloads = FxHashSet::default();

    for entry_signature in entry.signatures(db) {
        for downstream_signature in downstream.signatures(db) {
            let merged =
                merge_constructor_partial_signature(db, entry_signature, downstream_signature);
            let dedup_key = merged.clone().with_definition(None);
            if !seen_overloads.insert(dedup_key) {
                continue;
            }

            if constructor_partial_signature_has_possible_call(&merged) {
                satisfiable.push(merged);
            } else {
                fallback.push(merged);
            }
        }
    }

    let overloads = if satisfiable.is_empty() {
        fallback
    } else {
        satisfiable
    };

    CallableType::new(
        db,
        CallableSignature::from_overloads(overloads),
        CallableTypeKind::Regular,
    )
}

/// Replays the already-bound constructor arguments across entry/downstream overload pairs.
///
/// This preserves overload correlations when merging constructor partial signatures, rather than
/// blindly forming the cartesian product of already-reduced overloads.
fn merged_bound_constructor_partial_callables<'db>(
    db: &'db dyn Db,
    wrapped_callable_ty: Type<'db>,
    entry: &CallableBinding<'db>,
    downstream: &CallableBinding<'db>,
    constructor_class_literal: ClassLiteral<'db>,
    bound_call_arguments: &CallArguments<'_, 'db>,
) -> Option<CallableType<'db>> {
    let entry_indexes = entry.partial_signature_source_overload_indexes()?;
    let downstream_indexes = downstream.partial_signature_source_overload_indexes()?;
    let mut merged_signatures = Vec::new();
    let mut seen_signatures = FxHashSet::default();

    for entry_index in entry_indexes {
        let Some(entry_overload) = entry.overloads().get(entry_index) else {
            continue;
        };

        if !constructor_returns_instance(db, constructor_class_literal, entry_overload.return_ty) {
            continue;
        }

        for downstream_index in &downstream_indexes {
            let Some(downstream_overload) = downstream.overloads().get(*downstream_index) else {
                continue;
            };

            let entry_signature = entry_overload.signature.bind_self(db, entry.bound_type);
            let downstream_signature = downstream_overload
                .signature
                .bind_self(db, downstream.bound_type);
            let merged_signature = merge_constructor_partial_parameters_signature(
                db,
                &entry_signature,
                &downstream_signature,
                entry_signature.return_ty,
            );
            let mut merged_binding = Binding::single(wrapped_callable_ty, merged_signature);
            let mut argument_forms = ArgumentForms::new(bound_call_arguments.len());
            merged_binding.match_parameters(db, bound_call_arguments, &mut argument_forms);
            for signature in merged_binding.partially_applied_signatures(db, bound_call_arguments) {
                let dedup_key = signature.clone().with_definition(None);
                if seen_signatures.insert(dedup_key) {
                    merged_signatures.push(signature);
                }
            }
        }
    }

    if merged_signatures.is_empty() {
        None
    } else {
        Some(CallableType::new(
            db,
            CallableSignature::from_overloads(merged_signatures),
            CallableTypeKind::Regular,
        ))
    }
}

/// Merges constructor parameters while keeping the entry return type fixed.
///
/// This is used when re-binding the original constructor arguments, where the entry overload has
/// already determined the instance-like return type to keep.
fn merge_constructor_partial_parameters_signature<'db>(
    db: &'db dyn Db,
    entry: &Signature<'db>,
    downstream: &Signature<'db>,
    return_ty: Type<'db>,
) -> Signature<'db> {
    merge_constructor_partial_signature_with(db, entry, downstream, |_, _, _| return_ty)
}

/// Merges two reduced constructor signatures into a single partial signature.
fn merge_constructor_partial_signature<'db>(
    db: &'db dyn Db,
    entry: &Signature<'db>,
    downstream: &Signature<'db>,
) -> Signature<'db> {
    merge_constructor_partial_signature_with(db, entry, downstream, |db, entry, downstream| {
        combine_constructor_partial_return_types(db, entry.return_ty, downstream.return_ty)
    })
}

fn merge_constructor_partial_signature_with<'db>(
    db: &'db dyn Db,
    entry: &Signature<'db>,
    downstream: &Signature<'db>,
    return_ty: impl FnOnce(&'db dyn Db, &Signature<'db>, &Signature<'db>) -> Type<'db>,
) -> Signature<'db> {
    let ConstructorPartialMerge {
        parameter_matches,
        downstream_used,
        entry,
        downstream,
    } = prepare_constructor_partial_merge(db, entry, downstream);

    Signature::new_generic(
        GenericContext::merge_optional(db, entry.generic_context, downstream.generic_context),
        merge_constructor_partial_parameters_with_matches(
            db,
            entry.parameters(),
            downstream.parameters(),
            &parameter_matches,
            &downstream_used,
        ),
        return_ty(db, &entry, &downstream),
    )
}

struct ConstructorPartialMerge<'db> {
    parameter_matches: ConstructorPartialParameterMatches,
    downstream_used: Vec<bool>,
    entry: Signature<'db>,
    downstream: Signature<'db>,
}

fn prepare_constructor_partial_merge<'db>(
    db: &'db dyn Db,
    entry: &Signature<'db>,
    downstream: &Signature<'db>,
) -> ConstructorPartialMerge<'db> {
    let (parameter_matches, downstream_used) =
        find_constructor_partial_parameter_matches(entry.parameters(), downstream.parameters());
    let downstream = canonicalize_constructor_partial_shared_typevars(db, downstream, entry);
    let entry = specialize_constructor_partial_signature_correlations(
        db,
        entry,
        &downstream,
        &parameter_matches,
    );
    let downstream = specialize_constructor_partial_signature_correlations(
        db,
        &downstream,
        &entry,
        &reverse_constructor_partial_parameter_matches(&parameter_matches, downstream.parameters()),
    );

    ConstructorPartialMerge {
        parameter_matches,
        downstream_used,
        entry,
        downstream,
    }
}

/// Rebinds shared logical type variables onto the entry signature's binding context.
///
/// Constructor methods can mention the same legacy `TypeVar` in separate signatures, which binds
/// that variable independently in each method. Before we merge the reduced signatures, remap any
/// downstream occurrences onto the entry method's bound type variable so later intersections keep
/// the shared `TypeVar` correlated.
fn canonicalize_constructor_partial_shared_typevars<'db>(
    db: &'db dyn Db,
    signature: &Signature<'db>,
    canonical: &Signature<'db>,
) -> Signature<'db> {
    let (Some(signature_generic_context), Some(canonical_generic_context)) =
        (signature.generic_context, canonical.generic_context)
    else {
        return signature.clone();
    };

    let canonical_typevars_by_identity: FxHashMap<_, _> = canonical_generic_context
        .variables(db)
        .map(|typevar| (typevar.typevar(db).identity(db), typevar))
        .collect();

    let mut changed = false;
    let specialization = signature_generic_context.specialize_recursive(
        db,
        signature_generic_context.variables(db).map(|typevar| {
            let replacement = canonical_typevars_by_identity
                .get(&typevar.typevar(db).identity(db))
                .copied()
                .filter(|replacement| replacement.identity(db) != typevar.identity(db))
                .unwrap_or(typevar);
            changed |= replacement != typevar;
            Some(Type::TypeVar(replacement))
        }),
    );

    if !changed {
        return signature.clone();
    }

    signature.apply_specialization(db, specialization)
}

#[derive(Debug, Clone, Copy)]
enum ConstructorPartialParameterMatchKind {
    KeywordName { same_positional_order: bool },
    PositionalOrder,
    SameNameConflict,
    Variadic,
}

type ConstructorPartialParameterMatches =
    Vec<Option<(usize, ConstructorPartialParameterMatchKind)>>;

/// Matches constructor parameters across entry and downstream signatures.
///
/// Parameters are paired by shared keyword names first, then by positional order, then by plain
/// name conflicts, and finally by variadic shape.
fn find_constructor_partial_parameter_matches<'db>(
    entry: &Parameters<'db>,
    downstream: &Parameters<'db>,
) -> (ConstructorPartialParameterMatches, Vec<bool>) {
    let mut entry_matches = vec![None; entry.len()];
    let mut downstream_used = vec![false; downstream.len()];

    for (entry_index, entry_parameter) in entry.iter().enumerate() {
        let Some(entry_keyword_name) = entry_parameter.keyword_name() else {
            continue;
        };
        let Some((downstream_index, _)) =
            downstream
                .iter()
                .enumerate()
                .find(|(downstream_index, downstream_parameter)| {
                    !downstream_used[*downstream_index]
                        && downstream_parameter.keyword_name() == Some(entry_keyword_name)
                })
        else {
            continue;
        };

        entry_matches[entry_index] = Some((
            downstream_index,
            ConstructorPartialParameterMatchKind::KeywordName {
                same_positional_order: entry_index == downstream_index,
            },
        ));
        downstream_used[downstream_index] = true;
    }

    let mut downstream_positional_start = 0;
    for (entry_index, entry_parameter) in entry.iter().enumerate() {
        if entry_matches[entry_index].is_some() || !entry_parameter.is_positional() {
            continue;
        }

        let Some(downstream_index) = next_unmatched_positional_parameter_index(
            downstream,
            &downstream_used,
            &mut downstream_positional_start,
        ) else {
            continue;
        };

        entry_matches[entry_index] = Some((
            downstream_index,
            ConstructorPartialParameterMatchKind::PositionalOrder,
        ));
        downstream_used[downstream_index] = true;
    }

    for (entry_index, entry_parameter) in entry.iter().enumerate() {
        if entry_matches[entry_index].is_some() {
            continue;
        }

        let Some(entry_name) = entry_parameter.name() else {
            continue;
        };
        let Some((downstream_index, _)) =
            downstream
                .iter()
                .enumerate()
                .find(|(downstream_index, downstream_parameter)| {
                    !downstream_used[*downstream_index]
                        && downstream_parameter.name() == Some(entry_name)
                })
        else {
            continue;
        };

        entry_matches[entry_index] = Some((
            downstream_index,
            ConstructorPartialParameterMatchKind::SameNameConflict,
        ));
        downstream_used[downstream_index] = true;
    }

    for (entry_index, entry_parameter) in entry.iter().enumerate() {
        if entry_matches[entry_index].is_some() {
            continue;
        }

        let Some((downstream_index, _)) =
            downstream
                .iter()
                .enumerate()
                .find(|(downstream_index, downstream_parameter)| {
                    !downstream_used[*downstream_index]
                        && matches!(
                            (entry_parameter.kind(), downstream_parameter.kind()),
                            (
                                ParameterKind::Variadic { .. },
                                ParameterKind::Variadic { .. }
                            ) | (
                                ParameterKind::KeywordVariadic { .. },
                                ParameterKind::KeywordVariadic { .. }
                            )
                        )
                })
        else {
            continue;
        };

        entry_matches[entry_index] = Some((
            downstream_index,
            ConstructorPartialParameterMatchKind::Variadic,
        ));
        downstream_used[downstream_index] = true;
    }

    (entry_matches, downstream_used)
}

/// Reverses an entry-to-downstream parameter match table for the opposite specialization pass.
fn reverse_constructor_partial_parameter_matches(
    parameter_matches: &ConstructorPartialParameterMatches,
    reversed: &Parameters<'_>,
) -> ConstructorPartialParameterMatches {
    let mut reversed_matches = vec![None; reversed.len()];
    for (parameter_index, matched_parameter) in parameter_matches.iter().enumerate() {
        let Some((matched_index, match_kind)) = matched_parameter else {
            continue;
        };
        reversed_matches[*matched_index] = Some((parameter_index, *match_kind));
    }
    reversed_matches
}

/// Specializes correlated type variables in one constructor signature from the other.
///
/// When matched parameters constrain a type variable on one side to a narrower type on the other,
/// we apply that specialization before merging the signatures.
fn specialize_constructor_partial_signature_correlations<'db>(
    db: &'db dyn Db,
    signature: &Signature<'db>,
    counterpart: &Signature<'db>,
    parameter_matches: &ConstructorPartialParameterMatches,
) -> Signature<'db> {
    let Some(generic_context) = signature.generic_context else {
        return signature.clone();
    };

    let mut specialization_types = Vec::with_capacity(generic_context.len(db));
    let mut changed = false;
    for bound_typevar in generic_context.variables(db) {
        let typevar_identity = bound_typevar.typevar(db).identity(db);
        let mut counterpart_types =
            signature
                .parameters()
                .iter()
                .enumerate()
                .filter_map(|(parameter_index, parameter)| {
                    if parameter.annotated_type() != Type::TypeVar(bound_typevar) {
                        return None;
                    }
                    let (matched_index, _) = parameter_matches
                        .get(parameter_index)
                        .and_then(|matched_parameter| *matched_parameter)?;
                    counterpart
                        .parameters()
                        .get(matched_index)
                        .map(Parameter::annotated_type)
                });
        let Some(first_counterpart_type) = counterpart_types.next() else {
            specialization_types.push(Some(Type::TypeVar(bound_typevar)));
            continue;
        };
        let specialized_type = IntersectionType::from_elements(
            db,
            std::iter::once(first_counterpart_type).chain(counterpart_types),
        );
        if specialized_type == Type::TypeVar(bound_typevar)
            || specialized_type.references_typevar(db, typevar_identity)
        {
            specialization_types.push(Some(Type::TypeVar(bound_typevar)));
            continue;
        }

        changed = true;
        specialization_types.push(Some(specialized_type));
    }

    if !changed {
        return signature.clone();
    }

    let specialization = generic_context.specialize_recursive(db, specialization_types);
    let mut specialized_signature = signature.apply_specialization(db, specialization);
    specialized_signature.generic_context =
        retain_referenced_constructor_partial_typevars(db, &specialized_signature);
    specialized_signature
}

/// Keeps only the generic parameters that remain referenced after correlation specialization.
fn retain_referenced_constructor_partial_typevars<'db>(
    db: &'db dyn Db,
    signature: &Signature<'db>,
) -> Option<GenericContext<'db>> {
    let generic_context = signature.generic_context?;
    let referenced_typevars: Vec<_> = generic_context
        .variables(db)
        .filter(|bound_typevar| {
            let typevar_identity = bound_typevar.typevar(db).identity(db);
            signature.parameters().iter().any(|parameter| {
                parameter
                    .annotated_type()
                    .references_typevar(db, typevar_identity)
            }) || signature.return_ty.references_typevar(db, typevar_identity)
        })
        .collect();
    if referenced_typevars.is_empty() {
        None
    } else {
        Some(GenericContext::from_typevar_instances(
            db,
            referenced_typevars,
        ))
    }
}

/// Merges matched and unmatched constructor parameters into a single reduced parameter list.
fn merge_constructor_partial_parameters_with_matches<'db>(
    db: &'db dyn Db,
    entry: &Parameters<'db>,
    downstream: &Parameters<'db>,
    entry_matches: &ConstructorPartialParameterMatches,
    downstream_used: &[bool],
) -> Parameters<'db> {
    let entry_used: Vec<_> = entry_matches.iter().map(Option::is_some).collect();
    let mut merged_parameters = Vec::with_capacity(entry.len().max(downstream.len()));
    for (entry_index, entry_parameter) in entry.iter().enumerate() {
        if let Some((downstream_index, match_kind)) = entry_matches[entry_index] {
            if let Some(downstream_parameter) = downstream.get(downstream_index) {
                merged_parameters.push(merge_constructor_partial_parameter(
                    db,
                    entry_parameter,
                    downstream_parameter,
                    match_kind,
                ));
                continue;
            }
        }

        if let Some(parameter) = merge_unmatched_constructor_partial_parameter(
            entry_parameter,
            downstream,
            downstream_used,
        ) {
            merged_parameters.push(parameter);
        }
    }

    for (downstream_index, downstream_parameter) in downstream.iter().enumerate() {
        if !downstream_used[downstream_index] {
            if let Some(parameter) = merge_unmatched_constructor_partial_parameter(
                downstream_parameter,
                entry,
                &entry_used,
            ) {
                merged_parameters.push(parameter);
            }
        }
    }

    reorder_constructor_partial_parameters(db, merged_parameters)
}

/// Returns the next unused positional parameter index, advancing the scan cursor.
fn next_unmatched_positional_parameter_index(
    parameters: &Parameters<'_>,
    used: &[bool],
    start: &mut usize,
) -> Option<usize> {
    let (index, _parameter) = parameters
        .iter()
        .enumerate()
        .skip(*start)
        .find(|(index, parameter)| !used[*index] && parameter.is_positional())?;

    *start = index + 1;
    Some(index)
}

/// Normalizes merged constructor parameters into Python signature ordering.
fn reorder_constructor_partial_parameters<'db>(
    db: &'db dyn Db,
    parameters: Vec<Parameter<'db>>,
) -> Parameters<'db> {
    let mut positional_only = Vec::new();
    let mut positional_or_keyword = Vec::new();
    let mut variadic = Vec::new();
    let mut keyword_only = Vec::new();
    let mut keyword_variadic = Vec::new();

    for parameter in parameters {
        match parameter.kind() {
            ParameterKind::PositionalOnly { .. } => positional_only.push(parameter),
            ParameterKind::PositionalOrKeyword { .. } => positional_or_keyword.push(parameter),
            ParameterKind::Variadic { .. } => variadic.push(parameter),
            ParameterKind::KeywordOnly { .. } => keyword_only.push(parameter),
            ParameterKind::KeywordVariadic { .. } => keyword_variadic.push(parameter),
        }
    }

    Parameters::new(
        db,
        positional_only
            .into_iter()
            .chain(positional_or_keyword)
            .chain(variadic)
            .chain(keyword_only)
            .chain(keyword_variadic),
    )
}

/// Merges a matched pair of constructor parameters into the parameter shown in the partial.
fn merge_constructor_partial_parameter<'db>(
    db: &'db dyn Db,
    entry: &Parameter<'db>,
    downstream: &Parameter<'db>,
    match_kind: ConstructorPartialParameterMatchKind,
) -> Parameter<'db> {
    let default_type = match (entry.default_type(), downstream.default_type()) {
        (Some(entry_default), Some(downstream_default)) => Some(
            combine_constructor_partial_return_types(db, entry_default, downstream_default),
        ),
        _ => None,
    };

    let (parameter, annotated_type) = match match_kind {
        ConstructorPartialParameterMatchKind::KeywordName {
            same_positional_order,
        } => (
            merge_constructor_partial_keyword_parameter(entry, downstream, same_positional_order),
            combine_constructor_partial_parameter_types(
                db,
                entry.annotated_type(),
                downstream.annotated_type(),
            ),
        ),
        ConstructorPartialParameterMatchKind::PositionalOrder => (
            merge_constructor_partial_positional_parameter(entry, downstream),
            combine_constructor_partial_parameter_types(
                db,
                entry.annotated_type(),
                downstream.annotated_type(),
            ),
        ),
        ConstructorPartialParameterMatchKind::SameNameConflict => (entry.clone(), Type::Never),
        ConstructorPartialParameterMatchKind::Variadic => (
            entry.clone(),
            combine_constructor_partial_parameter_types(
                db,
                entry.annotated_type(),
                downstream.annotated_type(),
            ),
        ),
    };

    parameter
        .with_annotated_type(annotated_type)
        .with_optional_default_type(default_type)
}

/// Merges constructor parameters that were matched by keyword name.
fn merge_constructor_partial_keyword_parameter<'db>(
    entry: &Parameter<'db>,
    downstream: &Parameter<'db>,
    same_positional_order: bool,
) -> Parameter<'db> {
    let shared_name = entry
        .keyword_name()
        .filter(|entry_name| downstream.keyword_name() == Some(*entry_name))
        .cloned()
        .unwrap_or_else(|| {
            entry
                .keyword_name()
                .or_else(|| downstream.keyword_name())
                .cloned()
                .expect("keyword-matched parameters must have a shared keyword name")
        });

    match (entry.kind(), downstream.kind()) {
        (ParameterKind::PositionalOrKeyword { .. }, ParameterKind::PositionalOrKeyword { .. })
            if same_positional_order =>
        {
            Parameter::positional_or_keyword(shared_name)
        }
        (ParameterKind::PositionalOrKeyword { .. }, ParameterKind::PositionalOrKeyword { .. }) => {
            Parameter::keyword_only(shared_name)
        }
        (ParameterKind::PositionalOrKeyword { .. }, ParameterKind::KeywordOnly { .. })
        | (ParameterKind::KeywordOnly { .. }, ParameterKind::PositionalOrKeyword { .. })
        | (ParameterKind::KeywordOnly { .. }, ParameterKind::KeywordOnly { .. }) => {
            Parameter::keyword_only(shared_name)
        }
        _ => entry.clone(),
    }
}

/// Merges constructor parameters that were matched by positional order.
fn merge_constructor_partial_positional_parameter<'db>(
    entry: &Parameter<'db>,
    downstream: &Parameter<'db>,
) -> Parameter<'db> {
    let positional_name = entry.name().cloned().or_else(|| downstream.name().cloned());

    match (entry.kind(), downstream.kind()) {
        (ParameterKind::PositionalOnly { .. }, ParameterKind::PositionalOnly { .. })
        | (ParameterKind::PositionalOnly { .. }, ParameterKind::PositionalOrKeyword { .. })
        | (ParameterKind::PositionalOrKeyword { .. }, ParameterKind::PositionalOnly { .. }) => {
            Parameter::positional_only(positional_name)
        }
        (
            ParameterKind::PositionalOrKeyword { name, .. },
            ParameterKind::PositionalOrKeyword { .. },
        ) if downstream.keyword_name() == Some(name) => {
            Parameter::positional_or_keyword(name.clone())
        }
        (ParameterKind::PositionalOrKeyword { .. }, ParameterKind::PositionalOrKeyword { .. }) => {
            Parameter::positional_only(positional_name)
        }
        _ => entry.clone(),
    }
}

/// Projects an unmatched constructor parameter into the merged partial signature.
///
/// If the other side can no longer satisfy this parameter, we keep it as `Never` unless it was
/// already optional.
fn merge_unmatched_constructor_partial_parameter<'db>(
    parameter: &Parameter<'db>,
    other_parameters: &Parameters<'db>,
    other_used: &[bool],
) -> Option<Parameter<'db>> {
    let other_has_variadic =
        has_unmatched_constructor_partial_variadic(other_parameters, other_used);
    let other_has_keyword_variadic =
        has_unmatched_constructor_partial_keyword_variadic(other_parameters, other_used);
    let default_type = parameter.default_type();

    match parameter.kind() {
        ParameterKind::PositionalOnly { name, .. } => {
            if other_has_variadic {
                Some(
                    Parameter::positional_only(name.clone())
                        .with_annotated_type(parameter.annotated_type())
                        .with_optional_default_type(default_type),
                )
            } else if default_type.is_some() {
                None
            } else {
                Some(Parameter::positional_only(name.clone()).with_annotated_type(Type::Never))
            }
        }
        ParameterKind::PositionalOrKeyword { name, .. } => {
            let parameter = match (other_has_variadic, other_has_keyword_variadic) {
                (true, true) => Some(
                    Parameter::positional_or_keyword(name.clone())
                        .with_annotated_type(parameter.annotated_type()),
                ),
                (true, false) => Some(
                    Parameter::positional_only(Some(name.clone()))
                        .with_annotated_type(parameter.annotated_type()),
                ),
                (false, true) => Some(
                    Parameter::keyword_only(name.clone())
                        .with_annotated_type(parameter.annotated_type()),
                ),
                (false, false) if default_type.is_some() => None,
                (false, false) => Some(
                    Parameter::positional_or_keyword(name.clone()).with_annotated_type(Type::Never),
                ),
            };
            parameter.map(|parameter| parameter.with_optional_default_type(default_type))
        }
        ParameterKind::KeywordOnly { name, .. } => {
            if other_has_keyword_variadic {
                Some(
                    Parameter::keyword_only(name.clone())
                        .with_annotated_type(parameter.annotated_type())
                        .with_optional_default_type(default_type),
                )
            } else if default_type.is_some() {
                None
            } else {
                Some(Parameter::keyword_only(name.clone()).with_annotated_type(Type::Never))
            }
        }
        ParameterKind::Variadic { .. } | ParameterKind::KeywordVariadic { .. } => None,
    }
}

/// Returns whether an unmatched `*args` parameter remains on the other signature.
fn has_unmatched_constructor_partial_variadic(parameters: &Parameters<'_>, used: &[bool]) -> bool {
    parameters
        .iter()
        .enumerate()
        .any(|(index, parameter)| !used[index] && parameter.is_variadic())
}

/// Returns whether an unmatched `**kwargs` parameter remains on the other signature.
fn has_unmatched_constructor_partial_keyword_variadic(
    parameters: &Parameters<'_>,
    used: &[bool],
) -> bool {
    parameters
        .iter()
        .enumerate()
        .any(|(index, parameter)| !used[index] && parameter.is_keyword_variadic())
}

/// Combines two constructor component types, preferring the narrower subtype when possible.
fn combine_constructor_partial_return_types<'db>(
    db: &'db dyn Db,
    entry: Type<'db>,
    downstream: Type<'db>,
) -> Type<'db> {
    if entry.is_subtype_of(db, downstream) {
        entry
    } else if downstream.is_subtype_of(db, entry) {
        downstream
    } else {
        IntersectionType::from_two_elements(db, entry, downstream)
    }
}

/// Combines two matched constructor parameter types for a merged `partial` signature.
///
/// Exact tuple parameters keep their element-wise correlations instead of collapsing to a single
/// coarse intersection, which lets merged constructor signatures preserve shared legacy
/// `TypeVar` constraints across tuple positions.
fn combine_constructor_partial_parameter_types<'db>(
    db: &'db dyn Db,
    entry: Type<'db>,
    downstream: Type<'db>,
) -> Type<'db> {
    if let (Some(entry_tuple), Some(downstream_tuple)) = (
        entry.exact_tuple_instance_spec(db),
        downstream.exact_tuple_instance_spec(db),
    ) && let Some(intersection) =
        TupleSpecBuilder::from(entry_tuple.as_ref()).intersect(db, downstream_tuple.as_ref())
    {
        return specialize_constructor_partial_tuple_correlations(
            db,
            Type::tuple(TupleType::new(db, &intersection.build())),
        );
    }

    combine_constructor_partial_return_types(db, entry, downstream)
}

/// Re-applies shared `TypeVar` constraints to element-wise tuple intersections.
///
/// After intersecting exact tuple specs, any repeated bare bound type variables are narrowed by
/// the full set of tuple elements that constrain them so the reduced constructor signature keeps
/// the original tuple correlation visible at the `partial` call site.
fn specialize_constructor_partial_tuple_correlations<'db>(
    db: &'db dyn Db,
    tuple_ty: Type<'db>,
) -> Type<'db> {
    let Some(tuple_spec) = tuple_ty.exact_tuple_instance_spec(db) else {
        return tuple_ty;
    };

    let referenced_typevars = collect_constructor_partial_typevars(
        db,
        tuple_spec.as_ref().all_elements().iter().copied(),
    );
    if referenced_typevars.is_empty() {
        return tuple_ty;
    }

    let narrowed_typevars: FxHashMap<_, _> = referenced_typevars
        .iter()
        .filter_map(|typevar| {
            let typevar_identity = typevar.typevar(db).identity(db);
            let mut constraining_elements = tuple_spec
                .as_ref()
                .all_elements()
                .iter()
                .filter(|element| element.references_typevar(db, typevar_identity))
                .copied();
            let first_element = constraining_elements.next()?;
            let narrowed = IntersectionType::from_elements(
                db,
                std::iter::once(first_element).chain(constraining_elements),
            );
            (narrowed != Type::TypeVar(*typevar)).then_some((typevar.identity(db), narrowed))
        })
        .collect();
    if narrowed_typevars.is_empty() {
        return tuple_ty;
    }

    let mut builder = TupleSpecBuilder::from(tuple_spec.as_ref());
    let rewrite_element = |element: &mut Type<'db>| {
        if let Type::TypeVar(typevar) = *element
            && let Some(narrowed) = narrowed_typevars.get(&typevar.identity(db))
        {
            *element = *narrowed;
        }
    };

    match &mut builder {
        TupleSpecBuilder::Fixed(elements) => {
            for element in elements {
                rewrite_element(element);
            }
        }
        TupleSpecBuilder::Variable {
            prefix,
            variable,
            suffix,
        } => {
            for element in prefix {
                rewrite_element(element);
            }
            rewrite_element(variable);
            for element in suffix {
                rewrite_element(element);
            }
        }
    }

    Type::tuple(TupleType::new(db, &builder.build()))
}

/// Collects the bound type variables referenced by a sequence of constructor parameter types.
fn collect_constructor_partial_typevars<'db>(
    db: &'db dyn Db,
    types: impl IntoIterator<Item = Type<'db>>,
) -> FxOrderSet<BoundTypeVarInstance<'db>> {
    struct CollectBoundTypeVars<'db> {
        typevars: RefCell<FxOrderSet<BoundTypeVarInstance<'db>>>,
        recursion_guard: TypeCollector<'db>,
    }

    impl<'db> TypeVisitor<'db> for CollectBoundTypeVars<'db> {
        fn should_visit_lazy_type_attributes(&self) -> bool {
            false
        }

        fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
            walk_type_with_recursion_guard(db, ty, self, &self.recursion_guard);
        }

        fn visit_bound_type_var_type(
            &self,
            _db: &'db dyn Db,
            bound_typevar: BoundTypeVarInstance<'db>,
        ) {
            self.typevars.borrow_mut().insert(bound_typevar);
        }
    }

    let collector = CollectBoundTypeVars {
        typevars: RefCell::new(FxOrderSet::default()),
        recursion_guard: TypeCollector::default(),
    };

    for ty in types {
        collector.visit_type(db, ty);
    }

    collector.typevars.into_inner()
}

/// Returns whether a merged constructor partial signature still has at least one possible call.
fn constructor_partial_signature_has_possible_call(signature: &Signature<'_>) -> bool {
    signature.parameters().iter().all(|parameter| {
        !(parameter.annotated_type().is_never()
            && parameter.default_type().is_none()
            && matches!(
                parameter.kind(),
                ParameterKind::PositionalOnly { .. }
                    | ParameterKind::PositionalOrKeyword { .. }
                    | ParameterKind::KeywordOnly { .. }
            ))
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ConstructorContext<'db> {
    instance_type: Type<'db>,
    kind: ConstructorCallableKind,
}

impl<'db> ConstructorContext<'db> {
    pub(super) fn new(instance_type: Type<'db>, kind: ConstructorCallableKind) -> Self {
        Self {
            instance_type,
            kind,
        }
    }

    fn with_instance_type(self, instance_type: Type<'db>) -> Self {
        Self {
            instance_type,
            ..self
        }
    }

    fn instance_type(self) -> Type<'db> {
        self.instance_type
    }

    fn kind(self) -> ConstructorCallableKind {
        self.kind
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConstructorCallableKind {
    /// A metaclass `__call__` method.
    MetaclassCall,
    /// A `__new__` constructor.
    New,
    /// An `__init__` method.
    Init,
}

impl ConstructorCallableKind {
    fn is_init(self) -> bool {
        matches!(self, ConstructorCallableKind::Init)
    }
}

/// Classify a return type as either being an instance of the given `class_literal` or not, for
/// purposes of deciding whether downstream constructors should be checked. Some cases are obvious,
/// some are judgment calls (and we follow the judgment of the typing spec). For example, an
/// explicit `Any` is considered "not an instance", but an `Unknown` is considered "an instance".
fn constructor_returns_instance<'db>(
    db: &'db dyn Db,
    class_literal: ClassLiteral<'db>,
    return_ty: Type<'db>,
) -> bool {
    match return_ty.resolve_type_alias(db) {
        Type::Union(union) => union
            .elements(db)
            .iter()
            .all(|element| constructor_returns_instance(db, class_literal, *element)),
        Type::Intersection(intersection) => intersection
            .iter_positive(db)
            .any(|element| constructor_returns_instance(db, class_literal, element)),
        // Spec says an explicit `Any` return type should be considered non-instance.
        Type::Dynamic(DynamicType::Any) => false,
        // But a missing return annotation should be considered instance.
        // TODO currently this is also true for explicit annotations that resolve to `Unknown`;
        // should it be? Other type checkers also treat it this way.
        Type::Dynamic(_) => true,
        // A `Never` constructor return is terminal and does not run downstream construction.
        Type::Never => false,
        Type::NominalInstance(instance) => instance
            .class(db)
            .is_subtype_of_class_literal(db, class_literal),
        // We don't need to handle `ProtocolInstance` here, since the only way a protocol can be
        // instantiated is if a nominal class inherits it. If the nominal class inherits a
        // `__new__` from the protocol, either that `__new__` will return `Self` or equivalent,
        // in which case we'll already solve it to the subclass and consider it an instance
        // type, or it will return an explicit annotation of the protocol type itself, in which
        // case we shouldn't (and don't) consider it an instance of the subclass.
        _ => false,
    }
}

impl<'db> Binding<'db> {
    /// Is a type variable returned from a constructor method a representation of the self type?
    ///
    /// Handles `typing.Self` annotations and `__new__` methods returning `T` where `self:
    /// type[T]`.
    fn is_self_like_constructor_return_typevar(
        &self,
        db: &'db dyn Db,
        return_typevar: BoundTypeVarInstance<'db>,
    ) -> bool {
        if return_typevar.typevar(db).is_self(db) {
            return true;
        }

        let Some(cls_parameter_ty) = self
            .signature
            .parameters()
            .get(0)
            .map(Parameter::annotated_type)
        else {
            return false;
        };

        let Type::SubclassOf(subclass_of) = cls_parameter_ty else {
            return false;
        };
        let Some(cls_typevar) = subclass_of.into_type_var() else {
            return false;
        };

        cls_typevar.typevar(db).identity(db) == return_typevar.typevar(db).identity(db)
    }

    pub(super) fn set_constructor_context(
        &mut self,
        db: &'db dyn Db,
        constructor_context: ConstructorContext<'db>,
    ) {
        self.constructor_context = Some(constructor_context);
        self.return_ty = self.initial_return_type(db);
    }

    pub(super) fn initial_return_type(&self, db: &'db dyn Db) -> Type<'db> {
        self.unspecialized_return_type(db)
    }

    /// Return the declared return type after constructor normalization, but before applying any
    /// specialization inferred for this overload.
    pub(super) fn unspecialized_return_type(&self, db: &'db dyn Db) -> Type<'db> {
        self.normalized_constructor_return(db)
            .unwrap_or(self.signature.return_ty)
    }

    /// Normalize constructor return type. There are a few special cases we have to handle for
    /// constructors:
    ///
    ///   * `__init__` methods always return `None`, but for the purposes of type inference we want
    ///     to treat them as returning the constructed instance type.
    ///
    ///   * If a `__new__` method (or metaclass `__call__`) has no annotated return type (or is
    ///     annotated with an unknown return type), treat it as returning the constructed instance
    ///     type.
    ///
    ///   * If a `__new__` method returns `typing.Self` or `T` where the first parameter to
    ///     `__new__` is annotated as `type[T]`, replace it with the instance type.
    ///
    /// Although these cases should be resolved correctly later by the specialization machinery, we
    /// need to unwrap these early in case the constructed instance type is generic. Literal
    /// promotion and reverse inference from type context need to be able to see into the generic
    /// instance type.
    ///
    /// Return `None` if this is not a constructor call.
    pub(crate) fn normalized_constructor_return(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        let constructor_context = self.constructor_context?;
        let instance_type = constructor_context.instance_type();

        match (
            constructor_context.kind(),
            self.signature.return_ty.resolve_type_alias(db),
        ) {
            (ConstructorCallableKind::Init, _) => Some(instance_type),
            (_, ty) if ty.is_unknown() => Some(instance_type),
            (ConstructorCallableKind::New, Type::TypeVar(typevar))
                if self.is_self_like_constructor_return_typevar(db, typevar) =>
            {
                Some(instance_type)
            }
            _ => Some(self.signature.return_ty),
        }
    }
}
