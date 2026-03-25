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

    fn constructed_instance_type(&self) -> Type<'db> {
        self.constructor_context.instance_type()
    }

    fn constructed_class_literal(&self, db: &'db dyn Db) -> Option<ClassLiteral<'db>> {
        self.constructed_instance_type()
            .as_nominal_instance()
            .map(|instance| instance.class(db).class_literal(db))
    }

    pub(super) fn callable(&self) -> &CallableBinding<'db> {
        &self.entry
    }

    pub(super) fn callable_mut(&mut self) -> &mut CallableBinding<'db> {
        &mut self.entry
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

    /// Combine inferred specializations from all matched overloads of this constructor and
    /// downstream constructors, and apply the combined specialization to the constructed instance
    /// type. This is used as the return type of the constructor call in the common case, assuming
    /// that no metaclass `__call__` or `__new__` overload returns a non-instance type that takes
    /// precedence.
    fn instance_return_type(&self, db: &'db dyn Db) -> Type<'db> {
        let constructed_instance_type = self.constructed_instance_type();
        let Some(class_specialization) = constructed_instance_type.class_specialization(db) else {
            return constructed_instance_type;
        };
        let static_class_literal = self
            .constructed_class_literal(db)
            .and_then(ClassLiteral::as_static);
        let class_context = class_specialization.generic_context(db);

        let mut combined: Option<Specialization<'db>> = None;
        let mut combine_binding_specialization = |binding: &ConstructorBinding<'db>| {
            // For constructors, prefer the first matching overload (declaration order) to avoid
            // merging incompatible constructor specializations. For deferred `__init__` paths, we
            // still want partial generic inference from a single unmatched overload (e.g. missing
            // required arguments) so constructor specialization can reflect inferred arguments.
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
                // Fast path: use the already-resolved overload return type when possible.
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
        if let Some(downstream) = self.checkable_downstream_constructor(db) {
            for downstream_binding in downstream
                .bindings
                .iter_callable_items()
                .filter_map(CallableItem::as_constructor)
            {
                combine_binding_specialization(downstream_binding);
            }
        }

        // If constructor inference yields a specialization, rebuild the instance from the class's
        // identity specialization so explicit aliases like `C[int]` can still be remapped by
        // `__new__` return types (e.g. `C[list[T]]`).
        if let Some(specialization) = combined {
            let specialization =
                specialization.apply_optional_specialization(db, Some(class_specialization));
            if let Some(static_class_literal) = static_class_literal {
                let remapped_class =
                    static_class_literal.apply_specialization(db, |_| specialization);
                return Type::instance(db, remapped_class);
            }
            return constructed_instance_type.apply_specialization(db, specialization);
        }

        // If constructor inference doesn't yield a specialization, fall back to the default
        // specialization to avoid leaking inferable typevars in the constructed instance.
        let specialization = class_context.default_specialization(db, None);
        constructed_instance_type.apply_specialization(db, specialization)
    }

    pub(super) fn return_type(&self, db: &'db dyn Db) -> Type<'db> {
        let constructed_instance_type = self.constructed_instance_type();
        let mut single_relevant_overload = None;

        // If any matched overload's signature return type, when resolved with the inferred
        // specialization, is a non-instance type (e.g. `__new__[S] -> S` with `S` inferred as
        // `str`), use that resolved type directly. This handles arbitrary `__new__` return
        // types like `S`, `list[S]`, etc.
        let constructor_class = constructed_instance_type
            .as_nominal_instance()
            .map(|inst| inst.class(db));
        let constructor_class_literal =
            constructor_class.map(|class_ty| class_ty.class_literal(db));
        if let Some(constructor_class_literal) = constructor_class_literal {
            let callable = self.callable();
            let is_instance_of_constructor = |return_ty: Type<'db>| {
                classify_constructor_return(db, constructor_class_literal, return_ty).is_instance()
            };

            // `__init__` is a post-construction validator and does not determine the
            // constructor return type.
            if !self.constructor_kind().is_init() {
                match analyze_constructor_overloads(db, constructor_class_literal, callable) {
                    ConstructorOverloadAnalysis::NoMatchingOverloads {
                        all_instance_returns: false,
                    } => return Type::unknown(),
                    ConstructorOverloadAnalysis::NoMatchingOverloads {
                        all_instance_returns: true,
                    } => {}
                    ConstructorOverloadAnalysis::Relevant(relevant_overloads) => {
                        single_relevant_overload = relevant_overloads.single_relevant_overload;

                        match relevant_overloads.return_kind {
                            ConstructorOverloadReturns::AllSameNonInstance(return_ty) => {
                                return return_ty;
                            }
                            ConstructorOverloadReturns::DivergentNonInstance => {
                                return Type::unknown();
                            }
                            ConstructorOverloadReturns::AllInstance
                            | ConstructorOverloadReturns::Mixed => {
                                // For layered mixed-constructor handling (metaclass `__call__`
                                // mixed with downstream constructor logic), if the downstream
                                // constructor resolves to a non-instance return, that becomes the
                                // effective constructor return.
                                if let Some(downstream) = self.checkable_downstream_constructor(db)
                                {
                                    let downstream_return = downstream.bindings.return_type(db);
                                    if !is_instance_of_constructor(downstream_return) {
                                        return downstream_return;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let combined_return = self.instance_return_type(db);
        if let (Some(constructor_class_literal), Some(overload)) =
            (constructor_class_literal, single_relevant_overload)
        {
            let outcome = constructor_return_outcome(db, constructor_class_literal, overload);
            if outcome.kind.is_instance()
                && outcome.resolved_return.is_subtype_of(db, combined_return)
            {
                return outcome.resolved_return;
            }
        }

        combined_return
    }

    fn constructor_kind(&self) -> ConstructorCallableKind {
        self.constructor_context.kind()
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
        self.checkable_downstream_constructor_mut(db)
            .is_some_and(|downstream| {
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
        self.checkable_downstream_constructor(db)
            .map(|downstream| &downstream.bindings)
    }

    fn downstream_constructor(&self) -> Option<&DownstreamConstructor<'db>> {
        self.downstream_constructor.as_deref()
    }

    pub(super) fn downstream_constructor_mut(&mut self) -> Option<&mut DownstreamConstructor<'db>> {
        self.downstream_constructor.as_deref_mut()
    }

    fn checkable_downstream_constructor(
        &self,
        db: &'db dyn Db,
    ) -> Option<&DownstreamConstructor<'db>> {
        self.should_check_downstream_constructor(db)
            .then_some(self.downstream_constructor()?)
    }

    fn checkable_downstream_constructor_mut(
        &mut self,
        db: &'db dyn Db,
    ) -> Option<&mut DownstreamConstructor<'db>> {
        self.should_check_downstream_constructor(db)
            .then_some(self.downstream_constructor_mut()?)
    }

    fn should_check_downstream_constructor(&self, db: &'db dyn Db) -> bool {
        if self.downstream_constructor().is_none() {
            return false;
        }
        let Some(constructor_class_literal) = self.constructed_class_literal(db) else {
            return false;
        };

        let mut matching = self.entry.matching_overloads();

        if matching.next().is_some_and(|(_, overload)| {
            overload_returns_instance(db, constructor_class_literal, overload)
        }) && matching
            .all(|(_, overload)| overload_returns_instance(db, constructor_class_literal, overload))
        {
            return true;
        }

        // If metaclass `__call__` doesn't match, don't check `__new__` or `__init__`. But if
        // `__new__` doesn't match (but is instance-returning), we also check `__init__`.
        if self.constructor_kind().is_metaclass_call() {
            return false;
        }

        self.entry
            .overloads
            .iter()
            .all(|overload| overload_returns_instance(db, constructor_class_literal, overload))
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

#[derive(Debug, Clone, Copy)]
struct ConstructorReturnOutcome<'db> {
    resolved_return: Type<'db>,
    kind: ConstructorReturnKind,
}

#[derive(Debug, Clone, Copy)]
enum ConstructorOverloadReturns<'db> {
    AllInstance,
    AllSameNonInstance(Type<'db>),
    DivergentNonInstance,
    Mixed,
}

#[derive(Debug, Clone, Copy)]
struct RelevantConstructorOverloads<'a, 'db> {
    single_relevant_overload: Option<&'a Binding<'db>>,
    return_kind: ConstructorOverloadReturns<'db>,
}

#[derive(Debug, Clone, Copy)]
enum ConstructorOverloadAnalysis<'a, 'db> {
    Relevant(RelevantConstructorOverloads<'a, 'db>),
    NoMatchingOverloads { all_instance_returns: bool },
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

fn constructor_return_outcome<'db>(
    db: &'db dyn Db,
    class_literal: ClassLiteral<'db>,
    overload: &Binding<'db>,
) -> ConstructorReturnOutcome<'db> {
    let kind = classify_constructor_return(db, class_literal, overload.return_ty);
    ConstructorReturnOutcome {
        resolved_return: overload.return_ty,
        kind,
    }
}

fn overload_returns_instance<'db>(
    db: &'db dyn Db,
    class_literal: ClassLiteral<'db>,
    overload: &Binding<'db>,
) -> bool {
    constructor_return_outcome(db, class_literal, overload)
        .kind
        .is_instance()
}

fn analyze_relevant_constructor_overloads<'a, 'db, I>(
    db: &'db dyn Db,
    class_literal: ClassLiteral<'db>,
    overloads: I,
) -> Option<RelevantConstructorOverloads<'a, 'db>>
where
    I: IntoIterator<Item = &'a Binding<'db>>,
{
    let mut overloads = overloads.into_iter();
    let first_overload = overloads.next()?;
    let first_outcome = constructor_return_outcome(db, class_literal, first_overload);

    let mut single_relevant_overload = Some(first_overload);
    let mut saw_instance_return = first_outcome.kind.is_instance();
    let mut first_non_instance_return =
        (!first_outcome.kind.is_instance()).then_some(first_outcome.resolved_return);
    let mut saw_distinct_non_instance_return = false;

    for overload in overloads {
        single_relevant_overload = None;

        let outcome = constructor_return_outcome(db, class_literal, overload);
        if outcome.kind.is_instance() {
            saw_instance_return = true;
        } else if let Some(first_non_instance_return) = first_non_instance_return {
            saw_distinct_non_instance_return |=
                outcome.resolved_return != first_non_instance_return;
        } else {
            first_non_instance_return = Some(outcome.resolved_return);
        }
    }

    let return_kind = match (saw_instance_return, first_non_instance_return) {
        (true, Some(_)) => ConstructorOverloadReturns::Mixed,
        (true, None) => ConstructorOverloadReturns::AllInstance,
        (false, Some(_)) if saw_distinct_non_instance_return => {
            ConstructorOverloadReturns::DivergentNonInstance
        }
        (false, Some(first_non_instance_return)) => {
            ConstructorOverloadReturns::AllSameNonInstance(first_non_instance_return)
        }
        (false, None) => ConstructorOverloadReturns::AllInstance,
    };

    Some(RelevantConstructorOverloads {
        single_relevant_overload,
        return_kind,
    })
}

fn analyze_constructor_overloads<'a, 'db>(
    db: &'db dyn Db,
    class_literal: ClassLiteral<'db>,
    callable: &'a CallableBinding<'db>,
) -> ConstructorOverloadAnalysis<'a, 'db> {
    let mut matching_overloads = callable.matching_overloads().map(|(_, overload)| overload);

    if let Some(first_matching_overload) = matching_overloads.next()
        && let Some(relevant_overloads) = analyze_relevant_constructor_overloads(
            db,
            class_literal,
            std::iter::once(first_matching_overload).chain(matching_overloads),
        )
    {
        return ConstructorOverloadAnalysis::Relevant(relevant_overloads);
    }

    match callable.overloads() {
        [overload] => {
            analyze_relevant_constructor_overloads(db, class_literal, std::iter::once(overload))
                .map_or(
                    ConstructorOverloadAnalysis::NoMatchingOverloads {
                        all_instance_returns: false,
                    },
                    ConstructorOverloadAnalysis::Relevant,
                )
        }
        overloads => ConstructorOverloadAnalysis::NoMatchingOverloads {
            all_instance_returns: !overloads.is_empty()
                && overloads
                    .iter()
                    .all(|overload| overload_returns_instance(db, class_literal, overload)),
        },
    }
}

impl<'db> Binding<'db> {
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
        // Determine whether a typevar returned by a `__new__` method is "self-like", meaning it is
        // either `typing.Self`, or it is `T`, where the first parameter of the method is annotated
        // `type[T]`.
        let is_self_like = |return_typevar: BoundTypeVarInstance<'db>| {
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
        };

        let constructor_context = self.constructor_context?;
        let instance_type = constructor_context.instance_type();

        match (
            constructor_context.kind(),
            self.signature.return_ty.resolve_type_alias(db),
        ) {
            (ConstructorCallableKind::Init, _) => Some(instance_type),
            (_, ty) if ty.is_unknown() => Some(instance_type),
            (ConstructorCallableKind::New, Type::TypeVar(typevar)) if is_self_like(typevar) => {
                Some(instance_type)
            }
            _ => Some(self.signature.return_ty),
        }
    }
}
