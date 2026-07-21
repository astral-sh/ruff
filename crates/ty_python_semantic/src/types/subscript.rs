//! Inference for subscript expressions (e.g., `x[0]`, `list[int]`).

use crate::SemanticContext;
use std::fmt::{self, Display};

use compact_str::{CompactString, ToCompactString};
use itertools::Itertools;
use ruff_python_ast as ast;

use crate::subscript::{PyIndex, PySlice};
use crate::types::special_form::TypeQualifier;

use super::call::{Bindings, CallArguments, CallDunderError, CallErrorKind};
use super::class::KnownClass;
use super::class_base::ClassBase;
use super::context::InferContext;
use super::diagnostic::{
    CALL_NON_CALLABLE, INVALID_ARGUMENT_TYPE, INVALID_GENERIC_CLASS, NOT_SUBSCRIPTABLE,
    POSSIBLY_MISSING_IMPLICIT_CALL, report_index_out_of_bounds, report_invalid_key_on_typed_dict,
    report_not_subscriptable, report_slice_step_size_zero,
};
use super::infer::TypeContext;
use super::instance::SliceLiteral;
use super::special_form::SpecialFormType;
use super::{
    IntersectionBuilder, IntersectionType, KnownInstanceType, Type, TypeAliasType, TypedDictType,
    UnionBuilder, UnionType, todo_type,
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

#[derive(Debug)]
pub(crate) struct SubscriptError<'db> {
    result_ty: Type<'db>,
    errors: Vec<SubscriptErrorKind<'db>>,
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
    /// More than one `TypeVarTuple` was provided to `Generic` or `Protocol`.
    MultipleTypeVarTuples { origin: LegacyGenericOrigin },
}

impl<'db> SubscriptError<'db> {
    pub(crate) fn new(result_ty: Type<'db>, error: SubscriptErrorKind<'db>) -> Self {
        Self {
            result_ty,
            errors: vec![error],
        }
    }

    fn with_errors(result_ty: Type<'db>, errors: Vec<SubscriptErrorKind<'db>>) -> Self {
        Self { result_ty, errors }
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
            error.report_diagnostic(context, subscript, value_node, slice_node);
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
    ) {
        let db = context.db();
        let ctx = context.semantic_context();
        match self {
            Self::IndexOutOfBounds {
                kind,
                tuple_ty,
                length,
                index,
            } => {
                report_index_out_of_bounds(
                    context,
                    kind.as_str(),
                    value_node.into(),
                    *tuple_ty,
                    length,
                    *index,
                );
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
                                value_type.display(ctx)
                            ),
                        ));
                    }
                }
            }
            Self::DunderPossiblyUnbound { method, value_ty } => {
                if let Some(builder) =
                    context.report_lint(&POSSIBLY_MISSING_IMPLICIT_CALL, value_node)
                {
                    builder.into_diagnostic(format_args!(
                        "Method `{method}` of type `{}` may be missing",
                        value_ty.display(ctx),
                    ));
                }
            }
            Self::DunderCallError {
                method,
                value_ty,
                slice_ty,
                kind,
                bindings,
            } => match kind {
                CallErrorKind::NotCallable => {
                    if let Some(builder) = context.report_lint(&CALL_NON_CALLABLE, value_node) {
                        builder.into_diagnostic(format_args!(
                            "Method `{method}` of type `{}` is not callable \
                            on object of type `{}`",
                            bindings.callable_type().display(ctx),
                            value_ty.display(ctx),
                        ));
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
                    } else if let Some(builder) =
                        context.report_lint(&INVALID_ARGUMENT_TYPE, value_node)
                    {
                        builder.into_diagnostic(format_args!(
                            "Method `{method}` of type `{}` cannot be called \
                            with key of type `{}` on object of type `{}`",
                            bindings.callable_type().display(ctx),
                            slice_ty.display(ctx),
                            value_ty.display(ctx),
                        ));
                    }
                }
                CallErrorKind::PossiblyNotCallable => {
                    if let Some(builder) = context.report_lint(&CALL_NON_CALLABLE, value_node) {
                        builder.into_diagnostic(format_args!(
                            "Method `{method}` of type `{}` may not be callable \
                            on object of type `{}`",
                            bindings.callable_type().display(ctx),
                            value_ty.display(ctx),
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
                if let Some(builder) = context.report_lint(&INVALID_ARGUMENT_TYPE, value_node) {
                    builder.into_diagnostic(format_args!(
                        "`{}` is not a valid argument to `{origin}`",
                        argument_ty.display(ctx),
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
            Self::MultipleTypeVarTuples { origin } => {
                if let Some(builder) = context.report_lint(&INVALID_GENERIC_CLASS, subscript) {
                    builder.into_diagnostic(format_args!(
                        "Only one `TypeVarTuple` parameter is allowed \
                        in a `{origin}` subscription",
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
    ctx: &SemanticContext<'db>,
    union: UnionType<'db>,
    mut map_fn: F,
) -> Result<Type<'db>, SubscriptError<'db>>
where
    F: FnMut(Type<'db>) -> Result<Type<'db>, SubscriptError<'db>>,
{
    let db = ctx.db();
    let mut builder = UnionBuilder::new(ctx);
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
        Err(SubscriptError::with_errors(result_ty, errors))
    }
}

fn map_intersection_subscript<'db, F>(
    ctx: &SemanticContext<'db>,
    intersection: IntersectionType<'db>,
    mut map_fn: F,
) -> Result<Type<'db>, SubscriptError<'db>>
where
    F: FnMut(Type<'db>) -> Result<Type<'db>, SubscriptError<'db>>,
{
    let db = ctx.db();
    if let Some(alternatives) = intersection.finite_alternative_union(ctx) {
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
        let mut builder = IntersectionBuilder::new(ctx);
        for result in results {
            builder.add_positive_in_place(result);
        }
        return Ok(builder.build());
    }

    // All elements failed. Check if any element has the method available
    // (even if the call failed). If so, filter out `NotSubscriptable` errors
    // for elements that lack the method.
    let any_has_method = errors.iter().any(SubscriptError::any_method_available);

    let mut builder = IntersectionBuilder::new(ctx);
    let mut collected_errors = Vec::new();
    let full_object_ty = Type::Intersection(intersection);

    for error in errors {
        if !any_has_method || error.any_method_available() {
            builder.add_positive_in_place(error.result_type());
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
    ))
}

// `TypedDict` subscripts need custom handling because invalid keys should emit `invalid-key` while
// recovering with the union of value types for non-literal string keys on closed `TypedDict`s and
// `Unknown` otherwise. This is not naturally representable via synthesized `__getitem__` overloads.
fn typed_dict_subscript<'db>(
    ctx: &SemanticContext<'db>,
    typed_dict: TypedDictType<'db>,
    slice_ty: Type<'db>,
) -> Result<Type<'db>, SubscriptError<'db>> {
    let db = ctx.db();
    if let Some(fallback) = slice_ty.materialized_divergent_fallback() {
        return typed_dict_subscript(ctx, typed_dict, fallback);
    }

    if slice_ty.is_dynamic() {
        return Ok(Type::unknown());
    }

    let Some(key) = slice_ty
        .as_string_literal()
        .map(|literal| literal.value(db))
    else {
        if typed_dict.explicit_extra_items(db).is_some()
            && slice_ty.is_assignable_to(ctx, KnownClass::Str.to_instance(ctx))
        {
            return Ok(typed_dict.value_type(ctx));
        }
        let result_ty = if typed_dict.openness(db).is_closed()
            && slice_ty.is_assignable_to(ctx, KnownClass::Str.to_instance(ctx))
        {
            typed_dict.value_type(ctx)
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
            ))
        },
        |field| Ok(field.declared_ty),
    )
}

impl<'db> Type<'db> {
    pub(super) fn subscript(
        self,
        ctx: &SemanticContext<'db>,
        slice_ty: Type<'db>,
        expr_context: ast::ExprContext,
    ) -> Result<Type<'db>, SubscriptError<'db>> {
        let db = ctx.db();
        if let Some(fallback) = self.materialized_divergent_fallback() {
            return fallback.subscript(ctx, slice_ty, expr_context);
        }

        if let Some(fallback) = slice_ty.materialized_divergent_fallback() {
            return self.subscript(ctx, fallback, expr_context);
        }

        let value_ty = self;

        let inferred = match (value_ty, slice_ty) {
            (Type::Dynamic(_) | Type::Divergent(_) | Type::Never, _) => Some(Ok(value_ty)),

            (Type::TypeAlias(alias), _) => {
                Some(alias.value_type(ctx).subscript(ctx, slice_ty, expr_context))
            }

            (_, Type::TypeAlias(alias)) => {
                Some(value_ty.subscript(ctx, alias.value_type(ctx), expr_context))
            }

            (Type::Union(union), _) => Some(map_union_subscript(ctx, union, |element| {
                element.subscript(ctx, slice_ty, expr_context)
            })),

            (_, Type::Union(union)) => Some(map_union_subscript(ctx, union, |element| {
                value_ty.subscript(ctx, element, expr_context)
            })),

            (Type::EnumComplement(complement), _) => Some(
                complement
                    .remaining_literal_union(ctx)
                    .subscript(ctx, slice_ty, expr_context),
            ),

            (_, Type::EnumComplement(complement)) => {
                Some(value_ty.subscript(ctx, complement.remaining_literal_union(ctx), expr_context))
            }

            (Type::Intersection(intersection), _) => {
                Some(map_intersection_subscript(ctx, intersection, |element| {
                    element.subscript(ctx, slice_ty, expr_context)
                }))
            }

            (_, Type::Intersection(intersection)) => {
                Some(map_intersection_subscript(ctx, intersection, |element| {
                    value_ty.subscript(ctx, element, expr_context)
                }))
            }

            // Ex) Given `person["name"]`, return `str`
            (Type::TypedDict(typed_dict), _) if expr_context != ast::ExprContext::Store => {
                Some(typed_dict_subscript(ctx, typed_dict, slice_ty))
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
            ) && let Some(SliceLiteral { step: Some(0), .. }) =
                maybe_slice_nominal.slice_literal(db) =>
            {
                Some(Err(SubscriptError::new(
                    value_ty,
                    SubscriptErrorKind::SliceStepSizeZero,
                )))
            }

            // Ex) Given `("a", "b", "c", "d")[1]`, return `"b"`
            (Type::NominalInstance(nominal), Type::LiteralValue(literal))
                if let Some(i64_int) = literal.as_int()
                    && let Some(tuple) = nominal.tuple_spec(ctx)
                    && let Ok(i32_int) = i32::try_from(i64_int) =>
            {
                let result = tuple.py_index(ctx, i32_int).map_err(|_| {
                    SubscriptError::new(
                        Type::unknown(),
                        SubscriptErrorKind::IndexOutOfBounds {
                            kind: SubscriptKind::Tuple,
                            tuple_ty: value_ty,
                            length: tuple.len().display_minimum().into(),
                            index: i64_int,
                        },
                    )
                });

                Some(result)
            }

            // Ex) Given `("a", 1, Null)[0:2]`, return `("a", 1)`
            (
                Type::NominalInstance(maybe_tuple_nominal),
                Type::NominalInstance(maybe_slice_nominal),
            ) if let Some(tuple) = maybe_tuple_nominal.tuple_spec(ctx)
                && let Some(SliceLiteral { start, stop, step }) =
                    maybe_slice_nominal.slice_literal(db) =>
            {
                Some(tuple.py_slice_type(ctx, start, stop, step).map_err(|_| {
                    SubscriptError::new(Type::unknown(), SubscriptErrorKind::SliceStepSizeZero)
                }))
            }

            // Ex) Given `"value"[1]`, return `"a"`
            (Type::LiteralValue(lhs_literal), Type::LiteralValue(rhs_literal))
                if let Some(literal_ty) = lhs_literal.as_string()
                    && let Some(i64_int) = rhs_literal.as_int()
                    && let Ok(i32_int) = i32::try_from(i64_int) =>
            {
                let literal_value = literal_ty.value(db);

                let result = match (&mut literal_value.chars()).py_index(ctx, i32_int) {
                    Ok(ch) => Ok(Type::string_literal(db, ch.to_compact_string())),
                    Err(_) => Err(SubscriptError::new(
                        Type::unknown(),
                        SubscriptErrorKind::IndexOutOfBounds {
                            kind: SubscriptKind::String,
                            tuple_ty: value_ty,
                            length: literal_value.chars().count().to_string().into(),
                            index: i64_int,
                        },
                    )),
                };

                Some(result)
            }

            // Ex) Given `"value"[1:3]`, return `"al"`
            (Type::LiteralValue(literal), Type::NominalInstance(nominal))
                if let Some(literal_ty) = literal.as_string()
                    && let Some(SliceLiteral { start, stop, step }) = nominal.slice_literal(db) =>
            {
                let literal_value = literal_ty.value(db);
                let chars: Vec<_> = literal_value.chars().collect();

                let result = match chars.py_slice(db, start, stop, step) {
                    Ok(new_chars) => {
                        let literal = new_chars.collect::<CompactString>();
                        Ok(Type::string_literal(db, literal))
                    }
                    Err(_) => Err(SubscriptError::new(
                        Type::unknown(),
                        SubscriptErrorKind::SliceStepSizeZero,
                    )),
                };

                Some(result)
            }

            (Type::LiteralValue(lhs_literal), Type::LiteralValue(rhs_literal))
                if lhs_literal.is_literal_string()
                    && (rhs_literal.is_int() || rhs_literal.is_bool()) =>
            {
                Some(Ok(Type::literal_string()))
            }

            (Type::LiteralValue(literal), Type::NominalInstance(nominal))
                if literal.is_literal_string() && nominal.slice_literal(db).is_some() =>
            {
                Some(Ok(Type::literal_string()))
            }

            // Ex) Given `b"value"[1]`, return `97` (i.e., `ord(b"a")`)
            (Type::LiteralValue(lhs_literal), Type::LiteralValue(rhs_literal))
                if let Some(literal_ty) = lhs_literal.as_bytes()
                    && let Some(i64_int) = rhs_literal.as_int()
                    && let Ok(i32_int) = i32::try_from(i64_int) =>
            {
                let literal_value = literal_ty.value(db);

                let result = match literal_value.py_index(ctx, i32_int) {
                    Ok(byte) => Ok(Type::int_literal((*byte).into())),
                    Err(_) => Err(SubscriptError::new(
                        Type::unknown(),
                        SubscriptErrorKind::IndexOutOfBounds {
                            kind: SubscriptKind::BytesLiteral,
                            tuple_ty: value_ty,
                            length: literal_value.len().to_string().into(),
                            index: i64_int,
                        },
                    )),
                };

                Some(result)
            }

            // Ex) Given `b"value"[1:3]`, return `b"al"`
            (Type::LiteralValue(literal), Type::NominalInstance(nominal))
                if let Some(literal_ty) = literal.as_bytes()
                    && let Some(SliceLiteral { start, stop, step }) = nominal.slice_literal(db) =>
            {
                let literal_value = literal_ty.value(db);

                let result = match literal_value.py_slice(db, start, stop, step) {
                    Ok(new_bytes) => {
                        let new_bytes = new_bytes.collect::<Vec<u8>>();
                        Ok(Type::bytes_literal(db, &new_bytes))
                    }
                    Err(_) => Err(SubscriptError::new(
                        Type::unknown(),
                        SubscriptErrorKind::SliceStepSizeZero,
                    )),
                };

                Some(result)
            }

            // Ex) Given `"value"[True]`, return `"a"`
            (Type::LiteralValue(lhs_literal), Type::LiteralValue(rhs_literal))
                if (lhs_literal.is_string() || lhs_literal.is_bytes())
                    && let Some(bool) = rhs_literal.as_bool() =>
            {
                Some(value_ty.subscript(ctx, Type::int_literal(i64::from(bool)), expr_context))
            }

            (Type::NominalInstance(nominal), Type::LiteralValue(literal))
                if let Some(bool) = literal.as_bool()
                    && nominal.tuple_spec(ctx).is_some() =>
            {
                Some(value_ty.subscript(ctx, Type::int_literal(i64::from(bool)), expr_context))
            }

            (Type::KnownInstance(KnownInstanceType::SubscriptedProtocol(_)), _) => {
                // TODO: emit a diagnostic
                Some(Ok(todo_type!("doubly-specialized typing.Protocol")))
            }

            (Type::KnownInstance(KnownInstanceType::TypeAliasType(alias)), _)
                if alias.generic_context(db).is_none() =>
            {
                debug_assert!(alias.specialization(db).is_none());
                Some(Err(SubscriptError::new(
                    Type::unknown(),
                    SubscriptErrorKind::NonGenericTypeAlias { alias },
                )))
            }

            (Type::KnownInstance(KnownInstanceType::SubscriptedGeneric(_)), _) => {
                // TODO: emit a diagnostic
                Some(Ok(todo_type!("doubly-specialized typing.Generic")))
            }

            (Type::SpecialForm(SpecialFormType::Unpack), _) => {
                // TODO: Emit an invalid-type-form diagnostic for runtime subscripting of `Unpack`.
                Some(Ok(Type::unknown()))
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

            (Type::KnownInstance(known_instance), _)
                if known_instance.class(db).is_special_form() =>
            {
                Some(Ok(todo_type!("Inference of subscript on special form")))
            }

            // TODO: more complex logic required for the `Type::TypeVar(_) branch!
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
                | Type::TypeVar(_)
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
            ctx,
            "__getitem__",
            CallArguments::positional([slice_ty]),
            TypeContext::default(),
        ) {
            Ok(outcome) => {
                return Ok(outcome.return_type(ctx));
            }
            Err(CallDunderError::PossiblyUnbound { bindings, .. }) => {
                return Err(SubscriptError::new(
                    bindings.return_type(ctx),
                    SubscriptErrorKind::DunderPossiblyUnbound {
                        method: DunderMethod::GetItem,
                        value_ty,
                    },
                ));
            }
            Err(CallDunderError::CallError(call_error_kind, bindings, _)) => {
                return Err(SubscriptError::new(
                    bindings.return_type(ctx),
                    SubscriptErrorKind::DunderCallError {
                        method: DunderMethod::GetItem,
                        value_ty,
                        slice_ty,
                        kind: call_error_kind,
                        bindings,
                    },
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
        if value_ty.is_subtype_of(ctx, KnownClass::Type.to_instance(ctx)) {
            let call_arguments = CallArguments::positional([slice_ty]);
            match value_ty.try_call_dunder_on_class(
                ctx,
                "__class_getitem__",
                &call_arguments,
                TypeContext::default(),
            ) {
                Ok(bindings) => {
                    return Ok(bindings.return_type(ctx));
                }
                Err(CallDunderError::PossiblyUnbound { bindings, .. }) => {
                    return Err(SubscriptError::new(
                        bindings.return_type(ctx),
                        SubscriptErrorKind::DunderPossiblyUnbound {
                            method: DunderMethod::ClassGetItem,
                            value_ty,
                        },
                    ));
                }
                Err(CallDunderError::CallError(call_error_kind, bindings, _)) => {
                    return Err(SubscriptError::new(
                        bindings.return_type(ctx),
                        SubscriptErrorKind::DunderCallError {
                            method: DunderMethod::ClassGetItem,
                            value_ty,
                            slice_ty,
                            kind: call_error_kind,
                            bindings,
                        },
                    ));
                }
                Err(CallDunderError::MethodNotAvailable) => {
                    // Fall through to the logic below
                }
            }

            if let Type::ClassLiteral(class) = value_ty {
                if class.is_known(db, KnownClass::Type) {
                    return Ok(KnownClass::GenericAlias.to_instance(ctx));
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
                .is_some_and(|class| class.iter_mro(ctx).contains(&ClassBase::Generic))
            {
                return Err(SubscriptError::new(
                    Type::unknown(),
                    SubscriptErrorKind::NotSubscriptable {
                        value_ty,
                        method: DunderMethod::ClassGetItem,
                    },
                ));
            }
        } else if expr_context != ast::ExprContext::Store {
            return Err(SubscriptError::new(
                Type::unknown(),
                SubscriptErrorKind::NotSubscriptable {
                    value_ty,
                    method: DunderMethod::GetItem,
                },
            ));
        }

        Ok(Type::unknown())
    }
}
