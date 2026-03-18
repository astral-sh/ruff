use super::{ArgumentForms, Binding, Bindings, CallableBinding, CallableItem};
use crate::db::Db;
use crate::types::call::arguments::CallArguments;
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::generics::Specialization;
use crate::types::signatures::Parameter;
use crate::types::{
    BoundTypeVarInstance, ClassBase, ClassLiteral, ClassType, DynamicType, SpecialFormType, Type,
    TypeContext,
};

/// The full set of bindings for a constructor call, as a singly-linked list.
///
/// The "outer" `ConstructorBinding` will be the first-called constructor
/// method (could be a metaclass `__call__`, a `__new__`, or an `__init__`,
/// depending what is present on the constructed class). Its
/// `downstream_constructor` may link to the next downstream constructor, if
/// present (e.g. metaclass `__call__` could have `__new__` or `__init__` as
/// downstream; `__new__` could have `__init__` as downstream; `__init__` cannot
/// have a downstream).
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

    pub(super) fn callable(&self) -> &CallableBinding<'db> {
        &self.entry
    }

    pub(super) fn callable_mut(&mut self) -> &mut CallableBinding<'db> {
        &mut self.entry
    }

    pub(super) fn map(
        self,
        f: impl FnOnce(CallableBinding<'db>) -> CallableBinding<'db>,
    ) -> ConstructorBinding<'db> {
        ConstructorBinding {
            entry: f(self.entry),
            constructor_context: self.constructor_context,
            downstream_constructor: self.downstream_constructor,
        }
    }

    fn combine_constructor_return_type(&self, db: &'db dyn Db) -> Type<'db> {
        let constructor_instance_type = self.constructed_instance_type();
        let Some(class_specialization) = constructor_instance_type.class_specialization(db) else {
            return constructor_instance_type;
        };
        let class_literal = constructor_instance_type
            .as_nominal_instance()
            .and_then(|inst| inst.class(db).static_class_literal(db))
            .map(|(lit, _)| lit);
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
                    [overload] if binding.constructor_kind().is_init() => Some(overload),
                    _ => None,
                });
            let Some(overload) = overload else {
                return;
            };
            let self_parameter_specialization = class_literal.and_then(|lit| {
                let self_param_ty = overload.signature.parameters().get(0)?.annotated_type();
                let resolved_self_param_ty = overload
                    .specialization
                    .map(|specialization| self_param_ty.apply_specialization(db, specialization))
                    .unwrap_or(self_param_ty);
                resolved_self_param_ty.specialization_of(db, lit)
            });
            let return_specialization = class_literal
                // Fast path: use the already-resolved overload return type when possible.
                .and_then(|lit| overload.return_ty.specialization_of(db, lit));
            let return_specialization_is_informative =
                return_specialization.is_some_and(|specialization| {
                    class_context.variables(db).any(|class_typevar| {
                        specialization
                            .get(db, class_typevar)
                            .is_some_and(|mapped_ty| {
                                !mapped_ty.is_unknown() && mapped_ty != Type::TypeVar(class_typevar)
                            })
                    })
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
        if let Some(downstream) = self.downstream_constructor()
            && downstream.all_matching_overloads_are_instance_returning(db, self)
        {
            for init_binding in downstream
                .bindings
                .iter_semantic_items()
                .filter_map(CallableItem::as_constructor)
            {
                combine_binding_specialization(init_binding);
            }
        }

        // If constructor inference yields a specialization, rebuild the instance from the class's
        // identity specialization so explicit aliases like `C[int]` can still be remapped by
        // `__new__` return types (e.g. `C[list[T]]`).
        if let Some(specialization) = combined {
            let specialization =
                specialization.apply_optional_specialization(db, Some(class_specialization));
            if let Some(class_literal) = class_literal {
                let remapped_class = class_literal.apply_specialization(db, |_| specialization);
                return Type::instance(db, remapped_class);
            }
            return constructor_instance_type.apply_specialization(db, specialization);
        }

        // If constructor inference doesn't yield a specialization, fall back to the default
        // specialization to avoid leaking inferable typevars in the constructed instance.
        let specialization = class_context.default_specialization(db, None);
        constructor_instance_type.apply_specialization(db, specialization)
    }

    pub(super) fn return_type(&self, db: &'db dyn Db) -> Type<'db> {
        let constructor_instance_type = self.constructed_instance_type();
        let mut saw_matching_upstream_overload = false;
        let mut saw_unmatched_all_non_instance_overloaded_upstream = false;
        let mut saw_unmatched_instance_like_overloaded_upstream = false;

        // If any matched overload's signature return type, when resolved with the inferred
        // specialization, is a non-instance type (e.g. `__new__[S] -> S` with `S` inferred as
        // `str`), use that resolved type directly. This handles arbitrary `__new__` return
        // types like `S`, `list[S]`, etc.
        let constructor_class = constructor_instance_type
            .as_nominal_instance()
            .map(|inst| inst.class(db));
        let constructor_class_literal =
            constructor_class.map(|class_ty| class_ty.class_literal(db));
        let constructor_is_subclass_of_any = constructor_class.is_some_and(|class_ty| {
            class_ty
                .class_literal(db)
                .explicit_bases(db)
                .iter()
                .any(|base| {
                    matches!(
                        base,
                        Type::SpecialForm(SpecialFormType::Any) | Type::Dynamic(DynamicType::Any)
                    )
                })
        });
        if let Some(constructor_class_literal) = constructor_class_literal {
            let callable = self.callable();
            let is_instance_of_constructor = |return_ty: Type<'db>| {
                classify_constructor_return(db, constructor_class_literal, return_ty).is_instance()
            };

            // `__init__` is a post-construction validator and does not determine the
            // constructor return type.
            if !self.constructor_kind().is_init() {
                let matching_overloads = callable.matching_overloads().collect::<Vec<_>>();
                let candidate_overloads = if matching_overloads.is_empty() {
                    if callable.overloads().len() > 1 {
                        let has_instance_like_overload =
                            callable.overloads().iter().any(|overload| {
                                let outcome = constructor_return_outcome(
                                    db,
                                    constructor_class_literal,
                                    overload,
                                );
                                outcome.kind.is_instance()
                                    || outcome.resolved_return.is_unknown()
                                    || (outcome.resolved_return.has_typevar(db)
                                        && outcome.resolved_return.as_nominal_instance().is_none())
                            });
                        if has_instance_like_overload {
                            saw_unmatched_instance_like_overloaded_upstream = true;
                        } else {
                            saw_unmatched_all_non_instance_overloaded_upstream = true;
                        }
                    }
                    match callable.overloads() {
                        [overload] => vec![overload],
                        _ => Vec::new(),
                    }
                } else {
                    saw_matching_upstream_overload = true;
                    matching_overloads
                        .into_iter()
                        .map(|(_, overload)| overload)
                        .collect()
                };

                if !candidate_overloads.is_empty() {
                    let mut saw_instance_return = false;
                    let mut non_instance_returns = Vec::new();
                    for overload in candidate_overloads {
                        let outcome =
                            constructor_return_outcome(db, constructor_class_literal, overload);
                        let is_unknown_like = outcome.resolved_return.is_unknown()
                            || (outcome.resolved_return.has_typevar(db)
                                && outcome.resolved_return.as_nominal_instance().is_none());
                        if is_unknown_like {
                            saw_instance_return = true;
                            continue;
                        }

                        let is_simple_instance_return = outcome
                            .resolved_return
                            .as_nominal_instance()
                            .is_some_and(|instance| {
                                is_subtype_of_class_literal(
                                    db,
                                    instance.class(db),
                                    constructor_class_literal,
                                )
                            });
                        if is_simple_instance_return {
                            saw_instance_return = true;
                        } else {
                            non_instance_returns.push(outcome.resolved_return);
                        }
                    }

                    if !saw_instance_return {
                        let Some(first_non_instance_return) = non_instance_returns.first().copied()
                        else {
                            return self.combine_constructor_return_type(db);
                        };
                        if matches!(first_non_instance_return, Type::Dynamic(DynamicType::Any))
                            && (callable.matching_overloads().next().is_none()
                                || constructor_is_subclass_of_any)
                            && let Some(downstream) = self.downstream_constructor()
                            && !self.constructor_kind().is_metaclass_call()
                        {
                            return downstream.bindings.return_type(db);
                        }
                        if non_instance_returns
                            .iter()
                            .all(|return_ty| *return_ty == first_non_instance_return)
                        {
                            return first_non_instance_return;
                        }
                        return Type::unknown();
                    }

                    // For layered mixed-constructor handling (metaclass `__call__` mixed with
                    // downstream constructor logic), if the downstream constructor resolves to a
                    // non-instance return, that becomes the effective constructor return.
                    if let Some(downstream) = self.downstream_constructor()
                        && self.should_check_downstream_constructor(db)
                    {
                        let downstream_return = downstream.bindings.return_type(db);
                        if !is_instance_of_constructor(downstream_return) {
                            return downstream_return;
                        }
                    }
                }
            }
        }

        if !saw_matching_upstream_overload
            && saw_unmatched_all_non_instance_overloaded_upstream
            && !saw_unmatched_instance_like_overloaded_upstream
        {
            return self.entry.return_type();
        }

        // Preserve explicit strict-subclass constructor returns, e.g. constructing `C` from
        // `__new__ -> D` where `D` is a subclass of `C`.
        if let Some(constructor_class) = constructor_class {
            let constructor_class_literal = constructor_class.class_literal(db);
            let callable = self.callable();
            let mut matching_overloads = callable.matching_overloads();
            if !self.constructor_kind().is_init()
                && let Some((_, overload)) = matching_overloads.next()
                && matching_overloads.next().is_none()
            {
                let sig_return = overload.signature.return_ty;
                if !sig_return.has_typevar(db)
                    && !sig_return.is_unknown()
                    && let Some(returned_instance) = sig_return.as_nominal_instance()
                {
                    let returned_class = returned_instance.class(db);
                    if returned_class.class_literal(db) != constructor_class_literal
                        && is_subtype_of_class_literal(
                            db,
                            returned_class,
                            constructor_class_literal,
                        )
                    {
                        return sig_return;
                    }
                }
            }
        }

        self.combine_constructor_return_type(db)
    }

    fn constructor_kind(&self) -> ConstructorCallableKind {
        self.constructor_context.kind()
    }

    pub(super) fn set_constructed_instance_type(&mut self, instance_type: Type<'db>) {
        self.constructor_context = self.constructor_context.with_instance_type(instance_type);
    }

    pub(super) fn set_downstream_constructor(
        &mut self,
        class_literal: ClassLiteral<'db>,
        bindings: Bindings<'db>,
    ) {
        self.downstream_constructor = Some(Box::new(DownstreamConstructor {
            class_literal,
            bindings,
        }));
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
            for init_binding in downstream.bindings.iter_semantic_items_mut() {
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
        if !self.should_check_downstream_constructor(db) {
            return false;
        }

        let downstream = self.downstream_constructor.as_mut().expect("checked above");
        let init_error = downstream
            .bindings
            .check_types_impl(
                db,
                constraints,
                argument_types,
                call_expression_tcx,
                dataclass_field_specifiers,
            )
            .is_err();

        // In layered mixed-constructor flows, the deferred bindings may themselves
        // represent constructor logic (e.g. metaclass `__call__` -> `__new__`/`__init__`).
        // If that downstream constructor resolves to a non-instance return, `__init__`
        // should not be validated.
        let downstream_return = downstream.bindings.return_type(db);

        if !downstream.return_is_instance_of_constructor_class(db, downstream_return) {
            return false;
        }

        init_error
    }

    pub(super) fn checked_downstream_constructor_bindings(
        &self,
        db: &'db dyn Db,
    ) -> Option<&Bindings<'db>> {
        let downstream = self.downstream_constructor.as_ref()?;
        self.should_check_downstream_constructor(db)
            .then_some(&downstream.bindings)
    }

    pub(super) fn type_context_downstream_constructor_bindings(
        &self,
        db: &'db dyn Db,
    ) -> Option<&Bindings<'db>> {
        let downstream = self.downstream_constructor.as_ref()?;
        self.should_include_downstream_constructor_type_context(db)
            .then_some(&downstream.bindings)
    }

    fn downstream_constructor(&self) -> Option<&DownstreamConstructor<'db>> {
        self.downstream_constructor.as_deref()
    }

    pub(super) fn downstream_constructor_mut(&mut self) -> Option<&mut DownstreamConstructor<'db>> {
        self.downstream_constructor.as_deref_mut()
    }

    fn should_check_downstream_constructor(&self, db: &'db dyn Db) -> bool {
        let Some(downstream) = self.downstream_constructor.as_ref() else {
            return false;
        };

        if self.entry.matching_overloads().next().is_some() {
            return downstream.all_matching_overloads_are_instance_returning(db, self);
        }

        // If metaclass `__call__` itself doesn't match, constructor dispatch stops there.
        if self.constructor_kind().is_metaclass_call() {
            return false;
        }

        // If no overload matched, only validate deferred `__init__` when all upstream overloads
        // are potentially instance-returning. If any overload is definitely non-instance (mixed or
        // all-non-instance constructor), we cannot assume `__init__` would run.
        let mut saw_instance_like = false;
        let mut saw_non_instance = false;
        for overload in &self.entry.overloads {
            match downstream.overload_return_outcome(db, overload).kind {
                ConstructorReturnKind::NotInstance => saw_non_instance = true,
                ConstructorReturnKind::Instance => saw_instance_like = true,
            }
        }
        saw_instance_like && !saw_non_instance
    }

    fn should_include_downstream_constructor_type_context(&self, db: &'db dyn Db) -> bool {
        let Some(downstream) = self.downstream_constructor.as_ref() else {
            return false;
        };

        if self.should_check_downstream_constructor(db) {
            return true;
        }

        // For constructor argument inference, keep deferred `__init__` context available for
        // self-like `__new__(cls: type[T]) -> T` and `__new__(...) -> Self` signatures even
        // before specialization proves that the matched overload returns an instance of the
        // constructed class.
        if self.constructor_kind().is_metaclass_call() {
            return false;
        }

        if self.entry.matching_overloads().next().is_some() {
            return self.entry.matching_overloads().any(|(_, overload)| {
                downstream.overload_is_instance_like_for_type_context(db, overload)
            });
        }

        match self.entry.overloads() {
            [overload] => downstream.overload_is_instance_like_for_type_context(db, overload),
            _ => false,
        }
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
    /// The class literal of the class being constructed, used to determine whether
    /// the matched overload is instance-returning by checking if its return type is an
    /// instance of this class (or a subclass thereof).
    class_literal: ClassLiteral<'db>,
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
    let declared_return = overload.return_ty;
    let resolved_return = overload
        .specialization
        .map(|specialization| declared_return.apply_specialization(db, specialization))
        .unwrap_or(declared_return);
    let kind = classify_constructor_return(db, class_literal, resolved_return);
    ConstructorReturnOutcome {
        resolved_return,
        kind,
    }
}

impl<'db> DownstreamConstructor<'db> {
    fn return_kind(&self, db: &'db dyn Db, return_ty: Type<'db>) -> ConstructorReturnKind {
        classify_constructor_return(db, self.class_literal, return_ty)
    }

    fn overload_return_outcome(
        &self,
        db: &'db dyn Db,
        overload: &Binding<'db>,
    ) -> ConstructorReturnOutcome<'db> {
        constructor_return_outcome(db, self.class_literal, overload)
    }

    fn overload_is_instance_like_for_type_context(
        &self,
        db: &'db dyn Db,
        overload: &Binding<'db>,
    ) -> bool {
        self.overload_return_outcome(db, overload)
            .kind
            .is_instance()
    }

    /// Returns `true` if every matched overload is instance-returning (i.e., its return type
    /// is an instance of the constructor class). We have to check across all overloads, and not
    /// just resolve to a single return type and check that, because of the possibility that
    /// multiple overloads match due to a gradual argument, which would fall back to `Unknown`
    /// return type (which we would consider instance-returning), but we still want to skip
    /// validating `__init__` in case some of the matched overloads are not instance-returning.
    fn all_matching_overloads_are_instance_returning(
        &self,
        db: &'db dyn Db,
        binding: &ConstructorBinding<'db>,
    ) -> bool {
        let mut saw_match = false;
        for (_, overload) in binding.callable().matching_overloads() {
            saw_match = true;
            if !self
                .overload_return_outcome(db, overload)
                .kind
                .is_instance()
            {
                return false;
            }
        }
        saw_match
    }

    fn return_is_instance_of_constructor_class(
        &self,
        db: &'db dyn Db,
        return_ty: Type<'db>,
    ) -> bool {
        self.return_kind(db, return_ty).is_instance()
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
