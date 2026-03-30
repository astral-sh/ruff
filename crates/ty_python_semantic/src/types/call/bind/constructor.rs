use super::{ArgumentForms, Binding, Bindings, CallableBinding, CallableItem};
use crate::db::Db;
use crate::types::call::arguments::CallArguments;
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::generics::Specialization;
use crate::types::signatures::Parameter;
use crate::types::{
    BoundTypeVarInstance, ClassBase, ClassLiteral, ClassType, DynamicType, Type, TypeContext,
};

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
    pub(super) downstream_constructor: Option<Box<DownstreamConstructor<'db>>>,
}

impl<'db> ConstructorBinding<'db> {
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

    pub(super) fn return_type(&self, db: &'db dyn Db) -> Type<'db> {
        let constructed_instance_type = self.constructed_instance_type();
        let overload_analysis = self.constructor_overload_analysis(db);

        // For layered mixed-constructor handling (metaclass `__call__` mixed with
        // downstream constructor logic), if the downstream constructor resolves to a
        // non-instance return, that becomes the effective constructor return.
        if overload_analysis.check_downstream
            && let Some(downstream) = self.downstream_constructor()
            && let Some(constructor_class_literal) = self.constructed_class_literal(db)
        {
            let downstream_return = downstream.bindings.return_type(db);
            if !classify_constructor_return(db, constructor_class_literal, downstream_return)
                .is_instance()
            {
                return downstream_return;
            }
        }

        // If `__new__` or metaclass `__call__` produced an explicit return type, use it
        // directly rather than building an instance of the constructed class.
        if let Some(return_ty) = overload_analysis.return_type {
            return return_ty;
        }

        constructed_instance_type.apply_optional_specialization(
            db,
            self.instance_return_specialization(db, overload_analysis.check_downstream),
        )
    }

    pub(super) fn set_constructed_instance_type(&mut self, instance_type: Type<'db>) {
        self.constructor_context = self.constructor_context.with_instance_type(instance_type);
    }

    pub(super) fn set_downstream_constructor(&mut self, bindings: Bindings<'db>) {
        self.downstream_constructor = Some(Box::new(DownstreamConstructor { bindings }));
    }

    pub(super) fn match_parameters(
        &mut self,
        db: &'db dyn Db,
        arguments: &CallArguments<'_, 'db>,
        argument_forms: &mut ArgumentForms,
    ) {
        self.entry.match_parameters(db, arguments, argument_forms);

        if let Some(downstream) = self.downstream_constructor.as_mut() {
            // `init_binding.match_parameters` handles its own bound-`self` insertion, so pass the
            // original call arguments here.
            let mut init_forms = ArgumentForms::new(arguments.len());
            for init_binding in downstream.bindings.iter_callable_items_mut() {
                init_binding.match_parameters(db, arguments, &mut init_forms);
            }
        }
    }

    pub(super) fn check_types(
        &mut self,
        db: &'db dyn Db,
        constraints: &ConstraintSetBuilder<'db>,
        argument_types: &CallArguments<'_, 'db>,
        call_expression_tcx: TypeContext<'db>,
    ) -> Option<ArgumentForms> {
        self.entry
            .check_types(db, constraints, argument_types, call_expression_tcx)
    }

    pub(super) fn check_downstream_constructor(
        &mut self,
        db: &'db dyn Db,
        constraints: &ConstraintSetBuilder<'db>,
        argument_types: &CallArguments<'_, 'db>,
        call_expression_tcx: TypeContext<'db>,
        dataclass_field_specifiers: &[Type<'db>],
    ) -> bool {
        if !self.constructor_overload_analysis(db).check_downstream {
            return false;
        }

        self.downstream_constructor_mut().is_some_and(|downstream| {
            downstream
                .bindings
                .check_types_impl(
                    db,
                    constraints,
                    argument_types,
                    call_expression_tcx,
                    dataclass_field_specifiers,
                )
                .is_err()
        })
    }

    pub(super) fn checked_downstream_constructor_bindings(
        &self,
        db: &'db dyn Db,
    ) -> Option<&Bindings<'db>> {
        self.constructor_overload_analysis(db)
            .check_downstream
            .then_some(self.downstream_constructor()?)
            .map(|downstream| &downstream.bindings)
    }

    /// When inferring a specialization for a constructor return, we may have multiple matched
    /// overloads, or no matched overloads. In either case, we can get into trouble if we try to
    /// merge conflicting specializations from multiple overloads. To avoid that, prefer the
    /// specialization from the first matching overload (if any), falling back to the single
    /// overload if there is only one, and otherwise don't infer a specialization at all.
    fn preferred_overload_for_specialization(&self) -> Option<&Binding<'db>> {
        let callable = self.callable();
        callable
            .matching_overloads()
            .next()
            .map(|(_, overload)| overload)
            .or_else(|| match callable.overloads() {
                [overload] => Some(overload),
                _ => None,
            })
    }

    /// For constructors which may have downstreams (that is, metaclass `__call__` or `__new__`),
    /// analyze their overloads to determine how to handle downstream constructors.
    ///
    /// We have to analyze overloads individually rather than just relying on the resolved return
    /// type of the overall callable, because in no-matching-overload or multiple-matching-overload
    /// cases where the overload resolution algorithm might just collapse to `Unknown`, we want to
    /// make decisions based on whether all overloads return instance or non-instance types.
    fn constructor_overload_analysis(&self, db: &'db dyn Db) -> ConstructorOverloadAnalysis<'db> {
        let constructor_kind = self.constructor_kind();
        if constructor_kind.is_init() {
            return ConstructorOverloadAnalysis::default();
        }
        let Some(constructor_class_literal) = self.constructed_class_literal(db) else {
            return ConstructorOverloadAnalysis::default();
        };
        let callable = self.callable();
        let matching_overloads: Vec<_> = callable
            .matching_overloads()
            .map(|(_, overload)| overload)
            .collect();
        let selected_return_type = |check_downstream| {
            self.selected_instance_return_type(
                db,
                self.instance_return_specialization(db, check_downstream),
            )
        };
        let analyze_selected =
            |first_overload: &Binding<'db>,
             overloads: &[&Binding<'db>],
             check_downstream_override: Option<bool>| {
                let mut selected_return = true;
                let first_return_is_instance = classify_constructor_return(
                    db,
                    constructor_class_literal,
                    first_overload.return_ty,
                )
                .is_instance();
                let mut saw_instance_return = first_return_is_instance;
                let mut saw_non_instance_return = !first_return_is_instance;
                let mut first_non_instance_return =
                    (!first_return_is_instance).then_some(first_overload.return_ty);
                let mut saw_distinct_non_instance_return = false;

                for overload in overloads {
                    selected_return = false;

                    if classify_constructor_return(
                        db,
                        constructor_class_literal,
                        overload.return_ty,
                    )
                    .is_instance()
                    {
                        saw_instance_return = true;
                    } else if let Some(first_non_instance_return) = first_non_instance_return {
                        saw_non_instance_return = true;
                        saw_distinct_non_instance_return |=
                            overload.return_ty != first_non_instance_return;
                    } else {
                        saw_non_instance_return = true;
                        first_non_instance_return = Some(overload.return_ty);
                    }
                }

                if saw_instance_return {
                    let check_downstream =
                        check_downstream_override.unwrap_or(!saw_non_instance_return);
                    let return_type = selected_return
                        .then(|| selected_return_type(check_downstream))
                        .flatten();
                    return ConstructorOverloadAnalysis {
                        return_type,
                        check_downstream,
                    };
                }

                match first_non_instance_return {
                    Some(_) if saw_distinct_non_instance_return => ConstructorOverloadAnalysis {
                        return_type: Some(Type::unknown()),
                        check_downstream: false,
                    },
                    Some(first_non_instance_return) => ConstructorOverloadAnalysis {
                        return_type: Some(first_non_instance_return),
                        check_downstream: false,
                    },
                    None => {
                        let check_downstream = check_downstream_override.unwrap_or(true);
                        let return_type = selected_return
                            .then(|| selected_return_type(check_downstream))
                            .flatten();
                        ConstructorOverloadAnalysis {
                            return_type,
                            check_downstream,
                        }
                    }
                }
            };

        if let Some((first_overload, overloads)) = matching_overloads.split_first() {
            return analyze_selected(first_overload, overloads, None);
        }

        match callable.overloads() {
            [overload] => {
                analyze_selected(overload, &[], Some(!constructor_kind.is_metaclass_call()))
            }
            overloads
                if !overloads.is_empty()
                    && overloads.iter().all(|overload| {
                        classify_constructor_return(
                            db,
                            constructor_class_literal,
                            overload.return_ty,
                        )
                        .is_instance()
                    }) =>
            {
                ConstructorOverloadAnalysis {
                    return_type: None,
                    check_downstream: !constructor_kind.is_metaclass_call(),
                }
            }
            _ => ConstructorOverloadAnalysis {
                return_type: Some(callable.return_type()),
                check_downstream: false,
            },
        }
    }

    pub(super) fn map<F>(self, f: &F) -> ConstructorBinding<'db>
    where
        F: Fn(CallableBinding<'db>) -> CallableBinding<'db>,
    {
        ConstructorBinding {
            entry: f(self.entry),
            constructor_context: self.constructor_context,
            downstream_constructor: self.downstream_constructor.map(|downstream| {
                Box::new(DownstreamConstructor {
                    bindings: downstream.bindings.map_with(f),
                })
            }),
        }
    }

    /// Combine inferred specializations from this constructor and downstream constructors. The
    /// resulting specialization can be applied either to the constructed instance type or to an
    /// explicit `__new__` / `__call__` return annotation that is an instance of the constructed
    /// type or a subclass.
    fn instance_return_specialization(
        &self,
        db: &'db dyn Db,
        include_downstream: bool,
    ) -> Option<Specialization<'db>> {
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
            let callable = binding.callable();
            let overload = callable
                .matching_overloads()
                .next()
                .map(|(_, overload)| overload)
                .or_else(|| match callable.overloads() {
                    [overload] => Some(overload),
                    _ => None,
                });
            let Some(overload) = overload else {
                return;
            };
            let return_specialization = static_class_literal
                // Use the already-resolved overload return type when possible.
                .and_then(|lit| overload.return_ty.specialization_of(db, lit));
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
                            // TODO This is a hacky work-around to a situation that can occur with
                            // a case like `def __init__(self: "Class6[V1, V2]", v1: V1, v2: V2)`,
                            // where we don't currently solve across the entire call, so the self
                            // annotation gives us `V1 = T1`, `V2 = T2` (where `T1` and `T2` are
                            // the class typevars), and we consider T1 and T2 as unknowns. This
                            // will be fixed when we start building up constraint sets across the
                            // full call.
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
            // Prefer extracting the class specialization from the resolved overload return type.
            // Fall back to specialization inferred from annotated `self`, then to class-level
            // type variable mappings from the overload specialization.
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
            let Some(specialization) = specialization else {
                return;
            };
            combined = Some(match combined {
                None => specialization,
                Some(previous) => previous.combine(db, specialization),
            });
        };

        combine_binding_specialization(self);

        // Deferred downstream constructor bindings stay out-of-band for conditional validation.
        // If all matched overloads are instance-returning, include inferred specializations from
        // those deferred bindings as well.
        if include_downstream && let Some(downstream) = self.downstream_constructor() {
            for downstream_binding in downstream
                .bindings
                .iter_callable_items()
                .filter_map(CallableItem::as_constructor)
            {
                combine_binding_specialization(downstream_binding);
            }
        }

        // If constructor inference yields a specialization, compose it with the class's existing
        // specialization. Otherwise fall back to the default specialization to avoid leaking
        // inferable typevars in the constructed instance return.
        Some(
            combined
                .map(|specialization| {
                    specialization.apply_optional_specialization(db, Some(class_specialization))
                })
                .unwrap_or_else(|| class_context.default_specialization(db, None)),
        )
    }

    /// Compute the effective return type for the selected constructor overload when that overload
    /// returns an instance of the constructed class (or a subtype of it).
    ///
    /// For `Unknown` and self-like `TypeVar` returns, we treat the return as the constructed
    /// instance type directly. For explicit instance-returning annotations like `D[T]`, we apply
    /// two layers of specialization:
    ///
    /// 1. The overload's own specialization, but with class type variables preserved so we do not
    ///    eagerly collapse `D[T]` into `D[Unknown]`.
    /// 2. The constructor-wide instance specialization inferred from `__new__`, `__init__`, and
    ///    any downstream constructor checks.
    ///
    /// If the declared return annotation is not actually instance-returning, we fall back to the
    /// already-resolved overload return type instead.
    fn selected_instance_return_type(
        &self,
        db: &'db dyn Db,
        instance_return_specialization: Option<Specialization<'db>>,
    ) -> Option<Type<'db>> {
        let overload = self.preferred_overload_for_specialization()?;
        let signature_return_ty = overload.signature.return_ty.resolve_type_alias(db);

        match (self.constructor_kind(), signature_return_ty) {
            (_, ty) if ty.is_unknown() => Some(
                self.constructed_instance_type()
                    .apply_optional_specialization(db, instance_return_specialization),
            ),
            (ConstructorCallableKind::New, Type::TypeVar(typevar))
                if overload.is_self_like_constructor_return_typevar(db, typevar) =>
            {
                Some(
                    self.constructed_instance_type()
                        .apply_optional_specialization(db, instance_return_specialization),
                )
            }
            _ => {
                let return_ty = overload.signature.return_ty.apply_optional_specialization(
                    db,
                    overload.specialization.map(|specialization| {
                        self.return_annotation_specialization(db, specialization)
                    }),
                );
                if self
                    .constructed_class_literal(db)
                    .is_some_and(|class_literal| {
                        classify_constructor_return(db, class_literal, return_ty).is_instance()
                    })
                {
                    return Some(
                        return_ty.apply_optional_specialization(db, instance_return_specialization),
                    );
                }

                Some(
                    overload
                        .return_ty
                        .apply_optional_specialization(db, instance_return_specialization),
                )
            }
        }
    }

    /// Adapt an overload specialization before applying it to an explicit constructor return
    /// annotation.
    ///
    /// Overload specialization may contain defaulted `Unknown` solutions for the constructed
    /// class's own type variables. That is fine for parameter checking, but if we apply it
    /// directly to a return annotation like `D[T]`, we lose the class type variables before the
    /// constructor-wide specialization can refine them. To avoid that, this helper preserves any
    /// type variables belonging to the constructed class, while still applying substitutions for
    /// non-class variables such as method-level type parameters.
    fn return_annotation_specialization(
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

    fn downstream_constructor(&self) -> Option<&DownstreamConstructor<'db>> {
        self.downstream_constructor.as_deref()
    }

    pub(super) fn downstream_constructor_mut(&mut self) -> Option<&mut DownstreamConstructor<'db>> {
        self.downstream_constructor.as_deref_mut()
    }
}

/// Conditionally-validated downstream constructor.
///
/// Constructor call handling must defer downstream checks (`__new__`/`__init__`) until call-time
/// overload resolution determines whether an upstream return type is an instance of the class
/// being constructed. A `ConstructorBinding` for a metaclass `__call__` method might have a
/// `__new__` or `__init__` as downstream constructor; a `ConstructorBinding` for a `__new__` might
/// have `__init__` as downstream constructor.
#[derive(Debug, Clone)]
pub(super) struct DownstreamConstructor<'db> {
    /// Downstream constructor bindings to validate conditionally.
    pub(super) bindings: Bindings<'db>,
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
    /// Bindings for constructing a `T` from a call to a `type[T]`, which may have any or all of
    /// the below as downstream constructors (depending on the upper bound/constraints of `T`).
    TypeVar,
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

    fn is_metaclass_call(self) -> bool {
        matches!(self, ConstructorCallableKind::MetaclassCall)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConstructorReturnKind {
    /// Constructor method returns an instance of the class being constructed, so downstream
    /// constructor methods should be checked as normal.
    Instance,
    /// Constructor method returns a non-instance type, so downstream constructor methods should be
    /// skipped and this return type should be used as-is.
    NotInstance,
}

impl ConstructorReturnKind {
    fn is_instance(self) -> bool {
        matches!(self, ConstructorReturnKind::Instance)
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct ConstructorOverloadAnalysis<'db> {
    /// The effective constructor return, if overload resolution determined one directly. When this
    /// is `None`, callers should fall back to the constructed instance type specialized from the
    /// matched constructor bindings.
    return_type: Option<Type<'db>>,
    /// Whether downstream constructors should still be checked before the final return is
    /// determined.
    check_downstream: bool,
}

/// Return `true` if `class_ty` is a subtype of (any specialization of) `class_literal`.
fn is_subtype_of_class_literal<'db>(
    db: &'db dyn Db,
    class_ty: ClassType<'db>,
    class_literal: ClassLiteral<'db>,
) -> bool {
    class_ty
        .iter_mro(db)
        .filter_map(ClassBase::into_class)
        .any(|base| base.class_literal(db) == class_literal)
}

/// Classify a return type as either being an instance of the given `class_literal` or not, for
/// purposes of deciding whether downstream constructors should be checked. Some cases are obvious,
/// some are judgment calls (and we follow the judgment of the typing spec). For example, an
/// explicit `Any` is considered "not an instance", but an `Unknown` is considered "an instance".
fn classify_constructor_return<'db>(
    db: &'db dyn Db,
    class_literal: ClassLiteral<'db>,
    return_ty: Type<'db>,
) -> ConstructorReturnKind {
    match return_ty.resolve_type_alias(db) {
        Type::Union(union) => {
            for element in union.elements(db) {
                match classify_constructor_return(db, class_literal, *element) {
                    ConstructorReturnKind::NotInstance => {
                        return ConstructorReturnKind::NotInstance;
                    }
                    ConstructorReturnKind::Instance => {}
                }
            }
            ConstructorReturnKind::Instance
        }
        Type::Intersection(intersection) => {
            for element in intersection.iter_positive(db) {
                match classify_constructor_return(db, class_literal, element) {
                    ConstructorReturnKind::Instance => return ConstructorReturnKind::Instance,
                    ConstructorReturnKind::NotInstance => {}
                }
            }
            ConstructorReturnKind::NotInstance
        }
        // Spec says an explicit `Any` return type should be considered non-instance.
        Type::Dynamic(DynamicType::Any) => ConstructorReturnKind::NotInstance,
        // But a missing return annotation should be considered instance.
        // TODO currently this is also true for explicit annotations that resolve to `Unknown`;
        // should it be? Other type checkers also treat it this way.
        Type::Dynamic(_) => ConstructorReturnKind::Instance,
        // A `Never` constructor return is terminal and does not run downstream construction.
        Type::Never => ConstructorReturnKind::NotInstance,
        Type::NominalInstance(instance) => {
            if is_subtype_of_class_literal(db, instance.class(db), class_literal) {
                ConstructorReturnKind::Instance
            } else {
                ConstructorReturnKind::NotInstance
            }
        }
        _ => ConstructorReturnKind::NotInstance,
    }
}

impl<'db> Binding<'db> {
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
    ///     `__new__` is annotated as `type[T]`, replace it with the instance type. Although
    ///     these cases should be resolved correctly later by the specialization machinery, we need
    ///     to unwrap these early in case the constructed instance type is generic. Literal
    ///     promotion and reverse inference from type context need to be able to see into the
    ///     generic instance type.
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
