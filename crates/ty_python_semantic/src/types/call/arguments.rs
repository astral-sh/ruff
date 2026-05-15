use std::borrow::Cow;
use std::fmt::Display;

use itertools::{Either, Itertools};
use ruff_python_ast as ast;
use rustc_hash::FxHashMap;

use crate::Db;
use crate::types::enums::{enum_member_literals, enum_metadata};
use crate::types::tuple::Tuple;
use crate::types::typed_dict::extract_unpacked_typed_dict_keys_from_value_type;
use crate::types::{KnownClass, Type, TypeContext};

/// Maximum number of expanded types that can be generated from a single tuple's
/// Cartesian product in [`expand_type`].
///
/// See: [pyright's `maxSingleOverloadArgTypeExpansionCount`][pyright]
///
/// [pyright]: https://github.com/microsoft/pyright/blob/5a325e4874e775436671eed65ad696787a1ef74b/packages/pyright-internal/src/analyzer/typeEvaluator.ts#L570
const MAX_TUPLE_EXPANSION: usize = 64;

/// Maximum total number of expanded argument type combinations across all arguments
/// in [`CallArguments::expand`].
///
/// See: [pyright's `maxTotalOverloadArgTypeExpansionCount`][pyright]
///
/// [pyright]: https://github.com/microsoft/pyright/blob/5a325e4874e775436671eed65ad696787a1ef74b/packages/pyright-internal/src/analyzer/typeEvaluator.ts#L566
const MAX_TOTAL_EXPANSION: usize = 256;

#[derive(Clone, Copy, Debug)]
pub(crate) enum Argument<'a> {
    /// The synthetic `self` or `cls` argument, which doesn't appear explicitly at the call site.
    Synthetic,
    /// A positional argument.
    Positional,
    /// A starred positional argument (e.g. `*args`) containing the specified number of elements.
    Variadic,
    /// A keyword argument (e.g. `a=1`).
    Keyword(&'a str),
    /// The double-starred keywords argument (e.g. `**kwargs`).
    Keywords,
}

/// Arguments for a single call, in source order, along with inferred types for each argument.
#[derive(Clone, Debug, Default)]
pub(crate) struct CallArguments<'a, 'db> {
    items: Vec<CallArgument<'a, 'db>>,
}

#[derive(Clone, Debug)]
struct CallArgument<'a, 'db> {
    argument: Argument<'a>,
    types: CallArgumentTypes<'db>,
}

/// Inferred types for a given argument.
///
/// Note that a single argument may produce multiple distinct inferred types when inferred
/// with type context across multiple bindings.
#[derive(Clone, Debug, Default)]
pub(crate) struct CallArgumentTypes<'db> {
    fallback_type: Option<Type<'db>>,
    types: FxHashMap<Type<'db>, Type<'db>>,
}

impl<'db> CallArgumentTypes<'db> {
    pub(crate) fn new(fallback_ty: Option<Type<'db>>) -> Self {
        Self {
            fallback_type: fallback_ty,
            types: FxHashMap::default(),
        }
    }

    /// Returns the most appropriate type of this argument when there is no specific declared type.
    pub(crate) fn get_default(&self) -> Option<Type<'db>> {
        // If this type was inferred against exactly one declared type, or was inferred against
        // multiple, but resulted in a single inferred type, we have an exact type to return.
        if let Ok(exact_ty) = self
            .types
            .values()
            .exactly_one()
            .or_else(|_| self.types.values().all_equal_value())
        {
            return Some(*exact_ty);
        }

        self.fallback_type
    }

    /// Returns the type of this argument when inferred against the provided declared type.
    pub(crate) fn get_for_declared_type(&self, tcx: Type<'db>) -> Type<'db> {
        self.types
            .get(&tcx)
            .copied()
            .or_else(|| self.get_default())
            .unwrap_or(Type::unknown())
    }

    /// Insert the type of this argument when inferred with the provided type context.
    pub(crate) fn insert(&mut self, tcx: impl Into<TypeContext<'db>>, ty: Type<'db>) {
        match tcx.into().annotation {
            None => self.fallback_type = Some(ty),
            Some(tcx) => {
                self.types.insert(tcx, ty);
            }
        }
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (TypeContext<'db>, Type<'db>)> {
        self.types
            .iter()
            .map(|(tcx, ty)| (TypeContext::new(Some(*tcx)), *ty))
            .chain(self.fallback_type.map(|ty| (TypeContext::default(), ty)))
    }
}

impl<'a, 'db> CallArguments<'a, 'db> {
    /// Create `CallArguments` from AST arguments. We will use the provided callback to obtain the
    /// type of each splatted argument, so that we can determine its length. All other arguments
    /// will remain uninitialized as `Unknown`.
    pub(crate) fn from_arguments(
        arguments: &'a ast::Arguments,
        mut infer_argument_type: impl FnMut(&ast::ArgOrKeyword, &ast::Expr) -> Type<'db>,
    ) -> Self {
        let mut call_arguments = Self {
            items: Vec::with_capacity(arguments.len()),
        };

        for arg_or_keyword in arguments.iter_source_order() {
            let (argument, ty) = match arg_or_keyword {
                ast::ArgOrKeyword::Arg(arg) => match arg {
                    ast::Expr::Starred(ast::ExprStarred { value, .. }) => {
                        let ty = infer_argument_type(&arg_or_keyword, value);
                        (Argument::Variadic, Some(ty))
                    }
                    _ => (Argument::Positional, None),
                },
                ast::ArgOrKeyword::Keyword(ast::Keyword { arg, value, .. }) => {
                    if let Some(arg) = arg {
                        (Argument::Keyword(&arg.id), None)
                    } else {
                        let ty = infer_argument_type(&arg_or_keyword, value);
                        (Argument::Keywords, Some(ty))
                    }
                }
            };
            call_arguments.items.push(CallArgument {
                argument,
                types: CallArgumentTypes::new(ty),
            });
        }

        call_arguments
    }

    /// Like [`Self::from_arguments`] but fills as much typing info in as possible.
    ///
    /// This currently only exists for the LSP usecase, and shouldn't be used in normal
    /// typechecking.
    pub(crate) fn from_arguments_typed(
        arguments: &'a ast::Arguments,
        mut infer_argument_type: impl FnMut(&ast::Expr) -> Type<'db>,
    ) -> Self {
        arguments
            .iter_source_order()
            .map(|arg_or_keyword| match arg_or_keyword {
                ast::ArgOrKeyword::Arg(arg) => match arg {
                    ast::Expr::Starred(ast::ExprStarred { value, .. }) => {
                        let ty = infer_argument_type(value);
                        (Argument::Variadic, Some(ty))
                    }
                    _ => {
                        let ty = infer_argument_type(arg);
                        (Argument::Positional, Some(ty))
                    }
                },
                ast::ArgOrKeyword::Keyword(ast::Keyword { arg, value, .. }) => {
                    let ty = infer_argument_type(value);
                    if let Some(arg) = arg {
                        (Argument::Keyword(&arg.id), Some(ty))
                    } else {
                        (Argument::Keywords, Some(ty))
                    }
                }
            })
            .collect()
    }

    /// Create a [`CallArguments`] with no arguments.
    pub(crate) fn none() -> Self {
        Self::default()
    }

    /// Create a [`CallArguments`] from an iterator over non-variadic positional argument types.
    pub(crate) fn positional(positional_tys: impl IntoIterator<Item = Type<'db>>) -> Self {
        positional_tys
            .into_iter()
            .map(|ty| (Argument::Positional, Some(ty)))
            .collect()
    }

    pub(crate) fn len(&self) -> usize {
        self.items.len()
    }

    pub(crate) fn argument_types(&self, index: usize) -> Option<&CallArgumentTypes<'db>> {
        self.items.get(index).map(|item| &item.types)
    }

    pub(crate) fn insert_type(
        &mut self,
        index: usize,
        tcx: impl Into<TypeContext<'db>>,
        ty: Type<'db>,
    ) {
        self.items
            .get_mut(index)
            .expect("argument index should be valid")
            .types
            .insert(tcx, ty);
    }

    pub(crate) fn iter_types(&self) -> impl Iterator<Item = &CallArgumentTypes<'db>> + '_ {
        self.items.iter().map(|item| &item.types)
    }

    /// Prepend an optional extra synthetic argument (for a `self` or `cls` parameter) to the front
    /// of this argument list. (If `bound_self` is none, we return the argument list
    /// unmodified.)
    pub(crate) fn with_self(&self, bound_self: Option<Type<'db>>) -> Cow<'_, Self> {
        if bound_self.is_some() {
            let mut items = Vec::with_capacity(self.items.len() + 1);
            items.push(CallArgument {
                argument: Argument::Synthetic,
                types: CallArgumentTypes::new(bound_self),
            });
            items.extend(self.items.iter().cloned());
            Cow::Owned(CallArguments { items })
        } else {
            Cow::Borrowed(self)
        }
    }

    pub(crate) fn iter(
        &self,
    ) -> impl Iterator<Item = (Argument<'a>, &CallArgumentTypes<'db>)> + '_ {
        self.items.iter().map(|item| (item.argument, &item.types))
    }

    pub(crate) fn iter_mut(
        &mut self,
    ) -> impl Iterator<Item = (Argument<'a>, &mut CallArgumentTypes<'db>)> + '_ {
        self.items
            .iter_mut()
            .map(|item| (item.argument, &mut item.types))
    }

    /// Create a new [`CallArguments`] starting from the specified index.
    pub(crate) fn start_from(&self, index: usize) -> Self {
        Self {
            items: self.items[index..].to_vec(),
        }
    }

    /// Create a new [`CallArguments`] containing only the arguments at the specified indices.
    ///
    /// The resulting argument list preserves the order of `indices`. Unlike [`Self::start_from`],
    /// this can project a non-contiguous subset of the original call arguments. This is used to
    /// turn the forwarded outer arguments into the argument list for a synthetic sub-call:
    ///
    /// ```py
    /// def wrapper[**P, R](func: Callable[P, R], **kwargs: P.kwargs) -> R: ...
    /// wrapper(TagSet=[...], func=f)  # select `TagSet=[...]`, but not the later `func=f`
    /// ```
    pub(crate) fn select(&self, indices: &[usize]) -> Self {
        Self {
            items: indices
                .iter()
                .map(|index| self.items[*index].clone())
                .collect(),
        }
    }

    /// Returns the `functools.partial(...)` bound-argument slice when argument expansion is
    /// concrete enough for partial-application analysis.
    pub(crate) fn functools_partial_bound_arguments(&self, db: &'db dyn Db) -> Option<Self> {
        let bound_call_arguments = self.start_from(1);

        // We only handle variadics and keyword-maps that can be normalized to concrete argument
        // positions for overload matching.
        if bound_call_arguments.iter().any(|(argument, argument_ty)| {
            let argument_ty = argument_ty.get_default().unwrap_or_else(Type::unknown);
            match argument {
                Argument::Variadic => !matches!(
                    argument_ty
                        .as_nominal_instance()
                        .and_then(|nominal| nominal.tuple_spec(db)),
                    Some(spec) if spec.as_fixed_length().is_some()
                ),
                // Optional TypedDict keys may be absent at runtime, so we can only refine
                // `partial(...)` when every expanded key is guaranteed to be present.
                Argument::Keywords => {
                    extract_unpacked_typed_dict_keys_from_value_type(db, argument_ty).is_none_or(
                        |unpacked_keys| unpacked_keys.values().any(|key| !key.is_required),
                    )
                }
                Argument::Positional | Argument::Synthetic | Argument::Keyword(_) => false,
            }
        }) {
            return None;
        }

        Some(bound_call_arguments)
    }

    /// Returns an iterator on performing [argument type expansion].
    ///
    /// Each element of the iterator represents a set of argument lists, where each argument list
    /// contains the same arguments, but with one or more of the argument types expanded.
    ///
    /// [argument type expansion]: https://typing.python.org/en/latest/spec/overload.html#argument-type-expansion
    pub(super) fn expand(&self, db: &'db dyn Db) -> impl Iterator<Item = Expansion<'a, 'db>> + '_ {
        /// Represents the state of the expansion process.
        enum State<'a, 'db> {
            LimitReached(usize),
            Expanding(ExpandingState<'a, 'db>),
        }

        /// Represents the expanding state with either the initial types or the expanded types.
        ///
        /// This is useful to avoid cloning the initial types vector if none of the types can be
        /// expanded.
        enum ExpandingState<'a, 'db> {
            Initial,
            Expanded(Vec<CallArguments<'a, 'db>>),
        }

        impl<'a, 'db> ExpandingState<'a, 'db> {
            fn len(&self) -> usize {
                match self {
                    ExpandingState::Initial => 1,
                    ExpandingState::Expanded(expanded) => expanded.len(),
                }
            }

            fn iter<'s>(
                &'s self,
                initial: &'s CallArguments<'a, 'db>,
            ) -> impl Iterator<Item = &'s CallArguments<'a, 'db>> {
                match self {
                    ExpandingState::Initial => Either::Left(std::iter::once(initial)),
                    ExpandingState::Expanded(expanded) => Either::Right(expanded.iter()),
                }
            }
        }

        let mut index = 0;

        std::iter::successors(
            Some(State::Expanding(ExpandingState::Initial)),
            move |previous| {
                let state = match previous {
                    State::LimitReached(index) => return Some(State::LimitReached(*index)),
                    State::Expanding(expanding_state) => expanding_state,
                };

                // Find the next type that can be expanded.
                let expanded_types = loop {
                    let arg_type = self.argument_types(index)?;
                    // TODO: For types inferred multiple times with distinct type context, we currently only
                    // expand the default inference. Note that direct expansion of a type inferred against a
                    // given declared type would not likely be assignable to other declared types without
                    // re-inference, and so a more complete implementation would likely have to re-infer the
                    // argument type against the union a given subset of type contexts before expansion. However,
                    // this only shows up in very convoluted instances of generic call inference across multiple
                    // overloads, and is unlikely to happen in practice.
                    if let Some(arg_type) = arg_type.get_default()
                        && let Some(expanded_types) = expand_type(db, arg_type)
                    {
                        break expanded_types;
                    }
                    index += 1;
                };

                let expansion_size = expanded_types.len() * state.len();
                if expansion_size > MAX_TOTAL_EXPANSION {
                    tracing::debug!(
                        "Skipping argument type expansion as it would exceed the \
                            maximum number of expansions ({MAX_TOTAL_EXPANSION})"
                    );
                    return Some(State::LimitReached(index));
                }

                let mut expanded_arguments = Vec::with_capacity(expansion_size);

                for pre_expanded_types in state.iter(self) {
                    for subtype in &expanded_types {
                        let mut expanded_argument = pre_expanded_types.clone();
                        expanded_argument.items[index].types =
                            CallArgumentTypes::new(Some(*subtype));
                        expanded_arguments.push(expanded_argument);
                    }
                }

                // Increment the index to move to the next argument type for the next iteration.
                index += 1;

                Some(State::Expanding(ExpandingState::Expanded(
                    expanded_arguments,
                )))
            },
        )
        .skip(1) // Skip the initial state, which has no expanded types.
        .map(|state| match state {
            State::LimitReached(index) => Expansion::LimitReached(index),
            State::Expanding(ExpandingState::Initial) => {
                unreachable!("initial state should be skipped")
            }
            State::Expanding(ExpandingState::Expanded(expanded)) => Expansion::Expanded(expanded),
        })
    }

    pub(super) fn display(&self, db: &'db dyn Db) -> impl Display {
        struct DisplayCallArgumentTypes<'a, 'db> {
            types: &'a CallArgumentTypes<'db>,
            db: &'db dyn Db,
        }

        impl std::fmt::Display for DisplayCallArgumentTypes<'_, '_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_map()
                    .entries(self.types.iter().map(|(tcx, ty)| {
                        (
                            tcx.annotation.as_ref().map(|ty| ty.display(self.db)),
                            ty.display(self.db),
                        )
                    }))
                    .finish()
            }
        }

        struct DisplayCallArguments<'a, 'db> {
            call_arguments: &'a CallArguments<'a, 'db>,
            db: &'db dyn Db,
        }

        impl std::fmt::Display for DisplayCallArguments<'_, '_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("(")?;
                for (index, (argument, types)) in self.call_arguments.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    match argument {
                        Argument::Synthetic => {
                            write!(
                                f,
                                "self: {}",
                                DisplayCallArgumentTypes { types, db: self.db }
                            )?;
                        }
                        Argument::Positional => {
                            write!(f, "{}", DisplayCallArgumentTypes { types, db: self.db })?;
                        }
                        Argument::Variadic => {
                            write!(f, "*{}", DisplayCallArgumentTypes { types, db: self.db })?;
                        }
                        Argument::Keyword(name) => write!(
                            f,
                            "{}={}",
                            name,
                            DisplayCallArgumentTypes { types, db: self.db }
                        )?,
                        Argument::Keywords => {
                            write!(f, "**{}", DisplayCallArgumentTypes { types, db: self.db })?;
                        }
                    }
                }
                f.write_str(")")
            }
        }

        DisplayCallArguments {
            call_arguments: self,
            db,
        }
    }
}

/// Represents a single element of the expansion process for argument types for [`expand`].
///
/// [`expand`]: CallArguments::expand
pub(super) enum Expansion<'a, 'db> {
    /// Indicates that the expansion process has reached the maximum number of argument lists
    /// that can be generated in a single step.
    ///
    /// The contained `usize` is the index of the argument type which would have been expanded
    /// next, if not for the limit.
    LimitReached(usize),

    /// Contains the expanded argument lists, where each list contains the same arguments, but with
    /// one or more of the argument types expanded.
    Expanded(Vec<CallArguments<'a, 'db>>),
}

impl<'a, 'db> FromIterator<(Argument<'a>, Option<Type<'db>>)> for CallArguments<'a, 'db> {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = (Argument<'a>, Option<Type<'db>>)>,
    {
        let iter = iter.into_iter();
        let (lower, upper) = iter.size_hint();
        let mut items = Vec::with_capacity(upper.unwrap_or(lower));

        for (argument, ty) in iter {
            items.push(CallArgument {
                argument,
                types: CallArgumentTypes::new(ty),
            });
        }

        Self { items }
    }
}

/// Returns `true` if the type can be expanded into its subtypes.
///
/// In other words, it returns `true` if [`expand_type`] returns [`Some`] for the given type.
pub(crate) fn is_expandable_type<'db>(db: &'db dyn Db, ty: Type<'db>) -> bool {
    match ty {
        Type::NominalInstance(instance) => {
            let class = instance.class(db);
            class.is_known(db, KnownClass::Bool)
                || instance.tuple_spec(db).is_some_and(|spec| match &*spec {
                    Tuple::Fixed(fixed_length_tuple) => fixed_length_tuple
                        .iter_all_elements()
                        .any(|element| is_expandable_type(db, element)),
                    Tuple::Variable(_) => false,
                })
                || enum_metadata(db, class.class_literal(db)).is_some()
        }
        Type::Union(_) => true,
        Type::TypeAlias(alias) => is_expandable_type(db, alias.value_type(db)),
        _ => false,
    }
}

/// Expands a type into its possible subtypes, if applicable.
///
/// Returns [`None`] if the type cannot be expanded.
fn expand_type<'db>(db: &'db dyn Db, ty: Type<'db>) -> Option<Vec<Type<'db>>> {
    // NOTE: Update `is_expandable_type` if this logic changes accordingly.
    match ty {
        Type::NominalInstance(instance) => {
            let class = instance.class(db);

            if class.is_known(db, KnownClass::Bool) {
                return Some(vec![Type::bool_literal(true), Type::bool_literal(false)]);
            }

            // If the class is a fixed-length tuple subtype, we expand it to its elements.
            if let Some(spec) = instance.tuple_spec(db) {
                return match &*spec {
                    Tuple::Fixed(fixed_length_tuple) => {
                        // Pre-expand each element and compute the total Cartesian product size.
                        // Bail out early if the product would exceed `MAX_TUPLE_EXPANSION` to
                        // avoid exponential blowup (e.g. a 37-element tuple with 2-element
                        // unions would produce 2^37 types).
                        let per_element: Vec<_> = fixed_length_tuple
                            .iter_all_elements()
                            .map(|element| {
                                expand_type(db, element).unwrap_or_else(|| vec![element])
                            })
                            .collect();

                        let product_size: usize = per_element
                            .iter()
                            .try_fold(1usize, |acc, v| acc.checked_mul(v.len()))
                            .unwrap_or(usize::MAX);

                        if product_size <= 1 || product_size > MAX_TUPLE_EXPANSION {
                            None
                        } else {
                            let expanded = per_element
                                .into_iter()
                                .multi_cartesian_product()
                                .map(|types| Type::heterogeneous_tuple(db, types))
                                .collect::<Vec<_>>();
                            Some(expanded)
                        }
                    }
                    Tuple::Variable(_) => None,
                };
            }

            if let Some(enum_members) = enum_member_literals(db, class.class_literal(db), None) {
                return Some(enum_members.collect());
            }

            None
        }
        Type::Union(union) => Some(union.elements(db).to_vec()),
        // For type aliases, expand the underlying value type.
        Type::TypeAlias(alias) => expand_type(db, alias.value_type(db)),
        // We don't handle `type[A | B]` here because it's already stored in the expanded form
        // i.e., `type[A] | type[B]` which is handled by the `Type::Union` case.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::db::tests::setup_db;
    use crate::types::tuple::TupleType;
    use crate::types::{KnownClass, Type, UnionType};

    use super::expand_type;

    #[test]
    fn expand_union_type() {
        let db = setup_db();
        let types = [
            KnownClass::Int.to_instance(&db),
            KnownClass::Str.to_instance(&db),
            KnownClass::Bytes.to_instance(&db),
        ];
        let union_type = UnionType::from_elements(&db, types);
        let expanded = expand_type(&db, union_type).unwrap();
        assert_eq!(expanded.len(), types.len());
        assert_eq!(expanded, types);
    }

    #[test]
    fn expand_bool_type() {
        let db = setup_db();
        let bool_instance = KnownClass::Bool.to_instance(&db);
        let expanded = expand_type(&db, bool_instance).unwrap();
        let expected_types = [Type::bool_literal(true), Type::bool_literal(false)];
        assert_eq!(expanded.len(), expected_types.len());
        assert_eq!(expanded, expected_types);
    }

    #[test]
    fn expand_tuple_type() {
        let db = setup_db();

        let int_ty = KnownClass::Int.to_instance(&db);
        let str_ty = KnownClass::Str.to_instance(&db);
        let bytes_ty = KnownClass::Bytes.to_instance(&db);
        let bool_ty = KnownClass::Bool.to_instance(&db);
        let true_ty = Type::bool_literal(true);
        let false_ty = Type::bool_literal(false);

        // Empty tuple
        let empty_tuple = Type::empty_tuple(&db);
        let expanded = expand_type(&db, empty_tuple);
        assert!(expanded.is_none());

        // None of the elements can be expanded.
        let tuple_type1 = Type::heterogeneous_tuple(&db, [int_ty, str_ty]);
        let expanded = expand_type(&db, tuple_type1);
        assert!(expanded.is_none());

        // All elements can be expanded.
        let tuple_type2 = Type::heterogeneous_tuple(
            &db,
            [
                bool_ty,
                UnionType::from_elements(&db, [int_ty, str_ty, bytes_ty]),
            ],
        );
        let expected_types = [
            Type::heterogeneous_tuple(&db, [true_ty, int_ty]),
            Type::heterogeneous_tuple(&db, [true_ty, str_ty]),
            Type::heterogeneous_tuple(&db, [true_ty, bytes_ty]),
            Type::heterogeneous_tuple(&db, [false_ty, int_ty]),
            Type::heterogeneous_tuple(&db, [false_ty, str_ty]),
            Type::heterogeneous_tuple(&db, [false_ty, bytes_ty]),
        ];
        let expanded = expand_type(&db, tuple_type2).unwrap();
        assert_eq!(expanded, expected_types);

        // Mixed set of elements where some can be expanded while others cannot be.
        let tuple_type3 = Type::heterogeneous_tuple(
            &db,
            [
                bool_ty,
                int_ty,
                UnionType::from_elements(&db, [str_ty, bytes_ty]),
                str_ty,
            ],
        );
        let expected_types = [
            Type::heterogeneous_tuple(&db, [true_ty, int_ty, str_ty, str_ty]),
            Type::heterogeneous_tuple(&db, [true_ty, int_ty, bytes_ty, str_ty]),
            Type::heterogeneous_tuple(&db, [false_ty, int_ty, str_ty, str_ty]),
            Type::heterogeneous_tuple(&db, [false_ty, int_ty, bytes_ty, str_ty]),
        ];
        let expanded = expand_type(&db, tuple_type3).unwrap();
        assert_eq!(expanded, expected_types);

        // Variable-length tuples are not expanded.
        let variable_length_tuple = Type::tuple(TupleType::mixed(
            &db,
            [bool_ty],
            int_ty,
            [UnionType::from_elements(&db, [str_ty, bytes_ty]), str_ty],
        ));
        let expanded = expand_type(&db, variable_length_tuple);
        assert!(expanded.is_none());
    }
}
