use crate::Db;
use std::borrow::Cow;
use std::fmt::Display;

use itertools::{Either, Itertools};
use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use rustc_hash::FxHashMap;
use salsa::plumbing::AsId;
use ty_python_core::place::{PlaceExpr, ScopedPlaceId};
use ty_python_core::place_table;
use ty_python_core::scope::ScopeId;

use super::{Binding, Bindings};

use crate::ProgramEnvironment;
use crate::types::enums::enum_metadata;
use crate::types::infer::constraints::{InferenceConstraints, SymbolicType};
use crate::types::tuple::Tuple;
use crate::types::typed_dict::extract_unpacked_typed_dict_keys_from_value_type;
use crate::types::{InternedType, KnownClass, Type, TypeContext, expand_type};

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
    // Retain the narrowing target when a call is evaluated again after its inputs are solved.
    place: Option<(ScopeId<'db>, ScopedPlaceId)>,
}

/// An argument's ordinary type and the dependencies retained by that inference attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InferredArgument<'db> {
    pub(crate) ty: Type<'db>,
    pub(crate) symbolic: Option<SymbolicType<'db>>,
}

impl<'db> From<Type<'db>> for InferredArgument<'db> {
    fn from(ty: Type<'db>) -> Self {
        Self { ty, symbolic: None }
    }
}

/// Inferred types for a given argument.
///
/// Note that a single argument may produce multiple distinct inferred types when inferred
/// with type context across multiple bindings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CallArgumentTypes<'db> {
    fallback_type: Option<InferredArgument<'db>>,
    types: FxHashMap<Type<'db>, InferredArgument<'db>>,
}

impl<'db> CallArgumentTypes<'db> {
    fn new(fallback_ty: Option<Type<'db>>) -> Self {
        Self {
            fallback_type: fallback_ty.map(InferredArgument::from),
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
            .map(|inferred| inferred.ty)
            .exactly_one()
            .or_else(|_| {
                self.types
                    .values()
                    .map(|inferred| inferred.ty)
                    .all_equal_value()
            })
        {
            return Some(exact_ty);
        }

        self.fallback_type.map(|inferred| inferred.ty)
    }

    /// Returns the type of this argument when inferred against the provided declared type.
    pub(crate) fn get_for_declared_type(&self, tcx: Type<'db>) -> Type<'db> {
        self.types
            .get(&tcx)
            .map(|inferred| inferred.ty)
            .or_else(|| self.get_default())
            .unwrap_or(Type::unknown())
    }

    /// Insert the type of this argument when inferred with the provided type context.
    fn insert(&mut self, tcx: impl Into<TypeContext<'db>>, ty: InferredArgument<'db>) {
        match tcx.into().annotation {
            None => self.fallback_type = Some(ty),
            Some(tcx) => {
                self.types.insert(tcx, ty);
            }
        }
    }

    fn iter(&self) -> impl Iterator<Item = (TypeContext<'db>, Type<'db>)> {
        self.types
            .iter()
            .map(|(tcx, inferred)| (TypeContext::new(Some(*tcx)), inferred.ty))
            .chain(
                self.fallback_type
                    .map(|inferred| (TypeContext::default(), inferred.ty)),
            )
    }
}

/// Owned argument data for evaluating a call after its cyclic inputs have been solved.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub(crate) struct CapturedCallArguments<'db> {
    #[returns(ref)]
    arguments: Box<[CapturedArgument<'db>]>,
}

impl get_size2::GetSize for CapturedCallArguments<'_> {}

#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct CapturedArgument<'db> {
    kind: CapturedArgumentKind,
    place: Option<(ScopeId<'db>, ScopedPlaceId)>,
    types: Box<[(Option<Type<'db>>, Type<'db>)]>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
enum CapturedArgumentKind {
    Synthetic,
    Positional,
    Variadic,
    Keyword(Name),
    Keywords,
}

impl CapturedArgumentKind {
    fn as_argument(&self) -> Argument<'_> {
        match self {
            Self::Synthetic => Argument::Synthetic,
            Self::Positional => Argument::Positional,
            Self::Variadic => Argument::Variadic,
            Self::Keyword(name) => Argument::Keyword(name),
            Self::Keywords => Argument::Keywords,
        }
    }
}

impl<'db> CapturedCallArguments<'db> {
    /// Substitute dependencies in both declared-type keys and inferred argument types.
    pub(crate) fn map(self, db: &'db dyn Db, mut f: impl FnMut(Type<'db>) -> Type<'db>) -> Self {
        Self::new(
            db,
            self.arguments(db)
                .iter()
                .map(|argument| CapturedArgument {
                    kind: argument.kind.clone(),
                    place: argument.place,
                    types: argument
                        .types
                        .iter()
                        .map(|(tcx, ty)| (tcx.map(&mut f), f(*ty)))
                        .collect(),
                })
                .collect::<Box<[_]>>(),
        )
    }

    /// Restore the captured contexts for ordinary parameter matching and type checking.
    pub(crate) fn to_arguments(self, db: &'db dyn Db) -> CallArguments<'db, 'db> {
        CallArguments {
            items: self
                .arguments(db)
                .iter()
                .map(|argument| {
                    let mut types = CallArgumentTypes::default();
                    for (tcx, ty) in &argument.types {
                        types.insert(TypeContext::new(*tcx), (*ty).into());
                    }
                    CallArgument {
                        argument: argument.kind.as_argument(),
                        types,
                        place: argument.place,
                    }
                })
                .collect(),
        }
    }
}

impl<'a, 'db> CallArguments<'a, 'db> {
    /// Associate a type predicate's return type with the argument selected by call binding.
    pub(crate) fn bind_type_guard_return_type(
        &self,
        db: &'db dyn Db,
        return_ty: Type<'db>,
        bindings: &Bindings<'db>,
    ) -> Type<'db> {
        let narrowed_argument_index = || {
            bindings
                .single_element()
                .and_then(|binding| {
                    binding
                        .signature_type
                        .as_function_literal()
                        .or_else(|| binding.callable_type.as_function_literal())
                        .map(|function| {
                            usize::from(
                                function.has_implicit_receiver(db) && binding.bound_type.is_none(),
                            )
                        })
                })
                .unwrap_or(0)
        };

        let find_narrowed_place = || {
            // Use the call binding to find the argument that maps to the first parameter a type
            // guard can narrow. This supports keyword arguments without falling back to a later
            // parameter when the target is defaulted.
            let matched_narrowed_argument_index = bindings.single_element().and_then(|binding| {
                let has_implicit_receiver = binding
                    .signature_type
                    .as_function_literal()
                    .or_else(|| binding.callable_type.as_function_literal())
                    .is_some_and(|function| function.has_implicit_receiver(db));
                let bound_argument_offset = usize::from(binding.bound_type.is_some());
                let narrowed_parameter_index =
                    usize::from(bound_argument_offset > 0 || has_implicit_receiver);
                let narrowed_argument_index = |overload: &Binding<'db>| {
                    overload
                        .argument_matches()
                        .iter()
                        .enumerate()
                        .skip(bound_argument_offset)
                        .find_map(|(argument_index, matched_argument)| {
                            matched_argument
                                .parameters
                                .iter()
                                .any(|parameter| parameter.index == narrowed_parameter_index)
                                .then_some(argument_index - bound_argument_offset)
                        })
                };
                let mut matching_overloads = binding.matching_overloads();
                let (_, first_overload) = matching_overloads.next()?;
                let first_argument_index = narrowed_argument_index(first_overload);

                Some(
                    if matching_overloads.all(|(_, overload)| {
                        narrowed_argument_index(overload) == first_argument_index
                    }) {
                        first_argument_index
                    } else {
                        None
                    },
                )
            });

            let argument = match matched_narrowed_argument_index {
                Some(Some(argument_index)) => self.items.get(argument_index),
                // The target parameter was omitted, so there is no expression to narrow.
                Some(None) => None,
                // Preserve positional behavior when there isn't a unique callable binding whose
                // parameter mapping we can use.
                None => self
                    .items
                    .iter()
                    .filter(|argument| {
                        matches!(argument.argument, Argument::Positional | Argument::Variadic)
                    })
                    .nth(narrowed_argument_index()),
            }?;
            argument.place
        };

        match return_ty {
            Type::TypeIs(type_is) => match find_narrowed_place() {
                Some((scope, place)) => type_is.bind(db, scope, place),
                None => return_ty,
            },
            Type::TypeGuard(type_guard) => match find_narrowed_place() {
                Some((scope, place)) => type_guard.bind(db, scope, place),
                None => return_ty,
            },
            _ => return_ty,
        }
    }

    /// Whether any inference attempt retained dependencies on a cyclic input.
    pub(crate) fn has_symbolic_types(&self) -> bool {
        self.items.iter().any(|argument| {
            argument
                .types
                .fallback_type
                .iter()
                .chain(argument.types.types.values())
                .any(|inferred| inferred.symbolic.is_some())
        })
    }

    /// Retain every argument inference context without repeating expression inference.
    pub(crate) fn capture(
        &self,
        db: &'db dyn Db,
        constraints: &mut InferenceConstraints<'db>,
    ) -> CapturedCallArguments<'db> {
        let arguments = self
            .items
            .iter()
            .map(|argument| {
                let kind = match argument.argument {
                    Argument::Synthetic => CapturedArgumentKind::Synthetic,
                    Argument::Positional => CapturedArgumentKind::Positional,
                    Argument::Variadic => CapturedArgumentKind::Variadic,
                    Argument::Keyword(name) => CapturedArgumentKind::Keyword(Name::new(name)),
                    Argument::Keywords => CapturedArgumentKind::Keywords,
                };
                let mut types: Vec<_> = argument
                    .types
                    .types
                    .iter()
                    .map(|(tcx, inferred)| (Some(*tcx), *inferred))
                    .chain(
                        argument
                            .types
                            .fallback_type
                            .map(|inferred| (None, inferred)),
                    )
                    .map(|(tcx, inferred)| {
                        let ty = inferred
                            .symbolic
                            .map_or(inferred.ty, |symbolic| constraints.import(db, symbolic));
                        (tcx, ty)
                    })
                    .collect();
                // Context-specific inference is redundant when every context produced the same
                // type. Keep one fallback so cyclic calls do not alternate between equivalent
                // context lists as their callable approximation changes.
                if let Some((_, ty)) = types.first().copied()
                    && types.iter().all(|(_, candidate)| *candidate == ty)
                {
                    types = vec![(None, ty)];
                }
                types.sort_unstable_by_key(|(tcx, _)| {
                    tcx.map(|ty| InternedType::new(db, ty).as_id())
                });
                CapturedArgument {
                    kind,
                    place: argument.place,
                    types: types.into_boxed_slice(),
                }
            })
            .collect::<Box<[_]>>();
        CapturedCallArguments::new(db, arguments)
    }

    /// Create `CallArguments` from AST arguments. We will use the provided callback to obtain the
    /// type of each splatted argument, so that we can determine its length. All other arguments
    /// will remain uninitialized as `Unknown`.
    pub(crate) fn from_arguments(
        db: &'db dyn Db,
        scope: ScopeId<'db>,
        arguments: &'a ast::Arguments,
        mut infer_argument_type: impl FnMut(&ast::ArgOrKeyword, &ast::Expr) -> InferredArgument<'db>,
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
            let place = (!arg_or_keyword.is_variadic())
                .then(|| PlaceExpr::try_from_expr(arg_or_keyword.value()))
                .flatten()
                .and_then(|place| place_table(db, scope).place_id(&place))
                .map(|place| (scope, place));
            call_arguments.items.push(CallArgument {
                argument,
                place,
                types: CallArgumentTypes {
                    fallback_type: ty,
                    types: FxHashMap::default(),
                },
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
        ty: impl Into<InferredArgument<'db>>,
    ) {
        self.items
            .get_mut(index)
            .expect("argument index should be valid")
            .types
            .insert(tcx, ty.into());
    }

    pub(crate) fn clear_types(&mut self, index: usize) {
        self.items
            .get_mut(index)
            .expect("argument index should be valid")
            .types = CallArgumentTypes::default();
    }

    pub(crate) fn iter_types(&self) -> impl Iterator<Item = &CallArgumentTypes<'db>> + '_ {
        self.items.iter().map(|item| &item.types)
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
                place: None,
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
    fn start_from(&self, index: usize) -> Self {
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
        env: &ProgramEnvironment<'db>,
    ) -> Option<(Self, bool)> {
        let bound_call_arguments = self.start_from(1);
        let mut can_synthesize_signature = true;

        for (argument, argument_ty) in bound_call_arguments.iter() {
            let argument_ty = argument_ty.get_default().unwrap_or_else(Type::unknown);
            match argument {
                Argument::Variadic => {
                    if !matches!(
                        argument_ty.tuple_instance_spec(db, env),
                        Some(spec) if spec.as_fixed_length().is_some()
                    ) {
                        return None;
                    }
                }
                Argument::Keywords => {
                    // Known `TypedDict` items can still be checked against their target
                    // parameters, even though possible hidden items prevent us from synthesizing
                    // a precise partial signature.
                    extract_unpacked_typed_dict_keys_from_value_type(db, env, argument_ty)?;
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
    pub(super) fn expand(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> impl Iterator<Item = Expansion<'a, 'db>> + '_ {
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

        let env = env.clone();
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
                        && let Some(expanded_types) = expand_type(db, &env, arg_type)
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

    pub(super) fn display<'env>(
        &'env self,
        db: &'db dyn Db,
        env: &'env ProgramEnvironment<'db>,
    ) -> impl Display + 'env {
        struct DisplayCallArgumentTypes<'env, 'a, 'db> {
            types: &'a CallArgumentTypes<'db>,
            db: &'db dyn Db,
            env: &'env ProgramEnvironment<'db>,
        }

        impl std::fmt::Display for DisplayCallArgumentTypes<'_, '_, '_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let db = self.db;
                f.debug_map()
                    .entries(self.types.iter().map(|(tcx, ty)| {
                        (
                            tcx.annotation.as_ref().map(|ty| ty.display(db, self.env)),
                            ty.display(db, self.env),
                        )
                    }))
                    .finish()
            }
        }

        std::fmt::from_fn(move |f| {
            f.write_str("(")?;
            for (index, (argument, types)) in self.iter().enumerate() {
                if index > 0 {
                    write!(f, ", ")?;
                }
                match argument {
                    Argument::Synthetic => {
                        write!(f, "self: {}", DisplayCallArgumentTypes { types, db, env })?;
                    }
                    Argument::Positional => {
                        write!(f, "{}", DisplayCallArgumentTypes { types, db, env })?;
                    }
                    Argument::Variadic => {
                        write!(f, "*{}", DisplayCallArgumentTypes { types, db, env })?;
                    }
                    Argument::Keyword(name) => write!(
                        f,
                        "{}={}",
                        name,
                        DisplayCallArgumentTypes { types, db, env }
                    )?,
                    Argument::Keywords => {
                        write!(f, "**{}", DisplayCallArgumentTypes { types, db, env })?;
                    }
                }
            }
            f.write_str(")")
        })
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
                place: None,
            });
        }

        Self { items }
    }
}

/// Returns `true` if the type can be expanded into its subtypes.
///
/// In other words, it returns `true` if [`expand_type`] returns [`Some`] for the given type.
pub(crate) fn is_expandable_type<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
) -> bool {
    match ty {
        Type::EnumComplement(_) => true,
        Type::Intersection(intersection) => intersection.finite_alternatives(db, env).is_some(),
        Type::NominalInstance(instance) => {
            let class = instance.class(db, env);
            if class.is_known(db, KnownClass::Bool) {
                return true;
            }
            if let Some(tuple_spec) = instance.tuple_spec(db, env)
                && let Tuple::Fixed(fixed_length_tuple) = &*tuple_spec
                && fixed_length_tuple
                    .iter_all_elements()
                    .any(|element| is_expandable_type(db, env, element))
            {
                return true;
            }
            enum_metadata(db, class.class_literal(db)).is_some()
        }
        Type::Union(_) => true,
        Type::TypeAlias(alias) => is_expandable_type(db, env, alias.value_type(db)),
        _ => false,
    }
}
