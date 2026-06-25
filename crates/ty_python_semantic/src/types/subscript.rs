//! Inference for subscript expressions (e.g., `x[0]`, `list[int]`).

use std::fmt::{self, Display};

use itertools::Itertools;
use ruff_db::diagnostic::{Annotation, Span, SubDiagnostic, SubDiagnosticSeverity};
use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;

use crate::Db;
use crate::diagnostic::format_enumeration_with_or;
use crate::place::Provenance;
use crate::subscript::{PyIndex, PySlice};
use crate::types::call::bind::{FunctionKind, annotate_with_overloads};
use crate::types::special_form::TypeQualifier;

use super::call::{Bindings, CallArguments, CallDunderError, CallErrorKind};
use super::class::KnownClass;
use super::class_base::ClassBase;
use super::context::InferContext;
use super::diagnostic::{
    INVALID_ARGUMENT_TYPE, INVALID_GENERIC_CLASS, NOT_SUBSCRIPTABLE, report_index_out_of_bounds,
    report_invalid_key_on_typed_dict, report_not_subscriptable, report_slice_step_size_zero,
};
use super::infer::TypeContext;
use super::instance::SliceLiteral;
use super::special_form::SpecialFormType;
use super::tuple::TupleSpec;
use super::{
    DynamicType, IntersectionBuilder, IntersectionType, KnownInstanceType, Type, TypeAliasType,
    TypedDictType, UnionBuilder, UnionType, todo_type,
};

/// The kind of subscriptable type that had an out-of-bounds index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubscriptKind {
    Tuple,
    String,
    BytesLiteral,
}

impl SubscriptKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Tuple => "tuple",
            Self::String => "string",
            Self::BytesLiteral => "bytes literal",
        }
    }
}

impl Display for SubscriptKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A dunder method used for subscripting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DunderMethod {
    GetItem,
    ClassGetItem,
}

impl Display for DunderMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl DunderMethod {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::GetItem => "__getitem__",
            Self::ClassGetItem => "__class_getitem__",
        }
    }
}

/// The origin of a legacy generic subscription (`Generic` or `Protocol`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegacyGenericOrigin {
    Generic,
    Protocol,
}

impl Display for LegacyGenericOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Generic => "Generic",
            Self::Protocol => "Protocol",
        })
    }
}

impl From<LegacyGenericOrigin> for Type<'_> {
    fn from(origin: LegacyGenericOrigin) -> Self {
        match origin {
            LegacyGenericOrigin::Generic => Type::SpecialForm(SpecialFormType::Generic),
            LegacyGenericOrigin::Protocol => Type::SpecialForm(SpecialFormType::Protocol),
        }
    }
}

#[derive(Debug)]
pub(crate) struct SubscriptError<'db> {
    result_ty: Type<'db>,
    errors: Vec<SubscriptErrorKind<'db>>,
    subscripted_type: Type<'db>,
}

#[derive(Debug)]
pub(crate) enum SubscriptErrorKind<'db> {
    /// An index is out of bounds for a literal tuple/string/bytes subscript.
    IndexOutOfBounds {
        kind: SubscriptKind,
        tuple_ty: Type<'db>,
        length: Box<str>,
        index: i64,
    },
    /// A slice literal used a step size of zero.
    SliceStepSizeZero,
    /// A non-generic PEP 695 type alias was subscripted.
    NonGenericTypeAlias { alias: TypeAliasType<'db> },
    /// `__getitem__` or `__class_getitem__` exists but is possibly unbound.
    DunderPossiblyUnbound {
        method: DunderMethod,
        value_ty: Type<'db>,
    },
    /// `__getitem__` or `__class_getitem__` exists but can't be called with the given arguments.
    DunderCallError {
        method: DunderMethod,
        value_ty: Type<'db>,
        slice_ty: Type<'db>,
        kind: CallErrorKind,
        bindings: Box<Bindings<'db>>,
        provenance: Provenance<'db>,
    },
    /// A `TypedDict` was subscripted with an invalid key.
    InvalidTypedDictKey {
        typed_dict: TypedDictType<'db>,
        slice_ty: Type<'db>,
        full_object_ty: Option<Type<'db>>,
    },
    /// The type does not support subscripting via the expected dunder.
    NotSubscriptable {
        value_ty: Type<'db>,
        method: DunderMethod,
    },
    /// An invalid argument was provided to `Generic` or `Protocol`.
    InvalidLegacyGenericArgument {
        origin: LegacyGenericOrigin,
        argument_ty: Type<'db>,
    },
    /// A duplicate typevar was provided to `Generic` or `Protocol`.
    DuplicateTypevar {
        origin: LegacyGenericOrigin,
        typevar_name: &'db str,
    },
    /// A `TypeVarTuple` was provided to `Generic` or `Protocol` without being unpacked.
    TypeVarTupleNotUnpacked { origin: LegacyGenericOrigin },
}

impl<'db> SubscriptError<'db> {
    pub(crate) fn new(
        result_ty: Type<'db>,
        error: SubscriptErrorKind<'db>,
        subscripted_type: Type<'db>,
    ) -> Self {
        Self {
            result_ty,
            errors: vec![error],
            subscripted_type,
        }
    }

    fn with_errors(
        result_ty: Type<'db>,
        errors: Vec<SubscriptErrorKind<'db>>,
        subscripted_type: Type<'db>,
    ) -> Self {
        Self {
            result_ty,
            errors,
            subscripted_type,
        }
    }

    pub(crate) fn result_type(&self) -> Type<'db> {
        self.result_ty
    }

    fn into_errors(self) -> Vec<SubscriptErrorKind<'db>> {
        self.errors
    }

    /// Returns `true` if any error indicates the subscript method was available.
    fn any_method_available(&self) -> bool {
        self.errors.iter().any(SubscriptErrorKind::method_available)
    }

    pub(crate) fn report_diagnostics(
        &self,
        context: &InferContext<'db, '_>,
        subscript: &ast::ExprSubscript,
    ) {
        let value_node = subscript.value.as_ref();
        let slice_node = subscript.slice.as_ref();
        for error in &self.errors {
            error.report_diagnostic(
                context,
                subscript,
                value_node,
                slice_node,
                self.subscripted_type,
            );
        }
    }
}

impl<'db> SubscriptErrorKind<'db> {
    fn with_full_object_ty(self, full_object_ty: Type<'db>) -> Self {
        match self {
            Self::InvalidTypedDictKey {
                typed_dict,
                slice_ty,
                ..
            } => Self::InvalidTypedDictKey {
                typed_dict,
                slice_ty,
                full_object_ty: Some(full_object_ty),
            },
            other => other,
        }
    }

    fn report_diagnostic(
        &self,
        context: &InferContext<'db, '_>,
        subscript: &ast::ExprSubscript,
        value_node: &ast::Expr,
        slice_node: &ast::Expr,
        subscripted_type: Type<'db>,
    ) {
        #[derive(Debug, Copy, Clone, PartialEq, Eq)]
        enum TermOfArt {
            Index,
            Key,
        }

        impl TermOfArt {
            fn with_article(self) -> &'static str {
                match self {
                    TermOfArt::Index => "an index",
                    TermOfArt::Key => "a key",
                }
            }
        }

        impl std::fmt::Display for TermOfArt {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(match self {
                    TermOfArt::Index => "index",
                    TermOfArt::Key => "key",
                })
            }
        }

        let db = context.db();
        match self {
            Self::IndexOutOfBounds {
                kind,
                tuple_ty,
                length,
                index,
            } => {
                report_index_out_of_bounds(context, *kind, value_node, *tuple_ty, length, *index);
            }
            Self::SliceStepSizeZero => {
                report_slice_step_size_zero(context, value_node.into());
            }
            Self::NonGenericTypeAlias { alias } => {
                if let Some(builder) = context.report_lint(&NOT_SUBSCRIPTABLE, subscript) {
                    let mut diagnostic = builder.into_diagnostic(format_args!(
                        "Cannot subscript non-generic type alias `{}`",
                        alias.name(db)
                    ));
                    let value_type = alias.raw_value_type(db);
                    if value_type.is_specialized_generic(db) {
                        diagnostic.annotate(context.secondary(&*subscript.value).message(
                            format_args!(
                                "Alias to `{}`, which is already specialized",
                                value_type.display(db)
                            ),
                        ));
                    }
                }
            }
            Self::DunderPossiblyUnbound { method, value_ty } => {
                if let Some(builder) = context.report_lint(&NOT_SUBSCRIPTABLE, subscript) {
                    let mut diagnostic = builder.into_diagnostic("Invalid subscript read");
                    diagnostic.set_concise_message(format_args!(
                        "Cannot subscript an object of type `{}` \
                        with a possibly missing `{method}` method",
                        value_ty.display(db)
                    ));
                    diagnostic.annotate(
                        context
                            .secondary(value_node)
                            .message(format_args!("Has type `{}`", subscripted_type.display(db))),
                    );
                    diagnostic.annotate(
                        Annotation::primary(context.span(slice_node))
                            .message(format_args!("Method `{method}` may be missing")),
                    );
                    diagnostic.info(format_args!(
                        "`{method}` is implicitly called due to this subscript expression"
                    ));
                }
            }
            Self::DunderCallError {
                method,
                value_ty,
                slice_ty,
                kind,
                bindings,
                provenance,
            } => match kind {
                CallErrorKind::NotCallable => {
                    if let Some(builder) = context.report_lint(&NOT_SUBSCRIPTABLE, subscript) {
                        let mut diagnostic = builder.into_diagnostic("Invalid subscript read");
                        let value_type = value_ty.display(db);
                        let method_type = bindings.callable_type().display(db);
                        diagnostic.set_concise_message(format_args!(
                            "Cannot subscript an object of type `{value_type}` with an invalid `{method}` method",
                        ));
                        diagnostic.annotate(
                            context.secondary(value_node).message(format_args!(
                                "Has type `{}`",
                                subscripted_type.display(db)
                            )),
                        );
                        if let Some(definition) = provenance.definition() {
                            diagnostic.annotate(
                                Annotation::primary(context.span(slice_node)).message(
                                    format_args!(
                                        "Subscript expression implicitly calls `{method}`, \
                                        which is not callable"
                                    ),
                                ),
                            );
                            let mut sub = SubDiagnostic::new(
                                SubDiagnosticSeverity::Info,
                                format_args!("`{method}` defined here"),
                            );
                            let file = definition.file(db);
                            let module = parsed_module(db, file).load(db);
                            sub.annotate(
                                Annotation::primary(Span::from(
                                    definition.focus_range(db, &module),
                                ))
                                .message(format_args!("Has type `{method_type}`")),
                            );
                            diagnostic.sub(sub);
                        } else {
                            for message in [
                                format_args!("Method `{method}` has type `{method_type}`"),
                                format_args!("An object of type `{method_type}` cannot be called"),
                            ] {
                                diagnostic.annotate(
                                    Annotation::primary(context.span(slice_node)).message(message),
                                );
                            }
                            diagnostic.info(format_args!(
                                "Subscript expression implicitly calls `{method}`"
                            ));
                        }
                    }
                }
                CallErrorKind::BindingError => {
                    if let Some(typed_dict) = value_ty.as_typed_dict() {
                        report_invalid_key_on_typed_dict(
                            context,
                            value_node.into(),
                            slice_node.into(),
                            *value_ty,
                            None,
                            *slice_ty,
                            typed_dict.items(db),
                        );
                    } else {
                        let expected_types_for_slice = bindings.expected_types_for_argument(db, 0);

                        if let Some(expected_types) = expected_types_for_slice.as_deref() {
                            let Some(builder) =
                                context.report_lint(&INVALID_ARGUMENT_TYPE, subscript)
                            else {
                                return;
                            };
                            let mut diagnostic = builder.into_diagnostic("Invalid subscript read");
                            let term_of_art = if value_ty.is_redundant_with(
                                db,
                                KnownClass::Sequence.to_specialized_instance(db, &[Type::object()]),
                            ) {
                                TermOfArt::Index
                            } else {
                                TermOfArt::Key
                            };
                            let value_type = value_ty.display(db);
                            let slice_type = slice_ty.display(db);
                            diagnostic.annotate(context.secondary(value_node).message(
                                format_args!("Has type `{}`", subscripted_type.display(db)),
                            ));
                            let mut primary_annotation =
                                Annotation::primary(context.span(slice_node));
                            match expected_types {
                                [] => {
                                    primary_annotation = primary_annotation.message(format_args!(
                                        "Invalid {term_of_art} of type `{}`",
                                        slice_ty.display(db)
                                    ));
                                    diagnostic.set_concise_message(format_args!(
                                        "Cannot subscript an object of type `{value_type}` \
                                        with {} of type `{slice_type}`",
                                        term_of_art.with_article()
                                    ));
                                }
                                [single_expected] => {
                                    primary_annotation = primary_annotation.message(format_args!(
                                        "Expected `{}`, got object of type `{}`",
                                        single_expected.display(db),
                                        slice_ty.display(db),
                                    ));
                                    diagnostic.set_concise_message(format_args!(
                                        "Cannot subscript an object of type `{value_type}` \
                                        with {} of type `{slice_type}` (expected `{}`)",
                                        term_of_art.with_article(),
                                        single_expected.display(db)
                                    ));
                                }
                                multiple_expected => {
                                    let enumeration = format_enumeration_with_or(
                                        multiple_expected.iter().map(|ty| ty.display(db)),
                                    );
                                    primary_annotation = primary_annotation.message(format_args!(
                                        "Has type `{}`",
                                        slice_ty.display(db)
                                    ));
                                    diagnostic.set_concise_message(format_args!(
                                        "Cannot subscript an object of type `{value_type}` \
                                        with {} of type `{slice_type}` (expected one of {enumeration})",
                                        term_of_art.with_article()
                                    ));
                                }
                            }
                            diagnostic.annotate(primary_annotation);

                            if let Some(definition) = provenance.definition() {
                                let mut sub = SubDiagnostic::new(
                                    SubDiagnosticSeverity::Info,
                                    format_args!(
                                        "This subscript expression implicitly calls `{}.{method}`",
                                        value_ty.display(db)
                                    ),
                                );
                                let file = definition.file(db);
                                let module = parsed_module(db, file).load(db);

                                if bindings
                                    .single_element()
                                    .is_none_or(|single| single.overloads().len() == 1)
                                {
                                    let annotation = Annotation::primary(Span::from(
                                        definition.focus_range(db, &module),
                                    ));

                                    sub.annotate(annotation.message("Method defined here"));
                                    diagnostic.sub(sub);
                                } else if let Some(binding) = bindings.single_element() {
                                    diagnostic.sub(sub);
                                    let kind = FunctionKind::classify(db, bindings.callable_type());
                                    if let Some((kind, function)) = kind {
                                        annotate_with_overloads(
                                            context,
                                            binding,
                                            &mut diagnostic,
                                            function,
                                            kind,
                                        );
                                    }
                                }
                            }
                        } else {
                            bindings.report_diagnostics(context, ast::AnyNodeRef::from(subscript));
                        }
                    }
                }
                CallErrorKind::PossiblyNotCallable => {
                    if let Some(builder) = context.report_lint(&NOT_SUBSCRIPTABLE, subscript) {
                        let mut diagnostic = builder.into_diagnostic("Invalid subscript read");
                        let value_type = value_ty.display(db);
                        diagnostic.set_concise_message(format_args!(
                            "Cannot subscript an object of type `{value_type}` \
                            which may not have a valid `{method}` method",
                        ));
                        diagnostic.annotate(
                            context.secondary(value_node).message(format_args!(
                                "Has type `{}`",
                                subscripted_type.display(db)
                            )),
                        );
                        let method_type = bindings.callable_type().display(db);
                        for message in [
                            format_args!("Method `{method}` has type `{method_type}`"),
                            format_args!("An object of type `{method_type}` may not be callable"),
                        ] {
                            diagnostic.annotate(
                                Annotation::primary(context.span(slice_node)).message(message),
                            );
                        }
                        diagnostic.info(format_args!(
                            "`{method}` is implicitly called due to this subscript expression"
                        ));
                    }
                }
            },
            Self::InvalidTypedDictKey {
                typed_dict,
                slice_ty,
                full_object_ty,
            } => {
                let typed_dict_ty = Type::TypedDict(*typed_dict);
                report_invalid_key_on_typed_dict(
                    context,
                    value_node.into(),
                    slice_node.into(),
                    typed_dict_ty,
                    *full_object_ty,
                    *slice_ty,
                    typed_dict.items(db),
                );
            }
            Self::NotSubscriptable { value_ty, method } => {
                report_not_subscriptable(context, subscript, *value_ty, method.as_str());
            }
            Self::InvalidLegacyGenericArgument {
                origin,
                argument_ty,
            } => {
                if let Some(builder) = context.report_lint(&INVALID_ARGUMENT_TYPE, subscript) {
                    builder.into_diagnostic(format_args!(
                        "`{}` is not a valid argument to `{origin}`",
                        argument_ty.display(db),
                    ));
                }
            }
            Self::DuplicateTypevar {
                origin,
                typevar_name,
            } => {
                if let Some(builder) = context.report_lint(&INVALID_GENERIC_CLASS, subscript) {
                    builder.into_diagnostic(format_args!(
                        "Type parameter `{typevar_name}` cannot appear multiple times \
                        in `{origin}` subscription",
                    ));
                }
            }
            Self::TypeVarTupleNotUnpacked { origin } => {
                if let Some(builder) = context.report_lint(&INVALID_GENERIC_CLASS, subscript) {
                    builder.into_diagnostic(format_args!(
                        "`TypeVarTuple` must be unpacked with `*` or `Unpack[]` when \
                        used as an argument to `{origin}`",
                    ));
                }
            }
        }
    }

    /// Returns `true` if this error indicates the subscript method was available
    /// (even if the call failed). Returns `false` for `NotSubscriptable` errors.
    fn method_available(&self) -> bool {
        !matches!(self, Self::NotSubscriptable { .. })
    }
}

fn map_union_subscript<'db, F>(
    db: &'db dyn Db,
    union: UnionType<'db>,
    subscripted_type: Type<'db>,
    mut map_fn: F,
) -> Result<Type<'db>, SubscriptError<'db>>
where
    F: FnMut(Type<'db>) -> Result<Type<'db>, SubscriptError<'db>>,
{
    let mut builder = UnionBuilder::new(db);
    let mut errors = Vec::new();

    for element in union.elements(db) {
        match map_fn(*element) {
            Ok(result) => {
                builder = builder.add(result);
            }
            Err(error) => {
                let full_object_ty = Type::Union(union);
                builder = builder.add(error.result_type());
                errors.extend(
                    error
                        .into_errors()
                        .into_iter()
                        .map(|error| error.with_full_object_ty(full_object_ty)),
                );
            }
        }
    }

    builder = builder.recursively_defined(union.recursively_defined(db));
    let result_ty = builder.build();
    if errors.is_empty() {
        Ok(result_ty)
    } else {
        Err(SubscriptError::with_errors(
            result_ty,
            errors,
            subscripted_type,
        ))
    }
}

fn map_intersection_subscript<'db, F>(
    db: &'db dyn Db,
    intersection: IntersectionType<'db>,
    subscripted_type: Type<'db>,
    mut map_fn: F,
) -> Result<Type<'db>, SubscriptError<'db>>
where
    F: FnMut(Type<'db>) -> Result<Type<'db>, SubscriptError<'db>>,
{
    if let Some(alternatives) = intersection.finite_alternative_union(db) {
        return map_fn(alternatives);
    }

    let mut results = Vec::new();
    let mut errors = Vec::new();

    // Use `positive_elements_or_object` to ensure we always have at least one element.
    // An intersection with only negative elements (e.g., `~int & ~str`) is implicitly
    // `object & ~int & ~str`, so we fall back to `object`.
    for element in intersection.positive_elements_or_object(db) {
        match map_fn(element) {
            Ok(result) => results.push(result),
            Err(error) => errors.push(error),
        }
    }

    // If any element succeeded, return the intersection of successful results.
    if !results.is_empty() {
        let mut builder = IntersectionBuilder::new(db);
        for result in results {
            builder = builder.add_positive(result);
        }
        return Ok(builder.build());
    }

    // All elements failed. Check if any element has the method available
    // (even if the call failed). If so, filter out `NotSubscriptable` errors
    // for elements that lack the method.
    let any_has_method = errors.iter().any(SubscriptError::any_method_available);

    let mut builder = IntersectionBuilder::new(db);
    let mut collected_errors = Vec::new();
    let full_object_ty = Type::Intersection(intersection);

    for error in errors {
        if !any_has_method || error.any_method_available() {
            builder = builder.add_positive(error.result_type());
            let error_iter = error.into_errors().into_iter();
            if any_has_method {
                collected_errors.extend(
                    error_iter
                        .filter(SubscriptErrorKind::method_available)
                        .map(|error| error.with_full_object_ty(full_object_ty)),
                );
            } else {
                collected_errors
                    .extend(error_iter.map(|error| error.with_full_object_ty(full_object_ty)));
            }
        }
    }

    Err(SubscriptError::with_errors(
        builder.build(),
        collected_errors,
        subscripted_type,
    ))
}

// `TypedDict` subscripts need custom handling because invalid keys should emit `invalid-key` while
// recovering with the union of value types for non-literal string keys on closed `TypedDict`s and
// `Unknown` otherwise. This is not naturally representable via synthesized `__getitem__` overloads.
fn typed_dict_subscript<'db>(
    db: &'db dyn Db,
    typed_dict: TypedDictType<'db>,
    slice_ty: Type<'db>,
    subscripted_type: Type<'db>,
) -> Result<Type<'db>, SubscriptError<'db>> {
    if let Some(fallback) = slice_ty.materialized_divergent_fallback() {
        return typed_dict_subscript(db, typed_dict, fallback, subscripted_type);
    }

    if slice_ty.is_dynamic() {
        return Ok(Type::unknown());
    }

    let Some(key) = slice_ty
        .as_string_literal()
        .map(|literal| literal.value(db))
    else {
        if typed_dict.explicit_extra_items(db).is_some()
            && slice_ty.is_assignable_to(db, KnownClass::Str.to_instance(db))
        {
            return Ok(typed_dict.value_type(db));
        }
        let result_ty = if typed_dict.openness(db).is_closed()
            && slice_ty.is_assignable_to(db, KnownClass::Str.to_instance(db))
        {
            typed_dict.value_type(db)
        } else {
            Type::unknown()
        };
        return Err(SubscriptError::new(
            result_ty,
            SubscriptErrorKind::InvalidTypedDictKey {
                typed_dict,
                slice_ty,
                full_object_ty: None,
            },
            subscripted_type,
        ));
    };

    typed_dict.item(db, key).map_or_else(
        || {
            Err(SubscriptError::new(
                Type::unknown(),
                SubscriptErrorKind::InvalidTypedDictKey {
                    typed_dict,
                    slice_ty,
                    full_object_ty: None,
                },
                subscripted_type,
            ))
        },
        |field| Ok(field.declared_ty),
    )
}

impl<'db> Type<'db> {
    pub(super) fn subscript(
        self,
        db: &'db dyn Db,
        slice_ty: Type<'db>,
        expr_context: ast::ExprContext,
    ) -> Result<Type<'db>, SubscriptError<'db>> {
        self.subscript_impl(db, slice_ty, expr_context, self)
    }

    /// Implementation of [`Type::subscript`], which keeps track of the *original* type that was subscripted
    /// even as we recurse into unions and intersections. This is used to report diagnostics with the correct type.
    fn subscript_impl(
        self,
        db: &'db dyn Db,
        slice_ty: Type<'db>,
        expr_context: ast::ExprContext,
        subscripted_type: Type<'db>,
    ) -> Result<Type<'db>, SubscriptError<'db>> {
        if let Some(fallback) = self.materialized_divergent_fallback() {
            return fallback.subscript_impl(db, slice_ty, expr_context, subscripted_type);
        }

        if let Some(fallback) = slice_ty.materialized_divergent_fallback() {
            return self.subscript_impl(db, fallback, expr_context, subscripted_type);
        }

        let value_ty = self;

        let inferred = match (value_ty, slice_ty) {
            (Type::Dynamic(_) | Type::Divergent(_) | Type::Never, _) => Some(Ok(value_ty)),

            (Type::TypeAlias(alias), _) => {
                Some(alias.value_type(db).subscript_impl(db, slice_ty, expr_context, subscripted_type))
            }

            (_, Type::TypeAlias(alias)) => {
                Some(value_ty.subscript_impl(db, alias.value_type(db), expr_context, subscripted_type))
            }

            (Type::Union(union), _) => Some(map_union_subscript(db, union, subscripted_type,|element| {
                element.subscript_impl(db, slice_ty, expr_context, subscripted_type)
            })),

            (_, Type::Union(union)) => Some(map_union_subscript(db, union, subscripted_type,|element| {
                value_ty.subscript_impl(db, element, expr_context, subscripted_type)
            })),

            (Type::EnumComplement(complement), _) => {
                Some(complement.remaining_literal_union(db).subscript_impl(db, slice_ty, expr_context, subscripted_type))
            }

            (_, Type::EnumComplement(complement)) => {
                Some(value_ty.subscript_impl(db, complement.remaining_literal_union(db), expr_context, subscripted_type))
            }

            (Type::Intersection(intersection), _) => {
                Some(map_intersection_subscript(db, intersection, subscripted_type,|element| {
                    element.subscript_impl(db, slice_ty, expr_context, subscripted_type)
                }))
            }

            (_, Type::Intersection(intersection)) => {
                Some(map_intersection_subscript(db, intersection, subscripted_type,|element| {
                    value_ty.subscript_impl(db, element, expr_context, subscripted_type)
                }))
            }

            // Ex) Given `person["name"]`, return `str`
            (Type::TypedDict(typed_dict), _) if expr_context != ast::ExprContext::Store => {
                Some(typed_dict_subscript(db, typed_dict, slice_ty, subscripted_type))
            }

            (
                Type::NominalInstance(maybe_sequence_nominal),
                Type::NominalInstance(maybe_slice_nominal),
            ) if matches!(
                maybe_sequence_nominal.known_class(db),
                Some(
                    KnownClass::List
                        | KnownClass::Tuple
                        | KnownClass::Str
                        | KnownClass::Bytes
                        | KnownClass::Bytearray
                        | KnownClass::Range
                        | KnownClass::Memoryview
                )
            )
                && maybe_slice_nominal
                    .slice_literal(db)
                    .is_some_and(|slice| slice.step == Some(0)) =>
            {
                Some(Err(SubscriptError::new(
                    value_ty,
                    SubscriptErrorKind::SliceStepSizeZero,
                    subscripted_type
                )))
            }

            // Ex) Given `("a", "b", "c", "d")[1]`, return `"b"`
            (Type::NominalInstance(nominal), Type::LiteralValue(literal)) if literal.is_int() => {
                let i64_int = literal.as_int().unwrap();
                nominal
                    .tuple_spec(db)
                    .and_then(|tuple| Some((tuple, i32::try_from(i64_int).ok()?)))
                    .map(|(tuple, i32_int)| match tuple.py_index(db, i32_int) {
                        Ok(result) => Ok(result),
                        Err(_) => Err(SubscriptError::new(
                            Type::unknown(),
                            SubscriptErrorKind::IndexOutOfBounds {
                                kind: SubscriptKind::Tuple,
                                tuple_ty: value_ty,
                                length: tuple.len().display_minimum().into(),
                                index: i64_int,
                            },
                            subscripted_type
                        )),
                    })
            }

            // Ex) Given `("a", 1, Null)[0:2]`, return `("a", 1)`
            (
                Type::NominalInstance(maybe_tuple_nominal),
                Type::NominalInstance(maybe_slice_nominal),
            ) => maybe_tuple_nominal
                .tuple_spec(db)
                .as_deref()
                .and_then(|tuple_spec| Some((tuple_spec, maybe_slice_nominal.slice_literal(db)?)))
                .map(|(tuple, SliceLiteral { start, stop, step })| match tuple {
                    TupleSpec::Fixed(tuple) => match tuple.py_slice(db, start, stop, step) {
                        Ok(new_elements) => {
                            Ok(Type::heterogeneous_tuple(db, new_elements))
                        }
                        Err(_) => Err(SubscriptError::new(
                            Type::unknown(),
                            SubscriptErrorKind::SliceStepSizeZero,
                            subscripted_type
                        )),
                    },
                    TupleSpec::Variable(_) => {
                        Ok(todo_type!("slice into variable-length tuple"))
                    }
                }),

            // Ex) Given `"value"[1]`, return `"a"`
            (Type::LiteralValue(lhs_literal), Type::LiteralValue(rhs_literal)) if lhs_literal.is_string() && rhs_literal.is_int() => {
                let literal_ty = lhs_literal.as_string().unwrap();
                let i64_int = rhs_literal.as_int().unwrap();
                i32::try_from(i64_int).ok().map(|i32_int| {
                    let literal_value = literal_ty.value(db);
                    match (&mut literal_value.chars()).py_index(db, i32_int) {
                        Ok(ch) => Ok(Type::string_literal(db, &ch.to_string())),
                        Err(_) => Err(SubscriptError::new(
                            Type::unknown(),
                            SubscriptErrorKind::IndexOutOfBounds {
                                kind: SubscriptKind::String,
                                tuple_ty: value_ty,
                                length: literal_value.chars().count().to_string().into(),
                                index: i64_int,
                            },
                            subscripted_type
                        )),
                    }
                })
            }

            // Ex) Given `"value"[1:3]`, return `"al"`
            (Type::LiteralValue(literal), Type::NominalInstance(nominal)) if literal.is_string() => {
                let literal_ty = literal.as_string().unwrap();
                nominal
                .slice_literal(db)
                .map(|SliceLiteral { start, stop, step }| {
                    let literal_value = literal_ty.value(db);
                    let chars: Vec<_> = literal_value.chars().collect();

                    match chars.py_slice(db, start, stop, step) {
                        Ok(new_chars) => {
                            let literal = new_chars.collect::<String>();
                            Ok(Type::string_literal(db, &literal))
                        }
                        Err(_) => Err(SubscriptError::new(
                            Type::unknown(),
                            SubscriptErrorKind::SliceStepSizeZero,
                            subscripted_type
                        )),
                    }
                })
            },

            (Type::LiteralValue(lhs_literal), Type::LiteralValue(rhs_literal)) if lhs_literal.is_literal_string() && (rhs_literal.is_int() || rhs_literal.is_bool()) => {
                Some(Ok(Type::literal_string()))
            }

            (Type::LiteralValue(literal), Type::NominalInstance(nominal))
                if literal.is_literal_string() && nominal.slice_literal(db).is_some() =>
            {
                Some(Ok(Type::literal_string()))
            }

            // Ex) Given `b"value"[1]`, return `97` (i.e., `ord(b"a")`)
            (Type::LiteralValue(lhs_literal), Type::LiteralValue(rhs_literal)) if lhs_literal.is_bytes() && rhs_literal.is_int() => {
                let literal_ty = lhs_literal.as_bytes().unwrap();
                let i64_int = rhs_literal.as_int().unwrap();
                i32::try_from(i64_int).ok().map(|i32_int| {
                    let literal_value = literal_ty.value(db);
                    match literal_value.py_index(db, i32_int) {
                        Ok(byte) => Ok(Type::int_literal((*byte).into())),
                        Err(_) => Err(SubscriptError::new(
                            Type::unknown(),
                            SubscriptErrorKind::IndexOutOfBounds {
                                kind: SubscriptKind::BytesLiteral,
                                tuple_ty: value_ty,
                                length: literal_value.len().to_string().into(),
                                index: i64_int,
                            },
                            subscripted_type
                        )),
                    }
                })
            }

            // Ex) Given `b"value"[1:3]`, return `b"al"`
            (Type::LiteralValue(literal), Type::NominalInstance(nominal)) if literal.is_bytes() =>
            {
                let literal_ty = literal.as_bytes().unwrap();
                nominal
                .slice_literal(db)
                .map(|SliceLiteral { start, stop, step }| {
                    let literal_value = literal_ty.value(db);

                    match literal_value.py_slice(db, start, stop, step) {
                        Ok(new_bytes) => {
                            let new_bytes = new_bytes.collect::<Vec<u8>>();
                            Ok(Type::bytes_literal(db, &new_bytes))
                        }
                        Err(_) => Err(SubscriptError::new(
                            Type::unknown(),
                            SubscriptErrorKind::SliceStepSizeZero,
                            subscripted_type
                        )),
                    }
                })
            },

            // Ex) Given `"value"[True]`, return `"a"`
            (Type::LiteralValue(lhs_literal), Type::LiteralValue(rhs_literal)) if (lhs_literal.is_string() || lhs_literal.is_bytes()) && rhs_literal.is_bool() => {
                let bool = rhs_literal.as_bool().unwrap();
                Some(value_ty.subscript_impl(db, Type::int_literal(i64::from(bool)), expr_context, subscripted_type))
            }

            (Type::NominalInstance(nominal), Type::LiteralValue(literal))
                if literal.is_bool() && nominal.tuple_spec(db).is_some() =>
            {
                let bool = literal.as_bool().unwrap();
                Some(value_ty.subscript_impl(db, Type::int_literal(i64::from(bool)), expr_context, subscripted_type))
            }

            (Type::KnownInstance(KnownInstanceType::SubscriptedProtocol(_)), _) => {
                // TODO: emit a diagnostic
                Some(Ok(todo_type!("doubly-specialized typing.Protocol")))
            }

            (
                Type::KnownInstance(KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(alias))),
                _,
            ) if alias.generic_context(db).is_none() => {
                debug_assert!(alias.specialization(db).is_none());
                Some(Err(SubscriptError::new(
                    Type::unknown(),
                    SubscriptErrorKind::NonGenericTypeAlias {
                        alias: TypeAliasType::PEP695(alias),
                    },
                    subscripted_type
                )))
            }

            (Type::KnownInstance(KnownInstanceType::SubscriptedGeneric(_)), _) => {
                // TODO: emit a diagnostic
                Some(Ok(todo_type!("doubly-specialized typing.Generic")))
            }

            (Type::SpecialForm(SpecialFormType::Unpack), _) => {
                Some(Ok(Type::Dynamic(DynamicType::TodoUnpack)))
            }

            (Type::SpecialForm(SpecialFormType::TypeQualifier(TypeQualifier::InitVar)), _) => {
                // Subscripting `InitVar` gives you (bizarrely) an instance of `InitVar`,
                // which isn't representable in our model because we don't recognise there as being
                // an `InitVar` class at all. This doesn't really matter that much, so just infer `Any` here.
                Some(Ok(Type::any()))
            }

            (Type::SpecialForm(special_form), _) if special_form.class().is_special_form() => {
                Some(Ok(todo_type!("Inference of subscript on special form")))
            }

            (Type::KnownInstance(known_instance), _) if known_instance.class(db).is_special_form() => {
                Some(Ok(todo_type!("Inference of subscript on special form")))
            }

            (
                Type::FunctionLiteral(_)
                | Type::WrapperDescriptor(_)
                | Type::BoundMethod(_)
                | Type::DataclassDecorator(_)
                | Type::DataclassTransformer(_)
                | Type::Callable(_)
                | Type::ModuleLiteral(_)
                | Type::ClassLiteral(_)
                | Type::GenericAlias(_)
                | Type::SubclassOf(_)
                | Type::AlwaysFalsy
                | Type::AlwaysTruthy
                | Type::ProtocolInstance(_)
                | Type::PropertyInstance(_)
                | Type::BoundSuper(_)
                | Type::TypeIs(_)
                | Type::TypeGuard(_)
                | Type::TypeForm(_)
                | Type::TypedDict(_)
                | Type::NewTypeInstance(_)
                | Type::NominalInstance(_)
                | Type::SpecialForm(_)
                | Type::KnownInstance(_)
                | Type::LiteralValue(_)
                | Type::TypeVar(_)  // TODO: more complex logic required here!
                | Type::KnownBoundMethod(_),
                _,
            ) => None,
        };

        if let Some(inferred) = inferred {
            return inferred;
        }

        // If the class defines `__getitem__`, return its return type.
        //
        // See: https://docs.python.org/3/reference/datamodel.html#class-getitem-versus-getitem
        match value_ty.try_call_dunder(
            db,
            "__getitem__",
            CallArguments::positional([slice_ty]),
            TypeContext::default(),
        ) {
            Ok(outcome) => {
                return Ok(outcome.return_type(db));
            }
            Err(CallDunderError::PossiblyUnbound { bindings, .. }) => {
                return Err(SubscriptError::new(
                    bindings.return_type(db),
                    SubscriptErrorKind::DunderPossiblyUnbound {
                        method: DunderMethod::GetItem,
                        value_ty,
                    },
                    subscripted_type,
                ));
            }
            Err(CallDunderError::CallError(call_error_kind, bindings, provenance)) => {
                return Err(SubscriptError::new(
                    bindings.return_type(db),
                    SubscriptErrorKind::DunderCallError {
                        method: DunderMethod::GetItem,
                        value_ty,
                        slice_ty,
                        kind: call_error_kind,
                        bindings,
                        provenance,
                    },
                    subscripted_type,
                ));
            }
            Err(CallDunderError::MethodNotAvailable) => {
                // try `__class_getitem__`
            }
        }

        // Otherwise, if the value is itself a class and defines `__class_getitem__`,
        // return its return type.
        //
        // TODO: lots of classes are only subscriptable at runtime on Python 3.9+,
        // *but* we should also allow them to be subscripted in stubs
        // (and in annotations if `from __future__ import annotations` is enabled),
        // even if the target version is Python 3.8 or lower,
        // despite the fact that there will be no corresponding `__class_getitem__`
        // method in these `sys.version_info` branches.
        if value_ty.is_subtype_of(db, KnownClass::Type.to_instance(db)) {
            let call_arguments = CallArguments::positional([slice_ty]);
            match value_ty.try_call_dunder_on_class(
                db,
                "__class_getitem__",
                &call_arguments,
                TypeContext::default(),
            ) {
                Ok(bindings) => {
                    return Ok(bindings.return_type(db));
                }
                Err(CallDunderError::PossiblyUnbound { bindings, .. }) => {
                    return Err(SubscriptError::new(
                        bindings.return_type(db),
                        SubscriptErrorKind::DunderPossiblyUnbound {
                            method: DunderMethod::ClassGetItem,
                            value_ty,
                        },
                        subscripted_type,
                    ));
                }
                Err(CallDunderError::CallError(call_error_kind, bindings, provenance)) => {
                    return Err(SubscriptError::new(
                        bindings.return_type(db),
                        SubscriptErrorKind::DunderCallError {
                            method: DunderMethod::ClassGetItem,
                            value_ty,
                            slice_ty,
                            kind: call_error_kind,
                            bindings,
                            provenance,
                        },
                        subscripted_type,
                    ));
                }
                Err(CallDunderError::MethodNotAvailable) => {
                    // Fall through to the logic below
                }
            }

            if let Type::ClassLiteral(class) = value_ty {
                if class.is_known(db, KnownClass::Type) {
                    return Ok(KnownClass::GenericAlias.to_instance(db));
                }

                if class.generic_context(db).is_some() {
                    // TODO: specialize the generic class using these explicit type
                    // variable assignments. This branch is only encountered when an
                    // explicit class specialization appears inside of some other subscript
                    // expression, e.g. `tuple[list[int], ...]`. We have already inferred
                    // the type of the outer subscript slice as a value expression, which
                    // means we can't re-infer the inner specialization here as a type
                    // expression.
                    return Ok(value_ty);
                }
            }

            // TODO: properly handle old-style generics; get rid of this temporary hack
            if !value_ty
                .as_class_literal()
                .is_some_and(|class| class.iter_mro(db).contains(&ClassBase::Generic))
            {
                return Err(SubscriptError::new(
                    Type::unknown(),
                    SubscriptErrorKind::NotSubscriptable {
                        value_ty,
                        method: DunderMethod::ClassGetItem,
                    },
                    subscripted_type,
                ));
            }
        } else if expr_context != ast::ExprContext::Store {
            return Err(SubscriptError::new(
                Type::unknown(),
                SubscriptErrorKind::NotSubscriptable {
                    value_ty,
                    method: DunderMethod::GetItem,
                },
                subscripted_type,
            ));
        }

        Ok(Type::unknown())
    }
}
