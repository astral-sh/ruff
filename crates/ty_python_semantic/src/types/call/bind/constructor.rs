use super::{ArgumentForms, Binding, Bindings, CallableBinding, CallableItem};
use crate::db::Db;
use crate::types::call::arguments::CallArguments;
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::generics::Specialization;
use crate::types::signatures::Parameter;
use crate::types::{BoundTypeVarInstance, ClassLiteral, DynamicType, Type, TypeContext};

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

    pub(super) fn check_types(
        &mut self,
        db: &'db dyn Db,
        constraints: &ConstraintSetBuilder<'db>,
        argument_types: &CallArguments<'_, 'db>,
        call_expression_tcx: TypeContext<'db>,
    ) -> Option<ArgumentForms> {
        let forms = self
            .entry
            .check_types(db, constraints, argument_types, call_expression_tcx);

        // Now that we've fully checked our own callable, we can determine whether downstream
        // constructors should be checked or not.
        if !self.should_check_downstream(db) {
            // If not, we can discard the downstream constructor bindings entirely.
            self.downstream_constructor = None;
        }

        forms
    }

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

    /// For constructors which may have downstreams (that is, metaclass `__call__` or `__new__`),
    /// analyze their overloads to determine whether to check downstream constructors.
    ///
    /// We analyze overloads individually rather than just relying on the resolved return type of
    /// the overall callable, because in multiple-matching-overload cases where the overload
    /// resolution algorithm might just collapse to `Unknown`, we want to make a more informed
    /// decision based on whether all overloads return instance types, or not.
    ///
    /// This must be called after we've checked types on `self.entry` (in `self.check_types()`), so
    /// we know which overloads matched.
    fn should_check_downstream(&self, db: &'db dyn Db) -> bool {
        let constructor_kind = self.constructor_kind();
        if constructor_kind.is_init() || self.downstream_constructor().is_none() {
            return false;
        }

        let callable = self.callable();

        if callable.as_result().is_err() {
            return false;
        }

        let constructed_instance_type = self.constructed_instance_type();
        let constructor_class_literal = self.constructed_class_literal(db);

        // If any matching overload returns the constructed instance type itself, or an instance of
        // the constructed class, we need to check downstream constructors.
        callable.matching_overloads().any(|(_, overload)| {
            overload.return_ty == constructed_instance_type
                || constructor_class_literal.is_some_and(|class_literal| {
                    constructor_returns_instance(db, class_literal, overload.return_ty)
                })
        })
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
            .map(|instance| instance.class(db).class_literal(db))
    }

    fn constructor_kind(&self) -> ConstructorCallableKind {
        self.constructor_context.kind()
    }
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
