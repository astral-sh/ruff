use std::borrow::Cow;
use std::fmt::Display;

use itertools::{Either, Itertools};
use ruff_python_ast as ast;
use rustc_hash::FxHashMap;

use crate::Db;
use crate::types::enums::enum_metadata;
use crate::types::tuple::Tuple;
use crate::types::typed_dict::extract_unpacked_typed_dict_keys_from_value_type;
use crate::types::{KnownClass, Type, TypeContext, expand_type};

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
#[derive(Clone, Debug, Default, PartialEq, Eq)]
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

    pub(crate) fn is_variadic(&self, index: usize) -> bool {
        self.items.get(index).is_some_and(|argument| {
            matches!(argument.argument, Argument::Variadic | Argument::Keywords)
        })
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

    pub(crate) fn clear_types(&mut self, index: usize) {
        self.items
            .get_mut(index)
            .expect("argument index should be valid")
            .types = CallArgumentTypes::default();
    }

    /// Returns `true` if the inferred types are equal for the given set of argument indices.
    pub(crate) fn inferred_types_equal_at(&self, other: &Self, argument_indices: &[usize]) -> bool {
        argument_indices.iter().all(|&index| {
            self.items.get(index).map(|item| &item.types)
                == other.items.get(index).map(|item| &item.types)
        })
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

    /// Returns the `functools.partial(...)` bound-argument slice and whether it is concrete enough
    /// to synthesize a precise partial signature.
    pub(crate) fn functools_partial_bound_arguments(
        &self,
        db: &'db dyn Db,
    ) -> Option<(Self, bool)> {
        let bound_call_arguments = self.start_from(1);
        let mut can_synthesize_signature = true;

        for (argument, argument_ty) in bound_call_arguments.iter() {
            let argument_ty = argument_ty.get_default().unwrap_or_else(Type::unknown);
            match argument {
                Argument::Variadic => {
                    if !matches!(
                        argument_ty.tuple_instance_spec(db),
                        Some(spec) if spec.as_fixed_length().is_some()
                    ) {
                        return None;
                    }
                }
                Argument::Keywords => {
                    // Known `TypedDict` items can still be checked against their target
                    // parameters, even though possible hidden items prevent us from synthesizing
                    // a precise partial signature.
                    extract_unpacked_typed_dict_keys_from_value_type(db, argument_ty)?;
                    can_synthesize_signature = false;
                }
                Argument::Positional | Argument::Synthetic | Argument::Keyword(_) => {}
            }
        }

        Some((bound_call_arguments, can_synthesize_signature))
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
        Type::EnumComplement(_) => true,
        Type::Intersection(intersection) => intersection.finite_alternatives(db).is_some(),
        Type::NominalInstance(instance) => {
            let class = instance.class(db);
            if class.is_known(db, KnownClass::Bool) {
                return true;
            }
            if let Some(tuple_spec) = instance.tuple_spec(db)
                && let Tuple::Fixed(fixed_length_tuple) = &*tuple_spec
                && fixed_length_tuple
                    .iter_all_elements()
                    .any(|element| is_expandable_type(db, element))
            {
                return true;
            }
            enum_metadata(db, class.class_literal(db)).is_some()
        }
        Type::Union(_) => true,
        Type::TypeAlias(alias) => is_expandable_type(db, alias.value_type(db)),
        _ => false,
    }
}
