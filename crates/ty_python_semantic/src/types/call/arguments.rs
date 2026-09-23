use crate::Db;
use std::borrow::Cow;
use std::cell::OnceCell;
use std::fmt::Display;
use std::hash::BuildHasherDefault;

use itertools::{Either, Itertools};
use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use rustc_hash::FxHashMap;
use ty_python_core::definition::{BindingsOwner, DefinitionKind};
use ty_python_core::scope::{ScopeId, ScopeKind};
use ty_python_core::semantic_index;

use crate::FxIndexMap;
use crate::ProgramEnvironment;
use crate::subscript::PyIndex;
use crate::types::infer::infer_definition_types;
use crate::types::tuple::{TupleLength, TupleSpec};
use crate::types::type_expansion::expand_elements;
use crate::types::typed_dict::{
    TypedDictOpenness, UnpackedTypedDictKey, extract_unpacked_typed_dict_from_value_type,
};
use crate::types::{Parameters, Type, TypeContext, UnionType, expand_type};

/// Maximum total number of expanded argument type combinations across all arguments
/// in [`CallArgumentExpansions::iter`].
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

/// An argument to a call and its inferred types, when available.
///
/// Each variant represents one argument, even when `*args` or `**kwargs` supplies values for
/// multiple parameters. For unpacked arguments, the stored types describe the expression before
/// unpacking; the types of its values are derived when matching the call.
///
/// For example:
///
/// ```py
/// def pair(x: int, y: str) -> None: ...
///
/// pair(*(1, "two"))
/// ```
///
/// This call has one [`CallArgument::Variadic`] with source type
/// `tuple[Literal[1], Literal["two"]]`. Matching supplies `Literal[1]` to `x` and
/// `Literal["two"]` to `y`. Both parameter matches refer to the same source argument index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CallArgument<'a, 'db> {
    /// A receiver passed as `self` or `cls` without an explicit argument at the call site.
    Synthetic(CallArgumentTypes<'db>),
    /// A positional argument, such as `value` in `f(value)`.
    Positional(CallArgumentTypes<'db>),
    /// A named keyword argument, such as `name=value` in `f(name=value)`.
    Keyword {
        name: &'a str,
        types: CallArgumentTypes<'db>,
    },
    /// A starred positional argument, such as `*args` in `f(*args)`.
    Variadic(VariadicArgument<'db>),
    /// A double-starred keyword argument, such as `**kwargs` in `f(**kwargs)`.
    Keywords(KeywordArgument<'db>),
}

impl<'a, 'db> CallArgument<'a, 'db> {
    fn new(argument: Argument<'a>, ty: Option<Type<'db>>) -> Self {
        let types = CallArgumentTypes::new(ty);
        match argument {
            Argument::Synthetic => Self::Synthetic(types),
            Argument::Positional => Self::Positional(types),
            Argument::Keyword(name) => Self::Keyword { name, types },
            Argument::Variadic => Self::Variadic(VariadicArgument::Type(types)),
            Argument::Keywords => Self::Keywords(KeywordArgument::Type(types)),
        }
    }

    pub(crate) fn kind(&self) -> Argument<'a> {
        match self {
            Self::Synthetic(_) => Argument::Synthetic,
            Self::Positional(_) => Argument::Positional,
            Self::Keyword { name, .. } => Argument::Keyword(name),
            Self::Variadic(_) => Argument::Variadic,
            Self::Keywords(_) => Argument::Keywords,
        }
    }

    /// The inferred type of the source expression, before unpacking its elements.
    pub(crate) fn source_types(&self) -> &CallArgumentTypes<'db> {
        match self {
            Self::Synthetic(types) | Self::Positional(types) | Self::Keyword { types, .. } => types,
            Self::Variadic(argument) => argument.source_types(),
            Self::Keywords(argument) => argument.source_types(),
        }
    }

    fn source_types_mut(&mut self) -> &mut CallArgumentTypes<'db> {
        match self {
            Self::Synthetic(types) | Self::Positional(types) | Self::Keyword { types, .. } => types,
            Self::Variadic(
                VariadicArgument::Type(types) | VariadicArgument::Sequence { types, .. },
            )
            | Self::Keywords(KeywordArgument::Type(types) | KeywordArgument::Known { types, .. }) => {
                types
            }
        }
    }

    pub(crate) fn source_type(&self) -> Option<Type<'db>> {
        self.source_types().get_default()
    }

    /// The type supplied to a matched parameter. Splats supply their matched element type,
    /// while ordinary arguments may be inferred using the parameter's type as context.
    pub(crate) fn matched_type(
        &self,
        declared: impl Into<Option<Type<'db>>>,
        matched: Option<Type<'db>>,
    ) -> Option<Type<'db>> {
        let declared = declared.into();
        matched.or_else(|| match self {
            Self::Synthetic(types) | Self::Positional(types) | Self::Keyword { types, .. } => {
                declared.map_or_else(
                    || types.get_default(),
                    |declared| types.try_get_for_declared_type(declared),
                )
            }
            Self::Variadic(_) | Self::Keywords(_) => None,
        })
    }

    fn expand(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> ArgumentExpansion<'a, 'db> {
        Some(match self {
            Self::Variadic(argument) => argument
                .expand(db, env)?
                .into_iter()
                .map(Self::Variadic)
                .collect(),
            Self::Keywords(argument) => argument
                .expand(db, env)?
                .into_iter()
                .map(Self::Keywords)
                .collect(),
            Self::Synthetic(_) | Self::Positional(_) | Self::Keyword { .. } => {
                expand_type(db, env, self.source_type()?)?
                    .into_iter()
                    .map(|ty| Self::new(self.kind(), Some(ty)))
                    .collect()
            }
        })
    }
}

/// A starred argument, whose source type and unpacked positional values serve different purposes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum VariadicArgument<'db> {
    Type(CallArgumentTypes<'db>),
    Sequence {
        types: CallArgumentTypes<'db>,
        sequence: TupleSpec<'db>,
    },
}

/// Positional information used when matching a starred argument to a signature.
pub(crate) struct VariadicArgumentMatch<'db> {
    pub(crate) types: Vec<Type<'db>>,
    pub(crate) length: TupleLength,
    pub(crate) variable_element: Option<Type<'db>>,
}

impl<'db> VariadicArgument<'db> {
    fn source_types(&self) -> &CallArgumentTypes<'db> {
        match self {
            Self::Type(types) | Self::Sequence { types, .. } => types,
        }
    }

    fn source_type(&self) -> Option<Type<'db>> {
        self.source_types().get_default()
    }

    pub(crate) fn sequence(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<Cow<'_, TupleSpec<'db>>> {
        Some(match self {
            Self::Type(_) => self.source_type()?.iterate(db, env),
            Self::Sequence { sequence, .. } => Cow::Borrowed(sequence),
        })
    }

    fn is_fixed_sequence(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> bool {
        match self {
            Self::Type(_) => self
                .source_type()
                .and_then(|ty| ty.tuple_instance_spec(db, env))
                .is_some_and(|spec| spec.as_fixed_length().is_some()),
            Self::Sequence { sequence, .. } => sequence.as_fixed_length().is_some(),
        }
    }

    fn expand(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Option<Vec<Self>> {
        Some(match self {
            Self::Type(_) => expand_type(db, env, self.source_type()?)?
                .into_iter()
                .map(|ty| Self::Type(CallArgumentTypes::new(Some(ty))))
                .collect(),
            Self::Sequence { types, sequence } => {
                expand_elements(db, env, sequence.as_fixed_length()?.iter_all_elements())?
                    .into_iter()
                    .map(|elements| Self::Sequence {
                        types: types.clone(),
                        sequence: TupleSpec::heterogeneous(elements),
                    })
                    .collect()
            }
        })
    }

    pub(crate) fn matching(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        preserve_union_alternatives: bool,
    ) -> VariadicArgumentMatch<'db> {
        if let Self::Sequence { sequence, .. } = self {
            return VariadicArgumentMatch {
                types: sequence.iter_element_types(db).collect(),
                length: sequence.len(),
                variable_element: sequence.variable_element_type(db),
            };
        }
        let Some(ty) = self.source_type() else {
            return VariadicArgumentMatch {
                types: Vec::new(),
                length: TupleLength::unknown(),
                variable_element: None,
            };
        };

        // Iterating `P.args` would discard its identity and yield its `object` upper bound.
        if let Some(paramspec) = ty.as_paramspec_typevar(db) {
            return VariadicArgumentMatch {
                types: Vec::new(),
                length: TupleLength::unknown(),
                variable_element: Some(paramspec),
            };
        }

        // Iterating a union as a whole can introduce arities absent from every member. Iterating
        // members separately allows matching to use their minimum length and per-position types,
        // but loses correlations between members.
        if preserve_union_alternatives && let Type::Union(union) = ty {
            let sequences: Vec<_> = union
                .elements(db)
                .iter()
                .map(|ty| ty.iterate(db, env))
                .collect();
            let minimum = sequences
                .iter()
                .map(|spec| spec.len().minimum())
                .min()
                .unwrap_or(0);
            let any_variable = sequences.iter().any(|spec| spec.len().is_variable());
            let max_elements = sequences
                .iter()
                .map(|spec| spec.iter_element_types(db).count())
                .max()
                .unwrap_or(0);
            let variable_types: Vec<_> = sequences
                .iter()
                .filter_map(|spec| spec.variable_element_type(db))
                .collect();
            let variable_element = (!variable_types.is_empty())
                .then(|| UnionType::from_elements_leave_aliases(db, env, variable_types));
            let mut types = Vec::new();
            for index in 0..i32::try_from(max_elements).unwrap_or(i32::MAX) {
                let positional_types: Vec<_> = sequences
                    .iter()
                    .filter_map(|spec| spec.py_index(db, env, index).ok())
                    .collect();
                if positional_types.is_empty() {
                    break;
                }
                types.push(UnionType::from_elements_leave_aliases(
                    db,
                    env,
                    positional_types,
                ));
            }
            let length = if any_variable || types.len() > minimum {
                TupleLength::Variable(minimum, 0)
            } else {
                TupleLength::Fixed(minimum)
            };
            return VariadicArgumentMatch {
                types,
                length,
                variable_element,
            };
        }

        let sequence = ty.iterate(db, env);
        VariadicArgumentMatch {
            types: sequence.iter_element_types(db).collect(),
            length: sequence.len(),
            variable_element: sequence.variable_element_type(db),
        }
    }
}

/// A double-starred argument, with operations shared by all consumers of its keyword values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum KeywordArgument<'db> {
    Type(CallArgumentTypes<'db>),
    Known {
        types: CallArgumentTypes<'db>,
        keywords: UnpackedKeywords<'db>,
    },
}

/// Known keyword values and possible undeclared keys.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UnpackedKeywords<'db> {
    pub(crate) keys: Box<[(Name, UnpackedTypedDictKey<'db>)]>,
    pub(crate) openness: TypedDictOpenness<'db>,
}

impl<'db> UnpackedKeywords<'db> {
    fn from_literal(
        dictionary: &ast::ExprDict,
        mut expression_type: impl FnMut(&ast::Expr) -> Option<Type<'db>>,
    ) -> Option<Self> {
        let mut keywords = FxIndexMap::with_capacity_and_hasher(
            dictionary.items.len(),
            BuildHasherDefault::default(),
        );
        for ast::DictItem { key, value } in &dictionary.items {
            let key = key.as_ref()?.as_string_literal_expr()?;
            let value_ty = expression_type(value)?;
            // Replacing a duplicate key preserves its first position in the dictionary.
            keywords.insert(
                Name::new(key.value.to_str()),
                UnpackedTypedDictKey {
                    value_ty,
                    is_required: true,
                    definition: None,
                },
            );
        }
        Some(Self {
            keys: keywords.into_iter().collect(),
            openness: TypedDictOpenness::Closed,
        })
    }

    /// Recover a fresh local dictionary when its recorded uses cannot expose or mutate it.
    /// Checking every use of the name also excludes aliases carried across loop iterations.
    fn from_local(db: &'db dyn Db, scope: ScopeId<'db>, expression: &ast::Expr) -> Option<Self> {
        if scope.scope(db).kind() != ScopeKind::Function {
            return None;
        }
        let name = expression.as_name_expr()?;
        let file = scope.program_file(db);
        let index = semantic_index(db, file);
        let file_scope = scope.file_scope_id(db);
        let symbol = index.place_table(file_scope).symbol_by_name(&name.id)?;
        if !symbol.is_local()
            || symbol.is_declared()
            || !symbol.is_used_only_for_keyword_unpacking()
        {
            return None;
        }
        let use_id = index.try_expression_use_id(expression.into())?;
        let binding = index
            .use_def_map(file_scope)
            .bindings_at_use(use_id)
            .exactly_one()
            .ok()?;
        let definition = binding.binding.definition()?;
        let DefinitionKind::Assignment(assignment) = definition.kind(db) else {
            return None;
        };
        // Chained and destructuring assignments can bind another name to the same dictionary.
        if assignment.owner() != BindingsOwner::Definition {
            return None;
        }
        let module = parsed_module(db, file.python_file(db)).load(db);
        let dictionary = assignment.value(&module).as_dict_expr()?;
        let inference = infer_definition_types(db, definition);
        if inference.discards_dict_key_assignments() {
            return None;
        }
        Self::from_literal(dictionary, |value| inference.try_expression_type(value))
    }
}

impl<'db> KeywordArgument<'db> {
    fn source_types(&self) -> &CallArgumentTypes<'db> {
        match self {
            Self::Type(types) | Self::Known { types, .. } => types,
        }
    }

    pub(crate) fn source_type(&self) -> Option<Type<'db>> {
        self.source_types().get_default()
    }

    /// Whether all keyword values were captured at the call site, rather than described by a type.
    pub(crate) fn is_complete(&self) -> bool {
        matches!(self, Self::Known { keywords, .. }
            if matches!(keywords.openness, TypedDictOpenness::Closed)
                && keywords.keys.iter().all(|(_, key)| key.is_required))
    }

    pub(crate) fn explicit_keyword_names(&self) -> impl Iterator<Item = &Name> {
        let keys = match self {
            Self::Type(_) => [].as_slice(),
            Self::Known { keywords, .. } => keywords.keys.as_ref(),
        };
        keys.iter()
            .filter_map(|(name, key)| key.is_required.then_some(name))
    }

    fn expand(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Option<Vec<Self>> {
        Some(match self {
            Self::Type(_) => expand_type(db, env, self.source_type()?)?
                .into_iter()
                .map(|ty| Self::Type(CallArgumentTypes::new(Some(ty))))
                .collect(),
            Self::Known { types, keywords } => {
                expand_elements(db, env, keywords.keys.iter().map(|(_, key)| key.value_ty))?
                    .into_iter()
                    .map(|elements| Self::Known {
                        types: types.clone(),
                        keywords: UnpackedKeywords {
                            keys: keywords
                                .keys
                                .iter()
                                .zip(elements)
                                .map(|((name, key), value_ty)| {
                                    (name.clone(), UnpackedTypedDictKey { value_ty, ..*key })
                                })
                                .collect(),
                            openness: keywords.openness,
                        },
                    })
                    .collect()
            }
        })
    }

    pub(crate) fn unpack(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<UnpackedKeywords<'db>> {
        if let Self::Known { keywords, .. } = self {
            return Some(keywords.clone());
        }
        let unpacked = extract_unpacked_typed_dict_from_value_type(db, env, self.source_type()?)?;
        Some(UnpackedKeywords {
            keys: unpacked.keys.into_iter().collect(),
            openness: unpacked.openness,
        })
    }

    pub(crate) fn value_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: Option<&str>,
    ) -> Type<'db> {
        if let Self::Known { keywords, .. } = self {
            return keywords
                .keys
                .iter()
                .find_map(|(key, value)| (Some(key.as_str()) == name).then_some(value.value_ty))
                .unwrap_or(Type::unknown());
        }
        self.source_type()
            .and_then(|ty| {
                ty.as_paramspec_typevar(db)
                    .or_else(|| ty.getitem_dunder_call(db, env, name))
            })
            .unwrap_or(Type::unknown())
    }
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
    fn new(fallback_ty: Option<Type<'db>>) -> Self {
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
    ///
    /// If the type was not inferred against the declared type directly, this method will fall back to
    /// [`Self::get_default`].
    fn try_get_for_declared_type(&self, tcx: Type<'db>) -> Option<Type<'db>> {
        self.types.get(&tcx).copied().or_else(|| self.get_default())
    }

    /// Returns the type of this argument when inferred against the provided declared type.
    ///
    /// If the type was not inferred against the declared type directly, this method will fall back to
    /// [`Self::get_default`], or to `Unknown` if no fallback type exists.
    pub(crate) fn get_for_declared_type(&self, tcx: Type<'db>) -> Type<'db> {
        self.try_get_for_declared_type(tcx)
            .unwrap_or(Type::unknown())
    }

    /// Insert the type of this argument when inferred with the provided type context.
    fn insert(&mut self, tcx: impl Into<TypeContext<'db>>, ty: Type<'db>) {
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
            .map(|(tcx, ty)| (TypeContext::new(Some(*tcx)), *ty))
            .chain(self.fallback_type.map(|ty| (TypeContext::default(), ty)))
    }
}

impl<'a, 'db> CallArguments<'a, 'db> {
    /// Create `CallArguments` from AST arguments. We will use the provided callback to obtain the
    /// type of each splatted argument, so that we can determine its length. All other arguments
    /// will remain uninitialized.
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
            call_arguments.items.push(CallArgument::new(argument, ty));
        }

        call_arguments
    }

    /// Like [`Self::from_arguments`] but fills as much typing info in as possible.
    ///
    /// This currently only exists for the LSP usecase, and shouldn't be used in normal
    /// typechecking.
    pub(crate) fn from_arguments_typed(
        db: &'db dyn Db,
        scope: Option<ScopeId<'db>>,
        arguments: &'a ast::Arguments,
        mut infer_argument_type: impl FnMut(&ast::Expr) -> Option<Type<'db>>,
    ) -> Self {
        let call_arguments: Self = arguments
            .iter_source_order()
            .map(|arg_or_keyword| match arg_or_keyword {
                ast::ArgOrKeyword::Arg(arg) => match arg {
                    ast::Expr::Starred(ast::ExprStarred { value, .. }) => {
                        let ty = infer_argument_type(value).unwrap_or(Type::unknown());
                        (Argument::Variadic, Some(ty))
                    }
                    _ => {
                        let ty = infer_argument_type(arg).unwrap_or(Type::unknown());
                        (Argument::Positional, Some(ty))
                    }
                },
                ast::ArgOrKeyword::Keyword(ast::Keyword { arg, value, .. }) => {
                    let ty = infer_argument_type(value).unwrap_or(Type::unknown());
                    if let Some(arg) = arg {
                        (Argument::Keyword(&arg.id), Some(ty))
                    } else {
                        (Argument::Keywords, Some(ty))
                    }
                }
            })
            .collect();
        call_arguments.with_known_unpacking(db, scope, arguments, infer_argument_type)
    }

    /// Retain known collection elements at an unpacking site.
    ///
    /// ```py
    /// def pair(x: int, y: str) -> None: ...
    ///
    /// pair(*[1, "two"])  # VariadicArgument::Sequence
    /// pair(**{"x": 1, "y": "two"})  # KeywordArgument::Known
    /// ```
    ///
    /// The sequence records each element's type in order. The keyword collection associates each
    /// name with its value's type, so `1` binds to `x` and `"two"` binds to `y` in both calls.
    /// Local dictionaries also qualify when their initializer is known and every use is a direct
    /// keyword unpacking. Other aliases retain ordinary type-based unpacking.
    ///
    /// Ordinary tuple types already describe their elements. List and dictionary types do not
    /// retain the contents needed for call binding, even when those contents are known here.
    /// The callback reads types inferred while checking the enclosing collection expression.
    pub(crate) fn with_known_unpacking(
        mut self,
        db: &'db dyn Db,
        scope: Option<ScopeId<'db>>,
        arguments: &ast::Arguments,
        mut expression_type: impl FnMut(&ast::Expr) -> Option<Type<'db>>,
    ) -> Self {
        for (argument, source) in self.items.iter_mut().zip(arguments.iter_source_order()) {
            match (argument, source) {
                (
                    CallArgument::Variadic(argument),
                    ast::ArgOrKeyword::Arg(ast::Expr::Starred(ast::ExprStarred { value, .. })),
                ) => {
                    // Set literals keep ordinary type-based unpacking. Handling duplicate elements
                    // and unspecified iteration order is not worth the extra complexity for now.
                    let ast::Expr::List(ast::ExprList { elts, .. }) = value.as_ref() else {
                        continue;
                    };
                    let Some(elements) = elts
                        .iter()
                        .map(|element| {
                            if element.is_starred_expr() {
                                None
                            } else {
                                expression_type(element)
                            }
                        })
                        .collect::<Option<Vec<_>>>()
                    else {
                        continue;
                    };
                    *argument = VariadicArgument::Sequence {
                        types: argument.source_types().clone(),
                        sequence: TupleSpec::heterogeneous(elements),
                    };
                }
                (
                    CallArgument::Keywords(argument),
                    ast::ArgOrKeyword::Keyword(ast::Keyword {
                        arg: None, value, ..
                    }),
                ) => {
                    let keywords = match value {
                        ast::Expr::Dict(dictionary) => {
                            UnpackedKeywords::from_literal(dictionary, &mut expression_type)
                        }
                        _ => scope.and_then(|scope| UnpackedKeywords::from_local(db, scope, value)),
                    };
                    if let Some(keywords) = keywords {
                        *argument = KeywordArgument::Known {
                            types: argument.source_types().clone(),
                            keywords,
                        };
                    }
                }
                _ => {}
            }
        }
        self
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
            matches!(
                argument,
                CallArgument::Variadic(_) | CallArgument::Keywords(_)
            )
        })
    }

    pub(crate) fn get(&self, index: usize) -> Option<&CallArgument<'a, 'db>> {
        self.items.get(index)
    }

    /// The inferred source expression types, before any argument unpacking.
    pub(crate) fn source_types(&self, index: usize) -> Option<&CallArgumentTypes<'db>> {
        self.items.get(index).map(CallArgument::source_types)
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
            .source_types_mut()
            .insert(tcx, ty);
    }

    pub(crate) fn clear_types(&mut self, index: usize) {
        *self
            .items
            .get_mut(index)
            .expect("argument index should be valid")
            .source_types_mut() = CallArgumentTypes::default();
    }

    /// Returns `true` if the inferred types are equal for the given set of argument indices.
    pub(crate) fn inferred_types_equal_at(&self, other: &Self, argument_indices: &[usize]) -> bool {
        argument_indices
            .iter()
            .all(|&index| self.items.get(index) == other.items.get(index))
    }

    /// Prepend an optional extra synthetic argument (for a `self` or `cls` parameter) to the front
    /// of this argument list. (If `bound_self` is none, we return the argument list
    /// unmodified.)
    pub(crate) fn with_self(&self, bound_self: Option<Type<'db>>) -> Cow<'_, Self> {
        if bound_self.is_some() {
            let mut items = Vec::with_capacity(self.items.len() + 1);
            items.push(CallArgument::new(Argument::Synthetic, bound_self));
            items.extend(self.items.iter().cloned());
            Cow::Owned(CallArguments { items })
        } else {
            Cow::Borrowed(self)
        }
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &CallArgument<'a, 'db>> + '_ {
        self.items.iter()
    }

    /// Create a new [`CallArguments`] starting from the specified index.
    fn start_from(&self, index: usize) -> Self {
        Self {
            items: self.items[index..].to_vec(),
        }
    }

    /// Select the arguments forwarded to a `ParamSpec` sub-call.
    ///
    /// The resulting argument list preserves the order of `indices`. Unlike [`Self::start_from`],
    /// this can project a non-contiguous subset of the original call arguments. Known keyword
    /// arguments retain only keys that were not consumed by the wrapper's prefix:
    ///
    /// ```py
    /// def wrapper[**P, R](func: Callable[P, R], **kwargs: P.kwargs) -> R: ...
    /// wrapper(TagSet=[...], func=f)  # select `TagSet=[...]`, but not the later `func=f`
    /// ```
    pub(crate) fn select_for_paramspec(
        &self,
        indices: &[usize],
        parameters: &Parameters<'db>,
        prefix_len: usize,
    ) -> Self {
        Self {
            items: indices
                .iter()
                .map(|index| {
                    let mut argument = self.items[*index].clone();
                    if let CallArgument::Keywords(KeywordArgument::Known { keywords, .. }) =
                        &mut argument
                    {
                        keywords.keys = keywords
                            .keys
                            .iter()
                            .filter(|(name, _)| {
                                parameters
                                    .keyword_by_name(name.as_str())
                                    .is_none_or(|(index, _)| index >= prefix_len)
                            })
                            .cloned()
                            .collect();
                    }
                    argument
                })
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

        for argument in bound_call_arguments.iter() {
            match argument {
                CallArgument::Variadic(argument) => {
                    if !argument.is_fixed_sequence(db, env) {
                        return None;
                    }
                }
                CallArgument::Keywords(argument) => {
                    // Known `TypedDict` items can still be checked against their target
                    // parameters, even though possible hidden items prevent us from synthesizing
                    // a precise partial signature.
                    argument.unpack(db, env)?;
                    can_synthesize_signature &= argument.is_complete();
                }
                CallArgument::Positional(_)
                | CallArgument::Synthetic(_)
                | CallArgument::Keyword { .. } => {}
            }
        }

        Some((bound_call_arguments, can_synthesize_signature))
    }

    /// Prepares lazy argument type expansions for overload resolution.
    pub(super) fn expansions<'s>(
        &'s self,
        db: &'db dyn Db,
        env: &'s ProgramEnvironment<'db>,
    ) -> CallArgumentExpansions<'s, 'a, 'db> {
        CallArgumentExpansions {
            arguments: self,
            db,
            env,
            types: OnceCell::new(),
        }
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
            for (index, argument) in self.iter().enumerate() {
                if index > 0 {
                    write!(f, ", ")?;
                }
                let types = argument.source_types();
                match argument.kind() {
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

type ArgumentExpansion<'a, 'db> = Option<Vec<CallArgument<'a, 'db>>>;

/// Shares each argument's type expansion between overload checks and argument list expansion.
pub(super) struct CallArgumentExpansions<'s, 'a, 'db> {
    arguments: &'s CallArguments<'a, 'db>,
    db: &'db dyn Db,
    env: &'s ProgramEnvironment<'db>,
    types: OnceCell<Box<[OnceCell<ArgumentExpansion<'a, 'db>>]>>,
}

impl<'a, 'db> CallArgumentExpansions<'_, 'a, 'db> {
    /// Returns the expanded alternatives of an argument, computing them at most once.
    pub(super) fn argument_alternatives(&self, index: usize) -> Option<&[CallArgument<'a, 'db>]> {
        // TODO: For types inferred multiple times with distinct type context, we currently only
        // expand the default inference. Note that direct expansion of a type inferred against a
        // given declared type would not likely be assignable to other declared types without
        // re-inference, and so a more complete implementation would likely have to re-infer the
        // argument type against the union a given subset of type contexts before expansion. However,
        // this only shows up in very convoluted instances of generic call inference across multiple
        // overloads, and is unlikely to happen in practice.
        let argument = self.arguments.get(index)?;
        // Most calls need no expansion; allocate the cache only when a check asks for it.
        let types = self.types.get_or_init(|| {
            std::iter::repeat_with(OnceCell::new)
                .take(self.arguments.len())
                .collect()
        });
        types[index]
            .get_or_init(|| argument.expand(self.db, self.env))
            .as_deref()
    }

    /// Whether a starred positional argument can expand into alternative types.
    pub(super) fn has_expandable_variadic(&self) -> bool {
        self.arguments.iter().enumerate().any(|(index, argument)| {
            matches!(argument, CallArgument::Variadic(_))
                && self.argument_alternatives(index).is_some()
        })
    }

    /// Iterates over argument lists with successively more argument types expanded.
    ///
    /// See [argument type expansion](https://typing.python.org/en/latest/spec/overload.html#argument-type-expansion).
    pub(super) fn iter(&self) -> impl Iterator<Item = Expansion<'a, 'db>> + '_ {
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
                    self.arguments.get(index)?;
                    if let Some(expanded_types) = self.argument_alternatives(index) {
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

                for pre_expanded_types in state.iter(self.arguments) {
                    for alternative in expanded_types {
                        let mut expanded_argument = pre_expanded_types.clone();
                        expanded_argument.items[index] = alternative.clone();
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
}

/// Represents a single element of the expansion process for argument types for [`CallArgumentExpansions::iter`].
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
            items.push(CallArgument::new(argument, ty));
        }

        Self { items }
    }
}
