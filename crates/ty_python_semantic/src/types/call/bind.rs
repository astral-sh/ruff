//! When analyzing a call site, we create _bindings_, which match and type-check the actual
//! arguments against the parameters of the callable.
//!
//! ### Tracing
//!
//! This module is instrumented with debug-level `tracing` messages. You can set the `TY_LOG`
//! environment variable to see this output when testing locally. `tracing` log messages typically
//! have a `target` field, which is the name of the module the message appears in — in this case,
//! `ty_python_semantic::types::call::bind`.

mod constructor;

use std::borrow::Cow;
use std::collections::HashSet;
use std::fmt;

use itertools::Itertools;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::name::Name;
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::{SmallVec, smallvec, smallvec_inline};

use self::constructor::{ConstructorBinding, ConstructorContext};
use super::{Argument, CallArguments, CallError, CallErrorKind, InferContext, Signature, Type};
use crate::db::Db;
use crate::dunder_all::dunder_all_names;
use crate::place::{DefinedPlace, Definedness, Place, known_module_symbol};
use crate::subscript::PyIndex;
use crate::types::call::arguments::{CallArgumentTypes, Expansion, is_expandable_type};
use crate::types::callable::CallableTypeKind;
use crate::types::constraints::{ConstraintSet, ConstraintSetBuilder, PathBounds, Solutions};
use crate::types::diagnostic::{
    CALL_NON_CALLABLE, CALL_TOP_CALLABLE, CONFLICTING_ARGUMENT_FORMS, INVALID_ARGUMENT_TYPE,
    INVALID_DATACLASS, MISSING_ARGUMENT, NO_MATCHING_OVERLOAD, PARAMETER_ALREADY_ASSIGNED,
    POSITIONAL_ONLY_PARAMETER_AS_KWARG, TOO_MANY_POSITIONAL_ARGUMENTS, UNKNOWN_ARGUMENT,
    add_invariant_generic_hints, note_numbers_module_not_supported,
};
use crate::types::enums::is_enum_class;
use crate::types::function::{
    DataclassTransformerFlags, DataclassTransformerParams, FunctionType, KnownFunction,
    OverloadLiteral,
};
use crate::types::generics::{
    GenericContext, InferableTypeVars, Specialization, SpecializationBuilder, SpecializationError,
};
use crate::types::known_instance::FieldInstance;
use crate::types::signatures::{
    CallableSignature, Parameter, ParameterForm, ParameterKind, Parameters, ParametersKind,
    PartialApplication, PartialSignatureApplication,
};
use crate::types::tuple::{TupleLength, TupleSpec, TupleType};
use crate::types::typed_dict::extract_unpacked_typed_dict_keys_from_value_type;
use crate::types::typevar::BoundTypeVarIdentity;
use crate::types::{
    BoundMethodType, BoundTypeVarInstance, CallableType, CallableTypes, ClassLiteral,
    DATACLASS_FLAGS, DataclassFlags, DataclassParams, GenericAlias, InternedConstraintSet,
    IntersectionType, KnownBoundMethodType, KnownClass, KnownInstanceType, LiteralValueTypeKind,
    NominalInstanceType, PropertyInstanceType, SpecialFormType, TypeAliasType, TypeContext,
    TypeVarBoundOrConstraints, TypeVarVariance, UnionAccumulator, UnionBuilder, UnionType,
    WrapperDescriptorKind, enums, list_members,
};
use crate::{DisplaySettings, FxOrderSet, Program};
use ruff_db::diagnostic::{Annotation, Diagnostic, Span, SubDiagnostic, SubDiagnosticSeverity};
use ruff_python_ast::{self as ast, AnyNodeRef, ArgOrKeyword, PythonVersion};
use ty_module_resolver::KnownModule;
use ty_python_core::scope::NodeWithScopeKind;
use ty_python_core::{EvaluationMode, semantic_index};

pub(crate) use self::constructor::ConstructorCallableKind;

/// Priority levels for call errors in intersection types.
/// Higher values indicate more specific errors that should take precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum CallErrorPriority {
    /// Object is not callable at all (no `__call__` method).
    NotCallable = 0,
    /// Object is a top callable (e.g., `Top[Callable[..., object]]`) with unknown signature.
    TopCallable = 1,
    /// Specific binding error (invalid argument type, missing argument, etc.).
    BindingError = 2,
}

/// A single callable item within the union/intersection structure.
/// Either a regular callable, or a constructor callable.
#[derive(Debug, Clone)]
enum CallableItem<'db> {
    Regular(CallableBinding<'db>),
    Constructor(ConstructorBinding<'db>),
}

impl<'db> CallableItem<'db> {
    fn callable(&self) -> &CallableBinding<'db> {
        match self {
            CallableItem::Regular(binding) => binding,
            CallableItem::Constructor(binding) => binding.callable(),
        }
    }

    fn callable_mut(&mut self) -> &mut CallableBinding<'db> {
        match self {
            CallableItem::Regular(binding) => binding,
            CallableItem::Constructor(binding) => binding.callable_mut(),
        }
    }

    fn return_type(&self, db: &'db dyn Db) -> Type<'db> {
        match self {
            CallableItem::Regular(binding) => binding.return_type(),
            CallableItem::Constructor(binding) => binding.return_type(db),
        }
    }

    fn check_types(
        &mut self,
        db: &'db dyn Db,
        constraints: &ConstraintSetBuilder<'db>,
        argument_types: &CallArguments<'_, 'db>,
        call_expression_tcx: TypeContext<'db>,
    ) -> Option<ArgumentForms> {
        match self {
            CallableItem::Regular(binding) => {
                binding.check_types(db, constraints, argument_types, call_expression_tcx)
            }
            CallableItem::Constructor(binding) => {
                binding.check_types(db, constraints, argument_types, call_expression_tcx)
            }
        }
    }

    fn match_parameters(
        &mut self,
        db: &'db dyn Db,
        arguments: &CallArguments<'_, 'db>,
        argument_forms: &mut ArgumentForms,
    ) {
        match self {
            CallableItem::Regular(binding) => {
                binding.match_parameters(db, arguments, argument_forms);
            }
            CallableItem::Constructor(binding) => {
                binding.match_parameters(db, arguments, argument_forms);
            }
        }
    }

    fn as_constructor(&self) -> Option<&ConstructorBinding<'db>> {
        match self {
            CallableItem::Regular(_) => None,
            CallableItem::Constructor(binding) => Some(binding),
        }
    }

    fn as_constructor_mut(&mut self) -> Option<&mut ConstructorBinding<'db>> {
        match self {
            CallableItem::Regular(_) => None,
            CallableItem::Constructor(binding) => Some(binding),
        }
    }

    fn set_downstream_constructor(&mut self, bindings: &Bindings<'db>) {
        if let Some(binding) = self.as_constructor_mut() {
            binding.set_downstream_constructor(bindings.clone());
        }
    }

    fn as_result(&self, db: &'db dyn Db) -> Result<(), CallErrorKind> {
        self.callable().as_result()?;

        self.as_constructor()
            .and_then(|binding| binding.downstream_constructor())
            .map_or(Ok(()), |bindings| bindings.as_result(db))
    }

    fn has_own_diagnostics(&self) -> bool {
        self.callable().as_result().is_err()
    }

    fn error_priority(&self, db: &'db dyn Db) -> CallErrorPriority {
        let priority = self.callable().error_priority();
        self.as_constructor()
            .and_then(|binding| binding.downstream_constructor())
            .map_or(priority, |bindings| {
                priority.max(bindings.error_priority(db))
            })
    }

    fn is_callable(&self) -> bool {
        self.callable().is_callable()
    }

    fn callable_type(&self) -> Type<'db> {
        self.callable().callable_type
    }

    /// Returns the reduced callable synthesized from this callable item.
    fn functools_partial_callable<'a>(
        &self,
        db: &'db dyn Db,
        partial_overload: &mut Binding<'db>,
        bound_call_arguments: &CallArguments<'a, 'db>,
    ) -> Option<CallableType<'db>> {
        match self {
            CallableItem::Regular(binding) => CallableType::partially_apply(
                db,
                binding.partial_signature_applications(
                    db,
                    partial_overload,
                    bound_call_arguments,
                )?,
            ),
            CallableItem::Constructor(_) => None,
        }
    }

    fn map<F>(self, f: &F) -> CallableItem<'db>
    where
        F: Fn(CallableBinding<'db>) -> CallableBinding<'db>,
    {
        match self {
            CallableItem::Regular(binding) => CallableItem::Regular(f(binding)),
            CallableItem::Constructor(binding) => CallableItem::Constructor(binding.map(f)),
        }
    }

    fn wrap_as_constructor(
        self,
        constructed_instance_type: Type<'db>,
        constructor_kind: ConstructorCallableKind,
    ) -> CallableItem<'db> {
        match self {
            CallableItem::Regular(binding) => CallableItem::Constructor(ConstructorBinding::new(
                binding,
                ConstructorContext::new(constructed_instance_type, constructor_kind),
            )),
            CallableItem::Constructor(binding) => CallableItem::Constructor(binding),
        }
    }
}

/// A single element in a union of callables.
/// This could be a single callable or an intersection of callables.
/// If there are multiple items, they form an intersection.
#[derive(Debug, Clone)]
struct BindingsElement<'db> {
    items: SmallVec<[CallableItem<'db>; 1]>,
}

impl<'db> BindingsElement<'db> {
    fn items(&self) -> impl Iterator<Item = &CallableItem<'db>> {
        self.items.iter()
    }

    fn items_mut(&mut self) -> impl Iterator<Item = &mut CallableItem<'db>> {
        self.items.iter_mut()
    }

    fn callables(&self) -> impl Iterator<Item = &CallableBinding<'db>> {
        self.items.iter().map(CallableItem::callable)
    }

    fn callables_mut(&mut self) -> impl Iterator<Item = &mut CallableBinding<'db>> {
        self.items.iter_mut().map(CallableItem::callable_mut)
    }

    /// Returns true if this element is an intersection of multiple callables.
    fn is_intersection(&self) -> bool {
        self.items.len() > 1
    }

    fn return_type(&self, db: &'db dyn Db) -> Type<'db> {
        IntersectionType::from_elements(db, self.items.iter().map(|item| item.return_type(db)))
    }

    /// Check types for all bindings in this element.
    fn check_types(
        &mut self,
        db: &'db dyn Db,
        constraints: &ConstraintSetBuilder<'db>,
        call_arguments: &CallArguments<'_, 'db>,
        call_expression_tcx: TypeContext<'db>,
    ) -> Option<ArgumentForms> {
        let mut result = ArgumentForms::default();
        let mut any_forms = false;
        for item in &mut self.items {
            if let Some(forms) =
                item.check_types(db, constraints, call_arguments, call_expression_tcx)
            {
                result.merge(&forms);
                any_forms = true;
            }
        }
        any_forms.then_some(result)
    }

    /// Returns the result of calling this element.
    /// For intersections, if any binding succeeds, the element succeeds.
    /// When all bindings fail, returns the error from the highest-priority binding.
    fn as_result(&self, db: &'db dyn Db) -> Result<(), CallErrorKind> {
        // If any binding succeeds, the element succeeds
        if self.items.iter().any(|b| b.as_result(db).is_ok()) {
            return Ok(());
        }

        // All bindings failed - find highest priority and return that error kind
        let max_priority = self.error_priority(db);

        // Return the error from the first binding with the highest priority
        Err(self
            .items
            .iter()
            .find(|b| b.error_priority(db) == max_priority)
            .map(|b| b.as_result(db).unwrap_err())
            .unwrap_or(CallErrorKind::NotCallable))
    }

    /// Filter bindings in an intersection once at least one binding succeeded.
    ///
    /// We keep successful bindings, and also keep top-callable failures. Top callables contribute
    /// useful return-type information (e.g. `Awaitable[object]`) for narrowed intersections like
    /// `f: KnownCallable & Top[Callable[..., Awaitable[object]]]`, even though the top-callable
    /// call itself is unsafe. (We know that somewhere in the infinite-union of the top callable,
    /// there is a callable with the right parameters to match the call.)
    fn retain_successful(&mut self, db: &'db dyn Db) {
        if self.is_intersection() && self.as_result(db).is_ok() {
            self.items.retain(|item| {
                item.as_result(db).is_ok()
                    || item.error_priority(db) == CallErrorPriority::TopCallable
            });
        }
    }

    /// Returns the error priority for this element (used when all bindings failed).
    fn error_priority(&self, db: &'db dyn Db) -> CallErrorPriority {
        self.items
            .iter()
            .map(|item| item.error_priority(db))
            .max()
            .unwrap_or(CallErrorPriority::NotCallable)
    }

    /// Returns true if any binding in this element is callable.
    fn is_callable(&self) -> bool {
        self.items.iter().any(CallableItem::is_callable)
    }
}

/// Binding information for a union of callables, where each union element may be an intersection.
///
/// This structure represents a union (possibly size one) of callable elements, where each element
/// is an intersection (possibly size one) of callable bindings.
///
/// For the union level: At a call site, the arguments must be compatible with _all_ elements
/// in the union for the call to be valid. Return types are combined using union.
///
/// For the intersection level within each element: We try each binding and discard bindings
/// where the call fails. If at least one binding succeeds, the element succeeds. Return types
/// are combined using intersection.
#[derive(Debug, Clone)]
pub(crate) struct Bindings<'db> {
    /// The type that is (hopefully) callable.
    callable_type: Type<'db>,

    /// Whether implicit `__new__` calls may be missing in constructor bindings.
    implicit_dunder_new_is_possibly_unbound: bool,

    /// Whether implicit `__init__` calls may be missing in constructor bindings.
    implicit_dunder_init_is_possibly_unbound: bool,

    /// The elements of this binding. For a union, each element is a union variant.
    /// Each element may contain multiple `CallableBinding`s if it came from an intersection.
    elements: SmallVec<[BindingsElement<'db>; 1]>,

    /// Whether each argument will be used as a value and/or a type form in this call.
    argument_forms: ArgumentForms,
}

impl<'db> Bindings<'db> {
    fn as_result(&self, db: &'db dyn Db) -> Result<(), CallErrorKind> {
        let mut all_ok = true;
        let mut any_binding_error = false;
        let mut all_not_callable = true;

        if self.argument_forms.conflicting.contains(&true) {
            all_ok = false;
            any_binding_error = true;
            all_not_callable = false;
        }

        for element in &self.elements {
            let result = element.as_result(db);
            all_ok &= result.is_ok();
            any_binding_error |= matches!(result, Err(CallErrorKind::BindingError));
            all_not_callable &= matches!(result, Err(CallErrorKind::NotCallable));
        }

        if all_ok {
            Ok(())
        } else if any_binding_error {
            Err(CallErrorKind::BindingError)
        } else if all_not_callable {
            Err(CallErrorKind::NotCallable)
        } else {
            Err(CallErrorKind::PossiblyNotCallable)
        }
    }

    fn error_priority(&self, db: &'db dyn Db) -> CallErrorPriority {
        self.elements
            .iter()
            .map(|element| element.error_priority(db))
            .max()
            .unwrap_or(CallErrorPriority::NotCallable)
    }

    fn set_constructor_instance_type_in_place(
        &mut self,
        db: &'db dyn Db,
        constructor_instance_type: Type<'db>,
    ) {
        for element in &mut self.elements {
            for item in &mut element.items {
                match item {
                    CallableItem::Regular(_) => {}
                    CallableItem::Constructor(binding) => {
                        binding.set_constructed_instance_type(constructor_instance_type);
                        let constructor_context = binding.context();
                        for overload in &mut binding.entry.overloads {
                            overload.set_constructor_context(db, constructor_context);
                        }

                        // Deferred downstream constructor bindings still need constructor instance
                        // context for generic specialization inference (including literal
                        // promotion).
                        if let Some(downstream) = binding.downstream_constructor_mut() {
                            downstream.set_constructor_instance_type_in_place(
                                db,
                                constructor_instance_type,
                            );
                        }
                    }
                }
            }
        }
    }

    fn apply_generic_context_in_place(
        &mut self,
        db: &'db dyn Db,
        generic_context: GenericContext<'db>,
    ) {
        for element in &mut self.elements {
            for item in &mut element.items {
                match item {
                    CallableItem::Regular(binding) => {
                        for overload in &mut binding.overloads {
                            overload.signature.generic_context = GenericContext::merge_optional(
                                db,
                                overload.signature.generic_context,
                                Some(generic_context),
                            );
                        }
                    }
                    CallableItem::Constructor(binding) => {
                        for overload in &mut binding.entry.overloads {
                            overload.signature.generic_context = GenericContext::merge_optional(
                                db,
                                overload.signature.generic_context,
                                Some(generic_context),
                            );
                        }
                        if let Some(downstream) = binding.downstream_constructor_mut() {
                            downstream.apply_generic_context_in_place(db, generic_context);
                        }
                    }
                }
            }
        }
    }

    /// Creates a new `Bindings` from an iterator of [`Bindings`]s for a union type.
    /// Each input `Bindings` becomes a union element, preserving any intersection structure.
    /// Panics if the iterator is empty.
    pub(crate) fn from_union<I>(callable_type: Type<'db>, bindings_iter: I) -> Self
    where
        I: IntoIterator<Item = Bindings<'db>>,
    {
        let mut implicit_dunder_new_is_possibly_unbound = false;
        let mut implicit_dunder_init_is_possibly_unbound = false;
        let mut elements_acc = SmallVec::new();

        // Preserve each input's existing union/intersection structure.
        for set in bindings_iter {
            implicit_dunder_new_is_possibly_unbound |= set.implicit_dunder_new_is_possibly_unbound;
            implicit_dunder_init_is_possibly_unbound |=
                set.implicit_dunder_init_is_possibly_unbound;
            elements_acc.extend(set.elements);
        }

        let elements = elements_acc;
        assert!(!elements.is_empty());
        Self {
            callable_type,
            elements,
            argument_forms: ArgumentForms::new(0),
            implicit_dunder_new_is_possibly_unbound,
            implicit_dunder_init_is_possibly_unbound,
        }
    }

    /// Creates a new `Bindings` from an iterator of [`Bindings`]s for an intersection type.
    /// All input bindings are combined into a single intersection element.
    /// Panics if the iterator is empty.
    pub(crate) fn from_intersection<I>(callable_type: Type<'db>, bindings_iter: I) -> Self
    where
        I: IntoIterator<Item = Bindings<'db>>,
    {
        // Flatten all input bindings into a single intersection element
        let mut implicit_dunder_new_is_possibly_unbound = true;
        let mut implicit_dunder_init_is_possibly_unbound = true;
        let mut inner_items_acc = SmallVec::new();

        for set in bindings_iter {
            implicit_dunder_new_is_possibly_unbound &= set.implicit_dunder_new_is_possibly_unbound;
            implicit_dunder_init_is_possibly_unbound &=
                set.implicit_dunder_init_is_possibly_unbound;
            for element in set.elements {
                inner_items_acc.extend(element.items);
            }
        }
        assert!(!inner_items_acc.is_empty());
        let elements = smallvec![BindingsElement {
            items: inner_items_acc,
        }];
        Self {
            callable_type,
            implicit_dunder_new_is_possibly_unbound,
            implicit_dunder_init_is_possibly_unbound,
            elements,
            argument_forms: ArgumentForms::new(0),
        }
    }

    pub(crate) fn replace_callable_type(&mut self, before: Type<'db>, after: Type<'db>) {
        if self.callable_type == before {
            self.callable_type = after;
        }
        for binding in self.iter_flat_mut() {
            binding.replace_callable_type(before, after);
        }
    }

    pub(crate) fn with_constructed_instance_type(
        mut self,
        db: &'db dyn Db,
        constructor_instance_type: Type<'db>,
    ) -> Self {
        self.set_constructor_instance_type_in_place(db, constructor_instance_type);
        self
    }

    pub(crate) fn into_constructor_bindings(
        mut self,
        constructor_instance_type: Type<'db>,
        constructor_kind: ConstructorCallableKind,
    ) -> Self {
        for element in &mut self.elements {
            element.items = std::mem::take(&mut element.items)
                .into_iter()
                .map(|item| item.wrap_as_constructor(constructor_instance_type, constructor_kind))
                .collect();
        }
        self
    }

    pub(crate) fn with_generic_context(
        mut self,
        db: &'db dyn Db,
        generic_context: Option<GenericContext<'db>>,
    ) -> Self {
        let Some(generic_context) = generic_context else {
            return self;
        };
        self.apply_generic_context_in_place(db, generic_context);
        self
    }

    pub(crate) fn set_downstream_constructor(&mut self, bindings: &Bindings<'db>) {
        for item in self.iter_callable_items_mut() {
            item.set_downstream_constructor(bindings);
        }
    }

    pub(crate) fn set_dunder_call_is_possibly_unbound(&mut self) {
        for binding in self.iter_flat_mut() {
            binding.dunder_call_is_possibly_unbound = true;
        }
    }

    pub(crate) fn set_implicit_dunder_new_is_possibly_unbound(&mut self) {
        self.implicit_dunder_new_is_possibly_unbound = true;
    }

    pub(crate) fn set_implicit_dunder_init_is_possibly_unbound(&mut self) {
        self.implicit_dunder_init_is_possibly_unbound = true;
    }

    pub(crate) fn argument_forms(&self) -> &[Option<ParameterForm>] {
        &self.argument_forms.values
    }

    /// Returns the agreed parameter form for each call argument, in source order.
    ///
    /// An argument form is "non-conflicting" when the binding analysis did not observe that same
    /// call-site argument being used as both a value form and a type form across the participating
    /// bindings. For such arguments this returns `Some(form)`.
    ///
    /// This returns `None` for arguments whose form is unknown and for arguments where multiple
    /// bindings disagreed about whether the argument should be interpreted as a value or as a type.
    pub(crate) fn non_conflicting_argument_forms(
        &self,
    ) -> impl Iterator<Item = Option<ParameterForm>> + '_ {
        self.argument_forms
            .values
            .iter()
            .zip(&self.argument_forms.conflicting)
            .map(|(form, conflicting)| (!conflicting).then_some(*form).flatten())
    }

    pub(crate) fn has_implicit_dunder_new_is_possibly_unbound(&self) -> bool {
        self.implicit_dunder_new_is_possibly_unbound
    }

    pub(crate) fn has_implicit_dunder_init_is_possibly_unbound(&self) -> bool {
        self.implicit_dunder_init_is_possibly_unbound
    }

    /// Returns an iterator over all `CallableBinding`s, flattening the two-level structure.
    ///
    /// Note: This loses the union/intersection distinction. The returned iterator yields
    /// all `CallableBinding`s from all elements, which can then be further flattened to
    /// individual `Binding`s via `CallableBinding`'s `IntoIterator` implementation.
    pub(crate) fn iter_flat(&self) -> impl Iterator<Item = &CallableBinding<'db>> {
        self.elements.iter().flat_map(BindingsElement::callables)
    }

    /// Returns a mutable iterator over all `CallableBinding`s, flattening the two-level structure.
    ///
    /// Note: This loses the union/intersection distinction. Use only when you need to
    /// modify all bindings regardless of their union/intersection grouping.
    pub(crate) fn iter_flat_mut(&mut self) -> impl Iterator<Item = &mut CallableBinding<'db>> {
        self.elements
            .iter_mut()
            .flat_map(BindingsElement::callables_mut)
    }

    fn iter_callable_items(&self) -> impl Iterator<Item = &CallableItem<'db>> {
        self.elements.iter().flat_map(BindingsElement::items)
    }

    fn iter_callable_items_mut(&mut self) -> impl Iterator<Item = &mut CallableItem<'db>> {
        self.elements
            .iter_mut()
            .flat_map(BindingsElement::items_mut)
    }

    fn iter_constructor_items(&self) -> impl Iterator<Item = &ConstructorBinding<'db>> {
        self.iter_callable_items()
            .filter_map(CallableItem::as_constructor)
    }

    fn iter_constructor_items_mut(&mut self) -> impl Iterator<Item = &mut ConstructorBinding<'db>> {
        self.iter_callable_items_mut()
            .filter_map(CallableItem::as_constructor_mut)
    }

    fn clear_deferred_constructor_errors_for_partial_application(&mut self) {
        for binding in self.iter_flat_mut() {
            binding.clear_deferred_constructor_errors_for_partial_application();
        }

        for constructor in self.iter_constructor_items_mut() {
            if let Some(downstream) = constructor.downstream_constructor_mut() {
                downstream.clear_deferred_constructor_errors_for_partial_application();
            }
        }
    }

    /// Visits the callables that should contribute argument type context, including deferred
    /// constructor callables that are relevant to the matched upstream constructor path.
    pub(crate) fn visit_type_context_callables<'a>(
        &'a self,
        visit: &mut impl FnMut(&'a CallableBinding<'db>),
    ) {
        for item in self.iter_callable_items() {
            visit(item.callable());

            if let Some(constructor) = item.as_constructor()
                && let Some(downstream) = &constructor.downstream_constructor
            {
                downstream.visit_type_context_callables(visit);
            }
        }
    }

    /// Returns `true` if every element of the union contains an intersection element with a matching
    /// overload that satisfies the provided closure, or `false` otherwise.
    pub(crate) fn satisfies(&self, f: impl Fn(&Binding<'db>) -> bool) -> bool {
        self.elements.iter().all(|element| {
            element
                .callables()
                .flat_map(CallableBinding::matching_overloads)
                .any(|(_, overload)| f(overload))
        })
    }

    /// Maps each `CallableBinding` to a type and combines results while preserving
    /// the union-of-intersections structure:
    ///
    /// - callable bindings inside an element are intersected
    /// - elements are unioned
    pub(crate) fn map_types(
        &self,
        db: &'db dyn Db,
        mut map: impl FnMut(&CallableBinding<'db>) -> Option<Type<'db>>,
    ) -> Type<'db> {
        let mut element_types = Vec::with_capacity(self.elements.len());
        for element in &self.elements {
            let mut binding_types = Vec::new();
            for binding in element.callables() {
                if let Some(ty) = map(binding) {
                    binding_types.push(ty);
                }
            }

            if !binding_types.is_empty() {
                element_types.push(IntersectionType::from_elements(db, binding_types));
            }
        }

        UnionType::from_elements(db, element_types)
    }

    /// Maps each `CallableItem` to a type and combines results while preserving
    /// the union-of-intersections structure:
    ///
    /// - callable items inside an element are intersected
    /// - elements are unioned
    fn map_item_types(
        &self,
        db: &'db dyn Db,
        mut map: impl FnMut(&CallableItem<'db>) -> Option<Type<'db>>,
    ) -> Type<'db> {
        let mut element_types = Vec::with_capacity(self.elements.len());
        for element in &self.elements {
            let mut item_types = Vec::new();
            for item in element.items() {
                if let Some(ty) = map(item) {
                    item_types.push(ty);
                }
            }

            if !item_types.is_empty() {
                element_types.push(IntersectionType::from_elements(db, item_types));
            }
        }

        UnionType::from_elements(db, element_types)
    }

    /// Builds matched bindings for the callable wrapped by `functools.partial(...)`.
    ///
    /// This handles the shared partial-specific preprocessing (callable validation and argument
    /// normalization) used by both inference and known-call evaluation.
    pub(crate) fn functools_partial_matched_bindings<'a>(
        db: &'db dyn Db,
        wrapped_callable_ty: Type<'db>,
        call_arguments: &CallArguments<'a, 'db>,
    ) -> Option<(CallArguments<'a, 'db>, Bindings<'db>)> {
        // We can only infer bound-argument context from an actual callable.
        wrapped_callable_ty.try_upcast_to_callable(db)?;

        let bound_call_arguments = call_arguments.functools_partial_bound_arguments(db)?;

        let mut partial_bindings = wrapped_callable_ty
            .bindings(db)
            .match_parameters(db, &bound_call_arguments);
        for binding in partial_bindings.iter_flat_mut() {
            binding.clear_missing_argument_errors_for_partial_application();
        }
        for constructor in partial_bindings.iter_constructor_items_mut() {
            if let Some(downstream) = constructor.downstream_constructor_mut() {
                downstream.clear_deferred_constructor_errors_for_partial_application();
            }
        }
        Some((bound_call_arguments, partial_bindings))
    }

    /// Synthesizes the precise `functools.partial(...)` type for the already-matched bindings.
    ///
    /// Wrapped unions and intersections keep their original callable structure by partially
    /// applying each callable item independently. A single wrapped callable instead exposes one
    /// reduced callable whose overload set is merged before being wrapped as `partial[...]`.
    fn functools_partial_type<'a>(
        &self,
        db: &'db dyn Db,
        wrapped_callable_ty: Type<'db>,
        partial_overload: &mut Binding<'db>,
        bound_call_arguments: &CallArguments<'a, 'db>,
    ) -> Type<'db> {
        if wrapped_callable_ty.is_union() || wrapped_callable_ty.is_intersection() {
            return self.map_item_types(db, |partial_item| {
                partial_item
                    .functools_partial_callable(db, partial_overload, bound_call_arguments)
                    .map(|callable| {
                        callable.into_precise_functools_partial_instance(db, wrapped_callable_ty)
                    })
            });
        }

        let partial_callables: SmallVec<[CallableType<'db>; 1]> = self
            .iter_callable_items()
            .filter_map(|partial_item| {
                partial_item.functools_partial_callable(db, partial_overload, bound_call_arguments)
            })
            .collect();

        if partial_callables.is_empty() {
            Type::Never
        } else {
            CallableTypes::from_elements(partial_callables)
                .into_precise_functools_partial_instance(db, wrapped_callable_ty)
        }
    }

    fn map_with<F>(self, f: &F) -> Self
    where
        F: Fn(CallableBinding<'db>) -> CallableBinding<'db>,
    {
        Self {
            callable_type: self.callable_type,
            argument_forms: self.argument_forms,
            implicit_dunder_new_is_possibly_unbound: self.implicit_dunder_new_is_possibly_unbound,
            implicit_dunder_init_is_possibly_unbound: self.implicit_dunder_init_is_possibly_unbound,
            elements: self
                .elements
                .into_iter()
                .map(|elem| BindingsElement {
                    items: elem.items.into_iter().map(|item| item.map(f)).collect(),
                })
                .collect(),
        }
    }

    pub(crate) fn map(self, f: impl Fn(CallableBinding<'db>) -> CallableBinding<'db>) -> Self {
        self.map_with(&f)
    }

    /// Match the arguments of a call site against the parameters of a collection of possibly
    /// unioned, possibly overloaded signatures.
    ///
    /// The returned bindings tell you which parameter (in each signature) each argument was
    /// matched against. You can then perform type inference on each argument with extra context
    /// about the expected parameter types.
    ///
    /// Once you have argument types available, you can call [`check_types`][Self::check_types] to
    /// verify that each argument type is assignable to the corresponding parameter type.
    pub(crate) fn match_parameters(
        mut self,
        db: &'db dyn Db,
        arguments: &CallArguments<'_, 'db>,
    ) -> Self {
        self.match_parameters_in_place(db, arguments);
        self
    }

    fn match_parameters_in_place(&mut self, db: &'db dyn Db, arguments: &CallArguments<'_, 'db>) {
        let mut argument_forms = ArgumentForms::new(arguments.len());
        for item in self.iter_callable_items_mut() {
            item.match_parameters(db, arguments, &mut argument_forms);
        }
        argument_forms.shrink_to_fit();
        self.argument_forms = argument_forms;
    }

    /// Verify that the type of each argument is assignable to type of the parameter that it was
    /// matched to.
    ///
    /// You must provide an `call_arguments` that was created from the same `arguments` that you
    /// provided to [`match_parameters`][Self::match_parameters].
    ///
    /// The type context of the call expression is also used to infer the specialization of generic
    /// calls.
    ///
    /// We update the bindings to include the return type of the call, the bound types for all
    /// parameters, and any errors resulting from binding the call, all for each union element and
    /// overload (if any).
    pub(crate) fn check_types(
        mut self,
        db: &'db dyn Db,
        constraints: &ConstraintSetBuilder<'db>,
        call_arguments: &CallArguments<'_, 'db>,
        call_expression_tcx: TypeContext<'db>,
        dataclass_field_specifiers: &[Type<'db>],
    ) -> Result<Self, CallError<'db>> {
        match self.check_types_impl(
            db,
            constraints,
            call_arguments,
            call_expression_tcx,
            dataclass_field_specifiers,
        ) {
            Ok(()) => Ok(self),
            Err(err) => Err(CallError(err, Box::new(self))),
        }
    }

    pub(crate) fn check_types_impl(
        &mut self,
        db: &'db dyn Db,
        constraints: &ConstraintSetBuilder<'db>,
        call_arguments: &CallArguments<'_, 'db>,
        call_expression_tcx: TypeContext<'db>,
        dataclass_field_specifiers: &[Type<'db>],
    ) -> Result<(), CallErrorKind> {
        // Check types for each element (union variant)
        for element in &mut self.elements {
            if let Some(updated_argument_forms) =
                element.check_types(db, constraints, call_arguments, call_expression_tcx)
            {
                // If this element returned a new set of argument forms (indicating successful
                // argument type expansion), merge them into the existing forms.
                self.argument_forms.merge(&updated_argument_forms);
            }
        }
        self.argument_forms.shrink_to_fit();

        self.evaluate_known_cases(db, call_arguments, dataclass_field_specifiers);

        // For constructor bindings with deferred downstream checks: validate downstream bindings
        // if the matched overload is instance-returning.
        for constructor in self.iter_constructor_items_mut() {
            constructor.check_downstream_constructor(
                db,
                constraints,
                call_arguments,
                call_expression_tcx,
                dataclass_field_specifiers,
            );
        }

        // For intersection elements with at least one successful binding,
        // filter out the failing bindings after deferred constructor checks.
        for element in &mut self.elements {
            element.retain_successful(db);
        }

        self.as_result(db)
    }

    /// Returns true if this is a single callable (not a union or intersection).
    pub(crate) fn is_single(&self) -> bool {
        match &*self.elements {
            [single] => single.items.len() == 1,
            _ => false,
        }
    }

    /// Returns the single `CallableBinding` if this is not a union or intersection.
    pub(crate) fn single_element(&self) -> Option<&CallableBinding<'db>> {
        if self.is_single() {
            self.elements
                .first()
                .and_then(|e| e.items.first())
                .map(CallableItem::callable)
        } else {
            None
        }
    }

    fn single_item(&self) -> Option<&CallableItem<'db>> {
        if self.is_single() {
            self.elements.first().and_then(|e| e.items.first())
        } else {
            None
        }
    }

    pub(crate) fn callable_type(&self) -> Type<'db> {
        self.callable_type
    }

    /// Returns the return type of the call. For successful calls, this is the actual return type.
    /// For calls with binding errors, this is a type that best approximates the return type. For
    /// types that are not callable, returns `Type::Unknown`.
    pub(crate) fn return_type(&self, db: &'db dyn Db) -> Type<'db> {
        UnionType::from_elements(
            db,
            self.elements.iter().map(|element| element.return_type(db)),
        )
    }

    /// Returns the inferred type for the argument at the specified index.
    pub(crate) fn type_for_argument<'a>(
        &'a self,
        call_arguments: &'a CallArguments<'a, 'db>,
        argument_index: usize,
    ) -> Type<'db> {
        let argument_types = call_arguments
            .argument_types(argument_index)
            .expect("argument index should be valid");

        // If there is a single matching parameter, return the argument type inferred against
        // its declared type.
        if let Some(binding) = self.single_element()
            && let Ok((_, overload)) = binding.matching_overloads().exactly_one()
            && let [parameter_index] = *overload.argument_matches[argument_index].parameters
        {
            let declared_type = overload.signature.parameters()[parameter_index].annotated_type();
            return argument_types.get_for_declared_type(declared_type);
        }

        // Otherwise, return the default type.
        argument_types.get_default().unwrap_or(Type::unknown())
    }

    /// Report diagnostics for all of the errors that occurred when trying to match actual
    /// arguments to formal parameters. If the callable is a union, or has multiple overloads, we
    /// report a single diagnostic if we couldn't match any union element or overload.
    pub(crate) fn report_diagnostics(
        &self,
        context: &InferContext<'db, '_>,
        node: ast::AnyNodeRef,
    ) {
        // If all elements are not callable, report that the type as a whole is not callable.
        if self.elements.iter().all(|e| !e.is_callable()) {
            let range = all_arguments_range(node);
            if let Some(builder) = context.report_lint(&CALL_NON_CALLABLE, range) {
                builder.into_diagnostic(format_args!(
                    "Object of type `{}` is not callable",
                    self.callable_type().display(context.db())
                ));
            }
            return;
        }

        for (index, conflicting_form) in self.argument_forms.conflicting.iter().enumerate() {
            if *conflicting_form {
                let node = BindingError::get_node(node, Some(index));
                if let Some(builder) = context.report_lint(&CONFLICTING_ARGUMENT_FORMS, node) {
                    builder.into_diagnostic(
                        "Argument is used as both a value and a type form in call",
                    );
                }
            }
        }

        if let Some(item) = self.single_item() {
            if item.has_own_diagnostics() {
                item.callable().report_diagnostics(context, node, None);
            }
        } else {
            // Report diagnostics for each element (union variant).
            // Each element may be a single binding or an intersection of bindings.
            for element in &self.elements {
                self.report_element_diagnostics(context, node, element);
            }
        }

        // Report deferred constructor diagnostics when the matched overload is instance-returning.
        let mut reported_ctor_init_callables = FxHashSet::default();
        for constructor in self.iter_constructor_items() {
            let Some(downstream_bindings) = constructor.downstream_constructor() else {
                continue;
            };
            if !reported_ctor_init_callables.insert(downstream_bindings.callable_type()) {
                continue;
            }
            downstream_bindings.report_diagnostics(context, node);
        }
    }

    /// Report diagnostics for a single union element.
    /// If the element is an intersection where all bindings failed, use priority hierarchy.
    fn report_element_diagnostics(
        &self,
        context: &InferContext<'db, '_>,
        node: ast::AnyNodeRef,
        element: &BindingsElement<'db>,
    ) {
        // If this element succeeded, no diagnostics to report
        if element.as_result(context.db()).is_ok() {
            return;
        }

        let is_union = self.elements.len() > 1;

        // For intersection elements, use priority hierarchy
        if element.is_intersection() {
            // Find the highest priority error among bindings in this element
            let max_priority = element.error_priority(context.db());

            // Construct the intersection type from the bindings
            let intersection_type = IntersectionType::from_elements(
                context.db(),
                element.items.iter().map(CallableItem::callable_type),
            );

            // Only report errors from bindings with the highest priority
            for item in &element.items {
                let binding = item.callable();
                if item.error_priority(context.db()) == max_priority {
                    if !item.has_own_diagnostics() {
                        continue;
                    }
                    if is_union {
                        // Use layered diagnostic for intersection inside a union
                        let layered_diag = LayeredDiagnostic {
                            union_callable_type: self.callable_type(),
                            intersection_callable_type: intersection_type,
                            binding,
                        };
                        binding.report_diagnostics(context, node, Some(&layered_diag));
                    } else {
                        // Just intersection, no union context needed
                        let intersection_diag = IntersectionDiagnostic {
                            callable_type: intersection_type,
                            binding,
                        };
                        binding.report_diagnostics(context, node, Some(&intersection_diag));
                    }
                }
            }
        } else {
            // Single binding in this element - report as a union variant
            if let Some(item) = element.items.first() {
                if !item.has_own_diagnostics() {
                    return;
                }
                let binding = item.callable();
                if element.as_result(context.db()).is_ok() {
                    return;
                }
                let union_diag = UnionDiagnostic {
                    callable_type: self.callable_type(),
                    binding,
                };
                binding.report_diagnostics(context, node, Some(&union_diag));
            }
        }
    }

    /// Evaluates the return type of certain known callables, where we have special-case logic to
    /// determine the return type in a way that isn't directly expressible in the type system.
    fn evaluate_known_cases(
        &mut self,
        db: &'db dyn Db,
        call_arguments: &CallArguments<'_, 'db>,
        dataclass_field_specifiers: &[Type<'db>],
    ) {
        let to_bool = |ty: &Option<Type<'_>>, default: bool| -> bool {
            if let Some(ty) = ty
                && let Some(LiteralValueTypeKind::Bool(value)) = ty.as_literal_value_kind()
            {
                value
            } else {
                // TODO: emit a diagnostic if we receive `bool`
                default
            }
        };

        // Each special case listed here should have a corresponding clause in `Type::bindings`.
        for binding in self.iter_flat_mut() {
            let binding_type = binding.callable_type;
            for (overload_index, overload) in binding.matching_overloads_mut() {
                match binding_type {
                    Type::KnownBoundMethod(KnownBoundMethodType::FunctionTypeDunderGet(
                        function,
                    )) => {
                        if function.is_classmethod(db) {
                            match overload.parameter_types() {
                                [_, Some(owner)] => {
                                    overload.set_return_type(Type::BoundMethod(
                                        BoundMethodType::new(db, function, *owner),
                                    ));
                                }
                                [Some(instance), None] => {
                                    overload.set_return_type(Type::BoundMethod(
                                        BoundMethodType::new(
                                            db,
                                            function,
                                            instance.to_meta_type(db),
                                        ),
                                    ));
                                }
                                _ => {}
                            }
                        } else if function.is_staticmethod(db) {
                            overload.set_return_type(Type::FunctionLiteral(function));
                        } else if let [Some(first), _] = overload.parameter_types() {
                            if first.is_none(db) {
                                overload.set_return_type(Type::FunctionLiteral(function));
                            } else {
                                overload.set_return_type(Type::BoundMethod(BoundMethodType::new(
                                    db, function, *first,
                                )));
                            }
                        }
                    }

                    Type::WrapperDescriptor(WrapperDescriptorKind::FunctionTypeDunderGet) => {
                        if let [Some(function_ty @ Type::FunctionLiteral(function)), ..] =
                            overload.parameter_types()
                        {
                            if function.is_classmethod(db) {
                                match overload.parameter_types() {
                                    [_, _, Some(owner)] => {
                                        overload.set_return_type(Type::BoundMethod(
                                            BoundMethodType::new(db, *function, *owner),
                                        ));
                                    }

                                    [_, Some(instance), None] => {
                                        overload.set_return_type(Type::BoundMethod(
                                            BoundMethodType::new(
                                                db,
                                                *function,
                                                instance.to_meta_type(db),
                                            ),
                                        ));
                                    }

                                    _ => {}
                                }
                            } else if function.is_staticmethod(db) {
                                overload.set_return_type(*function_ty);
                            } else {
                                match overload.parameter_types() {
                                    [_, Some(instance), _] if instance.is_none(db) => {
                                        overload.set_return_type(*function_ty);
                                    }
                                    [_, Some(instance), _] => {
                                        overload.set_return_type(Type::BoundMethod(
                                            BoundMethodType::new(db, *function, *instance),
                                        ));
                                    }

                                    _ => {}
                                }
                            }
                        }
                    }

                    Type::WrapperDescriptor(WrapperDescriptorKind::PropertyDunderGet) => {
                        match overload.parameter_types() {
                            [
                                Some(property @ Type::PropertyInstance(_)),
                                Some(instance),
                                ..,
                            ] if instance.is_none(db) => {
                                overload.set_return_type(*property);
                            }
                            [
                                Some(Type::PropertyInstance(property)),
                                Some(Type::KnownInstance(KnownInstanceType::TypeAliasType(
                                    type_alias,
                                ))),
                                ..,
                            ] if property.getter(db).is_some_and(|getter| {
                                getter
                                    .as_function_literal()
                                    .is_some_and(|f| f.name(db) == "__name__")
                            }) =>
                            {
                                overload
                                    .set_return_type(Type::string_literal(db, type_alias.name(db)));
                            }
                            [
                                Some(Type::PropertyInstance(property)),
                                Some(Type::KnownInstance(KnownInstanceType::TypeVar(typevar))),
                                ..,
                            ] => {
                                match property
                                    .getter(db)
                                    .and_then(Type::as_function_literal)
                                    .map(|f| f.name(db).as_str())
                                {
                                    Some("__name__") => {
                                        overload.set_return_type(Type::string_literal(
                                            db,
                                            typevar.name(db),
                                        ));
                                    }
                                    Some("__bound__") => {
                                        overload.set_return_type(
                                            typevar
                                                .upper_bound(db)
                                                .unwrap_or_else(|| Type::none(db)),
                                        );
                                    }
                                    Some("__constraints__") => {
                                        overload.set_return_type(Type::heterogeneous_tuple(
                                            db,
                                            typevar.constraints(db).into_iter().flatten(),
                                        ));
                                    }
                                    Some("__default__") => {
                                        overload.set_return_type(
                                            typevar.default_type(db).unwrap_or_else(|| {
                                                KnownClass::NoDefaultType.to_instance(db)
                                            }),
                                        );
                                    }
                                    _ => {}
                                }
                            }
                            [Some(Type::PropertyInstance(property)), Some(instance), ..] => {
                                if let Some(getter) = property.getter(db) {
                                    if let Ok(return_ty) = getter
                                        .try_call(db, &CallArguments::positional([*instance]))
                                        .map(|binding| binding.return_type(db))
                                    {
                                        overload.set_return_type(return_ty);
                                    } else {
                                        overload.errors.push(BindingError::InternalCallError(
                                            "calling the getter failed",
                                        ));
                                        overload.set_return_type(Type::unknown());
                                    }
                                } else {
                                    overload
                                        .errors
                                        .push(BindingError::PropertyHasNoSetter(*property));
                                    overload.set_return_type(Type::Never);
                                }
                            }
                            _ => {}
                        }
                    }

                    Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderGet(property)) => {
                        match overload.parameter_types() {
                            [Some(instance), ..] if instance.is_none(db) => {
                                overload.set_return_type(Type::PropertyInstance(property));
                            }
                            [Some(instance), ..] => {
                                if let Some(getter) = property.getter(db) {
                                    if let Ok(return_ty) = getter
                                        .try_call(db, &CallArguments::positional([*instance]))
                                        .map(|binding| binding.return_type(db))
                                    {
                                        overload.set_return_type(return_ty);
                                    } else {
                                        overload.errors.push(BindingError::InternalCallError(
                                            "calling the getter failed",
                                        ));
                                        overload.set_return_type(Type::unknown());
                                    }
                                } else {
                                    overload.set_return_type(Type::Never);
                                    overload.errors.push(BindingError::InternalCallError(
                                        "property has no getter",
                                    ));
                                }
                            }
                            _ => {}
                        }
                    }

                    Type::WrapperDescriptor(WrapperDescriptorKind::PropertyDunderSet) => {
                        if let [
                            Some(Type::PropertyInstance(property)),
                            Some(instance),
                            Some(value),
                            ..,
                        ] = overload.parameter_types()
                        {
                            if let Some(setter) = property.setter(db) {
                                if let Ok(return_ty) = setter
                                    .try_call(db, &CallArguments::positional([*instance, *value]))
                                    .map(|binding| binding.return_type(db))
                                {
                                    // `property.__set__` returns `None` for ordinary setters, but
                                    // preserving `Never` keeps non-returning setters divergent.
                                    overload.set_return_type(if return_ty.is_never() {
                                        return_ty
                                    } else {
                                        Type::none(db)
                                    });
                                } else {
                                    overload.errors.push(BindingError::InternalCallError(
                                        "calling the setter failed",
                                    ));
                                    overload.set_return_type(Type::unknown());
                                }
                            } else {
                                overload
                                    .errors
                                    .push(BindingError::PropertyHasNoSetter(*property));
                            }
                        }
                    }

                    Type::WrapperDescriptor(WrapperDescriptorKind::PropertyDunderDelete) => {
                        if let [Some(Type::PropertyInstance(property)), Some(instance), ..] =
                            overload.parameter_types()
                        {
                            if let Some(deleter) = property.deleter(db) {
                                if let Ok(return_ty) = deleter
                                    .try_call(db, &CallArguments::positional([*instance]))
                                    .map(|binding| binding.return_type(db))
                                {
                                    // `property.__delete__` returns `None` for ordinary deleters,
                                    // but preserving `Never` keeps non-returning deleters divergent.
                                    overload.set_return_type(if return_ty.is_never() {
                                        return_ty
                                    } else {
                                        Type::none(db)
                                    });
                                } else {
                                    overload.errors.push(BindingError::InternalCallError(
                                        "calling the deleter failed",
                                    ));
                                    overload.set_return_type(Type::unknown());
                                }
                            } else {
                                overload
                                    .errors
                                    .push(BindingError::PropertyHasNoDeleter(*property));
                            }
                        }
                    }

                    Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderSet(property)) => {
                        if let [Some(instance), Some(value), ..] = overload.parameter_types() {
                            if let Some(setter) = property.setter(db) {
                                if let Ok(return_ty) = setter
                                    .try_call(db, &CallArguments::positional([*instance, *value]))
                                    .map(|binding| binding.return_type(db))
                                {
                                    // `property.__set__` returns `None` for ordinary setters, but
                                    // preserving `Never` keeps non-returning setters divergent.
                                    overload.set_return_type(if return_ty.is_never() {
                                        return_ty
                                    } else {
                                        Type::none(db)
                                    });
                                } else {
                                    overload.errors.push(BindingError::InternalCallError(
                                        "calling the setter failed",
                                    ));
                                    overload.set_return_type(Type::unknown());
                                }
                            } else {
                                overload
                                    .errors
                                    .push(BindingError::PropertyHasNoSetter(property));
                            }
                        }
                    }

                    Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderDelete(
                        property,
                    )) => {
                        if let [Some(instance), ..] = overload.parameter_types() {
                            if let Some(deleter) = property.deleter(db) {
                                if let Ok(return_ty) = deleter
                                    .try_call(db, &CallArguments::positional([*instance]))
                                    .map(|binding| binding.return_type(db))
                                {
                                    // `property.__delete__` returns `None` for ordinary deleters,
                                    // but preserving `Never` keeps non-returning deleters divergent.
                                    overload.set_return_type(if return_ty.is_never() {
                                        return_ty
                                    } else {
                                        Type::none(db)
                                    });
                                } else {
                                    overload.errors.push(BindingError::InternalCallError(
                                        "calling the deleter failed",
                                    ));
                                    overload.set_return_type(Type::unknown());
                                }
                            } else {
                                overload
                                    .errors
                                    .push(BindingError::PropertyHasNoDeleter(property));
                            }
                        }
                    }

                    Type::KnownBoundMethod(KnownBoundMethodType::StrStartswith(literal)) => {
                        if let [Some(first), None, None] = overload.parameter_types()
                            && let Some(prefix) = first.as_string_literal()
                        {
                            overload.set_return_type(Type::bool_literal(
                                literal.value(db).starts_with(prefix.value(db)),
                            ));
                        }
                    }

                    Type::DataclassTransformer(params) => {
                        if let [Some(Type::FunctionLiteral(function))] = overload.parameter_types()
                        {
                            overload.set_return_type(Type::FunctionLiteral(
                                function.with_dataclass_transformer_params(db, params),
                            ));
                        }
                    }

                    Type::DataclassDecorator(params) => match overload.parameter_types() {
                        [Some(Type::ClassLiteral(class_literal))] => {
                            if let Some(target) = invalid_dataclass_target(db, class_literal) {
                                overload
                                    .errors
                                    .push(BindingError::InvalidDataclassApplication(target));
                            } else {
                                overload.set_return_type(Type::from(
                                    class_literal.with_dataclass_params(db, Some(params)),
                                ));
                            }
                        }
                        [Some(Type::GenericAlias(generic_alias))] => {
                            let new_origin = generic_alias
                                .origin(db)
                                .with_dataclass_params(db, Some(params));
                            overload.set_return_type(Type::GenericAlias(GenericAlias::new(
                                db,
                                new_origin,
                                generic_alias.specialization(db),
                            )));
                        }
                        _ => {}
                    },

                    Type::BoundMethod(bound_method)
                        if bound_method.self_instance(db).is_property_instance() =>
                    {
                        match bound_method.function(db).name(db).as_str() {
                            "setter" => {
                                if let [Some(_), Some(setter)] = overload.parameter_types() {
                                    let mut ty_property = bound_method.self_instance(db);
                                    if let Type::PropertyInstance(property) = ty_property {
                                        ty_property =
                                            Type::PropertyInstance(PropertyInstanceType::new(
                                                db,
                                                property.getter(db),
                                                Some(*setter),
                                                property.deleter(db),
                                            ));
                                    }
                                    overload.set_return_type(ty_property);
                                }
                            }
                            "getter" => {
                                if let [Some(_), Some(getter)] = overload.parameter_types() {
                                    let mut ty_property = bound_method.self_instance(db);
                                    if let Type::PropertyInstance(property) = ty_property {
                                        ty_property =
                                            Type::PropertyInstance(PropertyInstanceType::new(
                                                db,
                                                Some(*getter),
                                                property.setter(db),
                                                property.deleter(db),
                                            ));
                                    }
                                    overload.set_return_type(ty_property);
                                }
                            }
                            "deleter" => {
                                if let [Some(_), Some(deleter)] = overload.parameter_types() {
                                    let mut ty_property = bound_method.self_instance(db);
                                    if let Type::PropertyInstance(property) = ty_property {
                                        ty_property =
                                            Type::PropertyInstance(PropertyInstanceType::new(
                                                db,
                                                property.getter(db),
                                                property.setter(db),
                                                Some(*deleter),
                                            ));
                                    }
                                    overload.set_return_type(ty_property);
                                }
                            }
                            _ => {
                                // Fall back to typeshed stubs for all other methods
                            }
                        }
                    }

                    // TODO: This branch can be removed once https://github.com/astral-sh/ty/issues/501 is resolved
                    Type::BoundMethod(bound_method)
                        if bound_method.function(db).name(db) == "__iter__"
                            && is_enum_class(db, bound_method.self_instance(db)) =>
                    {
                        if let Some(enum_instance) = bound_method.self_instance(db).to_instance(db)
                        {
                            overload.set_return_type(
                                KnownClass::Iterator.to_specialized_instance(db, &[enum_instance]),
                            );
                        }
                    }

                    function @ Type::FunctionLiteral(_)
                        if dataclass_field_specifiers.contains(&function) =>
                    {
                        // Helper to get the type of a keyword argument by name. We first try to get it from
                        // the parameter binding (for explicit parameters), and then fall back to checking the
                        // call site arguments (for field-specifier functions that use a `**kwargs` parameter,
                        // instead of specifying `init`, `default` etc. explicitly).
                        let get_argument_type = |name, fallback_to_default| -> Option<Type<'db>> {
                            if let Ok(ty) =
                                overload.parameter_type_by_name(name, fallback_to_default)
                            {
                                return ty;
                            }
                            call_arguments.iter().find_map(|(arg, types)| {
                                if matches!(arg, Argument::Keyword(arg_name) if arg_name == name) {
                                    types.get_default()
                                } else {
                                    None
                                }
                            })
                        };

                        let has_default_value = get_argument_type("default", false).is_some()
                            || get_argument_type("default_factory", false).is_some()
                            || get_argument_type("factory", false).is_some();

                        let init = get_argument_type("init", true);
                        let kw_only = get_argument_type("kw_only", true);
                        let alias = get_argument_type("alias", true);
                        let converter = get_argument_type("converter", true);

                        // `dataclasses.field` and field-specifier functions of commonly used
                        // libraries like `pydantic`, `attrs`, and `SQLAlchemy` all return
                        // the default type for the field (or `Any`) instead of an actual `Field`
                        // instance, even if this is not what happens at runtime (see also below).
                        // We still make use of this fact and pretend that all field specifiers
                        // return the type of the default value:
                        let default_ty = if has_default_value {
                            Some(overload.return_ty)
                        } else {
                            None
                        };

                        let init = init
                            .map(|init| !init.bool(db).is_always_false())
                            .unwrap_or(true);

                        let kw_only = if Program::get(db).python_version(db) >= PythonVersion::PY310
                        {
                            match kw_only.and_then(Type::as_literal_value_kind) {
                                // We are more conservative here when turning the type for `kw_only`
                                // into a bool, because a field specifier in a stub might use
                                // `kw_only: bool = ...` and the truthiness of `...` is always true.
                                // This is different from `init` above because may need to fall back
                                // to `kw_only_default`, whereas `init_default` does not exist.
                                Some(LiteralValueTypeKind::Bool(yes)) => Some(yes),
                                _ => None,
                            }
                        } else {
                            None
                        };

                        let alias = alias
                            .and_then(Type::as_string_literal)
                            .map(|literal| Box::from(literal.value(db)));

                        // Extract the first positional parameter type and the return type from the
                        // converter callable. The input type determines the "input type" for this
                        // field in the `__init__` signature and when assigning to this field on
                        // instances (`my_model.field = …`). The output type is used to validate
                        // that the converter's return type is assignable to the field's declared type.
                        let converter = converter.and_then(|converter_ty| {
                            let mut input_types = UnionBuilder::new(db);
                            let mut output_types = UnionBuilder::new(db);
                            let mut found_any = false;
                            let bindings = converter_ty.bindings(db);
                            // Note: `iter_callable_items` collapses the union/intersection
                            // structure. In principle, if the converter is a union of callables,
                            // we should only accept the intersection of all first parameter
                            // types for the input type. This seems unlikely to be a real world
                            // use case, so we currently don't have any special handling for this.
                            for item in bindings.iter_callable_items() {
                                let binding = item.callable();
                                // The index of the "actual" first parameters depends on whether or not there
                                // is a bound `self` parameter in the converter callable.
                                let first_index = usize::from(binding.bound_type.is_some());
                                // TODO: for generic converters, we currently use the default
                                // specialization so as not to produce any false-positives on
                                // the field declarations. Ideally, we would treat the type
                                // variables as inferable and use the declared field type as
                                // type context to solve them, but no other type checker seems
                                // to support this at the moment, and `converter` is not a
                                // widely used feature anyway.
                                let class_default_specialization = item
                                    .as_constructor()
                                    .map(ConstructorBinding::constructed_instance_type)
                                    .and_then(|ty| ty.class_specialization(db))
                                    .map(|specialization| {
                                        specialization
                                            .generic_context(db)
                                            .default_specialization(db, None)
                                    });
                                for overload in binding {
                                    let params = overload.signature.parameters();
                                    let return_ty = overload.return_ty;

                                    let default_specialization = class_default_specialization
                                        .or_else(|| {
                                            overload
                                                .signature
                                                .generic_context
                                                .map(|ctx| ctx.default_specialization(db, None))
                                        });

                                    if let Some(first_param) = params.get_positional(first_index) {
                                        let mut input_ty = first_param.annotated_type();
                                        if let Some(specialization) = default_specialization {
                                            input_ty =
                                                input_ty.apply_specialization(db, specialization);
                                        }
                                        input_types = input_types.add(input_ty);
                                        let mut output_ty = return_ty;
                                        if let Some(specialization) = default_specialization {
                                            output_ty =
                                                output_ty.apply_specialization(db, specialization);
                                        }
                                        output_types = output_types.add(output_ty);
                                        found_any = true;
                                    } else if let Some((_, variadic)) = params.variadic() {
                                        let mut input_ty = variadic.annotated_type();
                                        if let Some(specialization) = default_specialization {
                                            input_ty =
                                                input_ty.apply_specialization(db, specialization);
                                        }
                                        input_types = input_types.add(input_ty);
                                        output_types = output_types.add(return_ty);
                                        found_any = true;
                                    } else if params.is_gradual() {
                                        input_types = input_types.add(Type::unknown());
                                        output_types = output_types.add(return_ty);
                                        found_any = true;
                                    }
                                }
                            }
                            found_any.then(|| (input_types.build(), output_types.build()))
                        });

                        // `typeshed` pretends that `dataclasses.field()` returns the type of the
                        // default value directly. At runtime, however, this function returns an
                        // instance of `dataclasses.Field`. We also model it this way and return
                        // a known-instance type with information about the field. The drawback
                        // of this approach is that we need to pretend that instances of `Field`
                        // are assignable to `T` if the default type of the field is assignable
                        // to `T`. Otherwise, we would error on `name: str = field(default="")`.
                        overload.set_return_type(Type::KnownInstance(KnownInstanceType::Field(
                            FieldInstance::new(db, default_ty, init, kw_only, alias, converter),
                        )));
                    }

                    Type::FunctionLiteral(function_type) => match function_type.known(db) {
                        Some(KnownFunction::IsEquivalentTo) => {
                            if let [Some(ty_a), Some(ty_b)] = overload.parameter_types() {
                                let constraints = ConstraintSetBuilder::new();
                                let result = constraints.into_owned(|constraints| {
                                    ty_a.when_equivalent_to(db, *ty_b, constraints)
                                });
                                let tracked = InternedConstraintSet::new(db, result);
                                overload.set_return_type(Type::KnownInstance(
                                    KnownInstanceType::ConstraintSet(tracked),
                                ));
                            }
                        }

                        Some(KnownFunction::IsSubtypeOf) => {
                            if let [Some(ty_a), Some(ty_b)] = overload.parameter_types() {
                                let constraints = ConstraintSetBuilder::new();
                                let result = constraints.into_owned(|constraints| {
                                    ty_a.when_subtype_of(
                                        db,
                                        *ty_b,
                                        constraints,
                                        InferableTypeVars::None,
                                    )
                                });
                                let tracked = InternedConstraintSet::new(db, result);
                                overload.set_return_type(Type::KnownInstance(
                                    KnownInstanceType::ConstraintSet(tracked),
                                ));
                            }
                        }

                        Some(KnownFunction::IsAssignableTo) => {
                            if let [Some(ty_a), Some(ty_b)] = overload.parameter_types() {
                                let constraints = ConstraintSetBuilder::new();
                                let result = constraints.into_owned(|constraints| {
                                    ty_a.when_assignable_to(
                                        db,
                                        *ty_b,
                                        constraints,
                                        InferableTypeVars::None,
                                    )
                                });
                                let tracked = InternedConstraintSet::new(db, result);
                                overload.set_return_type(Type::KnownInstance(
                                    KnownInstanceType::ConstraintSet(tracked),
                                ));
                            }
                        }

                        Some(KnownFunction::IsDisjointFrom) => {
                            if let [Some(ty_a), Some(ty_b)] = overload.parameter_types() {
                                let constraints = ConstraintSetBuilder::new();
                                let result = constraints.into_owned(|constraints| {
                                    ty_a.when_disjoint_from(
                                        db,
                                        *ty_b,
                                        constraints,
                                        InferableTypeVars::None,
                                    )
                                });
                                let tracked = InternedConstraintSet::new(db, result);
                                overload.set_return_type(Type::KnownInstance(
                                    KnownInstanceType::ConstraintSet(tracked),
                                ));
                            }
                        }

                        Some(KnownFunction::IsSingleton) => {
                            if let [Some(ty)] = overload.parameter_types() {
                                overload.set_return_type(Type::bool_literal(ty.is_singleton(db)));
                            }
                        }

                        Some(KnownFunction::IsSingleValued) => {
                            if let [Some(ty)] = overload.parameter_types() {
                                overload
                                    .set_return_type(Type::bool_literal(ty.is_single_valued(db)));
                            }
                        }

                        Some(KnownFunction::GenericContext) => {
                            if let [Some(ty)] = overload.parameter_types() {
                                let wrap_generic_context = |generic_context| {
                                    Type::KnownInstance(KnownInstanceType::GenericContext(
                                        generic_context,
                                    ))
                                };

                                let signature_generic_context =
                                    |signature: &CallableSignature<'db>| {
                                        UnionType::try_from_elements(
                                            db,
                                            signature.overloads.iter().map(|signature| {
                                                signature.generic_context.map(wrap_generic_context)
                                            }),
                                        )
                                    };

                                let generic_context_for_simple_type = |ty: Type<'db>| match ty {
                                    Type::ClassLiteral(class) => {
                                        class.generic_context(db).map(wrap_generic_context)
                                    }

                                    Type::FunctionLiteral(function) => {
                                        signature_generic_context(function.signature(db))
                                    }

                                    Type::BoundMethod(bound_method) => signature_generic_context(
                                        bound_method.function(db).signature(db),
                                    ),

                                    Type::Callable(callable) => {
                                        signature_generic_context(callable.signatures(db))
                                    }

                                    Type::KnownInstance(KnownInstanceType::TypeAliasType(
                                        TypeAliasType::PEP695(alias),
                                    )) => alias.generic_context(db).map(wrap_generic_context),

                                    _ => None,
                                };

                                let generic_context = match ty {
                                    Type::Union(union_type) => UnionType::try_from_elements(
                                        db,
                                        union_type
                                            .elements(db)
                                            .iter()
                                            .map(|ty| generic_context_for_simple_type(*ty)),
                                    ),
                                    _ => generic_context_for_simple_type(*ty),
                                };

                                overload.set_return_type(
                                    generic_context.unwrap_or_else(|| Type::none(db)),
                                );
                            }
                        }

                        Some(
                            into_callable @ (KnownFunction::IntoCallable
                            | KnownFunction::IntoRegularCallable),
                        ) => {
                            let [Some(ty)] = overload.parameter_types() else {
                                continue;
                            };
                            let Some(callables) = ty.try_upcast_to_callable(db).map(|callables| {
                                if into_callable == KnownFunction::IntoRegularCallable {
                                    callables.map(|callable| callable.into_regular(db))
                                } else {
                                    callables
                                }
                            }) else {
                                continue;
                            };
                            overload.set_return_type(callables.into_type(db));
                        }

                        Some(KnownFunction::DunderAllNames) => {
                            if let [Some(ty)] = overload.parameter_types() {
                                overload.set_return_type(match ty {
                                    Type::ModuleLiteral(module_literal) => {
                                        let all_names = module_literal
                                            .module(db)
                                            .file(db)
                                            .map(|file| dunder_all_names(db, file))
                                            .unwrap_or_default();
                                        match all_names {
                                            Some(names) => {
                                                let mut names = names.iter().collect::<Vec<_>>();
                                                names.sort();
                                                Type::heterogeneous_tuple(
                                                    db,
                                                    names.iter().map(|name| {
                                                        Type::string_literal(db, name.as_str())
                                                    }),
                                                )
                                            }
                                            None => Type::none(db),
                                        }
                                    }
                                    _ => Type::none(db),
                                });
                            }
                        }

                        Some(KnownFunction::EnumMembers) => {
                            if let [Some(ty)] = overload.parameter_types() {
                                let return_ty = match ty {
                                    Type::ClassLiteral(class) => {
                                        if let Some(metadata) = enums::enum_metadata(db, *class) {
                                            Type::heterogeneous_tuple(
                                                db,
                                                metadata
                                                    .members
                                                    .keys()
                                                    .map(|member| Type::string_literal(db, member)),
                                            )
                                        } else {
                                            Type::unknown()
                                        }
                                    }
                                    _ => Type::unknown(),
                                };

                                overload.set_return_type(return_ty);
                            }
                        }

                        Some(KnownFunction::AllMembers) => {
                            if let [Some(ty)] = overload.parameter_types() {
                                overload.set_return_type(Type::heterogeneous_tuple(
                                    db,
                                    list_members::all_members(db, *ty)
                                        .into_iter()
                                        .sorted()
                                        .map(|member| Type::string_literal(db, &member.name)),
                                ));
                            }
                        }

                        Some(KnownFunction::Len) => {
                            if let [Some(first_arg)] = overload.parameter_types() {
                                if let Some(len_ty) = first_arg.len(db) {
                                    overload.set_return_type(len_ty);
                                }
                            }
                        }

                        Some(KnownFunction::Repr) => {
                            if let [Some(first_arg)] = overload.parameter_types() {
                                overload.set_return_type(first_arg.repr(db));
                            }
                        }

                        Some(KnownFunction::Cast) => {
                            if let [Some(casted_ty), Some(_)] = overload.parameter_types() {
                                overload.set_return_type(*casted_ty);
                            }
                        }

                        // TODO: Remove this special handling once we have full support for
                        // generic protocols in the solver.
                        Some(KnownFunction::AsyncContextManager) => {
                            if let [Some(callable)] = overload.parameter_types() {
                                if let Some(return_ty) =
                                    asynccontextmanager_return_type(db, *callable)
                                {
                                    overload.set_return_type(return_ty);
                                }
                            }
                        }

                        Some(KnownFunction::IsProtocol) => {
                            if let [Some(ty)] = overload.parameter_types() {
                                // We evaluate this to `Literal[True]` only if the runtime function `typing.is_protocol`
                                // would return `True` for the given type. Internally we consider `SupportsAbs[int]` to
                                // be a "(specialised) protocol class", but `typing.is_protocol(SupportsAbs[int])` returns
                                // `False` at runtime, so we do not set the return type to `Literal[True]` in this case.
                                overload.set_return_type(Type::bool_literal(
                                    ty.as_class_literal()
                                        .is_some_and(|class| class.is_protocol(db)),
                                ));
                            }
                        }

                        Some(KnownFunction::GetProtocolMembers) => {
                            // Similarly to `is_protocol`, we only evaluate to this a frozenset of literal strings if a
                            // class-literal is passed in, not if a generic alias is passed in, to emulate the behaviour
                            // of `typing.get_protocol_members` at runtime.
                            if let [Some(Type::ClassLiteral(class))] = overload.parameter_types() {
                                if let Some(protocol_class) = class.into_protocol_class(db) {
                                    let member_names = protocol_class
                                        .interface(db)
                                        .members(db)
                                        .map(|member| Type::string_literal(db, member.name()));
                                    let specialization = UnionType::from_elements(db, member_names);
                                    overload.set_return_type(
                                        KnownClass::FrozenSet
                                            .to_specialized_instance(db, &[specialization]),
                                    );
                                }
                            }
                        }

                        Some(KnownFunction::GetattrStatic) => {
                            let [Some(instance_ty), Some(attr_name), default] =
                                overload.parameter_types()
                            else {
                                continue;
                            };

                            let Some(attr_name) = attr_name.as_string_literal() else {
                                continue;
                            };

                            let default = if let Some(default) = default {
                                *default
                            } else {
                                Type::Never
                            };

                            let union_with_default =
                                |ty| UnionType::from_two_elements(db, ty, default);

                            // TODO: we could emit a diagnostic here (if default is not set)
                            overload.set_return_type(
                                match instance_ty.static_member(db, attr_name.value(db)) {
                                    Place::Defined(DefinedPlace {
                                        ty,
                                        definedness: Definedness::AlwaysDefined,
                                        ..
                                    }) => {
                                        if ty.is_dynamic() {
                                            // Here, we attempt to model the fact that an attribute lookup on
                                            // a dynamic type could fail

                                            union_with_default(ty)
                                        } else {
                                            ty
                                        }
                                    }
                                    Place::Defined(DefinedPlace {
                                        ty,
                                        definedness: Definedness::PossiblyUndefined,
                                        ..
                                    }) => union_with_default(ty),
                                    Place::Undefined => default,
                                },
                            );
                        }

                        Some(KnownFunction::Dataclass) => {
                            if let [
                                init,
                                repr,
                                eq,
                                order,
                                unsafe_hash,
                                frozen,
                                match_args,
                                kw_only,
                                slots,
                                weakref_slot,
                            ] = overload.parameter_types()
                            {
                                let mut flags = DataclassFlags::empty();

                                if to_bool(init, true) {
                                    flags |= DataclassFlags::INIT;
                                }
                                if to_bool(repr, true) {
                                    flags |= DataclassFlags::REPR;
                                }
                                if to_bool(eq, true) {
                                    flags |= DataclassFlags::EQ;
                                }
                                if to_bool(order, false) {
                                    flags |= DataclassFlags::ORDER;
                                }
                                if to_bool(unsafe_hash, false) {
                                    flags |= DataclassFlags::UNSAFE_HASH;
                                }
                                if to_bool(frozen, false) {
                                    flags |= DataclassFlags::FROZEN;
                                }
                                if to_bool(match_args, true) {
                                    if Program::get(db).python_version(db) >= PythonVersion::PY310 {
                                        flags |= DataclassFlags::MATCH_ARGS;
                                    } else {
                                        // TODO: emit diagnostic
                                    }
                                }
                                if to_bool(kw_only, false) {
                                    if Program::get(db).python_version(db) >= PythonVersion::PY310 {
                                        flags |= DataclassFlags::KW_ONLY;
                                    } else {
                                        // TODO: emit diagnostic
                                    }
                                }
                                if to_bool(slots, false) {
                                    if Program::get(db).python_version(db) >= PythonVersion::PY310 {
                                        flags |= DataclassFlags::SLOTS;
                                    } else {
                                        // TODO: emit diagnostic
                                    }
                                }
                                if to_bool(weakref_slot, false) {
                                    if Program::get(db).python_version(db) >= PythonVersion::PY311 {
                                        flags |= DataclassFlags::WEAKREF_SLOT;
                                    } else {
                                        // TODO: emit diagnostic
                                    }
                                }

                                let params = DataclassParams::from_flags(db, flags);

                                overload.set_return_type(Type::DataclassDecorator(params));
                            }

                            // `dataclass` being used as a non-decorator (i.e., `dataclass(SomeClass)`)
                            if let [Some(Type::ClassLiteral(class_literal)), ..] =
                                overload.parameter_types()
                            {
                                if let Some(target) = invalid_dataclass_target(db, class_literal) {
                                    overload
                                        .errors
                                        .push(BindingError::InvalidDataclassApplication(target));
                                } else {
                                    let params = DataclassParams::default_params(db);
                                    overload.set_return_type(Type::from(
                                        class_literal.with_dataclass_params(db, Some(params)),
                                    ));
                                }
                            }
                        }

                        Some(KnownFunction::DataclassTransform) => {
                            // Use named parameter lookup to handle custom
                            // `__dataclass_transform__` functions that follow older versions
                            // of the spec.
                            let mut flags = DataclassTransformerFlags::empty();

                            let eq_default = overload
                                .parameter_type_by_name("eq_default", false)
                                .ok()
                                .flatten();
                            let order_default = overload
                                .parameter_type_by_name("order_default", false)
                                .ok()
                                .flatten();
                            let kw_only_default = overload
                                .parameter_type_by_name("kw_only_default", false)
                                .ok()
                                .flatten();
                            let frozen_default = overload
                                .parameter_type_by_name("frozen_default", false)
                                .ok()
                                .flatten();

                            if to_bool(&eq_default, true) {
                                flags |= DataclassTransformerFlags::EQ_DEFAULT;
                            }
                            if to_bool(&order_default, false) {
                                flags |= DataclassTransformerFlags::ORDER_DEFAULT;
                            }
                            if to_bool(&kw_only_default, false) {
                                flags |= DataclassTransformerFlags::KW_ONLY_DEFAULT;
                            }
                            if to_bool(&frozen_default, false) {
                                flags |= DataclassTransformerFlags::FROZEN_DEFAULT;
                            }

                            // Accept both `field_specifiers` (current name) and
                            // `field_descriptors` (legacy name).
                            let field_specifiers_param = overload
                                .parameter_type_by_name("field_specifiers", false)
                                .ok()
                                .flatten()
                                .or_else(|| {
                                    overload
                                        .parameter_type_by_name("field_descriptors", false)
                                        .ok()
                                        .flatten()
                                });

                            let field_specifiers: Box<[Type<'db>]> = field_specifiers_param
                                .map(|tuple_type| {
                                    tuple_type
                                        .exact_tuple_instance_spec(db)
                                        .iter()
                                        .flat_map(|tuple_spec| tuple_spec.fixed_elements())
                                        .copied()
                                        .collect::<Vec<_>>()
                                        .into_boxed_slice()
                                })
                                .unwrap_or_default();

                            let params =
                                DataclassTransformerParams::new(db, flags, field_specifiers);

                            overload.set_return_type(Type::DataclassTransformer(params));
                        }

                        Some(KnownFunction::Unpack) => {
                            let [Some(format), Some(_buffer)] = overload.parameter_types() else {
                                continue;
                            };

                            let Some(format_literal) = format.as_string_literal() else {
                                continue;
                            };

                            let return_type = parse_struct_format(db, format_literal.value(db))
                                .map(|elements| Type::heterogeneous_tuple(db, elements))
                                .unwrap_or_else(|| Type::homogeneous_tuple(db, Type::unknown()));

                            overload.set_return_type(return_type);
                        }

                        _ => {
                            // Ideally, either the implementation, or exactly one of the overloads
                            // of the function can have the dataclass_transform decorator applied.
                            // However, we do not yet enforce this, and in the case of multiple
                            // applications of the decorator, we will only consider the last one.
                            let transformer_params = function_type
                                .iter_overloads_and_implementation(db)
                                .rev()
                                .find_map(|function_overload| {
                                    function_overload.dataclass_transformer_params(db)
                                });

                            if let Some(params) = transformer_params {
                                // If this function was called with a keyword argument like
                                // `order=False`, we extract the argument type and overwrite
                                // the corresponding flag in `dataclass_params`.
                                let dataclass_params =
                                    DataclassParams::from_transformer_params(db, params);
                                let mut flags = dataclass_params.flags(db);

                                for (param, flag) in DATACLASS_FLAGS {
                                    if let Some(ty) =
                                        call_arguments.iter().find_map(|(arg, arg_types)| {
                                            if let Argument::Keyword(arg_name) = arg
                                                && *arg_name == **param
                                            {
                                                arg_types.get_default()
                                            } else {
                                                None
                                            }
                                        })
                                        && let Some(LiteralValueTypeKind::Bool(value)) =
                                            ty.as_literal_value_kind()
                                    {
                                        flags.set(*flag, value);
                                    }
                                }

                                let dataclass_params = DataclassParams::new(
                                    db,
                                    flags,
                                    dataclass_params.field_specifiers(db),
                                );

                                // The dataclass_transform spec doesn't clarify how to tell whether
                                // a decorated function is a decorator or a decorator factory. We
                                // use heuristics based on the number and type of positional arguments:
                                //
                                // - Zero positional arguments: assume it's a decorator factory.
                                // - More than one positional argument: assume it's a decorator factory.
                                // - Exactly one positional argument that's a class: ambiguous, so check
                                //   the return type to disambiguate (class-like means decorate directly).
                                let mut positional_args = overload
                                    .signature
                                    .parameters()
                                    .iter()
                                    .zip(overload.parameter_types())
                                    .filter(|(param, ty)| ty.is_some() && !param.is_keyword_only())
                                    .map(|(_, ty)| ty);

                                let first_positional = positional_args.next();
                                let has_more = positional_args.next().is_some();

                                // Only attempt direct decoration if exactly one positional argument.
                                if !has_more {
                                    // Helper to check if return type is class-like.
                                    let returns_class = || {
                                        matches!(
                                            overload.return_type(),
                                            Type::ClassLiteral(_)
                                                | Type::GenericAlias(_)
                                                | Type::SubclassOf(_)
                                        )
                                    };

                                    match first_positional {
                                        Some(Some(Type::ClassLiteral(class_literal)))
                                            if returns_class() =>
                                        {
                                            overload.set_return_type(Type::from(
                                                class_literal.with_dataclass_params(
                                                    db,
                                                    Some(dataclass_params),
                                                ),
                                            ));
                                            continue;
                                        }
                                        Some(Some(Type::GenericAlias(generic_alias)))
                                            if returns_class() =>
                                        {
                                            let new_origin = generic_alias
                                                .origin(db)
                                                .with_dataclass_params(db, Some(dataclass_params));
                                            overload.set_return_type(Type::GenericAlias(
                                                GenericAlias::new(
                                                    db,
                                                    new_origin,
                                                    generic_alias.specialization(db),
                                                ),
                                            ));
                                            continue;
                                        }
                                        _ => {}
                                    }
                                }

                                // Zero or more than one positional argument, or the argument is
                                // not a class: assume it's a decorator factory.
                                overload
                                    .set_return_type(Type::DataclassDecorator(dataclass_params));
                            }
                        }
                    },

                    Type::KnownBoundMethod(KnownBoundMethodType::ConstraintSetRange) => {
                        let [Some(lower), Some(Type::TypeVar(typevar)), Some(upper)] =
                            overload.parameter_types()
                        else {
                            return;
                        };
                        let constraints = ConstraintSetBuilder::new();
                        let result = constraints.into_owned(|constraints| {
                            ConstraintSet::constrain_typevar(
                                db,
                                constraints,
                                *typevar,
                                *lower,
                                *upper,
                            )
                        });
                        let tracked = InternedConstraintSet::new(db, result);
                        overload.set_return_type(Type::KnownInstance(
                            KnownInstanceType::ConstraintSet(tracked),
                        ));
                    }

                    Type::KnownBoundMethod(KnownBoundMethodType::ConstraintSetAlways) => {
                        if !overload.parameter_types().is_empty() {
                            return;
                        }
                        let constraints = ConstraintSetBuilder::new();
                        let result = constraints
                            .into_owned(|constraints| ConstraintSet::from_bool(constraints, true));
                        let tracked = InternedConstraintSet::new(db, result);
                        overload.set_return_type(Type::KnownInstance(
                            KnownInstanceType::ConstraintSet(tracked),
                        ));
                    }

                    Type::KnownBoundMethod(KnownBoundMethodType::ConstraintSetNever) => {
                        if !overload.parameter_types().is_empty() {
                            return;
                        }
                        let constraints = ConstraintSetBuilder::new();
                        let result = constraints
                            .into_owned(|constraints| ConstraintSet::from_bool(constraints, false));
                        let tracked = InternedConstraintSet::new(db, result);
                        overload.set_return_type(Type::KnownInstance(
                            KnownInstanceType::ConstraintSet(tracked),
                        ));
                    }

                    Type::KnownBoundMethod(
                        KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(tracked),
                    ) => {
                        let [Some(ty_a), Some(ty_b)] = overload.parameter_types() else {
                            continue;
                        };

                        let constraints = ConstraintSetBuilder::new();
                        let result = constraints.into_owned(|constraints| {
                            ty_a.when_subtype_of_assuming(
                                db,
                                *ty_b,
                                constraints.load(db, tracked.constraints(db)),
                                constraints,
                                InferableTypeVars::None,
                            )
                        });
                        let tracked = InternedConstraintSet::new(db, result);
                        overload.set_return_type(Type::KnownInstance(
                            KnownInstanceType::ConstraintSet(tracked),
                        ));
                    }

                    Type::KnownBoundMethod(KnownBoundMethodType::ConstraintSetSatisfies(
                        tracked,
                    )) => {
                        let [Some(other)] = overload.parameter_types() else {
                            continue;
                        };
                        let Type::KnownInstance(KnownInstanceType::ConstraintSet(other)) = other
                        else {
                            continue;
                        };

                        let constraints = ConstraintSetBuilder::new();
                        let result = constraints.into_owned(|constraints| {
                            let lhs = constraints.load(db, tracked.constraints(db));
                            let rhs = constraints.load(db, other.constraints(db));
                            lhs.implies(db, constraints, || rhs)
                        });
                        let tracked = InternedConstraintSet::new(db, result);
                        overload.set_return_type(Type::KnownInstance(
                            KnownInstanceType::ConstraintSet(tracked),
                        ));
                    }

                    Type::KnownBoundMethod(
                        KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(tracked),
                    ) => {
                        let extract_inferable = |instance: &NominalInstanceType<'db>| {
                            if instance.has_known_class(db, KnownClass::NoneType) {
                                // Caller explicitly passed None, so no typevars are inferable.
                                return Some(InferableTypeVars::None);
                            }
                            let typevars: Option<FxOrderSet<_>> = instance
                                .tuple_spec(db)?
                                .fixed_elements()
                                .map(|ty| {
                                    ty.as_typevar()
                                        .map(|bound_typevar| bound_typevar.identity(db))
                                })
                                .collect();
                            typevars.map(|typevars| InferableTypeVars::from_typevars(db, typevars))
                        };

                        let inferable = match overload.parameter_types() {
                            // Caller did not provide argument, so no typevars are inferable.
                            [None] => InferableTypeVars::None,
                            [Some(Type::NominalInstance(instance))] => {
                                match extract_inferable(instance) {
                                    Some(inferable) => inferable,
                                    None => continue,
                                }
                            }
                            _ => continue,
                        };

                        let constraints = ConstraintSetBuilder::new();
                        let set = constraints.load(db, tracked.constraints(db));
                        let result = set.satisfied_by_all_typevars(db, &constraints, inferable);
                        overload.set_return_type(Type::bool_literal(result));
                    }

                    Type::ClassLiteral(class) => match class.known(db) {
                        Some(KnownClass::Bool) => match overload.parameter_types() {
                            [Some(arg)] => {
                                overload.set_return_type(Type::from_truthiness(db, arg.bool(db)));
                            }
                            [None] => overload.set_return_type(Type::bool_literal(false)),
                            _ => {}
                        },

                        Some(KnownClass::Str) if overload_index == 0 => {
                            match overload.parameter_types() {
                                [Some(arg)] => overload.set_return_type(arg.str(db)),
                                [None] => {
                                    overload.set_return_type(Type::string_literal(db, ""));
                                }
                                _ => {}
                            }
                        }

                        Some(KnownClass::Type) if overload_index == 0 => {
                            if let [Some(arg)] = overload.parameter_types() {
                                overload.set_return_type(arg.dunder_class(db));
                            }
                        }

                        Some(KnownClass::Property) => {
                            if let [getter, setter, deleter, ..] = overload.parameter_types() {
                                let getter = getter.filter(|ty| !ty.is_none(db));
                                let setter = setter.filter(|ty| !ty.is_none(db));
                                let deleter = deleter.filter(|ty| !ty.is_none(db));
                                overload.set_return_type(Type::PropertyInstance(
                                    PropertyInstanceType::new(db, getter, setter, deleter),
                                ));
                            }
                        }

                        Some(KnownClass::FunctoolsPartial) => {
                            if let Some(new_return_type) =
                                overload.functools_partial_return_type(db, call_arguments)
                            {
                                overload.set_return_type(new_return_type);
                            }
                        }

                        Some(KnownClass::Tuple) if overload_index == 1 => {
                            // `tuple(range(42))` => `tuple[int, ...]`
                            // BUT `tuple((1, 2))` => `tuple[Literal[1], Literal[2]]` rather than `tuple[Literal[1, 2], ...]`
                            if let [Some(argument)] = overload.parameter_types() {
                                // We deliberately use `.iterate()` here (falling back to `Unknown` if it isn't iterable)
                                // rather than `.try_iterate().expect()`. Even though we know at this point that the input
                                // type is assignable to `Iterable`, that doesn't mean that the input type is *actually*
                                // iterable (it could be a Liskov-uncompliant subtype of the `Iterable` class that sets
                                // `__iter__ = None`, for example). That would be badly written Python code, but we still
                                // need to be able to handle it without crashing.
                                let return_type = if let Type::Union(union) = argument {
                                    union.map(db, |element| {
                                        Type::tuple(TupleType::new(db, &element.iterate(db)))
                                    })
                                } else {
                                    Type::tuple(TupleType::new(db, &argument.iterate(db)))
                                };
                                overload.set_return_type(return_type);
                            }
                        }

                        _ => {}
                    },

                    // Not a special case
                    _ => {}
                }
            }
        }
    }
}

impl<'db> From<CallableBinding<'db>> for Bindings<'db> {
    fn from(from: CallableBinding<'db>) -> Bindings<'db> {
        Bindings {
            callable_type: from.callable_type,
            elements: smallvec_inline![BindingsElement {
                items: smallvec_inline![CallableItem::Regular(from)],
            }],
            argument_forms: ArgumentForms::new(0),
            implicit_dunder_new_is_possibly_unbound: false,
            implicit_dunder_init_is_possibly_unbound: false,
        }
    }
}

impl<'db> From<Binding<'db>> for Bindings<'db> {
    fn from(from: Binding<'db>) -> Bindings<'db> {
        let callable_type = from.callable_type;
        let signature_type = from.signature_type;
        let callable_binding = CallableBinding {
            callable_type,
            signature_type,
            dunder_call_is_possibly_unbound: false,
            bound_type: None,
            overload_call_return_type: None,
            matching_overload_before_type_checking: None,
            overloads: smallvec_inline![from],
        };
        Bindings {
            callable_type,
            elements: smallvec_inline![BindingsElement {
                items: smallvec_inline![CallableItem::Regular(callable_binding)],
            }],
            argument_forms: ArgumentForms::new(0),
            implicit_dunder_new_is_possibly_unbound: false,
            implicit_dunder_init_is_possibly_unbound: false,
        }
    }
}

/// Binding information for a single callable. If the callable is overloaded, there is a separate
/// [`Binding`] for each overload.
///
/// For a successful binding, each argument is mapped to one of the callable's formal parameters.
/// If the callable has multiple overloads, the first one that matches is used as the overall
/// binding match.
///
/// If the arguments cannot be matched to formal parameters, we store information about the
/// specific errors that occurred when trying to match them up. If the callable has multiple
/// overloads, we store this error information for each overload.
#[derive(Debug, Clone)]
pub(crate) struct CallableBinding<'db> {
    /// The type that is (hopefully) callable.
    pub(crate) callable_type: Type<'db>,

    /// The type we'll use for error messages referring to details of the called signature. For
    /// calls to functions this will be the same as `callable_type`; for other callable instances
    /// it may be a `__call__` method.
    pub(crate) signature_type: Type<'db>,

    /// If this is a callable object (i.e. called via a `__call__` method), the boundness of
    /// that call method.
    pub(crate) dunder_call_is_possibly_unbound: bool,

    /// The type of the bound `self` or `cls` parameter if this signature is for a bound method.
    pub(crate) bound_type: Option<Type<'db>>,

    /// The return type of this overloaded callable.
    ///
    /// This is [`Some`] only in the following cases:
    /// 1. Argument type expansion was performed and one of the expansions evaluated successfully
    ///    for all of the argument lists, or
    /// 2. Overload call evaluation was ambiguous, meaning that multiple overloads matched the
    ///    argument lists, but they all had different return types
    ///
    /// For (1), the final return type is the union of all the return types of the matched
    /// overloads for the expanded argument lists.
    ///
    /// For (2), the final return type is [`Unknown`].
    ///
    /// [`Unknown`]: crate::types::DynamicType::Unknown
    overload_call_return_type: Option<OverloadCallReturnType<'db>>,

    /// The index of the overload that matched for this overloaded callable before type checking.
    ///
    /// This is [`Some`] only for step 1 of the [overload call evaluation algorithm][1] to surface
    /// the diagnostics for the matching overload directly instead of using the
    /// `no-matching-overload` diagnostic. The [`Self::matching_overload_index`] method cannot be
    /// used here because a single overload could be matched in step 1 but then filtered out in the
    /// following steps.
    ///
    /// [1]: https://typing.python.org/en/latest/spec/overload.html#overload-call-evaluation
    matching_overload_before_type_checking: Option<usize>,

    /// The bindings of each overload of this callable. Will be empty if the type is not callable.
    ///
    /// By using `SmallVec`, we avoid an extra heap allocation for the common case of a
    /// non-overloaded callable.
    overloads: SmallVec<[Binding<'db>; 1]>,
}

#[derive(Copy, Clone)]
enum FailingOverloadSelection {
    /// Consider all errors that participate in overload filtering.
    AffectsOverloadResolution,
    /// Consider only errors that are reported during `functools.partial(...)` construction.
    ReportableForPartial,
}

impl FailingOverloadSelection {
    /// Returns whether this selection mode should count the given error.
    fn includes(self, error: &BindingError<'_>) -> bool {
        match self {
            Self::AffectsOverloadResolution => error.affects_overload_resolution(),
            Self::ReportableForPartial => error.is_relevant_for_partial_application(),
        }
    }
}

impl<'db> CallableBinding<'db> {
    pub(crate) fn from_overloads(
        signature_type: Type<'db>,
        overloads: impl IntoIterator<Item = Signature<'db>>,
    ) -> Self {
        let overloads = overloads
            .into_iter()
            .map(|signature| Binding::single(signature_type, signature))
            .collect();
        Self {
            callable_type: signature_type,
            signature_type,
            dunder_call_is_possibly_unbound: false,
            bound_type: None,
            overload_call_return_type: None,
            matching_overload_before_type_checking: None,
            overloads,
        }
    }

    pub(crate) fn not_callable(signature_type: Type<'db>) -> Self {
        Self {
            callable_type: signature_type,
            signature_type,
            dunder_call_is_possibly_unbound: false,
            bound_type: None,
            overload_call_return_type: None,
            matching_overload_before_type_checking: None,
            overloads: smallvec![],
        }
    }

    /// Rewrites overload signatures as if an implicit bound receiver argument had already been
    /// consumed.
    pub(crate) fn bake_bound_type_into_overloads(&mut self, db: &'db dyn Db) {
        let Some(bound_self) = self.bound_type.take() else {
            return;
        };
        for overload in &mut self.overloads {
            overload.signature = overload.signature.bind_self(db, Some(bound_self));
        }
    }

    /// Ignore missing-argument errors when constructing `functools.partial(...)`.
    ///
    /// Partial application intentionally leaves some parameters unbound, so we still want to
    /// type-check all explicitly bound arguments against each overload.
    fn clear_missing_argument_errors_for_partial_application(&mut self) {
        for overload in &mut self.overloads {
            overload.clear_missing_argument_errors_for_partial_application();
        }
    }

    /// Ignore downstream constructor call-shape errors when constructing
    /// `functools.partial(...)`.
    ///
    /// The merged partial signature decides which parameters remain callable, so downstream
    /// arity/name mismatches caused by as-yet-unbound constructor parameters should not reject
    /// partial construction. Explicit bound-argument type errors are still preserved.
    fn clear_deferred_constructor_errors_for_partial_application(&mut self) {
        for overload in &mut self.overloads {
            overload.clear_deferred_constructor_errors_for_partial_application();
        }
    }

    /// Chooses which overload to use as the source for diagnostics when no overload fully matches.
    ///
    /// If step 1 of overload resolution identified a single arity match, we keep using that
    /// overload as the diagnostic source. Otherwise, we rank failing overloads by error quality:
    /// fewer unknown-argument errors and fewer relevant errors are preferred.
    fn best_failing_overload_index(&self, selection: FailingOverloadSelection) -> Option<usize> {
        self.matching_overload_before_type_checking.or_else(|| {
            self.overloads
                .iter()
                .enumerate()
                .filter_map(|(index, overload)| {
                    let mut relevant_count = 0;
                    let mut unknown_argument_count = 0;

                    for error in &overload.errors {
                        if !selection.includes(error) {
                            continue;
                        }
                        relevant_count += 1;
                        if matches!(error, BindingError::UnknownArgument { .. }) {
                            unknown_argument_count += 1;
                        }
                    }

                    (relevant_count > 0).then_some((index, unknown_argument_count, relevant_count))
                })
                .min_by_key(|(_, unknown_argument_count, relevant_count)| {
                    (*unknown_argument_count, *relevant_count)
                })
                .map(|(index, _, _)| index)
        })
    }

    /// Returns the matching overload indexes when `functools.partial(...)` ignores errors that are
    /// only relevant at invocation time.
    fn matching_partial_overload_index(&self) -> MatchingOverloadIndex {
        let mut matching_overloads = self.overloads.iter().enumerate().filter(|(_, overload)| {
            !overload
                .errors
                .iter()
                .any(BindingError::is_relevant_for_partial_application)
        });
        match matching_overloads.next() {
            None => MatchingOverloadIndex::None,
            Some((first, _)) => {
                if let Some((second, _)) = matching_overloads.next() {
                    let mut indexes = vec![first, second];
                    for (index, _) in matching_overloads {
                        indexes.push(index);
                    }
                    MatchingOverloadIndex::Multiple(indexes)
                } else {
                    MatchingOverloadIndex::Single(first)
                }
            }
        }
    }

    /// Selects the reduced signature applications for this `functools.partial(...)` binding.
    ///
    /// Diagnostics for invalid bound arguments are still reported back to the outer `partial(...)`
    /// overload. Callable construction happens in the callable layer after this summary is built.
    fn partial_signature_applications<'a>(
        &self,
        db: &'db dyn Db,
        partial_overload: &mut Binding<'db>,
        bound_call_arguments: &CallArguments<'a, 'db>,
    ) -> Option<SmallVec<[PartialSignatureApplication<'db>; 1]>> {
        if self.overloads().is_empty() {
            return None;
        }

        let selected_overload_indexes = match self.matching_partial_overload_index() {
            MatchingOverloadIndex::Single(index) => vec![index],
            MatchingOverloadIndex::Multiple(indexes) => indexes,
            MatchingOverloadIndex::None => {
                let source_overload_index = self
                    .best_failing_overload_index(FailingOverloadSelection::ReportableForPartial)
                    .unwrap_or(0);
                let source_errors = &self.overloads()[source_overload_index].errors;
                for error in source_errors {
                    if error.is_relevant_for_partial_application() {
                        let error = error.clone().maybe_apply_argument_index_offset(Some(1));
                        if !partial_overload.errors.contains(&error) {
                            partial_overload.errors.push(error);
                        }
                    }
                }

                // When no overload is compatible with the bound arguments, don't manufacture a
                // precise reduced signature from an arbitrary overloaded callable shape.
                if self.overloads().len() > 1 {
                    return None;
                }

                vec![source_overload_index]
            }
        };

        let signature_arguments = bound_call_arguments.with_self(self.bound_type);
        let applications: SmallVec<_> = selected_overload_indexes
            .into_iter()
            .filter_map(|index| {
                self.overloads().get(index).map(|overload| {
                    overload.partial_signature_application(signature_arguments.as_ref(), db)
                })
            })
            .collect();
        (!applications.is_empty()).then_some(applications)
    }

    pub(crate) fn with_bound_type(mut self, bound_type: Type<'db>) -> Self {
        self.bound_type = Some(bound_type);
        self
    }

    fn replace_callable_type(&mut self, before: Type<'db>, after: Type<'db>) {
        if self.callable_type == before {
            self.callable_type = after;
        }
        for binding in &mut self.overloads {
            binding.replace_callable_type(before, after);
        }
    }

    fn match_parameters(
        &mut self,
        db: &'db dyn Db,
        arguments: &CallArguments<'_, 'db>,
        argument_forms: &mut ArgumentForms,
    ) {
        // If this callable is a bound method, prepend the self instance onto the arguments list
        // before checking.
        let bound_arguments = arguments.with_self(self.bound_type);

        for overload in &mut self.overloads {
            overload.match_parameters(db, bound_arguments.as_ref(), argument_forms);
        }
    }

    fn check_types(
        &mut self,
        db: &'db dyn Db,
        constraints: &ConstraintSetBuilder<'db>,
        call_arguments: &CallArguments<'_, 'db>,
        call_expression_tcx: TypeContext<'db>,
    ) -> Option<ArgumentForms> {
        // If this callable is a bound method, prepend the self instance onto the arguments list
        // before checking.
        let call_arguments = call_arguments.with_self(self.bound_type);

        let _span = tracing::trace_span!(
            "CallableBinding::check_types",
            arguments = %call_arguments.display(db),
            signature = %self.signature_type.display(db),
        )
        .entered();

        tracing::trace!(
            target: "ty_python_semantic::types::call::bind",
            matching_overload_index = ?self.matching_overload_index(),
            "after step 1",
        );

        // Step 1: Check the result of the arity check which is done by `match_parameters`

        // For overloaded calls with expandable `*args`, any arity-based overload pruning is only
        // provisional. If we have an arity-2 overload and an arity-3 overload, and the call has
        // `*arg` where `arg` is a union of a 2-tuple and a 3-tuple, we shouldn't eliminate any
        // overload for arity reasons before trying argument expansion.
        let (should_retry_after_provisional_arity, overloads_for_expansion) =
            if self.overloads.len() > 1
                && self.matching_overload_index().len() < self.overloads.len()
                && call_arguments.iter().any(|(argument, argument_types)| {
                    matches!(argument, Argument::Variadic)
                        && argument_types
                            .get_default()
                            .is_some_and(|argument_type| is_expandable_type(db, argument_type))
                })
            {
                // We will retry all overloads after argument expansion.
                (true, (0..self.overloads.len()).collect())
            } else {
                match self.matching_overload_index() {
                    MatchingOverloadIndex::None => {
                        // If no candidate overloads remain from the arity check, we can stop here. We
                        // still perform type checking for non-overloaded function to provide better
                        // user experience.
                        if let [overload] = self.overloads.as_mut_slice() {
                            overload.check_types(
                                db,
                                constraints,
                                call_arguments.as_ref(),
                                call_expression_tcx,
                            );
                        }
                        return None;
                    }
                    MatchingOverloadIndex::Single(index) => {
                        // If only one candidate overload remains, it is the winning match. Evaluate
                        // it as a regular (non-overloaded) call.
                        self.matching_overload_before_type_checking = Some(index);
                        self.overloads[index].check_types(
                            db,
                            constraints,
                            call_arguments.as_ref(),
                            call_expression_tcx,
                        );
                        return None;
                    }
                    MatchingOverloadIndex::Multiple(indexes) => (false, indexes),
                }
            };

        // Step 2: Evaluate each remaining overload as a regular (non-overloaded) call to determine
        // whether it is compatible with the supplied argument list.
        for (_, overload) in self.matching_overloads_mut() {
            overload.check_types(
                db,
                constraints,
                call_arguments.as_ref(),
                call_expression_tcx,
            );
        }

        tracing::trace!(
            target: "ty_python_semantic::types::call::bind",
            matching_overload_index = ?self.matching_overload_index(),
            "after step 2",
        );

        // If we are in the "retry for provisional arity" case, we have to try argument expansion
        // before deciding we are done or moving on to step 4+.
        if !should_retry_after_provisional_arity {
            match self.matching_overload_index() {
                MatchingOverloadIndex::None => {
                    // If all overloads result in errors, proceed to step 3.
                }
                MatchingOverloadIndex::Single(_) => {
                    // If only one overload evaluates without error, it is the winning match.
                    return None;
                }
                MatchingOverloadIndex::Multiple(indexes) => {
                    // If two or more candidate overloads remain, proceed to step 4.
                    self.filter_overloads_containing_variadic(&indexes);

                    tracing::trace!(
                        target: "ty_python_semantic::types::call::bind",
                        matching_overload_index = ?self.matching_overload_index(),
                        "after step 4",
                    );

                    match self.matching_overload_index() {
                        MatchingOverloadIndex::None => {
                            // This shouldn't be possible because step 4 can only filter out overloads
                            // when there _is_ a matching variadic argument.
                            tracing::debug!("All overloads have been filtered out in step 4");
                            return None;
                        }
                        MatchingOverloadIndex::Single(_) => {
                            // If only one candidate overload remains, it is the winning match.
                            return None;
                        }
                        MatchingOverloadIndex::Multiple(indexes) => {
                            // If two or more candidate overloads remain, proceed to step 5.
                            self.filter_overloads_using_any_or_unknown(
                                db,
                                constraints,
                                call_arguments.as_ref(),
                                &indexes,
                            );

                            tracing::trace!(
                                target: "ty_python_semantic::types::call::bind",
                                matching_overload_index = ?self.matching_overload_index(),
                                "after step 5",
                            );
                        }
                    }

                    // This shouldn't lead to argument type expansion.
                    return None;
                }
            }
        }

        // Step 3: Perform "argument type expansion". Reference:
        // https://typing.python.org/en/latest/spec/overload.html#argument-type-expansion
        let mut expansions = call_arguments.expand(db).peekable();

        // Return early if there are no argument types to expand.
        expansions.peek()?;

        // At this point, there's at least one argument that can be expanded.
        //
        // This heuristic tries to detect if there's any need to perform argument type expansion or
        // not by checking whether there are any non-expandable argument type that cannot be
        // assigned to any of the overloads.
        for (argument_index, (argument, argument_types)) in call_arguments.iter().enumerate() {
            // TODO: Remove `Keywords` once `**kwargs` support is added
            if matches!(argument, Argument::Synthetic | Argument::Keywords) {
                continue;
            }
            // TODO: For types inferred multiple times with distinct type context, we currently only
            // expand the default inference.
            let Some(argument_type) = argument_types.get_default() else {
                continue;
            };
            if is_expandable_type(db, argument_type) {
                continue;
            }
            let mut is_argument_assignable_to_any_overload = false;
            'overload: for overload in &self.overloads {
                for parameter_index in &overload.argument_matches[argument_index].parameters {
                    let parameter_type =
                        overload.signature.parameters()[*parameter_index].annotated_type();
                    let argument_type = argument_types.get_for_declared_type(parameter_type);
                    if argument_type
                        .when_assignable_to(
                            db,
                            parameter_type,
                            constraints,
                            overload.inferable_typevars,
                        )
                        .is_always_satisfied(db)
                    {
                        is_argument_assignable_to_any_overload = true;
                        break 'overload;
                    }
                }
            }
            if !is_argument_assignable_to_any_overload {
                tracing::debug!(
                    "Argument at {argument_index} (`{}`) is not assignable to any of the \
                    remaining overloads, skipping argument type expansion",
                    argument_type.display(db)
                );
                return None;
            }
        }

        let snapshotter = CallableBindingSnapshotter::new(overloads_for_expansion);

        // State of the bindings _after_ evaluating (type checking) the matching overloads using
        // the non-expanded argument types.
        let post_evaluation_snapshot = snapshotter.take(self);

        for expansion in expansions {
            let expanded_argument_lists = match expansion {
                Expansion::LimitReached(index) => {
                    snapshotter.restore(self, post_evaluation_snapshot);
                    self.overload_call_return_type = Some(
                        OverloadCallReturnType::ArgumentTypeExpansionLimitReached(index),
                    );
                    return None;
                }
                Expansion::Expanded(argument_lists) => argument_lists,
            };

            // This is the merged state of the bindings after evaluating all of the expanded
            // argument lists. This will be the final state to restore the bindings to if all of
            // the expanded argument lists evaluated successfully.
            let mut merged_evaluation_state: Option<CallableBindingSnapshot<'db>> = None;

            // Merged argument forms after evaluating all the argument lists in this expansion.
            let mut merged_argument_forms = ArgumentForms::default();

            // The return types of each of the expanded argument lists that evaluated successfully.
            let mut return_types = Vec::new();

            for expanded_arguments in &expanded_argument_lists {
                let mut argument_forms = ArgumentForms::new(expanded_arguments.len());

                // The spec mentions that each expanded argument list should be re-evaluated from
                // step 2 but we need to re-evaluate from step 1 because our step 1 does more than
                // what the spec mentions. Step 1 of the spec means only "eliminate impossible
                // overloads due to arity mismatch" while our step 1 (`match_parameters`) also
                // includes "match arguments to the parameters". This is important because it
                // allows us to correctly handle cases involving a variadic argument that could
                // expand into different number of arguments with each expansion. Refer to
                // https://github.com/astral-sh/ty/issues/735 for more details.
                for overload in &mut self.overloads {
                    // Clear the state of all overloads before re-evaluating from step 1
                    overload.reset(db);
                    overload.match_parameters(db, expanded_arguments, &mut argument_forms);
                }

                tracing::trace!(
                    target: "ty_python_semantic::types::call::bind",
                    matching_overload_index = ?self.matching_overload_index(),
                    "after step 1",
                );

                merged_argument_forms.merge(&argument_forms);

                for (_, overload) in self.matching_overloads_mut() {
                    overload.check_types(db, constraints, expanded_arguments, call_expression_tcx);
                }

                tracing::trace!(
                    target: "ty_python_semantic::types::call::bind",
                    matching_overload_index = ?self.matching_overload_index(),
                    "after step 2",
                );

                let return_type = match self.matching_overload_index() {
                    MatchingOverloadIndex::None => None,
                    MatchingOverloadIndex::Single(index) => {
                        Some(self.overloads[index].return_type())
                    }
                    MatchingOverloadIndex::Multiple(matching_overload_indexes) => {
                        self.filter_overloads_containing_variadic(&matching_overload_indexes);

                        tracing::trace!(
                            target: "ty_python_semantic::types::call::bind",
                            matching_overload_index = ?self.matching_overload_index(),
                            "after step 4",
                        );

                        match self.matching_overload_index() {
                            MatchingOverloadIndex::None => {
                                tracing::debug!(
                                    "All overloads have been filtered out in step 4 during argument type expansion"
                                );
                                None
                            }
                            MatchingOverloadIndex::Single(_) => Some(self.return_type()),
                            MatchingOverloadIndex::Multiple(indexes) => {
                                self.filter_overloads_using_any_or_unknown(
                                    db,
                                    constraints,
                                    expanded_arguments,
                                    &indexes,
                                );

                                tracing::trace!(
                                    target: "ty_python_semantic::types::call::bind",
                                    matching_overload_index = ?self.matching_overload_index(),
                                    "after step 5",
                                );

                                Some(self.return_type())
                            }
                        }
                    }
                };

                // This split between initializing and updating the merged evaluation state is
                // required because otherwise it's difficult to differentiate between the
                // following:
                // 1. An initial unmatched overload becomes a matched overload when evaluating the
                //    first argument list
                // 2. An unmatched overload after evaluating the first argument list becomes a
                //    matched overload when evaluating the second argument list
                if let Some(merged_evaluation_state) = merged_evaluation_state.as_mut() {
                    merged_evaluation_state.update(self);
                } else {
                    merged_evaluation_state = Some(snapshotter.take(self));
                }

                if let Some(return_type) = return_type {
                    return_types.push(return_type);
                } else {
                    // No need to check the remaining argument lists if the current argument list
                    // doesn't evaluate successfully. Move on to expanding the next argument type.
                    break;
                }
            }

            if return_types.len() == expanded_argument_lists.len() {
                // Restore the bindings state to the one that merges the bindings state evaluating
                // each of the expanded argument list.
                //
                // Note that this needs to happen *before* setting the return type, because this
                // will restore the return type to the one before argument type expansion.
                if let Some(merged_evaluation_state) = merged_evaluation_state {
                    snapshotter.restore(self, merged_evaluation_state);
                }

                // If the number of return types is equal to the number of expanded argument lists,
                // they all evaluated successfully. So, we need to combine their return types by
                // union to determine the final return type.
                self.overload_call_return_type =
                    Some(OverloadCallReturnType::ArgumentTypeExpansion(
                        UnionType::from_elements(db, return_types),
                    ));

                return Some(merged_argument_forms);
            }
        }

        // If the type expansion didn't yield any successful return type, we need to restore the
        // bindings state back to the one after the type checking step using the non-expanded
        // argument types. This is necessary because we restore the state to the pre-evaluation
        // snapshot when processing the expanded argument lists.
        snapshotter.restore(self, post_evaluation_snapshot);

        None
    }

    /// Filter overloads based on variadic argument to variadic parameter match.
    ///
    /// This is the step 4 of the [overload call evaluation algorithm][1].
    ///
    /// [1]: https://typing.python.org/en/latest/spec/overload.html#overload-call-evaluation
    fn filter_overloads_containing_variadic(&mut self, matching_overload_indexes: &[usize]) {
        let variadic_matching_overloads = matching_overload_indexes
            .iter()
            .filter(|&&overload_index| {
                self.overloads[overload_index].variadic_argument_matched_to_variadic_parameter
            })
            .collect::<HashSet<_>>();

        if variadic_matching_overloads.is_empty()
            || variadic_matching_overloads.len() == matching_overload_indexes.len()
        {
            return;
        }

        for overload_index in matching_overload_indexes {
            if !variadic_matching_overloads.contains(overload_index) {
                self.overloads[*overload_index].mark_as_unmatched_overload();
            }
        }
    }

    /// Filter overloads based on [`Any`] or [`Unknown`] argument types.
    ///
    /// This is the step 5 of the [overload call evaluation algorithm][1].
    ///
    /// The filtering works on the remaining overloads that are present at the
    /// `matching_overload_indexes` and are filtered out by marking them as unmatched overloads
    /// using the [`mark_as_unmatched_overload`] method.
    ///
    /// [`Any`]: crate::types::DynamicType::Any
    /// [`Unknown`]: crate::types::DynamicType::Unknown
    /// [`mark_as_unmatched_overload`]: Binding::mark_as_unmatched_overload
    /// [1]: https://typing.python.org/en/latest/spec/overload.html#overload-call-evaluation
    fn filter_overloads_using_any_or_unknown(
        &mut self,
        db: &'db dyn Db,
        constraints: &ConstraintSetBuilder<'db>,
        arguments: &CallArguments<'_, 'db>,
        matching_overload_indexes: &[usize],
    ) {
        struct OverloadFilterSlot<'db> {
            parameter: Type<'db>,
            argument: Type<'db>,
            variadic_argument: Option<Type<'db>>,
        }

        let matching_overload_slots = matching_overload_indexes
            .iter()
            .map(|&index| {
                let overload = &self.overloads[index];
                let slots = overload
                    .argument_matches
                    .iter()
                    .zip(arguments.iter_types())
                    .flat_map(move |(matched_argument, argument_types)| {
                        matched_argument.iter().map(
                            move |(parameter_index, variadic_argument_type)| {
                                // TODO: For an unannotated `self` / `cls` parameter, the type should be
                                // `typing.Self` / `type[typing.Self]`
                                let parameter_type = overload.signature.parameters()
                                    [parameter_index]
                                    .annotated_type()
                                    .apply_optional_specialization(db, overload.specialization);
                                OverloadFilterSlot {
                                    parameter: parameter_type,
                                    argument: argument_types.get_for_declared_type(parameter_type),
                                    variadic_argument: variadic_argument_type,
                                }
                            },
                        )
                    })
                    .collect::<Vec<_>>();
                (index, slots)
            })
            .collect::<Vec<_>>();

        let max_slot_count = matching_overload_slots
            .iter()
            .map(|(_, slots)| slots.len())
            .max()
            .unwrap_or(0);

        let mut participating_slot_indices = HashSet::new();
        for slot_index in 0..max_slot_count {
            let mut first_parameter_type: Option<Type<'db>> = None;
            for (_, overload_slots) in &matching_overload_slots {
                let current_parameter_type =
                    overload_slots.get(slot_index).map(|slot| slot.parameter);
                match (first_parameter_type, current_parameter_type) {
                    (Some(first_parameter_type), Some(current_parameter_type)) => {
                        if !first_parameter_type
                            .when_equivalent_to(db, current_parameter_type, constraints)
                            .is_always_satisfied(db)
                        {
                            participating_slot_indices.insert(slot_index);
                        }
                    }
                    (Some(_), None) => {
                        participating_slot_indices.insert(slot_index);
                    }
                    (None, Some(current_parameter_type)) => {
                        first_parameter_type = Some(current_parameter_type);
                    }
                    (None, None) => {}
                }
            }
        }

        // A flag to indicate whether we've found the overload that makes the remaining overloads
        // unmatched for the given argument types.
        let mut filter_remaining_overloads = false;

        for (upto, current_index) in matching_overload_indexes.iter().enumerate() {
            if filter_remaining_overloads {
                self.overloads[*current_index].mark_as_unmatched_overload();
                continue;
            }

            let mut union_argument_type_builders = std::iter::repeat_with(|| UnionBuilder::new(db))
                .take(max_slot_count)
                .collect::<Vec<_>>();

            let (_, current_slots) = &matching_overload_slots[upto];

            for (_, slots) in &matching_overload_slots {
                for (slot_index, slot) in slots.iter().enumerate() {
                    if participating_slot_indices.contains(&slot_index) {
                        let argument_type = slot.variadic_argument.unwrap_or_else(|| {
                            current_slots
                                .get(slot_index)
                                .map_or(Type::unknown(), |slot| slot.argument)
                        });
                        union_argument_type_builders[slot_index]
                            .add_in_place(argument_type.top_materialization(db));
                    }
                }
            }

            let top_materialized_argument_type = Type::heterogeneous_tuple(
                db,
                union_argument_type_builders
                    .into_iter()
                    .filter_map(|builder| {
                        if builder.is_empty() {
                            None
                        } else {
                            Some(builder.build())
                        }
                    }),
            );

            let mut union_parameter_types = std::iter::repeat_with(|| UnionBuilder::new(db))
                .take(max_slot_count)
                .collect::<Vec<_>>();
            for (_, slots) in &matching_overload_slots[..=upto] {
                for (slot_index, slot) in slots.iter().enumerate() {
                    if participating_slot_indices.contains(&slot_index) {
                        union_parameter_types[slot_index].add_in_place(slot.parameter);
                    }
                }
            }

            let parameter_types = Type::heterogeneous_tuple(
                db,
                union_parameter_types.into_iter().filter_map(|builder| {
                    if builder.is_empty() {
                        None
                    } else {
                        Some(builder.build())
                    }
                }),
            );

            if top_materialized_argument_type.is_assignable_to(db, parameter_types) {
                filter_remaining_overloads = true;
            }
        }

        // Once this filtering process is applied for all arguments, examine the return types of
        // the remaining overloads. If the resulting return types for all remaining overloads are
        // equivalent, proceed to step 6.
        let are_return_types_equivalent_for_all_matching_overloads = {
            let mut matching_overloads = self.matching_overloads();
            if let Some(first_overload_return_type) = matching_overloads
                .next()
                .map(|(_, overload)| overload.return_type())
            {
                matching_overloads.all(|(_, overload)| {
                    overload
                        .return_type()
                        .when_equivalent_to(db, first_overload_return_type, constraints)
                        .is_always_satisfied(db)
                })
            } else {
                // No matching overload
                true
            }
        };

        if !are_return_types_equivalent_for_all_matching_overloads {
            // Overload matching is ambiguous.
            self.overload_call_return_type = Some(OverloadCallReturnType::Ambiguous);
        }
    }

    fn as_result(&self) -> Result<(), CallErrorKind> {
        if !self.is_callable() {
            return Err(CallErrorKind::NotCallable);
        }

        if self.has_binding_errors() {
            return Err(CallErrorKind::BindingError);
        }

        if self.dunder_call_is_possibly_unbound {
            return Err(CallErrorKind::PossiblyNotCallable);
        }

        Ok(())
    }

    pub(crate) fn is_callable(&self) -> bool {
        !self.overloads.is_empty()
    }

    /// Returns the error priority for this binding, used to determine which errors
    /// to show when all intersection elements fail.
    fn error_priority(&self) -> CallErrorPriority {
        if !self.is_callable() {
            return CallErrorPriority::NotCallable;
        }

        // Check if this is a top-callable error
        for overload in &self.overloads {
            for error in &overload.errors {
                if matches!(error, BindingError::CalledTopCallable(_)) {
                    return CallErrorPriority::TopCallable;
                }
            }
        }

        // Any other binding error
        CallErrorPriority::BindingError
    }

    /// Returns whether there were any errors binding this call site.
    ///
    /// This is true if either:
    /// - No overloads matched (all had type/arity errors).
    /// - A matching overload has errors (including semantic errors that don't affect
    ///   overload resolution, like applying `@dataclass` to a `NamedTuple`).
    fn has_binding_errors(&self) -> bool {
        let mut matching_overloads = self.matching_overloads();

        // If there are no matching overloads, we have binding errors.
        let Some((_, first_overload)) = matching_overloads.next() else {
            return true;
        };

        // If any matching overload has semantic errors (that don't affect overload
        // resolution), we have binding errors.
        if !first_overload.errors.is_empty() {
            return true;
        }
        for (_, overload) in matching_overloads {
            if !overload.errors.is_empty() {
                return true;
            }
        }

        false
    }

    /// Returns the index of the matching overload in the form of [`MatchingOverloadIndex`].
    pub(crate) fn matching_overload_index(&self) -> MatchingOverloadIndex {
        let mut matching_overloads = self.matching_overloads();
        match matching_overloads.next() {
            None => MatchingOverloadIndex::None,
            Some((first, _)) => {
                if let Some((second, _)) = matching_overloads.next() {
                    let mut indexes = vec![first, second];
                    for (index, _) in matching_overloads {
                        indexes.push(index);
                    }
                    MatchingOverloadIndex::Multiple(indexes)
                } else {
                    MatchingOverloadIndex::Single(first)
                }
            }
        }
    }

    /// Returns all overloads for this call binding, including overloads that did not match.
    pub(crate) fn overloads(&self) -> &[Binding<'db>] {
        self.overloads.as_slice()
    }

    /// Returns an iterator over all the overloads that matched for this call binding.
    pub(crate) fn matching_overloads(
        &self,
    ) -> impl Iterator<Item = (usize, &Binding<'db>)> + Clone {
        self.overloads
            .iter()
            .enumerate()
            .filter(|(_, overload)| !overload.has_errors_affecting_overload_resolution())
    }

    /// Returns an iterator over all the mutable overloads that matched for this call binding.
    pub(crate) fn matching_overloads_mut(
        &mut self,
    ) -> impl Iterator<Item = (usize, &mut Binding<'db>)> {
        self.overloads
            .iter_mut()
            .enumerate()
            .filter(|(_, overload)| !overload.has_errors_affecting_overload_resolution())
    }

    /// Returns the return type of this call.
    ///
    /// For a valid call, this is the return type of either a successful argument type expansion of
    /// an overloaded function, or the return type of the first overload that the arguments matched
    /// against.
    ///
    /// For an invalid call to a non-overloaded function, this is the return type of the function.
    ///
    /// For an invalid call to an overloaded function, we return `Type::unknown`, since we cannot
    /// make any useful conclusions about which overload was intended to be called.
    pub(crate) fn return_type(&self) -> Type<'db> {
        if let Some(overload_call_return_type) = self.overload_call_return_type {
            return match overload_call_return_type {
                OverloadCallReturnType::ArgumentTypeExpansion(return_type) => return_type,
                OverloadCallReturnType::ArgumentTypeExpansionLimitReached(_)
                | OverloadCallReturnType::Ambiguous => Type::unknown(),
            };
        }
        if let Some((_, first_overload)) = self.matching_overloads().next() {
            return first_overload.return_type();
        }
        if let [overload] = self.overloads.as_slice() {
            return overload.return_type();
        }
        Type::unknown()
    }

    fn report_diagnostics(
        &self,
        context: &InferContext<'db, '_>,
        node: ast::AnyNodeRef,
        compound_diag: Option<&dyn CompoundDiagnostic>,
    ) {
        if !self.is_callable() {
            let range = all_arguments_range(node);
            if let Some(builder) = context.report_lint(&CALL_NON_CALLABLE, range) {
                let mut diag = builder.into_diagnostic(format_args!(
                    "Object of type `{}` is not callable",
                    self.callable_type.display(context.db()),
                ));
                if let Some(compound_diag) = compound_diag {
                    compound_diag.add_context(context.db(), &mut diag);
                }
            }
            return;
        }

        if self.dunder_call_is_possibly_unbound {
            let range = all_arguments_range(node);
            if let Some(builder) = context.report_lint(&CALL_NON_CALLABLE, range) {
                let mut diag = builder.into_diagnostic(format_args!(
                    "Object of type `{}` is not callable (possibly missing `__call__` method)",
                    self.callable_type.display(context.db()),
                ));
                if let Some(compound_diag) = compound_diag {
                    compound_diag.add_context(context.db(), &mut diag);
                }
            }
            return;
        }

        match self.overloads.as_slice() {
            [] => {}
            [overload] => {
                let callable_description =
                    CallableDescription::new(context.db(), self.signature_type);
                overload.report_diagnostics(
                    context,
                    node,
                    self.signature_type,
                    callable_description.as_ref(),
                    compound_diag,
                    None,
                );
            }
            _overloads => {
                // TODO: This should probably be adapted to handle more
                // types of callables[1]. At present, it just handles
                // standard function and method calls.
                //
                // [1]: https://github.com/astral-sh/ty/issues/274#issuecomment-2881856028
                let function_type_and_kind = match self.signature_type {
                    Type::FunctionLiteral(function) => Some((FunctionKind::Function, function)),
                    Type::BoundMethod(bound_method) => Some((
                        FunctionKind::BoundMethod,
                        bound_method.function(context.db()),
                    )),
                    Type::KnownBoundMethod(KnownBoundMethodType::FunctionTypeDunderGet(
                        function,
                    )) => Some((FunctionKind::MethodWrapper, function)),
                    _ => None,
                };

                // If only one overload passed arity check, report its errors directly.
                if let Some(matching_overload_index) = self.matching_overload_before_type_checking {
                    let callable_description =
                        CallableDescription::new(context.db(), self.signature_type);
                    let matching_overload =
                        function_type_and_kind.map(|(kind, function)| MatchingOverloadLiteral {
                            index: matching_overload_index,
                            kind,
                            function,
                        });
                    self.overloads[matching_overload_index].report_diagnostics(
                        context,
                        node,
                        self.signature_type,
                        callable_description.as_ref(),
                        compound_diag,
                        matching_overload.as_ref(),
                    );
                    return;
                }

                // If multiple overloads passed arity check but only one matched types
                // (possibly with semantic errors), report its errors directly instead
                // of the generic "no matching overload" message.
                if let MatchingOverloadIndex::Single(matching_overload_index) =
                    self.matching_overload_index()
                {
                    let callable_description =
                        CallableDescription::new(context.db(), self.signature_type);
                    let matching_overload =
                        function_type_and_kind.map(|(kind, function)| MatchingOverloadLiteral {
                            index: matching_overload_index,
                            kind,
                            function,
                        });
                    self.overloads[matching_overload_index].report_diagnostics(
                        context,
                        node,
                        self.signature_type,
                        callable_description.as_ref(),
                        compound_diag,
                        matching_overload.as_ref(),
                    );
                    return;
                }

                let range = all_arguments_range(node);
                let Some(builder) = context.report_lint(&NO_MATCHING_OVERLOAD, range) else {
                    return;
                };
                let callable_description =
                    CallableDescription::new(context.db(), self.callable_type);
                let mut diag = builder.into_diagnostic(format_args!(
                    "No overload{} matches arguments",
                    callable_description
                        .map(|description| format!(" of {description}"))
                        .unwrap_or_default()
                ));

                if let Some(index) =
                    self.overload_call_return_type
                        .and_then(
                            |overload_call_return_type| match overload_call_return_type {
                                OverloadCallReturnType::ArgumentTypeExpansionLimitReached(
                                    index,
                                ) => Some(index),
                                _ => None,
                            },
                        )
                {
                    diag.info(format_args!(
                        "Limit of argument type expansion reached at argument {index}"
                    ));
                }

                if let Some((kind, function)) = function_type_and_kind {
                    let (overloads, implementation) =
                        function.overloads_and_implementation(context.db());

                    if let Some(overload) = overloads.first() {
                        let mut sub = SubDiagnostic::new(
                            SubDiagnosticSeverity::Info,
                            "First overload defined here",
                        );
                        let file = function.file(context.db());
                        let module = parsed_module(context.db(), file).load(context.db());
                        let node =
                            overload.node(context.db(), function.file(context.db()), &module);
                        let span = if node.body.len() == 1 {
                            Span::from(file).with_range(node.range())
                        } else {
                            overload.spans(context.db()).decorators_and_header
                        };
                        sub.annotate(
                            Annotation::primary(span).message("First overload defined here"),
                        );
                        diag.sub(sub);
                    }

                    diag.info(format_args!(
                        "Possible overloads for {kind} `{}`:",
                        function.name(context.db())
                    ));

                    for overload in overloads.iter().take(MAXIMUM_OVERLOADS) {
                        diag.info(format_args!(
                            "  {}",
                            overload.signature(context.db()).display(context.db())
                        ));
                    }
                    if overloads.len() > MAXIMUM_OVERLOADS {
                        diag.info(format_args!(
                            "... omitted {remaining} overloads",
                            remaining = overloads.len() - MAXIMUM_OVERLOADS
                        ));
                    }

                    if let Some(implementation) = implementation {
                        let mut sub = SubDiagnostic::new(
                            SubDiagnosticSeverity::Info,
                            "Overload implementation defined here",
                        );
                        sub.annotate(Annotation::primary(
                            implementation.spans(context.db()).signature,
                        ));
                        diag.sub(sub);
                    }
                }

                if let Some(compound_diag) = compound_diag {
                    compound_diag.add_context(context.db(), &mut diag);
                }
            }
        }
    }
}

impl<'a, 'db> IntoIterator for &'a CallableBinding<'db> {
    type Item = &'a Binding<'db>;
    type IntoIter = std::slice::Iter<'a, Binding<'db>>;

    fn into_iter(self) -> Self::IntoIter {
        self.overloads.iter()
    }
}

impl<'db> IntoIterator for CallableBinding<'db> {
    type Item = Binding<'db>;
    type IntoIter = smallvec::IntoIter<[Binding<'db>; 1]>;

    fn into_iter(self) -> Self::IntoIter {
        self.overloads.into_iter()
    }
}

#[derive(Debug, Copy, Clone)]
enum OverloadCallReturnType<'db> {
    ArgumentTypeExpansion(Type<'db>),
    ArgumentTypeExpansionLimitReached(usize),
    Ambiguous,
}

#[derive(Debug)]
pub(crate) enum MatchingOverloadIndex {
    /// No matching overloads found.
    None,

    /// Exactly one matching overload found at the given index.
    Single(usize),

    /// Multiple matching overloads found at the given indexes.
    Multiple(Vec<usize>),
}

impl MatchingOverloadIndex {
    pub(crate) fn len(&self) -> usize {
        match self {
            MatchingOverloadIndex::None => 0,
            MatchingOverloadIndex::Single(_) => 1,
            MatchingOverloadIndex::Multiple(indexes) => indexes.len(),
        }
    }
}

#[derive(Default, Debug, Clone)]
struct ArgumentForms {
    values: Vec<Option<ParameterForm>>,
    conflicting: Vec<bool>,
}

impl ArgumentForms {
    /// Create a new argument forms initialized to the given length and the default values.
    fn new(len: usize) -> Self {
        Self {
            values: vec![None; len],
            conflicting: vec![false; len],
        }
    }

    fn merge(&mut self, other: &ArgumentForms) {
        if self.values.len() < other.values.len() {
            self.values.resize(other.values.len(), None);
            self.conflicting.resize(other.conflicting.len(), false);
        }

        for (index, (other_form, other_conflict)) in other
            .values
            .iter()
            .zip(other.conflicting.iter())
            .enumerate()
        {
            if let Some(self_form) = &mut self.values[index] {
                if let Some(other_form) = other_form {
                    if *self_form != *other_form {
                        // Different parameter forms, mark as conflicting
                        self.conflicting[index] = true;
                        *self_form = *other_form; // Use the new form
                    }
                }
            } else {
                self.values[index] = *other_form;
            }

            // Update the conflicting form (true takes precedence)
            self.conflicting[index] |= *other_conflict;
        }
    }

    fn shrink_to_fit(&mut self) {
        self.values.shrink_to_fit();
        self.conflicting.shrink_to_fit();
    }
}

#[derive(Default, Clone, Copy)]
struct ParameterInfo {
    matched: bool,
    suppress_missing_error: bool,
}

struct ArgumentMatcher<'a, 'db> {
    arguments: &'a CallArguments<'a, 'db>,
    parameters: &'a Parameters<'db>,
    argument_forms: &'a mut ArgumentForms,
    errors: &'a mut Vec<BindingError<'db>>,

    argument_matches: Vec<MatchedArgument<'db>>,
    parameter_info: Vec<ParameterInfo>,
    next_positional: usize,
    first_excess_positional: Option<usize>,
    num_synthetic_args: usize,
    variadic_argument_matched_to_variadic_parameter: bool,

    /// Parameter indices that have explicit keyword arguments (e.g., `foo=value`).
    ///
    /// This is used to prevent variadic arguments from greedily matching parameters that will be
    /// explicitly provided via keyword arguments.
    explicit_keyword_parameters: FxHashSet<usize>,
}

impl<'a, 'db> ArgumentMatcher<'a, 'db> {
    fn new(
        arguments: &'a CallArguments<'a, 'db>,
        parameters: &'a Parameters<'db>,
        argument_forms: &'a mut ArgumentForms,
        errors: &'a mut Vec<BindingError<'db>>,
    ) -> Self {
        let explicit_keyword_parameters: FxHashSet<usize> = arguments
            .iter()
            .filter_map(|(argument, _)| {
                if let Argument::Keyword(name) = argument {
                    parameters.keyword_by_name(name).map(|(idx, _)| idx)
                } else {
                    None
                }
            })
            .collect();

        Self {
            arguments,
            parameters,
            argument_forms,
            errors,
            argument_matches: vec![MatchedArgument::default(); arguments.len()],
            parameter_info: vec![ParameterInfo::default(); parameters.len()],
            next_positional: 0,
            first_excess_positional: None,
            num_synthetic_args: 0,
            variadic_argument_matched_to_variadic_parameter: false,
            explicit_keyword_parameters,
        }
    }

    fn has_later_positional_input(&self, argument_index: usize) -> bool {
        self.arguments
            .iter()
            .skip(argument_index + 1)
            .any(|(argument, _)| {
                matches!(
                    argument,
                    Argument::Synthetic | Argument::Positional | Argument::Variadic
                )
            })
    }

    fn get_argument_index(&self, argument_index: usize) -> Option<usize> {
        if argument_index >= self.num_synthetic_args {
            // Adjust the argument index to skip synthetic args, which don't appear at the call
            // site and thus won't be in the Call node arguments list.
            Some(argument_index - self.num_synthetic_args)
        } else {
            // we are erroring on a synthetic argument, we'll just emit the diagnostic on the
            // entire Call node, since there's no argument node for this argument at the call site
            None
        }
    }

    #[expect(clippy::too_many_arguments)]
    fn assign_argument(
        &mut self,
        argument_index: usize,
        argument: Argument<'a>,
        argument_type: Option<Type<'db>>,
        parameter_index: usize,
        parameter: &Parameter<'db>,
        positional: bool,
        variable_argument_length: bool,
    ) {
        if !matches!(argument, Argument::Synthetic) {
            let adjusted_argument_index = argument_index - self.num_synthetic_args;
            if let Some(existing) =
                self.argument_forms.values[adjusted_argument_index].replace(parameter.form)
            {
                if existing != parameter.form {
                    self.argument_forms.conflicting[argument_index - self.num_synthetic_args] =
                        true;
                }
            }
        }
        if self.parameter_info[parameter_index].matched {
            if !parameter.is_variadic() && !parameter.is_keyword_variadic() {
                self.errors.push(BindingError::ParameterAlreadyAssigned {
                    argument_index: self.get_argument_index(argument_index),
                    parameter: ParameterContext::new(parameter, parameter_index, positional),
                });
            }
        }
        if variable_argument_length
            && matches!(
                (argument, parameter.kind()),
                (Argument::Variadic, ParameterKind::Variadic { .. })
                    | (Argument::Keywords, ParameterKind::KeywordVariadic { .. })
            )
        {
            self.variadic_argument_matched_to_variadic_parameter = true;
        }
        let matched_argument = &mut self.argument_matches[argument_index];
        matched_argument.parameters.push(parameter_index);
        matched_argument.types.push(argument_type);
        matched_argument.matched = true;
        self.parameter_info[parameter_index].matched = true;
    }

    fn match_positional(
        &mut self,
        argument_index: usize,
        argument: Argument<'a>,
        argument_type: Option<Type<'db>>,
        variable_argument_length: bool,
    ) -> Result<(), ()> {
        if matches!(argument, Argument::Synthetic) {
            self.num_synthetic_args += 1;
        }
        let Some((parameter_index, parameter)) = self
            .parameters
            .get_positional(self.next_positional)
            .map(|param| (self.next_positional, param))
            .or_else(|| self.parameters.variadic())
        else {
            self.first_excess_positional.get_or_insert(argument_index);
            self.next_positional += 1;
            return Err(());
        };
        self.next_positional += 1;
        self.assign_argument(
            argument_index,
            argument,
            argument_type,
            parameter_index,
            parameter,
            !parameter.is_variadic(),
            variable_argument_length,
        );
        Ok(())
    }

    fn match_keyword(
        &mut self,
        argument_index: usize,
        argument: Argument<'a>,
        argument_type: Option<Type<'db>>,
        name: &str,
    ) -> Result<(), ()> {
        let Some((parameter_index, parameter)) = self
            .parameters
            .keyword_by_name(name)
            .or_else(|| self.parameters.keyword_variadic())
        else {
            if let Some((parameter_index, parameter)) =
                self.parameters.positional_only_by_name(name)
            {
                self.errors
                    .push(BindingError::PositionalOnlyParameterAsKwarg {
                        argument_index: self.get_argument_index(argument_index),
                        parameter: ParameterContext::new(parameter, parameter_index, true),
                    });
                self.parameter_info[parameter_index].suppress_missing_error = true;
            } else {
                self.errors.push(BindingError::UnknownArgument {
                    argument_name: ast::name::Name::new(name),
                    argument_index: self.get_argument_index(argument_index),
                });
            }
            return Err(());
        };
        self.assign_argument(
            argument_index,
            argument,
            argument_type,
            parameter_index,
            parameter,
            false,
            false,
        );
        Ok(())
    }

    /// Match a variadic argument to the remaining positional, standard or variadic parameters.
    fn match_variadic(
        &mut self,
        db: &'db dyn Db,
        argument_index: usize,
        argument: Argument<'a>,
        argument_type: Option<Type<'db>>,
    ) -> Result<(), ()> {
        enum VariadicArgumentType<'db> {
            ParamSpec(Type<'db>),
            /// A union type where each element has been individually iterated into a tuple spec.
            /// We pre-compute the per-position union types, length bounds, and variable element
            /// so the rest of the matching logic can handle unions without special-casing.
            Union {
                argument_types: Vec<Type<'db>>,
                length: TupleLength,
                variable_element: Option<Type<'db>>,
            },
            Other(Cow<'db, TupleSpec<'db>>),
            None,
        }

        let variadic_type = match argument_type {
            Some(argument_type) => match argument_type.as_paramspec_typevar(db) {
                // If the argument is a `ParamSpec` `P.args`, we should not call `iterate` on it.
                // This would lose the `ParamSpec` information and just flatten to `object` from
                // the upper bound. What we want is to always use the `P.args` type to perform type
                // checking against the parameter type. This will allow us to error when `*args:
                // P.args` is matched against, for example, `n: int` and correctly type check when
                // `*args: P.args` is matched against `*args: P.args` (another `ParamSpec`).
                Some(paramspec) => VariadicArgumentType::ParamSpec(paramspec),
                None => match argument_type {
                    // `Type::iterate` unions tuple specs in a way that can invent additional
                    // arities. Iterate each union element individually and compute per-position
                    // union types, length bounds, and variable element so that the rest of the
                    // matching logic handles unions correctly.
                    //
                    // The per-position union loses the correlation between tuple length and the
                    // later element types. `match_variadic` accounts for that by treating
                    // positions beyond the guaranteed minimum as only conditionally present: they
                    // can satisfy optional parameters, but any required positional parameter
                    // beyond the minimum still causes the match to fail provisionally. This is
                    // only sound when no later argument can still contribute more positional
                    // slots; otherwise, a later positional argument could shift left differently
                    // for different union members.
                    Type::Union(union)
                        if self.parameters.variadic().is_none()
                            && !self.has_later_positional_input(argument_index) =>
                    {
                        let tuple_specs: Vec<_> =
                            union.elements(db).iter().map(|ty| ty.iterate(db)).collect();

                        let min_len = tuple_specs
                            .iter()
                            .map(|s| s.len().minimum())
                            .min()
                            .unwrap_or(0);

                        let any_variable = tuple_specs.iter().any(|s| s.len().is_variable());
                        let max_elements = tuple_specs
                            .iter()
                            .map(|s| s.all_elements().len())
                            .max()
                            .unwrap_or(0);

                        let variable_element = {
                            let var_types: Vec<_> = tuple_specs
                                .iter()
                                .filter_map(|s| s.variable_element().copied())
                                .collect();
                            if var_types.is_empty() {
                                None
                            } else {
                                Some(UnionType::from_elements_leave_aliases(db, var_types))
                            }
                        };

                        let max_elements = i32::try_from(max_elements).unwrap_or(i32::MAX);
                        let mut argument_types_vec = Vec::new();
                        for index in 0..max_elements {
                            let positional_types: Vec<_> = tuple_specs
                                .iter()
                                .filter_map(|s| s.py_index(db, index).ok())
                                .collect();
                            if positional_types.is_empty() {
                                break;
                            }
                            argument_types_vec
                                .push(UnionType::from_elements_leave_aliases(db, positional_types));
                        }

                        let length = if any_variable || argument_types_vec.len() > min_len {
                            TupleLength::Variable(min_len, 0)
                        } else {
                            TupleLength::Fixed(min_len)
                        };

                        VariadicArgumentType::Union {
                            argument_types: argument_types_vec,
                            length,
                            variable_element,
                        }
                    }
                    _ => VariadicArgumentType::Other(argument_type.iterate(db)),
                },
            },
            None => VariadicArgumentType::None,
        };

        let (argument_types, length, variable_element) = match &variadic_type {
            VariadicArgumentType::ParamSpec(paramspec) => {
                ([].as_slice(), TupleLength::unknown(), Some(*paramspec))
            }
            VariadicArgumentType::Union {
                argument_types,
                length,
                variable_element,
            } => (argument_types.as_slice(), *length, *variable_element),
            VariadicArgumentType::Other(tuple) => (
                tuple.all_elements(),
                tuple.len(),
                tuple.variable_element().copied(),
            ),
            VariadicArgumentType::None => ([].as_slice(), TupleLength::unknown(), None),
        };

        let mut argument_types = argument_types.iter().copied();
        // This can be true either if we have a true variable-length tuple (in which case
        // `variable_element.is_some()`) or if we have a union of different fixed-length tuples (in
        // which case `variable_element.is_none()`).
        let is_variable = length.is_variable();
        let has_fixed_union_tail = is_variable && variable_element.is_none();

        // We must be able to match up the fixed-length portion of the argument with positional
        // parameters, so we pass on any errors that occur.
        for _ in 0..length.minimum() {
            self.match_positional(
                argument_index,
                argument,
                argument_types.next().or(variable_element),
                is_variable,
            )?;
        }

        // For a union of fixed-length tuples, positions beyond the guaranteed minimum are only
        // present in the longer union members. They therefore cannot satisfy a required
        // positional parameter, because the shorter members would still be missing that argument.
        if has_fixed_union_tail {
            while let Some(parameter) = self.parameters.get_positional(self.next_positional) {
                if self
                    .explicit_keyword_parameters
                    .contains(&self.next_positional)
                {
                    break;
                }
                let Some(argument_type) = argument_types.next() else {
                    break;
                };
                if parameter.default_type().is_none() {
                    return Err(());
                }
                self.match_positional(argument_index, argument, Some(argument_type), is_variable)?;
            }
        // If the tuple is truly variable-length, we assume that it will soak up all remaining
        // positional parameters, stopping only when we reach a parameter that has an explicit
        // keyword argument or a parameter that can only be provided via keyword argument, or if
        // we run out of `argument_types` and have no `variable_element`.
        } else if is_variable {
            while self
                .parameters
                .get_positional(self.next_positional)
                .is_some()
            {
                if self
                    .explicit_keyword_parameters
                    .contains(&self.next_positional)
                {
                    break;
                }
                let arg_type = argument_types.next().or(variable_element);
                if arg_type.is_none() {
                    break;
                }
                self.match_positional(argument_index, argument, arg_type, is_variable)?;
            }
        }

        // A "variable" length with no `variable_element` only comes from a union of different
        // fixed-length tuples. Any remaining `argument_types` are therefore still concrete
        // positions from the longer union members, not an open-ended variadic tail. Feed them back
        // through normal positional matching so we report the same errors as a concrete longer
        // tuple would (`too-many-positional-arguments`, or a later
        // `parameter-already-assigned` when an explicit keyword also targets that parameter)
        // instead of silently dropping those extra positions.
        if has_fixed_union_tail {
            for argument_type in argument_types.by_ref() {
                self.match_positional(argument_index, argument, Some(argument_type), is_variable)?;
            }
        }

        // Finally, if there is a variadic parameter we can match any of the remaining unpacked
        // argument types to it, but only if there is at least one remaining argument type. This is
        // because a variadic parameter is optional, so if this was done unconditionally, ty could
        // raise a false positive as "too many arguments".
        if self.parameters.variadic().is_some() {
            if let Some(argument_type) = argument_types.next().or(variable_element) {
                self.match_positional(argument_index, argument, Some(argument_type), is_variable)?;
                for argument_type in argument_types {
                    self.match_positional(
                        argument_index,
                        argument,
                        Some(argument_type),
                        is_variable,
                    )?;
                }
            }
        }

        Ok(())
    }

    fn match_keyword_variadic(
        &mut self,
        db: &'db dyn Db,
        argument_index: usize,
        argument_type: Option<Type<'db>>,
    ) {
        if let Some(unpacked_keys) =
            argument_type.and_then(|ty| extract_unpacked_typed_dict_keys_from_value_type(db, ty))
        {
            // Special case TypedDict-shaped values because we know which keys are present.
            for (name, unpacked_key) in unpacked_keys {
                let _ = self.match_keyword(
                    argument_index,
                    Argument::Keywords,
                    Some(unpacked_key.value_ty),
                    name.as_str(),
                );
            }
        } else {
            for (parameter_index, parameter) in self.parameters.iter().enumerate() {
                if self.parameter_info[parameter_index].matched && !parameter.is_keyword_variadic()
                {
                    continue;
                }

                if matches!(
                    parameter.kind(),
                    ParameterKind::PositionalOnly { .. } | ParameterKind::Variadic { .. }
                ) {
                    continue;
                }

                let parameter_name = self.parameters[parameter_index]
                    .keyword_name()
                    .map(Name::as_str);

                let value_type = match argument_type {
                    Some(argument_type) => argument_type
                        .as_paramspec_typevar(db)
                        .or_else(|| argument_type.getitem_dunder_call(db, parameter_name))
                        .unwrap_or(Type::unknown()),

                    None => Type::unknown(),
                };

                self.assign_argument(
                    argument_index,
                    Argument::Keywords,
                    Some(value_type),
                    parameter_index,
                    parameter,
                    false,
                    true,
                );
            }
        }
    }

    fn finish(self) -> Box<[MatchedArgument<'db>]> {
        if let Some(first_excess_argument_index) = self.first_excess_positional {
            self.errors.push(BindingError::TooManyPositionalArguments {
                first_excess_argument_index: self.get_argument_index(first_excess_argument_index),
                expected_positional_count: self.parameters.positional().count(),
                provided_positional_count: self.next_positional,
            });
        }

        // For ParamSpec parameters, both *args and **kwargs are required since we don't know
        // what arguments the underlying callable expects. For all other callables, variadic
        // and keyword_variadic parameters are optional.
        let paramspec = self.parameters.as_paramspec();

        let mut missing = vec![];
        for (
            index,
            ParameterInfo {
                matched,
                suppress_missing_error,
            },
        ) in self.parameter_info.iter().copied().enumerate()
        {
            if !matched {
                if suppress_missing_error {
                    continue;
                }
                let param = &self.parameters[index];
                if paramspec.is_none() && (param.is_variadic() || param.is_keyword_variadic())
                    || param.default_type().is_some()
                {
                    // variadic/keywords and defaulted arguments are not required
                    // (unless the parameters represent a ParamSpec)
                    continue;
                }
                missing.push(ParameterContext::new(param, index, false));
            }
        }
        if !missing.is_empty() {
            self.errors.push(BindingError::MissingArguments {
                parameters: ParameterContexts(missing),
                paramspec,
            });
        }

        self.argument_matches.into_boxed_slice()
    }
}

struct ArgumentTypeChecker<'a, 'db> {
    db: &'db dyn Db,
    signature_type: Type<'db>,
    signature: &'a Signature<'db>,
    arguments: &'a CallArguments<'a, 'db>,
    argument_matches: &'a [MatchedArgument<'db>],
    parameter_tys: &'a mut [Option<Type<'db>>],
    parameter_ty_builders: Vec<Option<UnionBuilder<'db>>>,
    call_expression_tcx: TypeContext<'db>,
    return_ty: Type<'db>,
    errors: &'a mut Vec<BindingError<'db>>,

    inferable_typevars: InferableTypeVars<'db>,
    specialization: Option<Specialization<'db>>,

    /// Argument indices for which specialization inference has already produced a sufficiently
    /// precise argument mismatch. We can then silence `check_argument_type` for those arguments to
    /// avoid duplicate diagnostics.
    ///
    /// TODO: Once specialization inference fully owns generic argument validation, this field can
    /// be removed.
    constraint_set_errors: Vec<bool>,
}

/// Result of checking only the key type of a keyword-unpack argument.
enum KeywordUnpackKeyTypeCheck<'db> {
    /// The argument type is handled by a more specific path, or does not expose mapping keys.
    NotApplicable,
    /// The argument exposes mapping keys, and they are assignable to `str`.
    Valid,
    /// The argument exposes mapping keys, but the key type is not assignable to `str`.
    Invalid(Type<'db>),
}

/// Validate the key type of a keyword-unpack argument without checking its value type.
fn validate_keyword_unpack_key_type<'db>(
    db: &'db dyn Db,
    constraints: &ConstraintSetBuilder<'db>,
    argument_type: Type<'db>,
    inferable_typevars: InferableTypeVars<'db>,
) -> KeywordUnpackKeyTypeCheck<'db> {
    if matches!(argument_type, Type::TypedDict(_))
        || argument_type.as_paramspec_typevar(db).is_some()
    {
        return KeywordUnpackKeyTypeCheck::NotApplicable;
    }

    let Some((key_type, _)) = argument_type.unpack_keys_and_items(db) else {
        return KeywordUnpackKeyTypeCheck::NotApplicable;
    };

    if key_type
        .when_assignable_to(
            db,
            KnownClass::Str.to_instance(db),
            constraints,
            inferable_typevars,
        )
        .is_always_satisfied(db)
    {
        KeywordUnpackKeyTypeCheck::Valid
    } else {
        KeywordUnpackKeyTypeCheck::Invalid(key_type)
    }
}

impl<'a, 'db> ArgumentTypeChecker<'a, 'db> {
    #[expect(clippy::too_many_arguments)]
    fn new(
        db: &'db dyn Db,
        signature_type: Type<'db>,
        signature: &'a Signature<'db>,
        arguments: &'a CallArguments<'a, 'db>,
        argument_matches: &'a [MatchedArgument<'db>],
        parameter_tys: &'a mut [Option<Type<'db>>],
        call_expression_tcx: TypeContext<'db>,
        return_ty: Type<'db>,
        errors: &'a mut Vec<BindingError<'db>>,
    ) -> Self {
        Self {
            db,
            signature_type,
            signature,
            arguments,
            argument_matches,
            parameter_tys,
            parameter_ty_builders: Vec::new(),
            call_expression_tcx,
            return_ty,
            errors,
            inferable_typevars: InferableTypeVars::None,
            specialization: None,
            constraint_set_errors: vec![false; arguments.len()],
        }
    }

    fn enumerate_argument_types(
        &self,
    ) -> impl Iterator<Item = (usize, Option<usize>, Argument<'a>, &CallArgumentTypes<'db>)> + 'a
    {
        let mut iter = self.arguments.iter().enumerate();
        let mut num_synthetic_args = 0;
        std::iter::from_fn(move || {
            let (argument_index, (argument, argument_types)) = iter.next()?;
            let adjusted_argument_index = if matches!(argument, Argument::Synthetic) {
                // If we are erroring on a synthetic argument, we'll just emit the
                // diagnostic on the entire Call node, since there's no argument node for
                // this argument at the call site
                num_synthetic_args += 1;
                None
            } else {
                // Adjust the argument index to skip synthetic args, which don't appear at
                // the call site and thus won't be in the Call node arguments list.
                Some(argument_index - num_synthetic_args)
            };
            Some((
                argument_index,
                adjusted_argument_index,
                argument,
                argument_types,
            ))
        })
    }

    fn infer_specialization(&mut self, constraints: &ConstraintSetBuilder<'db>) {
        let Some(generic_context) = self.signature.generic_context else {
            return;
        };

        let return_with_tcx = Some(self.return_ty).zip(self.call_expression_tcx.annotation);

        self.inferable_typevars = generic_context.inferable_typevars(self.db);
        let mut builder = SpecializationBuilder::new(self.db, constraints, self.inferable_typevars);

        // Type variables for which we inferred a declared type based on a partially specialized
        // type from an outer generic context. For these type variables, we may infer types that
        // are not assignable to the concrete subset of the declared type, as they may be assignable
        // to a wider declared type after specialization.
        let mut partially_specialized_declared_type: FxHashSet<BoundTypeVarIdentity<'_>> =
            FxHashSet::default();

        // Attempt to solve the specialization while preferring the declared type of non-covariant
        // type parameters from generic classes, or callable types.
        //
        // We use an assignability check (`return_ty ≤ tcx`) to infer what each typevar in the
        // function's return type maps to in the type context. (We use _constraint set_
        // assignability so that we get a constraint set describing the typevars.) For example, if
        // the return type is `list[T]` and the type context is `list[int]`, the check produces
        // `T = int`, from which we extract the preferred type `int`.
        //
        // TODO: This two-phase approach (extract preferred types from the type context, then check
        // argument compatibility) should eventually be replaced by conjoining the type context
        // constraint set directly with the argument constraint sets in the builder. The current
        // solution-level filtering (variance, inferable typevars, concrete content) works around
        // extracting solutions too early. When the builder maintains a single constraint set, the
        // combined set `(return_ty ≤ tcx) ∧ (∧ᵢ actual_i ≤ formal_i)` will naturally resolve the
        // tension between type context preferences and argument constraints. If the combined set
        // is unsatisfiable, we will fall back to argument constraints alone (which the current
        // code does via `assignable_to_declared_type`).
        let preferred_type_mappings = return_with_tcx
            .and_then(|(return_ty, tcx)| {
                if !tcx
                    .filter_union(self.db, |ty| ty.may_prefer_declared_type(self.db))
                    .may_prefer_declared_type(self.db)
                {
                    return None;
                }

                let return_ty =
                    return_ty.filter_disjoint_elements(self.db, tcx, self.inferable_typevars);
                let tcx = tcx.filter_disjoint_elements(self.db, return_ty, self.inferable_typevars);
                let path_bounds = return_ty.assignable_solutions_with_inferable(
                    self.db,
                    tcx,
                    self.inferable_typevars,
                );

                // Use `solutions_with` to determine per-typevar variance from the raw
                // lower/upper bounds on each BDD path.
                let mut variance_map: FxHashMap<BoundTypeVarIdentity<'_>, TypeVarVariance> =
                    FxHashMap::default();
                let solutions = path_bounds.solve_with(|typevar, variance, lower, upper| {
                    let identity = typevar.identity(self.db);
                    variance_map
                        .entry(identity)
                        .and_modify(|current| *current = current.join(variance))
                        .or_insert(variance);
                    PathBounds::default_solve(self.db, constraints, typevar, lower, upper)
                });

                let Solutions::Constrained(solutions) = solutions else {
                    return None;
                };

                let mut preferred: FxHashMap<BoundTypeVarIdentity<'db>, UnionAccumulator<'db>> =
                    FxHashMap::default();

                for solution in &solutions {
                    for binding in solution {
                        let identity = binding.bound_typevar.identity(self.db);

                        // Avoid unnecessarily widening the return type based on a covariant
                        // type parameter from the type context, as it can lead to argument
                        // assignability errors if the type variable is constrained by a narrower
                        // parameter type.
                        if variance_map
                            .get(&identity)
                            .is_some_and(|v| v.is_covariant())
                        {
                            continue;
                        }

                        // Filter out inferable typevars (cross-typevar references from
                        // SequentMap transitivity) and unspecialized typevars (from partially
                        // specialized contexts).
                        let inferred_ty = binding.solution.filter_union(self.db, |ty| {
                            if ty.has_unspecialized_type_var(self.db) {
                                partially_specialized_declared_type.insert(identity);
                                return false;
                            }
                            true
                        });
                        if inferred_ty.has_unspecialized_type_var(self.db) {
                            continue;
                        }

                        // Skip preferred types where every non-TypeVar union element still
                        // deeply contains non-inferable typevars. Such types (e.g.,
                        // `T@h | list[T@h]` from an outer generic scope) don't provide
                        // useful concrete information and would cause over-expansion.
                        let concrete_content =
                            inferred_ty.filter_union(self.db, |ty| !ty.has_typevar(self.db));
                        if concrete_content.is_never() && inferred_ty.has_typevar(self.db) {
                            continue;
                        }

                        preferred
                            .entry(identity)
                            .and_modify(|existing| existing.add(self.db, inferred_ty))
                            .or_insert_with(|| UnionAccumulator::new(inferred_ty));
                    }
                }

                let preferred: FxHashMap<BoundTypeVarIdentity<'db>, Type<'db>> = preferred
                    .into_iter()
                    .map(|(identity, accumulator)| (identity, accumulator.into_type(self.db)))
                    .collect();

                // Add preferred types to the builder so they serve as the base mapping
                // when argument inference adds more types.
                for solution in &solutions {
                    for binding in solution {
                        let identity = binding.bound_typevar.identity(self.db);
                        if let Some(&ty) = preferred.get(&identity) {
                            builder.insert_type_mapping(binding.bound_typevar, ty);
                        }
                    }
                }

                Some(preferred)
            })
            .unwrap_or_default();

        let mut specialization_errors = Vec::new();
        let assignable_to_declared_type = self.infer_argument_constraints(
            &mut builder,
            &preferred_type_mappings,
            &partially_specialized_declared_type,
            &mut specialization_errors,
        );

        // If we failed to prefer the declared type, attempt inference again, ignoring
        // the declared type.
        //
        // Note that this will still lead to an invalid specialization, but may
        // produce more precise diagnostics.
        if !assignable_to_declared_type {
            builder = SpecializationBuilder::new(self.db, constraints, self.inferable_typevars);
            specialization_errors.clear();

            self.infer_argument_constraints(
                &mut builder,
                &FxHashMap::default(),
                &FxHashSet::default(),
                &mut specialization_errors,
            );
        }

        self.errors.extend(specialization_errors);

        // Attempt to promote any promotable types assigned to the specialization.
        // The hook receives (typevar, lower_bound, upper_bound) and returns Some(ty) to
        // override the default solution, or None to keep it.
        let maybe_promote = |typevar: BoundTypeVarInstance<'db>,
                             bounds: Option<(Type<'db>, Type<'db>)>| {
            let (lower, _upper) = bounds?;
            let bound_or_constraints = typevar.typevar(self.db).bound_or_constraints(self.db);

            // For constrained TypeVars, the inferred type is already one of the
            // constraints. Promoting literals would produce a type that doesn't
            // match any constraint.
            if matches!(
                bound_or_constraints,
                Some(TypeVarBoundOrConstraints::Constraints(_))
            ) {
                return None;
            }

            let mut variance_in_return = TypeVarVariance::Bivariant;

            // Find all occurrences of the type variable in the return type.
            self.return_ty
                .visit_specialization(self.db, |ty, variance| {
                    if ty != Type::TypeVar(typevar) {
                        return;
                    }

                    variance_in_return = variance_in_return.join(variance);
                });

            // Promotion is only useful if the type variable is in non-covariant position
            // in the return type.
            if variance_in_return.is_covariant() {
                return None;
            }

            let promoted = lower.promote(self.db);

            // If the TypeVar has an upper bound, only use the promoted type if it
            // still satisfies the bound.
            if let Some(TypeVarBoundOrConstraints::UpperBound(bound)) = bound_or_constraints {
                if !promoted.is_assignable_to(self.db, bound) {
                    return None;
                }
            }

            Some(promoted)
        };

        let specialization = builder.build_with(generic_context, maybe_promote);

        self.return_ty = self.return_ty.apply_specialization(self.db, specialization);
        self.specialization = Some(specialization);
    }

    fn infer_argument_constraints<'c>(
        &mut self,
        builder: &mut SpecializationBuilder<'db, 'c>,
        preferred_type_mappings: &FxHashMap<BoundTypeVarIdentity<'db>, Type<'db>>,
        partially_specialized_declared_type: &FxHashSet<BoundTypeVarIdentity<'_>>,
        specialization_errors: &mut Vec<BindingError<'db>>,
    ) -> bool {
        let mut assignable_to_declared_type = true;

        let parameters = self.signature.parameters();
        for (argument_index, adjusted_argument_index, _, argument_types) in
            self.enumerate_argument_types()
        {
            for (parameter_index, variadic_argument_type) in
                self.argument_matches[argument_index].iter()
            {
                if self.is_gradual_variadic_parameter(parameter_index) {
                    continue;
                }

                let declared_type = parameters[parameter_index].annotated_type();
                let argument_type = argument_types.get_for_declared_type(declared_type);

                let specialization_result = builder.infer_map(
                    declared_type,
                    variadic_argument_type.unwrap_or(argument_type),
                    |(identity, _, inferred_ty)| {
                        // Avoid widening the inferred type if it is already assignable to the
                        // preferred declared type.
                        if let Some(preferred_ty) = preferred_type_mappings.get(&identity) {
                            if inferred_ty.is_assignable_to(self.db, *preferred_ty) {
                                return None;
                            }

                            // If this is a partially specialized type, the type we infer may still
                            // be assignable to it once fully specialized.
                            if !partially_specialized_declared_type.contains(&identity) {
                                assignable_to_declared_type = false;
                            }
                        }

                        Some(inferred_ty)
                    },
                );

                if let Err(error) = specialization_result {
                    specialization_errors.push(BindingError::SpecializationError {
                        error,
                        argument_index: adjusted_argument_index,
                    });
                }
            }
        }

        assignable_to_declared_type
    }

    fn check_argument_type(
        &mut self,
        constraints: &ConstraintSetBuilder<'db>,
        argument_index: usize,
        adjusted_argument_index: Option<usize>,
        argument: Argument<'a>,
        mut argument_type: Type<'db>,
        parameter_index: usize,
    ) {
        let parameters = self.signature.parameters();
        let parameter = &parameters[parameter_index];
        if self.is_gradual_variadic_parameter(parameter_index) {
            return;
        }

        let mut expected_ty = parameter.annotated_type();
        if let Some(specialization) = self.specialization {
            argument_type = argument_type.apply_specialization(self.db, specialization);
            expected_ty = expected_ty.apply_specialization(self.db, specialization);
        }
        // This is one of the few places where we want to check if there's _any_ specialization
        // where assignability holds; normally we want to check that assignability holds for
        // _all_ specializations.
        //
        // Note that we silence diagnostics here if we already got a SpecializationError from the
        // new constraint set solver for this argument. The constraint-set solver is the authority
        // for these parameters, and this assignability check would re-detect the same
        // incompatibility against a less-informative fallback specialization.
        //
        // TODO: Soon we will go further, and build the actual specializations from the
        // constraint set that we get from this assignability check, instead of inferring and
        // building them in an earlier separate step.
        //
        // TODO: handle starred annotations, e.g. `*args: *Ts` or `*args: *tuple[int, *tuple[str, ...]]`
        if !self.constraint_set_errors[argument_index]
            && !parameter.has_starred_annotation()
            && argument_type
                .when_assignable_to(self.db, expected_ty, constraints, self.inferable_typevars)
                .is_never_satisfied(self.db)
        {
            let positional = matches!(argument, Argument::Positional | Argument::Synthetic)
                && !parameter.is_variadic();
            self.errors.push(BindingError::InvalidArgumentType {
                parameter: ParameterContext::new(parameter, parameter_index, positional),
                argument_index: adjusted_argument_index,
                expected_ty,
                provided_ty: argument_type,
            });
        }
        // We still update the actual type of the parameter in this binding to match the argument,
        // even if the argument type is not assignable to the expected parameter type.
        //
        // For a broad variadic sink such as `*args: object` or `**kwargs: object`, preserving a
        // union of every individual argument type does not improve call checking or special-case
        // return inference. Keep the already-declared top type instead of repeatedly growing a
        // large union.
        if (parameter.is_variadic() || parameter.is_keyword_variadic()) && expected_ty.is_object() {
            self.parameter_tys[parameter_index].get_or_insert(expected_ty);
            return;
        }

        if let Some(builder) = self
            .parameter_ty_builders
            .get_mut(parameter_index)
            .and_then(Option::as_mut)
        {
            builder.add_in_place(argument_type);
        } else if let Some(existing) = self.parameter_tys[parameter_index] {
            let mut builder = UnionBuilder::new(self.db);
            builder.add_in_place(existing);
            builder.add_in_place(argument_type);
            if self.parameter_ty_builders.is_empty() {
                self.parameter_ty_builders = std::iter::repeat_with(|| None)
                    .take(self.parameter_tys.len())
                    .collect();
            }
            self.parameter_ty_builders[parameter_index] = Some(builder);
        } else {
            self.parameter_tys[parameter_index] = Some(argument_type);
        }
    }

    fn is_gradual_variadic_parameter(&self, parameter_index: usize) -> bool {
        let parameters = self.signature.parameters();
        let parameter = &parameters[parameter_index];

        matches!(parameters.kind(), ParametersKind::Gradual)
            && matches!(parameter.annotated_type(), Type::Dynamic(_))
            && (parameter.is_variadic() || parameter.is_keyword_variadic())
    }

    fn check_argument_types(&mut self, constraints: &ConstraintSetBuilder<'db>) {
        let paramspec = self.signature.parameters().as_paramspec_with_prefix();

        for (argument_index, adjusted_argument_index, argument, argument_types) in
            self.enumerate_argument_types()
        {
            if let Some((_, paramspec)) = paramspec {
                if self.try_paramspec_evaluation_at(constraints, argument_index, paramspec) {
                    // Once we find an argument that matches the `ParamSpec`, we can stop checking
                    // the remaining arguments since `ParamSpec` should always be the last
                    // parameter.
                    return;
                }
            }

            match argument {
                Argument::Variadic => self.check_variadic_argument_type(
                    constraints,
                    argument_index,
                    adjusted_argument_index,
                    argument,
                ),
                Argument::Keywords => self.check_keyword_variadic_argument_type(
                    constraints,
                    argument_index,
                    adjusted_argument_index,
                    argument,
                    // Splatted arguments are inferred without type context.
                    argument_types.get_default().unwrap_or(Type::unknown()),
                ),
                _ => {
                    // If the argument isn't splatted, just check its type directly.
                    for parameter_index in &self.argument_matches[argument_index].parameters {
                        let declared_type =
                            self.signature.parameters()[*parameter_index].annotated_type();
                        let argument_type = argument_types.get_for_declared_type(declared_type);

                        self.check_argument_type(
                            constraints,
                            argument_index,
                            adjusted_argument_index,
                            argument,
                            argument_type,
                            *parameter_index,
                        );
                    }
                }
            }
        }

        if let Some((_, paramspec)) = paramspec {
            // If we reach here, none of the arguments matched the `ParamSpec` parameter, but the
            // `ParamSpec` could specialize to a parameter list containing some parameters. For
            // example,
            //
            // ```py
            // from typing import Callable
            //
            // def foo[**P](f: Callable[P, None], *args: P.args, **kwargs: P.kwargs) -> None: ...
            //
            // def f(x: int) -> None: ...
            //
            // foo(f)
            // ```
            //
            // Here, no arguments match the `ParamSpec` parameter, but `P` specializes to `(x: int)`,
            // so we need to perform a sub-call with no arguments.
            self.evaluate_paramspec_sub_call(constraints, None, paramspec);
        }
    }

    /// Try to evaluate a `ParamSpec` sub-call at the given argument index.
    ///
    /// The `ParamSpec` parameter is always going to be at the end of the parameter list but there
    /// can be other parameter before it. If one of these prepended positional parameters contains
    /// a free `ParamSpec`, we consider that variable in scope for the purposes of extracting the
    /// components of that `ParamSpec`. For example:
    ///
    /// ```py
    /// from typing import Callable
    ///
    /// def foo[**P](f: Callable[P, None], *args: P.args, **kwargs: P.kwargs) -> None: ...
    ///
    /// def f(x: int, y: str) -> None: ...
    ///
    /// foo(f, 1, "hello")  # P: (x: int, y: str)
    /// ```
    ///
    /// Here, `P` specializes to `(x: int, y: str)` when `foo` is called with `f`, which means that
    /// the parameters of `f` become a part of `foo`'s parameter list replacing the `ParamSpec`
    /// parameter which is:
    ///
    /// ```py
    /// def foo(f: Callable[[x: int, y: str], None], x: int, y: str) -> None: ...
    /// ```
    ///
    /// This method will check whether the parameter matching the argument at `argument_index` is
    /// annotated with the components of `ParamSpec`, and if so, will invoke a sub-call considering
    /// the arguments starting from `argument_index` against the specialized parameter list.
    ///
    /// Returns `true` if the sub-call was invoked, `false` otherwise.
    fn try_paramspec_evaluation_at(
        &mut self,
        constraints: &ConstraintSetBuilder<'db>,
        argument_index: usize,
        paramspec: BoundTypeVarInstance<'db>,
    ) -> bool {
        let [parameter_index] = self.argument_matches[argument_index].parameters.as_slice() else {
            return false;
        };

        let Type::TypeVar(typevar) = self.signature.parameters()[*parameter_index].annotated_type()
        else {
            return false;
        };
        if !typevar.is_paramspec(self.db) {
            return false;
        }

        self.evaluate_paramspec_sub_call(constraints, Some(argument_index), paramspec)
    }

    /// Invoke a sub-call for the given `ParamSpec` type variable, using the remaining arguments.
    ///
    /// The remaining arguments start from `argument_index` if provided, otherwise no arguments
    /// are passed.
    ///
    /// This method returns `false` if the specialization does not contain a mapping for the given
    /// `paramspec` or contains an invalid mapping (i.e., not a `Callable` of kind `ParamSpecValue`).
    ///
    /// For more details, refer to [`Self::try_paramspec_evaluation_at`].
    fn evaluate_paramspec_sub_call(
        &mut self,
        constraints: &ConstraintSetBuilder<'db>,
        argument_index: Option<usize>,
        paramspec: BoundTypeVarInstance<'db>,
    ) -> bool {
        let Some(Type::Callable(callable)) = self
            .specialization
            .and_then(|specialization| specialization.get(self.db, paramspec))
        else {
            return false;
        };

        if callable.kind(self.db) != CallableTypeKind::ParamSpecValue {
            return false;
        }

        let signatures = &callable.signatures(self.db).overloads;
        if signatures.is_empty() {
            return false;
        }

        let (sub_arguments, error_offset) = if let Some(argument_index) = argument_index {
            let num_synthetic_args = self
                .arguments
                .iter()
                .filter(|(arg, _)| matches!(arg, Argument::Synthetic))
                .count();

            (
                self.arguments.start_from(argument_index),
                Some(argument_index - num_synthetic_args),
            )
        } else {
            (CallArguments::none(), None)
        };

        // Create Bindings with all overloads and perform full overload resolution
        let callable_binding =
            CallableBinding::from_overloads(self.signature_type, signatures.iter().cloned());
        let bindings = match Bindings::from(callable_binding)
            .match_parameters(self.db, &sub_arguments)
            .check_types(
                self.db,
                constraints,
                &sub_arguments,
                self.call_expression_tcx,
                &[],
            ) {
            Ok(bindings) => bindings,
            Err(CallError(_, bindings)) => *bindings,
        };

        // SAFETY: `bindings` was created from a single `CallableBinding` above.
        let callable_binding = bindings
            .single_element()
            .expect("ParamSpec sub-call should only contain a single CallableBinding");

        match callable_binding.matching_overload_index() {
            MatchingOverloadIndex::None => {
                if let [binding] = callable_binding.overloads() {
                    // This is not an overloaded function, so we can propagate its errors to the
                    // outer bindings.
                    self.errors.extend(
                        binding
                            .errors
                            .iter()
                            .cloned()
                            .map(|err| err.maybe_apply_argument_index_offset(error_offset)),
                    );
                } else {
                    let index = callable_binding
                        .best_failing_overload_index(
                            FailingOverloadSelection::AffectsOverloadResolution,
                        )
                        .unwrap_or(0);
                    // TODO: We should also update the specialization for the `ParamSpec` to reflect
                    // the matching overload here.
                    self.errors.extend(
                        callable_binding.overloads()[index]
                            .errors
                            .iter()
                            .cloned()
                            .map(|err| err.maybe_apply_argument_index_offset(error_offset)),
                    );
                }
            }
            MatchingOverloadIndex::Single(index) => {
                // TODO: We should also update the specialization for the `ParamSpec` to reflect the
                // matching overload here.
                self.errors.extend(
                    callable_binding.overloads()[index]
                        .errors
                        .iter()
                        .cloned()
                        .map(|err| err.maybe_apply_argument_index_offset(error_offset)),
                );
            }
            MatchingOverloadIndex::Multiple(_) => {
                if !matches!(
                    callable_binding.overload_call_return_type,
                    Some(OverloadCallReturnType::ArgumentTypeExpansion(_))
                ) {
                    self.errors.extend(
                        callable_binding
                            .overloads()
                            .first()
                            .unwrap()
                            .errors
                            .iter()
                            .cloned()
                            .map(|err| err.maybe_apply_argument_index_offset(error_offset)),
                    );
                }
            }
        }

        true
    }

    fn check_variadic_argument_type(
        &mut self,
        constraints: &ConstraintSetBuilder<'db>,
        argument_index: usize,
        adjusted_argument_index: Option<usize>,
        argument: Argument<'a>,
    ) {
        for (parameter_index, variadic_argument_type) in
            self.argument_matches[argument_index].iter()
        {
            self.check_argument_type(
                constraints,
                argument_index,
                adjusted_argument_index,
                argument,
                variadic_argument_type.unwrap_or_else(Type::unknown),
                parameter_index,
            );
        }
    }

    fn check_keyword_variadic_argument_type(
        &mut self,
        constraints: &ConstraintSetBuilder<'db>,
        argument_index: usize,
        adjusted_argument_index: Option<usize>,
        argument: Argument<'a>,
        argument_type: Type<'db>,
    ) {
        if let Some(unpacked_keys) =
            extract_unpacked_typed_dict_keys_from_value_type(self.db, argument_type)
        {
            for (argument_type, parameter_index) in unpacked_keys
                .values()
                .map(|unpacked_key| unpacked_key.value_ty)
                .zip(&self.argument_matches[argument_index].parameters)
            {
                self.check_argument_type(
                    constraints,
                    argument_index,
                    adjusted_argument_index,
                    argument,
                    argument_type,
                    *parameter_index,
                );
            }

            return;
        }

        let value_type_paramspec =
            if let Some(paramspec) = argument_type.as_paramspec_typevar(self.db) {
                Some(paramspec)
            } else {
                match validate_keyword_unpack_key_type(
                    self.db,
                    constraints,
                    argument_type,
                    self.inferable_typevars,
                ) {
                    KeywordUnpackKeyTypeCheck::NotApplicable => return,
                    KeywordUnpackKeyTypeCheck::Valid => {}
                    KeywordUnpackKeyTypeCheck::Invalid(provided_ty) => {
                        self.errors.push(BindingError::InvalidKeyType {
                            argument_index: adjusted_argument_index,
                            provided_ty,
                        });
                    }
                }

                None
            };

        for parameter_index in &self.argument_matches[argument_index].parameters {
            let value_type = if let Some(value_type) = value_type_paramspec {
                value_type
            } else {
                let parameter_name = self.signature.parameters()[*parameter_index]
                    .keyword_name()
                    .map(Name::as_str);

                argument_type
                    .getitem_dunder_call(self.db, parameter_name)
                    .unwrap_or(Type::unknown())
            };

            self.check_argument_type(
                constraints,
                argument_index,
                adjusted_argument_index,
                Argument::Keywords,
                value_type,
                *parameter_index,
            );
        }
    }

    fn finish(
        self,
    ) -> (
        InferableTypeVars<'db>,
        Option<Specialization<'db>>,
        Type<'db>,
    ) {
        for (parameter_ty, builder) in self
            .parameter_tys
            .iter_mut()
            .zip(self.parameter_ty_builders)
        {
            if let Some(builder) = builder {
                *parameter_ty = Some(builder.build());
            }
        }

        (self.inferable_typevars, self.specialization, self.return_ty)
    }
}

/// Information about which parameter(s) an argument was matched against. This is tracked
/// separately for each overload.
#[derive(Clone, Debug, Default)]
pub struct MatchedArgument<'db> {
    /// The index of the parameter(s) that an argument was matched against. A splatted argument
    /// might be matched against multiple parameters.
    pub parameters: SmallVec<[usize; 1]>,

    /// Whether there were errors matching this argument. For a splatted argument, _all_ splatted
    /// elements must have been successfully matched. (That means that this can be `false` while
    /// the `parameters` field is non-empty.)
    pub matched: bool,

    /// The types of a variadic argument when it's unpacked.
    ///
    /// The length of this vector is always the same as the `parameters` vector i.e., these are the
    /// types assigned to each matched parameter. This isn't necessarily the same as the number of
    /// types in the argument type which might not be a fixed-length iterable.
    ///
    /// Another thing to note is that the way this is populated means that for any other argument
    /// kind (synthetic, positional, keyword, keyword-variadic), this will be a single-element
    /// vector containing `None`, since we don't know the type of the argument when this is
    /// constructed. So, this field is populated only for variadic arguments.
    ///
    /// For example, given a `*args` whose type is `tuple[A, B, C]` and the following parameters:
    /// - `(x, *args)`: the `types` field will only have two elements (`B`, `C`) since `A` has been
    ///   matched with `x`.
    /// - `(*args)`: the `types` field will have all the three elements (`A`, `B`, `C`)
    types: SmallVec<[Option<Type<'db>>; 1]>,
}

impl<'db> MatchedArgument<'db> {
    /// Returns an iterator over the parameter indices and the corresponding argument type.
    pub fn iter(&self) -> impl Iterator<Item = (usize, Option<Type<'db>>)> + '_ {
        self.parameters
            .iter()
            .copied()
            .zip(self.types.iter().copied())
    }
}

/// Indicates that a parameter of the given name was not found.
#[derive(Debug, Clone, Copy)]
pub(crate) struct UnknownParameterNameError;

/// Binding information for one of the overloads of a callable.
#[derive(Debug, Clone)]
pub(crate) struct Binding<'db> {
    pub(crate) signature: Signature<'db>,

    /// The type that is (hopefully) callable.
    pub(crate) callable_type: Type<'db>,

    /// The type we'll use for error messages referring to details of the called signature. For
    /// calls to functions this will be the same as `callable_type`; for other callable instances
    /// it may be a `__call__` method.
    pub(crate) signature_type: Type<'db>,

    /// Return type of the call.
    pub(crate) return_ty: Type<'db>,

    /// Constructor metadata used to normalize the declared return type before type checking.
    constructor_context: Option<ConstructorContext<'db>>,

    /// The inferable typevars in this signature.
    inferable_typevars: InferableTypeVars<'db>,

    /// The specialization that was inferred from the argument types, if the callable is generic.
    specialization: Option<Specialization<'db>>,

    /// Information about which parameter(s) each argument was matched with, in argument source
    /// order.
    argument_matches: Box<[MatchedArgument<'db>]>,

    /// Whether an argument that supplies an indeterminate number of positional or keyword
    /// arguments is mapped to a variadic parameter (`*args` or `**kwargs`).
    variadic_argument_matched_to_variadic_parameter: bool,

    /// Bound types for parameters, in parameter source order, or `None` if no argument was matched
    /// to that parameter.
    parameter_tys: Box<[Option<Type<'db>>]>,

    /// Call binding errors, if any.
    errors: Vec<BindingError<'db>>,
}

impl<'db> Binding<'db> {
    pub(crate) fn single(signature_type: Type<'db>, signature: Signature<'db>) -> Binding<'db> {
        let return_ty = signature.return_ty;
        Binding {
            signature,
            callable_type: signature_type,
            signature_type,
            return_ty,
            constructor_context: None,
            inferable_typevars: InferableTypeVars::None,
            specialization: None,
            argument_matches: Box::from([]),
            variadic_argument_matched_to_variadic_parameter: false,
            parameter_tys: Box::from([]),
            errors: vec![],
        }
    }

    fn replace_callable_type(&mut self, before: Type<'db>, after: Type<'db>) {
        if self.callable_type == before {
            self.callable_type = after;
        }
    }

    fn match_parameters(
        &mut self,
        db: &'db dyn Db,
        arguments: &CallArguments<'_, 'db>,
        argument_forms: &mut ArgumentForms,
    ) {
        let parameters = self.signature.parameters();
        let mut matcher =
            ArgumentMatcher::new(arguments, parameters, argument_forms, &mut self.errors);
        let mut keywords_arguments = vec![];
        for (argument_index, (argument, argument_types)) in arguments.iter().enumerate() {
            match argument {
                Argument::Positional | Argument::Synthetic => {
                    let _ = matcher.match_positional(argument_index, argument, None, false);
                }
                Argument::Keyword(name) => {
                    let _ = matcher.match_keyword(argument_index, argument, None, name);
                }
                Argument::Variadic => {
                    let _ = matcher.match_variadic(
                        db,
                        argument_index,
                        argument,
                        // Splatted arguments are inferred without type context.
                        argument_types.get_default(),
                    );
                }
                Argument::Keywords => {
                    keywords_arguments.push((argument_index, argument_types));
                }
            }
        }
        for (keywords_index, keywords_type) in keywords_arguments {
            matcher.match_keyword_variadic(
                db,
                keywords_index,
                // Splatted arguments are inferred without type context.
                keywords_type.get_default(),
            );
        }
        self.parameter_tys = vec![None; parameters.len()].into_boxed_slice();
        self.variadic_argument_matched_to_variadic_parameter =
            matcher.variadic_argument_matched_to_variadic_parameter;
        self.argument_matches = matcher.finish();
    }

    fn check_types(
        &mut self,
        db: &'db dyn Db,
        constraints: &ConstraintSetBuilder<'db>,
        arguments: &CallArguments<'_, 'db>,
        call_expression_tcx: TypeContext<'db>,
    ) {
        let parameters = self.signature.parameters();

        if parameters.is_top() {
            self.errors
                .push(BindingError::CalledTopCallable(self.signature_type));
            return;
        }

        if matches!(parameters.kind(), ParametersKind::Gradual)
            && parameters
                .as_slice()
                .iter()
                .all(|parameter| parameter.is_variadic() || parameter.is_keyword_variadic())
        {
            self.check_keyword_unpack_key_types(db, constraints, arguments);
            return;
        }

        let mut checker = ArgumentTypeChecker::new(
            db,
            self.signature_type,
            &self.signature,
            arguments,
            &self.argument_matches,
            &mut self.parameter_tys,
            call_expression_tcx,
            self.return_ty,
            &mut self.errors,
        );

        // If this overload is generic, first see if we can infer a specialization of the function
        // from the arguments that were passed in.
        checker.infer_specialization(constraints);
        checker.check_argument_types(constraints);

        (self.inferable_typevars, self.specialization, self.return_ty) = checker.finish();
    }

    fn check_keyword_unpack_key_types(
        &mut self,
        db: &'db dyn Db,
        constraints: &ConstraintSetBuilder<'db>,
        arguments: &CallArguments<'_, 'db>,
    ) {
        let mut num_synthetic_args = 0;

        for (argument_index, (argument, argument_types)) in arguments.iter().enumerate() {
            let adjusted_argument_index = if matches!(argument, Argument::Synthetic) {
                num_synthetic_args += 1;
                None
            } else {
                Some(argument_index - num_synthetic_args)
            };

            if !matches!(argument, Argument::Keywords) {
                continue;
            }

            let argument_type = argument_types.get_default().unwrap_or(Type::unknown());
            if let KeywordUnpackKeyTypeCheck::Invalid(provided_ty) =
                validate_keyword_unpack_key_type(
                    db,
                    constraints,
                    argument_type,
                    InferableTypeVars::None,
                )
            {
                self.errors.push(BindingError::InvalidKeyType {
                    argument_index: adjusted_argument_index,
                    provided_ty,
                });
            }
        }
    }

    pub(crate) fn set_return_type(&mut self, return_ty: Type<'db>) {
        self.return_ty = return_ty;
    }

    pub(crate) fn return_type(&self) -> Type<'db> {
        self.return_ty
    }

    /// Returns the bound types for each parameter, in parameter source order, or `None` if no
    /// argument was matched to that parameter.
    pub(crate) fn parameter_types(&self) -> &[Option<Type<'db>>] {
        &self.parameter_tys
    }

    /// Returns the reduced callable type exposed by this `functools.partial(...)` overload.
    fn functools_partial_return_type<'a>(
        &mut self,
        db: &'db dyn Db,
        call_arguments: &CallArguments<'a, 'db>,
    ) -> Option<Type<'db>> {
        // `partial(...)` receives the wrapped callable as its first explicit argument (after
        // constructor receiver handling).
        let func_ty = match self.parameter_types() {
            [Some(func_ty), ..] => *func_ty,
            _ => return None,
        };
        let fallback_return_type =
            KnownClass::FunctoolsPartial.to_specialized_instance(db, &[Type::unknown()]);

        let (bound_call_arguments, partial_bindings) =
            Bindings::functools_partial_matched_bindings(db, func_ty, call_arguments)?;

        // Reuse call-binding machinery to resolve which wrapped overloads are compatible with
        // bound arguments and to surface binding diagnostics.
        let partial_bindings = match partial_bindings.check_types(
            db,
            &ConstraintSetBuilder::new(),
            &bound_call_arguments,
            TypeContext::default(),
            &[],
        ) {
            Ok(bindings) => bindings,
            Err(CallError(_, bindings)) => *bindings,
        };
        let new_return_type =
            partial_bindings.functools_partial_type(db, func_ty, self, &bound_call_arguments);

        Some(if new_return_type.is_never() {
            fallback_return_type
        } else {
            new_return_type
        })
    }

    /// `functools.partial(...)` is allowed to leave required parameters unbound.
    fn clear_missing_argument_errors_for_partial_application(&mut self) {
        self.errors
            .retain(|error| !matches!(error, BindingError::MissingArguments { .. }));
    }

    /// Downstream constructor validation is deferred until after partial signatures are merged.
    fn clear_deferred_constructor_errors_for_partial_application(&mut self) {
        self.errors.retain(|error| {
            !matches!(
                error,
                BindingError::MissingArguments { .. }
                    | BindingError::UnknownArgument { .. }
                    | BindingError::PositionalOnlyParameterAsKwarg { .. }
                    | BindingError::TooManyPositionalArguments { .. }
                    | BindingError::ParameterAlreadyAssigned { .. }
            )
        });
    }

    /// Collects the parameter-level effects of a `functools.partial(...)` application.
    fn partial_application(&self, arguments: &CallArguments<'_, 'db>) -> PartialApplication<'db> {
        let parameters = self.signature.parameters().as_slice();
        let mut partial_application = PartialApplication::new(parameters.len());

        for ((argument, argument_ty), argument_matches) in
            arguments.iter().zip(&self.argument_matches)
        {
            match argument {
                Argument::Positional | Argument::Synthetic | Argument::Variadic => {
                    for (parameter_index, _) in argument_matches.iter() {
                        let parameter = &parameters[parameter_index];
                        if parameter.is_positional()
                            && parameter.annotated_type() != Type::Never
                            && !parameter.is_variadic()
                            && !parameter.is_keyword_variadic()
                        {
                            partial_application.bind_positionally(parameter_index);
                        }
                    }
                }
                Argument::Keyword(_) | Argument::Keywords => {
                    for (parameter_index, matched_ty) in argument_matches.iter() {
                        if partial_application.is_positionally_bound(parameter_index) {
                            continue;
                        }

                        let parameter = &parameters[parameter_index];
                        if parameter.is_positional_only()
                            || parameter.is_variadic()
                            || parameter.is_keyword_variadic()
                        {
                            continue;
                        }

                        partial_application.bind_by_keyword(
                            parameter_index,
                            (parameter.annotated_type() != Type::Never).then(|| {
                                matched_ty.unwrap_or_else(|| {
                                    argument_ty.get_default().unwrap_or_else(Type::unknown)
                                })
                            }),
                        );
                    }
                }
            }
        }

        partial_application
    }

    /// Packages the information needed to synthesize this overload's reduced partial signature.
    fn partial_signature_application(
        &self,
        arguments: &CallArguments<'_, 'db>,
        db: &'db dyn Db,
    ) -> PartialSignatureApplication<'db> {
        PartialSignatureApplication::new(
            self.signature.clone(),
            self.partial_application(arguments),
            self.specialization,
            self.unspecialized_return_type(db),
        )
    }

    /// Returns the bound type for the specified parameter, or `None` if no argument was matched to
    /// that parameter.
    ///
    /// Returns an error if the parameter name is not found.
    pub(crate) fn parameter_type_by_name(
        &self,
        parameter_name: &str,
        fallback_to_default: bool,
    ) -> Result<Option<Type<'db>>, UnknownParameterNameError> {
        let parameters = self.signature.parameters();

        let index = parameters
            .keyword_by_name(parameter_name)
            .map(|(i, _)| i)
            .ok_or(UnknownParameterNameError)?;

        let parameter_ty = self.parameter_tys[index];

        if parameter_ty.is_some() {
            Ok(parameter_ty)
        } else if fallback_to_default {
            Ok(parameters[index].default_type())
        } else {
            Ok(None)
        }
    }

    pub(crate) fn arguments_for_parameter<'a>(
        &'a self,
        call_arguments: &'a CallArguments<'a, 'db>,
        parameter_index: usize,
    ) -> impl Iterator<Item = (Argument<'a>, Type<'db>)> + 'a {
        call_arguments
            .iter()
            .zip(&self.argument_matches)
            .filter(move |(_, argument_matches)| {
                argument_matches.parameters.contains(&parameter_index)
            })
            .map(move |((argument, argument_types), _)| {
                let declared_type = self.signature.parameters()[parameter_index].annotated_type();
                (
                    argument,
                    argument_types.get_for_declared_type(declared_type),
                )
            })
    }

    /// Mark this overload binding as an unmatched overload.
    fn mark_as_unmatched_overload(&mut self) {
        self.errors.push(BindingError::UnmatchedOverload);
    }

    fn report_diagnostics(
        &self,
        context: &InferContext<'db, '_>,
        node: ast::AnyNodeRef,
        callable_ty: Type<'db>,
        callable_description: Option<&CallableDescription>,
        compound_diag: Option<&dyn CompoundDiagnostic>,
        matching_overload: Option<&MatchingOverloadLiteral<'db>>,
    ) {
        for error in &self.errors {
            error.report_diagnostic(
                context,
                node,
                callable_ty,
                callable_description,
                compound_diag,
                matching_overload,
            );
        }
    }

    fn has_errors_affecting_overload_resolution(&self) -> bool {
        self.errors
            .iter()
            .any(BindingError::affects_overload_resolution)
    }

    fn snapshot(&self) -> BindingSnapshot<'db> {
        BindingSnapshot {
            return_ty: self.return_ty,
            inferable_typevars: self.inferable_typevars,
            specialization: self.specialization,
            argument_matches: self.argument_matches.clone(),
            parameter_tys: self.parameter_tys.clone(),
            errors: self.errors.clone(),
        }
    }

    fn restore(&mut self, snapshot: BindingSnapshot<'db>) {
        let BindingSnapshot {
            return_ty,
            inferable_typevars,
            specialization,
            argument_matches,
            parameter_tys,
            errors,
        } = snapshot;

        self.return_ty = return_ty;
        self.inferable_typevars = inferable_typevars;
        self.specialization = specialization;
        self.argument_matches = argument_matches;
        self.parameter_tys = parameter_tys;
        self.errors = errors;
    }

    /// Returns a vector where each index corresponds to an argument position,
    /// and the value is the parameter index that argument maps to (if any).
    pub(crate) fn argument_matches(&self) -> &[MatchedArgument<'db>] {
        &self.argument_matches
    }

    pub(crate) fn specialization(&self) -> Option<Specialization<'db>> {
        self.specialization
    }

    pub(crate) fn errors(&self) -> &[BindingError<'db>] {
        &self.errors
    }

    /// Resets the state of this binding to its initial state.
    fn reset(&mut self, db: &'db dyn Db) {
        self.return_ty = self.initial_return_type(db);
        self.inferable_typevars = InferableTypeVars::None;
        self.specialization = None;
        self.argument_matches = Box::from([]);
        self.parameter_tys = Box::from([]);
        self.errors.clear();
    }
}

#[derive(Clone, Debug)]
struct BindingSnapshot<'db> {
    return_ty: Type<'db>,
    inferable_typevars: InferableTypeVars<'db>,
    specialization: Option<Specialization<'db>>,
    argument_matches: Box<[MatchedArgument<'db>]>,
    parameter_tys: Box<[Option<Type<'db>>]>,
    errors: Vec<BindingError<'db>>,
}

#[derive(Clone, Debug)]
struct CallableBindingSnapshot<'db> {
    overload_return_type: Option<OverloadCallReturnType<'db>>,

    /// Represents the snapshot of the matched overload bindings.
    ///
    /// Usually this contains only the overloads that survived the initial arity check, to avoid
    /// duplicating errors when merging snapshots after a successful evaluation of all expanded
    /// argument lists. For provisional arity retries on expandable `*args`, however, it can also
    /// include overloads that were filtered out in step 1 so those overloads can be reconsidered
    /// against concrete expanded argument lists.
    matching_overloads: Vec<(usize, BindingSnapshot<'db>)>,
}

impl<'db> CallableBindingSnapshot<'db> {
    /// Update the state of the matched overload bindings in this snapshot with the current
    /// state in the given `binding`.
    fn update(&mut self, binding: &CallableBinding<'db>) {
        // Here, the `snapshot` is the state of this binding for the previous argument list and
        // `binding` would contain the state after evaluating the current argument list.
        for (snapshot, binding) in self
            .matching_overloads
            .iter_mut()
            .map(|(index, snapshot)| (snapshot, &binding.overloads[*index]))
        {
            if binding.errors.is_empty() {
                // If the binding has no errors, this means that the current argument list was
                // evaluated successfully and this is the matching overload.
                //
                // Clear the errors from the snapshot of this overload to signal this change ...
                snapshot.errors.clear();

                // ... and update the snapshot with the current state of the binding.
                snapshot.return_ty = binding.return_ty;
                snapshot.inferable_typevars = binding.inferable_typevars;
                snapshot.specialization = binding.specialization;
                snapshot
                    .argument_matches
                    .clone_from(&binding.argument_matches);
                snapshot.parameter_tys.clone_from(&binding.parameter_tys);
            }

            // If the errors in the snapshot was empty, then this binding is the matching overload
            // for a previously evaluated argument list. This means that we don't need to change
            // any information for an already matched overload binding.
            //
            // If it does have errors, we could extend it with the errors from evaluating the
            // current argument list. Arguably, this isn't required, since the errors in the
            // snapshot should already signal that this is an unmatched overload which is why we
            // don't do it. Similarly, due to this being an unmatched overload, there's no point in
            // updating the binding state.
        }
    }
}

/// A helper to take snapshots of the matched overload bindings for the current state of the
/// bindings.
struct CallableBindingSnapshotter(Vec<usize>);

impl CallableBindingSnapshotter {
    /// Creates a new snapshotter for the given indexes of the matched overloads.
    fn new(indexes: Vec<usize>) -> Self {
        debug_assert!(indexes.len() > 1);
        CallableBindingSnapshotter(indexes)
    }

    /// Takes a snapshot of the current state of the matched overload bindings.
    ///
    /// # Panics
    ///
    /// Panics if the indexes of the matched overloads are not valid for the given binding.
    fn take<'db>(&self, binding: &CallableBinding<'db>) -> CallableBindingSnapshot<'db> {
        CallableBindingSnapshot {
            overload_return_type: binding.overload_call_return_type,
            matching_overloads: self
                .0
                .iter()
                .map(|index| (*index, binding.overloads[*index].snapshot()))
                .collect(),
        }
    }

    /// Restores the state of the matched overload bindings from the given snapshot.
    fn restore<'db>(
        &self,
        binding: &mut CallableBinding<'db>,
        snapshot: CallableBindingSnapshot<'db>,
    ) {
        debug_assert_eq!(self.0.len(), snapshot.matching_overloads.len());
        binding.overload_call_return_type = snapshot.overload_return_type;
        for (index, snapshot) in snapshot.matching_overloads {
            binding.overloads[index].restore(snapshot);
        }
    }
}

/// Describes a callable for the purposes of diagnostics.
#[derive(Debug)]
pub(crate) struct CallableDescription<'a> {
    pub(crate) name: Cow<'a, str>,
    pub(crate) kind: Option<&'static str>,
}

impl<'db> CallableDescription<'db> {
    pub(crate) fn new(
        db: &'db dyn Db,
        callable_type: Type<'db>,
    ) -> Option<CallableDescription<'db>> {
        fn qualified_function_name<'db>(
            db: &'db dyn Db,
            function: FunctionType<'db>,
        ) -> Cow<'db, str> {
            let file = function.file(db);
            let semantic_index = semantic_index(db, file);
            let enclosing_scope = semantic_index.scope(function.definition(db).file_scope(db));
            match enclosing_scope.node() {
                NodeWithScopeKind::Class(class) => Cow::Owned(format!(
                    "{}.{}",
                    class.node(&parsed_module(db, file).load(db)).name,
                    function.name(db)
                )),
                _ => Cow::Borrowed(function.name(db)),
            }
        }

        match callable_type {
            Type::FunctionLiteral(function) => Some(CallableDescription {
                kind: Some(if function.name(db) == "__new__" {
                    "constructor"
                } else {
                    "function"
                }),
                name: qualified_function_name(db, function),
            }),
            Type::ClassLiteral(class_type) => Some(CallableDescription {
                kind: Some("class"),
                name: Cow::Borrowed(class_type.name(db)),
            }),
            Type::BoundMethod(bound_method) => Some({
                let function = bound_method.function(db);
                let kind = if function.name(db) == "__init__" {
                    None
                } else {
                    Some("bound method")
                };
                CallableDescription {
                    kind,
                    name: qualified_function_name(db, function),
                }
            }),
            Type::KnownBoundMethod(KnownBoundMethodType::FunctionTypeDunderGet(function)) => {
                Some(CallableDescription {
                    kind: Some("method wrapper `__get__` of function"),
                    name: Cow::Borrowed(function.name(db)),
                })
            }
            Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderGet(_)) => {
                Some(CallableDescription {
                    kind: Some("method wrapper"),
                    name: Cow::Borrowed("`__get__` of property"),
                })
            }
            Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderDelete(_)) => {
                Some(CallableDescription {
                    kind: Some("method wrapper"),
                    name: Cow::Borrowed("`__delete__` of property"),
                })
            }
            Type::WrapperDescriptor(kind) => Some(CallableDescription {
                kind: Some("wrapper descriptor"),
                name: Cow::Borrowed(match kind {
                    WrapperDescriptorKind::FunctionTypeDunderGet => "FunctionType.__get__",
                    WrapperDescriptorKind::PropertyDunderGet => "property.__get__",
                    WrapperDescriptorKind::PropertyDunderSet => "property.__set__",
                    WrapperDescriptorKind::PropertyDunderDelete => "property.__delete__",
                }),
            }),
            _ => None,
        }
    }
}

impl std::fmt::Display for CallableDescription<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(kind) = self.kind {
            write!(f, "{kind} `{}`", self.name)
        } else {
            write!(f, "`{}`", self.name)
        }
    }
}

/// Information needed to emit a diagnostic regarding a parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParameterContext {
    name: Option<ast::name::Name>,
    index: usize,

    /// Was the argument for this parameter passed positionally, and matched to a non-variadic
    /// positional parameter? (If so, we will provide the index in the diagnostic, not just the
    /// name.)
    positional: bool,
}

impl ParameterContext {
    fn new(parameter: &Parameter, index: usize, positional: bool) -> Self {
        Self {
            name: parameter.display_name(),
            index,
            positional,
        }
    }
}

impl std::fmt::Display for ParameterContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(name) = &self.name {
            if self.positional {
                write!(f, "{} (`{name}`)", self.index + 1)
            } else {
                write!(f, "`{name}`")
            }
        } else {
            write!(f, "{}", self.index + 1)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParameterContexts(Vec<ParameterContext>);

impl std::fmt::Display for ParameterContexts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut iter = self.0.iter();
        if let Some(first) = iter.next() {
            write!(f, "{first}")?;
            for param in iter {
                f.write_str(", ")?;
                write!(f, "{param}")?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BindingError<'db> {
    /// The type of an argument is not assignable to the annotated type of its corresponding
    /// parameter.
    InvalidArgumentType {
        parameter: ParameterContext,
        argument_index: Option<usize>,
        expected_ty: Type<'db>,
        provided_ty: Type<'db>,
    },
    /// The type of the keyword-variadic argument's key is not `str`.
    InvalidKeyType {
        argument_index: Option<usize>,
        provided_ty: Type<'db>,
    },
    /// One or more required parameters (that is, with no default) is not supplied by any argument.
    MissingArguments {
        parameters: ParameterContexts,
        /// If the missing arguments are for a `ParamSpec`, this contains the `ParamSpec` typevar.
        /// This is used to provide more informative error messages explaining why `*args` and
        /// `**kwargs` are required.
        paramspec: Option<BoundTypeVarInstance<'db>>,
    },
    /// A call argument can't be matched to any parameter.
    UnknownArgument {
        argument_name: ast::name::Name,
        argument_index: Option<usize>,
    },
    /// A positional-only parameter is passed as keyword argument.
    PositionalOnlyParameterAsKwarg {
        argument_index: Option<usize>,
        parameter: ParameterContext,
    },
    /// More positional arguments are provided in the call than can be handled by the signature.
    TooManyPositionalArguments {
        first_excess_argument_index: Option<usize>,
        expected_positional_count: usize,
        provided_positional_count: usize,
    },
    /// Multiple arguments were provided for a single parameter.
    ParameterAlreadyAssigned {
        argument_index: Option<usize>,
        parameter: ParameterContext,
    },
    /// An inferred specialization was invalid.
    SpecializationError {
        error: SpecializationError<'db>,
        argument_index: Option<usize>,
    },
    PropertyHasNoSetter(PropertyInstanceType<'db>),
    PropertyHasNoDeleter(PropertyInstanceType<'db>),
    /// The call itself might be well constructed, but an error occurred while evaluating the call.
    /// We use this variant to report errors in `property.__get__` and `property.__set__`, which
    /// can occur when the call to the underlying getter/setter fails.
    InternalCallError(&'static str),
    /// This overload binding of the callable does not match the arguments.
    // TODO: We could expand this with an enum to specify why the overload is unmatched.
    UnmatchedOverload,
    /// The callable type is a top materialization (e.g., `Top[Callable[..., object]]`), which
    /// represents the infinite union of all callables. While such types *are* callable (they pass
    /// `callable()`), any specific call should fail because we don't know the actual signature.
    CalledTopCallable(Type<'db>),
    /// The `@dataclass` decorator was applied to an invalid target.
    InvalidDataclassApplication(InvalidDataclassTarget),
}

impl BindingError<'_> {
    /// Returns whether this error is relevant to `functools.partial(...)` construction.
    ///
    /// These errors are used both to filter incompatible wrapped overloads and to report
    /// statically-detectable call-shape errors at construction time. (Runtime `functools.partial`
    /// can defer some call-shape errors until invocation.)
    ///
    /// For example, `partial(f, 1)` should ignore `MissingArguments` for the parameters that stay
    /// unbound, while `partial(f, "x")` should still report `InvalidArgumentType` immediately.
    fn is_relevant_for_partial_application(&self) -> bool {
        matches!(
            self,
            Self::InvalidArgumentType { .. }
                | Self::InvalidKeyType { .. }
                | Self::UnknownArgument { .. }
                | Self::PositionalOnlyParameterAsKwarg { .. }
                | Self::TooManyPositionalArguments { .. }
                | Self::ParameterAlreadyAssigned { .. }
                | Self::SpecializationError { .. }
        )
    }

    pub(crate) fn maybe_apply_argument_index_offset(mut self, offset: Option<usize>) -> Self {
        if let Some(offset) = offset {
            self.apply_argument_index_offset(offset);
        }
        self
    }

    /// Applies the given offset to the argument indices in this error, if any.
    ///
    /// This is mainly used to adjust error argument indices for errors that were generated in a
    /// sub-call for a `ParamSpec`, where the argument indices are relative to the sub-call's
    /// argument list rather than the original call's argument list. The `offset` should be the
    /// number of arguments in the original call that were matched before the `ParamSpec` component.
    pub(crate) fn apply_argument_index_offset(&mut self, offset: usize) {
        match self {
            BindingError::InvalidArgumentType { argument_index, .. }
            | BindingError::InvalidKeyType { argument_index, .. }
            | BindingError::UnknownArgument { argument_index, .. }
            | BindingError::PositionalOnlyParameterAsKwarg { argument_index, .. }
            | BindingError::ParameterAlreadyAssigned { argument_index, .. }
            | BindingError::SpecializationError { argument_index, .. } => {
                if let Some(argument_index) = argument_index {
                    *argument_index += offset;
                }
            }

            BindingError::TooManyPositionalArguments {
                first_excess_argument_index,
                ..
            } => {
                if let Some(first_excess_argument_index) = first_excess_argument_index {
                    *first_excess_argument_index += offset;
                }
            }

            BindingError::CalledTopCallable(..)
            | BindingError::InternalCallError(..)
            | BindingError::InvalidDataclassApplication(..)
            | BindingError::MissingArguments { .. }
            | BindingError::UnmatchedOverload
            | BindingError::PropertyHasNoSetter(..)
            | BindingError::PropertyHasNoDeleter(..) => {}
        }
    }
}

/// The target of an invalid `@dataclass` application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InvalidDataclassTarget {
    NamedTuple,
    TypedDict,
    Enum,
    Protocol,
}

/// Returns the invalid dataclass target for a class literal, if any.
fn invalid_dataclass_target<'db>(
    db: &'db dyn Db,
    class_literal: &ClassLiteral<'db>,
) -> Option<InvalidDataclassTarget> {
    if matches!(class_literal, ClassLiteral::DynamicNamedTuple(_))
        || class_literal
            .as_static()
            .is_some_and(|class| class.has_named_tuple_class_in_mro(db))
    {
        Some(InvalidDataclassTarget::NamedTuple)
    } else if class_literal.is_typed_dict(db) {
        Some(InvalidDataclassTarget::TypedDict)
    } else if is_enum_class(db, Type::from(*class_literal)) {
        Some(InvalidDataclassTarget::Enum)
    } else if class_literal.is_protocol(db) {
        Some(InvalidDataclassTarget::Protocol)
    } else {
        None
    }
}

impl<'db> BindingError<'db> {
    /// Returns `true` if this error indicates the overload didn't match the call arguments.
    ///
    /// Returns `false` for semantic errors where the overload matched the types but the
    /// usage is invalid for other reasons (e.g., applying `@dataclass` to a `NamedTuple`).
    /// These semantic errors should be reported directly rather than causing "no matching
    /// overload" errors.
    fn affects_overload_resolution(&self) -> bool {
        match self {
            // Semantic errors: the overload matched, but the usage is invalid
            Self::InvalidDataclassApplication(_)
            | Self::PropertyHasNoSetter(_)
            | Self::PropertyHasNoDeleter(_)
            | Self::CalledTopCallable(_)
            | Self::InternalCallError(_) => false,

            // Matching errors: the overload doesn't apply to these arguments
            Self::InvalidArgumentType { .. }
            | Self::InvalidKeyType { .. }
            | Self::MissingArguments { .. }
            | Self::UnknownArgument { .. }
            | Self::PositionalOnlyParameterAsKwarg { .. }
            | Self::TooManyPositionalArguments { .. }
            | Self::ParameterAlreadyAssigned { .. }
            | Self::SpecializationError { .. }
            | Self::UnmatchedOverload => true,
        }
    }

    fn report_diagnostic(
        &self,
        context: &InferContext<'db, '_>,
        node: ast::AnyNodeRef,
        callable_ty: Type<'db>,
        callable_description: Option<&CallableDescription>,
        compound_diag: Option<&dyn CompoundDiagnostic>,
        matching_overload: Option<&MatchingOverloadLiteral<'_>>,
    ) {
        let callable_kind = match callable_ty {
            Type::FunctionLiteral(_) => "Function",
            Type::BoundMethod(_) => "Method",
            _ => "Callable",
        };

        match self {
            Self::InvalidArgumentType {
                parameter,
                argument_index,
                expected_ty,
                provided_ty,
            } => {
                // Certain special forms in the typing module are aliases for classes
                // elsewhere in the standard library. These special forms are not instances of `type`,
                // and you cannot use them in place of their aliased classes in *all* situations:
                // for example, `dict()` succeeds at runtime, but `typing.Dict()` fails. However,
                // they *can* all be used as the second argument to `isinstance` and `issubclass`.
                // We model that specific aspect of their behaviour here.
                //
                // This is implemented as a special case in call-binding machinery because overriding
                // typeshed's signatures for `isinstance()` and `issubclass()` would be complex and
                // error-prone, due to the fact that they are annotated with recursive type aliases.
                if parameter.index == 1
                    && *argument_index == Some(1)
                    && matches!(
                        callable_ty
                            .as_function_literal()
                            .and_then(|function| function.known(context.db())),
                        Some(KnownFunction::IsInstance | KnownFunction::IsSubclass)
                    )
                    && provided_ty
                        .as_special_form()
                        .is_some_and(SpecialFormType::is_valid_isinstance_target)
                {
                    return;
                }

                // TODO: Ideally we would not emit diagnostics for `TypedDict` literal arguments
                // here (see `diagnostic::is_invalid_typed_dict_literal`). However, we may have
                // silenced diagnostics during overload evaluation, and rely on the assignability
                // diagnostic being emitted here.

                let range = Self::get_node(node, *argument_index);
                let Some(builder) = context.report_lint(&INVALID_ARGUMENT_TYPE, range) else {
                    return;
                };

                let display_settings = DisplaySettings::from_possibly_ambiguous_types(
                    context.db(),
                    [provided_ty, expected_ty],
                );
                let provided_ty_display =
                    provided_ty.display_with(context.db(), display_settings.clone());
                let expected_ty_display = expected_ty.display_with(context.db(), display_settings);

                let mut diag = builder.into_diagnostic(format_args!(
                    "Argument{} is incorrect",
                    callable_description
                        .map(|description| format!(" to {description}"))
                        .unwrap_or_default()
                ));
                diag.set_primary_message(format_args!(
                    "Expected `{expected_ty_display}`, found `{provided_ty_display}`"
                ));

                let error_context =
                    provided_ty.assignability_error_context(context.db(), *expected_ty);
                error_context.attach_to(context.db(), &mut diag);

                if let Some(matching_overload) = matching_overload {
                    if let Some(overload_literal) = matching_overload.get(context.db()) {
                        let mut sub = SubDiagnostic::new(
                            SubDiagnosticSeverity::Info,
                            "Matching overload defined here",
                        );
                        let (name_span, parameter_span) =
                            overload_literal.parameter_span(context.db(), Some(parameter.index));
                        sub.annotate(Annotation::primary(name_span));
                        sub.annotate(
                            Annotation::secondary(parameter_span)
                                .message("Parameter declared here"),
                        );
                        diag.sub(sub);
                        diag.info(format_args!(
                            "Non-matching overloads for {} `{}`:",
                            matching_overload.kind,
                            matching_overload.function.name(context.db())
                        ));
                        let (overloads, _) = matching_overload
                            .function
                            .overloads_and_implementation(context.db());
                        for (overload_index, overload) in
                            overloads.iter().enumerate().take(MAXIMUM_OVERLOADS)
                        {
                            if overload_index == matching_overload.index {
                                continue;
                            }
                            diag.info(format_args!(
                                "  {}",
                                overload.signature(context.db()).display(context.db())
                            ));
                        }
                        if overloads.len() > MAXIMUM_OVERLOADS {
                            diag.info(format_args!(
                                "... omitted {remaining} overloads",
                                remaining = overloads.len() - MAXIMUM_OVERLOADS
                            ));
                        }
                    }
                } else if let Some((name_span, parameter_span)) =
                    callable_ty.parameter_span(context.db(), Some(parameter.index))
                {
                    let mut sub = SubDiagnostic::new(
                        SubDiagnosticSeverity::Info,
                        format_args!("{callable_kind} defined here"),
                    );
                    sub.annotate(Annotation::primary(name_span));
                    sub.annotate(
                        Annotation::secondary(parameter_span).message("Parameter declared here"),
                    );
                    diag.sub(sub);
                }

                if let Some(compound_diag) = compound_diag {
                    compound_diag.add_context(context.db(), &mut diag);
                }

                // If the type comes from first-party code, the user may have some control over
                // the parameter annotation; provide additional context to help them fix it.
                if callable_ty
                    .definition(context.db())
                    .and_then(|definition| definition.file(context.db()))
                    .is_some_and(|file| context.db().should_check_file(file))
                {
                    note_numbers_module_not_supported(
                        context.db(),
                        &mut diag,
                        *expected_ty,
                        *provided_ty,
                    );
                }

                add_invariant_generic_hints(context.db(), &mut diag, *expected_ty, *provided_ty);
            }

            Self::InvalidKeyType {
                argument_index,
                provided_ty,
            } => {
                let range = Self::get_node(node, *argument_index);
                let Some(builder) = context.report_lint(&INVALID_ARGUMENT_TYPE, range) else {
                    return;
                };

                let provided_ty_display = provided_ty.display(context.db());
                let mut diag = builder.into_diagnostic(
                    "Argument expression after ** must be a mapping with `str` key type",
                );
                diag.set_primary_message(format_args!("Found `{provided_ty_display}`"));

                if let Some(compound_diag) = compound_diag {
                    compound_diag.add_context(context.db(), &mut diag);
                }
            }

            Self::TooManyPositionalArguments {
                first_excess_argument_index,
                expected_positional_count,
                provided_positional_count,
            } => {
                let node = Self::get_node(node, *first_excess_argument_index);
                if let Some(builder) = context.report_lint(&TOO_MANY_POSITIONAL_ARGUMENTS, node) {
                    let mut diag = builder.into_diagnostic(format_args!(
                        "Too many positional arguments{}: expected \
                        {expected_positional_count}, got {provided_positional_count}",
                        callable_description
                            .map(|description| format!(" to {description}"))
                            .unwrap_or_default()
                    ));
                    if let Some(compound_diag) = compound_diag {
                        compound_diag.add_context(context.db(), &mut diag);
                    } else if let Some(spans) = callable_ty.function_spans(context.db()) {
                        let mut sub = SubDiagnostic::new(
                            SubDiagnosticSeverity::Info,
                            format_args!("{callable_kind} signature here"),
                        );
                        sub.annotate(Annotation::primary(spans.signature));
                        diag.sub(sub);
                    }
                }
            }

            Self::MissingArguments {
                parameters,
                paramspec,
            } => {
                let range = all_arguments_range(node);
                if let Some(builder) = context.report_lint(&MISSING_ARGUMENT, range) {
                    let s = if parameters.0.len() == 1 { "" } else { "s" };
                    let mut diag = builder.into_diagnostic(format_args!(
                        "No argument{s} provided for required parameter{s} {parameters}{}",
                        callable_description
                            .map(|description| format!(" of {description}"))
                            .unwrap_or_default()
                    ));
                    if let Some(compound_diag) = compound_diag {
                        compound_diag.add_context(context.db(), &mut diag);
                    } else {
                        let span = callable_ty.parameter_span(
                            context.db(),
                            (parameters.0.len() == 1).then(|| parameters.0[0].index),
                        );
                        if let Some((_, parameter_span)) = span {
                            let mut sub = SubDiagnostic::new(
                                SubDiagnosticSeverity::Info,
                                format_args!("Parameter{s} declared here"),
                            );
                            sub.annotate(Annotation::primary(parameter_span));
                            diag.sub(sub);
                        }
                    }
                    if let Some(paramspec) = paramspec {
                        let paramspec_name = paramspec.name(context.db());
                        diag.sub(SubDiagnostic::new(
                            SubDiagnosticSeverity::Info,
                            format_args!(
                                "These arguments are required because `ParamSpec` `{paramspec_name}` \
                                 could represent any set of parameters at runtime"
                            ),
                        ));
                    }
                }
            }

            Self::UnknownArgument {
                argument_name,
                argument_index,
            } => {
                let node = Self::get_node(node, *argument_index);
                if let Some(builder) = context.report_lint(&UNKNOWN_ARGUMENT, node) {
                    let mut diag = builder.into_diagnostic(format_args!(
                        "Argument `{argument_name}` does not match any known parameter{}",
                        callable_description
                            .map(|description| format!(" of {description}"))
                            .unwrap_or_default()
                    ));
                    if let Some(compound_diag) = compound_diag {
                        compound_diag.add_context(context.db(), &mut diag);
                    } else if let Some(spans) = callable_ty.function_spans(context.db()) {
                        let mut sub = SubDiagnostic::new(
                            SubDiagnosticSeverity::Info,
                            format_args!("{callable_kind} signature here"),
                        );
                        sub.annotate(Annotation::primary(spans.signature));
                        diag.sub(sub);
                    }
                }
            }

            Self::PositionalOnlyParameterAsKwarg {
                argument_index,
                parameter,
            } => {
                let node = Self::get_node(node, *argument_index);
                if let Some(builder) =
                    context.report_lint(&POSITIONAL_ONLY_PARAMETER_AS_KWARG, node)
                {
                    let mut diag = builder.into_diagnostic(format_args!(
                        "Positional-only parameter {parameter} passed as keyword argument{}",
                        callable_description
                            .map(|description| format!(" of {description}"))
                            .unwrap_or_default()
                    ));
                    if let Some(compound_diag) = compound_diag {
                        compound_diag.add_context(context.db(), &mut diag);
                    } else if let Some(spans) = callable_ty.function_spans(context.db()) {
                        let mut sub = SubDiagnostic::new(
                            SubDiagnosticSeverity::Info,
                            format_args!("{callable_kind} signature here"),
                        );
                        sub.annotate(Annotation::primary(spans.signature));
                        diag.sub(sub);
                    }
                }
            }

            Self::ParameterAlreadyAssigned {
                argument_index,
                parameter,
            } => {
                let node = Self::get_node(node, *argument_index);
                if let Some(builder) = context.report_lint(&PARAMETER_ALREADY_ASSIGNED, node) {
                    let mut diag = builder.into_diagnostic(format_args!(
                        "Multiple values provided for parameter {parameter}{}",
                        callable_description
                            .map(|description| format!(" of {description}"))
                            .unwrap_or_default()
                    ));
                    if let Some(compound_diag) = compound_diag {
                        compound_diag.add_context(context.db(), &mut diag);
                    }
                }
            }

            Self::SpecializationError {
                error,
                argument_index,
            } => {
                let range = Self::get_node(node, *argument_index);
                let Some(builder) = context.report_lint(&INVALID_ARGUMENT_TYPE, range) else {
                    return;
                };

                let argument_type = error.argument_type();
                let argument_ty_display = argument_type.display(context.db());

                let mut diag = builder.into_diagnostic(format_args!(
                    "Argument{} is incorrect",
                    callable_description
                        .map(|description| format!(" to {description}"))
                        .unwrap_or_default()
                ));

                match error {
                    SpecializationError::MismatchedBound { bound_typevar, .. } => {
                        let typevar = bound_typevar.typevar(context.db());
                        let typevar_name = typevar.name(context.db());
                        diag.set_primary_message(format_args!(
                            "Argument type `{argument_ty_display}` does not \
                                satisfy upper bound `{}` of type variable `{typevar_name}`",
                            typevar
                                .upper_bound(context.db())
                                .expect(
                                    "type variable should have an upper bound if this error occurs"
                                )
                                .display(context.db())
                        ));
                    }
                    SpecializationError::MismatchedConstraint { bound_typevar, .. } => {
                        let typevar = bound_typevar.typevar(context.db());
                        let typevar_name = typevar.name(context.db());
                        diag.set_primary_message(format_args!(
                            "Argument type `{argument_ty_display}` does not \
                                satisfy constraints ({}) of type variable `{typevar_name}`",
                            typevar
                                .constraints(context.db())
                                .expect(
                                    "type variable should have constraints if this error occurs"
                                )
                                .iter()
                                .format_with(", ", |ty, f| f(&format_args!(
                                    "`{}`",
                                    ty.display(context.db())
                                )))
                        ));
                    }
                }

                if let Some(typevar_definition) = error
                    .bound_typevar()
                    .typevar(context.db())
                    .definition(context.db())
                {
                    let module = parsed_module(context.db(), typevar_definition.file(context.db()))
                        .load(context.db());
                    let typevar_range = typevar_definition.full_range(context.db(), &module);
                    let mut sub = SubDiagnostic::new(
                        SubDiagnosticSeverity::Info,
                        "Type variable defined here",
                    );
                    sub.annotate(Annotation::primary(typevar_range.into()));
                    diag.sub(sub);
                }

                if let Some(compound_diag) = compound_diag {
                    compound_diag.add_context(context.db(), &mut diag);
                }
            }

            Self::PropertyHasNoSetter(_) => {
                BindingError::InternalCallError("property has no setter").report_diagnostic(
                    context,
                    node,
                    callable_ty,
                    callable_description,
                    compound_diag,
                    matching_overload,
                );
            }

            Self::PropertyHasNoDeleter(_) => {
                BindingError::InternalCallError("property has no deleter").report_diagnostic(
                    context,
                    node,
                    callable_ty,
                    callable_description,
                    compound_diag,
                    matching_overload,
                );
            }

            Self::InternalCallError(reason) => {
                let node = Self::get_node(node, None);
                if let Some(builder) = context.report_lint(&CALL_NON_CALLABLE, node) {
                    let mut diag = builder.into_diagnostic(format_args!(
                        "Call{} failed: {reason}",
                        callable_description
                            .map(|description| format!(" of {description}"))
                            .unwrap_or_default()
                    ));
                    if let Some(compound_diag) = compound_diag {
                        compound_diag.add_context(context.db(), &mut diag);
                    }
                }
            }

            Self::UnmatchedOverload => {}

            Self::CalledTopCallable(callable_ty) => {
                let node = Self::get_node(node, None);
                if let Some(builder) = context.report_lint(&CALL_TOP_CALLABLE, node) {
                    let callable_ty_display = callable_ty.display(context.db());
                    let mut diag = builder.into_diagnostic(format_args!(
                        "Object of type `{callable_ty_display}` is not safe to call; \
                        its signature is not known"
                    ));
                    diag.info(
                        "This type includes all possible callables, so it cannot safely be called \
                        because there is no valid set of arguments for it",
                    );
                    if let Some(compound_diag) = compound_diag {
                        compound_diag.add_context(context.db(), &mut diag);
                    }
                }
            }

            Self::InvalidDataclassApplication(target) => {
                let node = Self::get_node(node, None);
                if let Some(builder) = context.report_lint(&INVALID_DATACLASS, node) {
                    let (message, info) = match target {
                        InvalidDataclassTarget::NamedTuple => (
                            "Cannot use `dataclass()` on a `NamedTuple` class",
                            "An exception will be raised when instantiating the class at runtime",
                        ),
                        InvalidDataclassTarget::TypedDict => (
                            "Cannot use `dataclass()` on a `TypedDict` class",
                            "An exception will often be raised when instantiating the class at runtime",
                        ),
                        InvalidDataclassTarget::Enum => (
                            "Cannot use `dataclass()` on an enum class",
                            "Applying `@dataclass` to an enum is not supported at runtime",
                        ),
                        InvalidDataclassTarget::Protocol => (
                            "Cannot use `dataclass()` on a protocol class",
                            "Protocols define abstract interfaces and cannot be instantiated",
                        ),
                    };
                    let mut diag = builder.into_diagnostic(message);
                    diag.info(info);
                }
            }
        }
    }

    fn get_node(node: ast::AnyNodeRef<'_>, argument_index: Option<usize>) -> ast::AnyNodeRef<'_> {
        // If we have a Call node and an argument index, report the diagnostic on the correct
        // argument node; otherwise, report it on the entire provided node.
        match (Self::get_argument_node(node, argument_index), node) {
            (Some(ast::ArgOrKeyword::Arg(expr)), _) => expr.into(),
            (Some(ast::ArgOrKeyword::Keyword(expr)), _) => expr.into(),
            (None, ast::AnyNodeRef::StmtClassDef(class_def)) => class_def
                .arguments
                .as_deref()
                .map(ast::AnyNodeRef::Arguments)
                .unwrap_or(node),
            (None, _) => node,
        }
    }

    fn get_argument_node(
        node: ast::AnyNodeRef<'_>,
        argument_index: Option<usize>,
    ) -> Option<ArgOrKeyword<'_>> {
        match (node, argument_index) {
            (ast::AnyNodeRef::ExprCall(call_node), Some(argument_index)) => Some(
                call_node
                    .arguments
                    .iter_source_order()
                    .nth(argument_index)
                    .expect("argument index should not be out of range"),
            ),
            // If we've been passed a `ClassDef` node, it indicates that we're reporting an error
            // relating to the class's keyword arguments. Keyword arguments are passed to `__init_subclass__`,
            // or `__new__`/`__prepare__` on the metaclass -- but positional arguments are not, and neither
            // is the special keyword argument `metaclass`. These need to be excluded from the
            // argument index when looking up the relevant keyword-argument node.
            (ast::AnyNodeRef::StmtClassDef(class_def), Some(argument_index)) => {
                class_def.arguments.as_deref().and_then(|args| {
                    args.iter_source_order()
                        .filter_map(ArgOrKeyword::as_keyword)
                        .filter(|keyword| {
                            keyword.arg.as_deref().is_none_or(|arg| arg != "metaclass")
                        })
                        .nth(argument_index)
                        .map(ast::ArgOrKeyword::Keyword)
                })
            }
            _ => None,
        }
    }
}

/// Trait for adding context about compound types (unions/intersections) to diagnostics.
trait CompoundDiagnostic {
    /// Adds context about any relevant compound type function types to the given diagnostic.
    fn add_context(&self, db: &dyn Db, diag: &mut Diagnostic);
}

/// Contains additional context for union specific diagnostics.
///
/// This is used when a function call is inconsistent with one or more variants
/// of a union. This can be used to attach sub-diagnostics that clarify that
/// the error is part of a union.
struct UnionDiagnostic<'b, 'db> {
    /// The type of the union.
    callable_type: Type<'db>,
    /// The specific binding that failed.
    binding: &'b CallableBinding<'db>,
}

impl CompoundDiagnostic for UnionDiagnostic<'_, '_> {
    fn add_context(&self, db: &dyn Db, diag: &mut Diagnostic) {
        let sub = SubDiagnostic::new(
            SubDiagnosticSeverity::Info,
            format_args!(
                "Union variant `{callable_ty}` is incompatible with this call site",
                callable_ty = self.binding.callable_type.display(db),
            ),
        );
        diag.sub(sub);

        let sub = SubDiagnostic::new(
            SubDiagnosticSeverity::Info,
            format_args!(
                "Attempted to call union type `{}`",
                self.callable_type.display(db)
            ),
        );
        diag.sub(sub);
    }
}

/// Contains additional context for intersection specific diagnostics.
///
/// This is used when a function call is inconsistent with all elements
/// of an intersection. This can be used to attach sub-diagnostics that clarify that
/// the error is part of an intersection.
struct IntersectionDiagnostic<'b, 'db> {
    /// The type of the intersection.
    callable_type: Type<'db>,
    /// The specific binding that failed.
    binding: &'b CallableBinding<'db>,
}

impl CompoundDiagnostic for IntersectionDiagnostic<'_, '_> {
    fn add_context(&self, db: &dyn Db, diag: &mut Diagnostic) {
        let sub = SubDiagnostic::new(
            SubDiagnosticSeverity::Info,
            format_args!(
                "Intersection element `{callable_ty}` is incompatible with this call site",
                callable_ty = self.binding.callable_type.display(db),
            ),
        );
        diag.sub(sub);

        let sub = SubDiagnostic::new(
            SubDiagnosticSeverity::Info,
            format_args!(
                "Attempted to call intersection type `{}`",
                self.callable_type.display(db)
            ),
        );
        diag.sub(sub);
    }
}

/// Contains both union and intersection context for layered diagnostics.
///
/// Used when an intersection fails inside a union - we want to report both
/// that this is a union variant AND that this is an intersection element.
struct LayeredDiagnostic<'b, 'db> {
    /// The type of the union.
    union_callable_type: Type<'db>,
    /// The type of the intersection (for intersection context).
    intersection_callable_type: Type<'db>,
    /// The specific binding that failed.
    binding: &'b CallableBinding<'db>,
}

impl CompoundDiagnostic for LayeredDiagnostic<'_, '_> {
    fn add_context(&self, db: &dyn Db, diag: &mut Diagnostic) {
        // Add intersection context first (more specific)
        let sub = SubDiagnostic::new(
            SubDiagnosticSeverity::Info,
            format_args!(
                "Intersection element `{callable_ty}` is incompatible with this call site",
                callable_ty = self.binding.callable_type.display(db),
            ),
        );
        diag.sub(sub);

        let sub = SubDiagnostic::new(
            SubDiagnosticSeverity::Info,
            format_args!(
                "Attempted to call intersection type `{}`",
                self.intersection_callable_type.display(db)
            ),
        );
        diag.sub(sub);

        // Then add union context (outer layer)
        let sub = SubDiagnostic::new(
            SubDiagnosticSeverity::Info,
            format_args!(
                "Attempted to call union type `{}`",
                self.union_callable_type.display(db)
            ),
        );
        diag.sub(sub);
    }
}

/// Represents the matching overload of a function literal that was found via the overload call
/// evaluation algorithm.
struct MatchingOverloadLiteral<'db> {
    /// The position of the matching overload in the list of overloads.
    index: usize,
    /// The kind of function this overload is for.
    kind: FunctionKind,
    /// The function literal that this overload belongs to.
    ///
    /// This is used to retrieve the overload at the given index.
    function: FunctionType<'db>,
}

impl<'db> MatchingOverloadLiteral<'db> {
    /// Returns the [`OverloadLiteral`] representing this matching overload.
    fn get(&self, db: &'db dyn Db) -> Option<OverloadLiteral<'db>> {
        let (overloads, _) = self.function.overloads_and_implementation(db);

        // TODO: This should actually be safe to index directly but isn't so as of this writing.
        // The main reason is that we've custom overload signatures that are constructed manually
        // and does not belong to any file. For example, the `__get__` method of a function literal
        // has a custom overloaded signature. So, when we try to retrieve the actual overloads
        // above, we get an empty list of overloads because the implementation of that method
        // relies on it existing in the file.
        overloads.get(self.index).copied()
    }
}

#[derive(Clone, Copy, Debug)]
enum FunctionKind {
    Function,
    BoundMethod,
    MethodWrapper,
}

impl fmt::Display for FunctionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FunctionKind::Function => write!(f, "function"),
            FunctionKind::BoundMethod => write!(f, "bound method"),
            FunctionKind::MethodWrapper => write!(f, "method wrapper `__get__` of function"),
        }
    }
}

// When the number of unmatched overloads exceeds this number, we stop printing them to avoid
// excessive output.
//
// An example of a routine with many many overloads:
// https://github.com/henribru/google-api-python-client-stubs/blob/master/googleapiclient-stubs/discovery.pyi
const MAXIMUM_OVERLOADS: usize = 50;

/// Infer the return type for a call to `asynccontextmanager`.
///
/// The `@asynccontextmanager` decorator transforms a function that returns (a subtype of) `AsyncIterator[T]`
/// into a function that returns `_AsyncGeneratorContextManager[T]`.
///
/// TODO: This function only handles the most basic case. It should be removed once we have
/// full support for generic protocols in the solver.
fn asynccontextmanager_return_type<'db>(db: &'db dyn Db, func_ty: Type<'db>) -> Option<Type<'db>> {
    let bindings = func_ty.bindings(db);
    let binding = bindings
        .single_element()?
        .overloads
        .iter()
        .exactly_one()
        .ok()?;
    let signature = &binding.signature;

    let yield_ty = signature
        .return_ty
        .try_iterate_with_mode(db, EvaluationMode::Async)
        .ok()?
        .homogeneous_element_type(db);

    let context_manager =
        known_module_symbol(db, KnownModule::Contextlib, "_AsyncGeneratorContextManager")
            .place
            .ignore_possibly_undefined()?
            .as_class_literal()?;

    let context_manager = context_manager.apply_specialization(db, |generic_context| {
        generic_context.specialize_partial(db, [Some(yield_ty), None])
    });

    let new_return_ty = Type::from(context_manager).to_instance(db)?;
    let new_signature = Signature::new(signature.parameters().clone(), new_return_ty);

    Some(Type::Callable(CallableType::new(
        db,
        CallableSignature::single(new_signature),
        CallableTypeKind::FunctionLike,
    )))
}

/// Maximum repetition count for struct format specifiers.
/// Larger counts fall back to `tuple[Unknown, ...]`.
const STRUCT_FORMAT_MAX_REPETITION: usize = 32;

/// Parse a `struct` module format string and return the element types.
///
/// Returns `None` if the format contains unsupported specifiers or
/// repetition counts exceed the limit, indicating a fallback to `tuple[Unknown, ...]`.
fn parse_struct_format<'db>(db: &'db dyn Db, format_string: &str) -> Option<Vec<Type<'db>>> {
    // Strip the byte order/size/alignment prefix
    let format = format_string.trim_start_matches(['@', '=', '<', '>', '!']);
    let mut chars = format.chars().peekable();
    let mut elements = Vec::new();

    while chars.peek().is_some() {
        // Skip whitespace between format specifiers
        while chars.next_if(char::is_ascii_whitespace).is_some() {}

        // Parse optional repeat count (defaults to 1)
        let mut count: usize = 1;
        if let Some(digit) = chars.next_if(char::is_ascii_digit) {
            count = digit.to_digit(10).unwrap() as usize;
            while let Some(digit) = chars.next_if(char::is_ascii_digit) {
                count = count
                    .saturating_mul(10)
                    .saturating_add(digit.to_digit(10).unwrap() as usize);
            }
        }

        let Some(specifier) = chars.next() else {
            break;
        };

        // Map specifier to (type, repeat_count). For 's'/'p', count is byte length, not repetition.
        let (ty, repeat) = match specifier {
            'x' => continue, // Pad byte: no value produced
            's' | 'p' => (KnownClass::Bytes.to_instance(db), 1),
            'c' => (KnownClass::Bytes.to_instance(db), count),
            'b' | 'B' | 'h' | 'H' | 'i' | 'I' | 'l' | 'L' | 'q' | 'Q' | 'n' | 'N' | 'P' => {
                (KnownClass::Int.to_instance(db), count)
            }
            '?' => (KnownClass::Bool.to_instance(db), count),
            'e' | 'f' | 'd' => (KnownClass::Float.to_instance(db), count),
            'F' | 'D' if Program::get(db).python_version(db) >= PythonVersion::PY314 => {
                (KnownClass::Complex.to_instance(db), count)
            }
            _ => return None,
        };

        if repeat > STRUCT_FORMAT_MAX_REPETITION {
            return None;
        }
        elements.extend(std::iter::repeat_n(ty, repeat));
    }

    Some(elements)
}

/// Return the range for a binding diagnostic that is not related to one specific
/// argument.
///
/// For a normal function call, this is just the range of the entire call.
/// If we're reporting diagnostics for bad arguments in a class definition,
/// however,
/// restrict the range to just the range of the class name + its arguments.
fn all_arguments_range(node: AnyNodeRef) -> TextRange {
    node.as_stmt_class_def()
        .map(|class| {
            TextRange::new(
                class.start(),
                class
                    .arguments
                    .as_deref()
                    .map(Ranged::end)
                    .unwrap_or(class.name.end()),
            )
        })
        .unwrap_or(node.range())
}
