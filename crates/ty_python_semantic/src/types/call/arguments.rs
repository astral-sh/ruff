use std::borrow::Cow;

use itertools::{Either, Itertools};
use ruff_python_ast as ast;

use crate::Db;
use crate::types::KnownClass;
use crate::types::enums::enum_member_literals;
use crate::types::tuple::{TupleLength, TupleSpec};

use super::Type;

#[derive(Clone, Copy, Debug)]
pub(crate) enum Argument<'a> {
    /// The synthetic `self` or `cls` argument, which doesn't appear explicitly at the call site.
    Synthetic,
    /// A positional argument.
    Positional,
    /// A starred positional argument (e.g. `*args`) containing the specified number of elements.
    Variadic(TupleLength),
    /// A keyword argument (e.g. `a=1`).
    Keyword(&'a str),
    /// The double-starred keywords argument (e.g. `**kwargs`).
    Keywords,
}

/// Arguments for a single call, in source order, along with inferred types for each argument.
#[derive(Clone, Debug, Default)]
pub(crate) struct CallArguments<'a, 'db> {
    arguments: Vec<Argument<'a>>,
    types: Vec<Option<Type<'db>>>,
}

impl<'a, 'db> CallArguments<'a, 'db> {
    fn new(arguments: Vec<Argument<'a>>, types: Vec<Option<Type<'db>>>) -> Self {
        debug_assert!(arguments.len() == types.len());
        Self { arguments, types }
    }

    /// Create `CallArguments` from AST arguments. We will use the provided callback to obtain the
    /// type of each splatted argument, so that we can determine its length. All other arguments
    /// will remain uninitialized as `Unknown`.
    pub(crate) fn from_arguments(
        db: &'db dyn Db,
        arguments: &'a ast::Arguments,
        mut infer_argument_type: impl FnMut(&ast::Expr, &ast::Expr) -> Type<'db>,
    ) -> Self {
        arguments
            .arguments_source_order()
            .map(|arg_or_keyword| match arg_or_keyword {
                ast::ArgOrKeyword::Arg(arg) => match arg {
                    ast::Expr::Starred(ast::ExprStarred { value, .. }) => {
                        let ty = infer_argument_type(arg, value);
                        let length = ty
                            .try_iterate(db)
                            .map(|tuple| tuple.len())
                            .unwrap_or(TupleLength::unknown());
                        (Argument::Variadic(length), Some(ty))
                    }
                    _ => (Argument::Positional, None),
                },
                ast::ArgOrKeyword::Keyword(ast::Keyword { arg, .. }) => {
                    if let Some(arg) = arg {
                        (Argument::Keyword(&arg.id), None)
                    } else {
                        (Argument::Keywords, None)
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
        let types: Vec<_> = positional_tys.into_iter().map(Some).collect();
        let arguments = vec![Argument::Positional; types.len()];
        Self { arguments, types }
    }

    pub(crate) fn len(&self) -> usize {
        self.arguments.len()
    }

    pub(crate) fn types(&self) -> &[Option<Type<'db>>] {
        &self.types
    }

    pub(crate) fn iter_types(&self) -> impl Iterator<Item = Type<'db>> {
        self.types.iter().map(|ty| ty.unwrap_or_else(Type::unknown))
    }

    /// Prepend an optional extra synthetic argument (for a `self` or `cls` parameter) to the front
    /// of this argument list. (If `bound_self` is none, we return the argument list
    /// unmodified.)
    pub(crate) fn with_self(&self, bound_self: Option<Type<'db>>) -> Cow<'_, Self> {
        if bound_self.is_some() {
            let arguments = std::iter::once(Argument::Synthetic)
                .chain(self.arguments.iter().copied())
                .collect();
            let types = std::iter::once(bound_self)
                .chain(self.types.iter().copied())
                .collect();
            Cow::Owned(CallArguments { arguments, types })
        } else {
            Cow::Borrowed(self)
        }
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (Argument<'a>, Option<Type<'db>>)> + '_ {
        (self.arguments.iter().copied()).zip(self.types.iter().copied())
    }

    pub(crate) fn iter_mut(
        &mut self,
    ) -> impl Iterator<Item = (Argument<'a>, &mut Option<Type<'db>>)> + '_ {
        (self.arguments.iter().copied()).zip(self.types.iter_mut())
    }

    /// Returns an iterator on performing [argument type expansion].
    ///
    /// Each element of the iterator represents a set of argument lists, where each argument list
    /// contains the same arguments, but with one or more of the argument types expanded.
    ///
    /// [argument type expansion]: https://typing.python.org/en/latest/spec/overload.html#argument-type-expansion
    pub(crate) fn expand(
        &self,
        db: &'db dyn Db,
    ) -> impl Iterator<Item = Vec<CallArguments<'a, 'db>>> + '_ {
        /// Represents the state of the expansion process.
        ///
        /// This is useful to avoid cloning the initial types vector if none of the types can be
        /// expanded.
        enum State<'a, 'b, 'db> {
            Initial(&'b Vec<Option<Type<'db>>>),
            Expanded(Vec<CallArguments<'a, 'db>>),
        }

        impl<'db> State<'_, '_, 'db> {
            fn len(&self) -> usize {
                match self {
                    State::Initial(_) => 1,
                    State::Expanded(expanded) => expanded.len(),
                }
            }

            fn iter(&self) -> impl Iterator<Item = &[Option<Type<'db>>]> + '_ {
                match self {
                    State::Initial(types) => Either::Left(std::iter::once(types.as_slice())),
                    State::Expanded(expanded) => {
                        Either::Right(expanded.iter().map(CallArguments::types))
                    }
                }
            }
        }

        let mut index = 0;

        std::iter::successors(Some(State::Initial(&self.types)), move |previous| {
            // Find the next type that can be expanded.
            let expanded_types = loop {
                let arg_type = self.types.get(index)?;
                if let Some(arg_type) = arg_type {
                    if let Some(expanded_types) = expand_type(db, *arg_type) {
                        break expanded_types;
                    }
                }
                index += 1;
            };

            let mut expanded_arguments = Vec::with_capacity(expanded_types.len() * previous.len());

            for pre_expanded_types in previous.iter() {
                for subtype in &expanded_types {
                    let mut new_expanded_types = pre_expanded_types.to_vec();
                    new_expanded_types[index] = Some(*subtype);
                    expanded_arguments.push(CallArguments::new(
                        self.arguments.clone(),
                        new_expanded_types,
                    ));
                }
            }

            // Increment the index to move to the next argument type for the next iteration.
            index += 1;

            Some(State::Expanded(expanded_arguments))
        })
        .skip(1) // Skip the initial state, which has no expanded types.
        .map(|state| match state {
            State::Initial(_) => unreachable!("initial state should be skipped"),
            State::Expanded(expanded) => expanded,
        })
    }
}

impl<'a, 'db> FromIterator<(Argument<'a>, Option<Type<'db>>)> for CallArguments<'a, 'db> {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = (Argument<'a>, Option<Type<'db>>)>,
    {
        let (arguments, types) = iter.into_iter().unzip();
        Self { arguments, types }
    }
}

/// Expands a type into its possible subtypes, if applicable.
///
/// Returns [`None`] if the type cannot be expanded.
fn expand_type<'db>(db: &'db dyn Db, ty: Type<'db>) -> Option<Vec<Type<'db>>> {
    match ty {
        Type::NominalInstance(instance) => {
            if instance.class.is_known(db, KnownClass::Bool) {
                return Some(vec![
                    Type::BooleanLiteral(true),
                    Type::BooleanLiteral(false),
                ]);
            }

            let class_literal = instance.class.class_literal(db).0;

            if let Some(enum_members) = enum_member_literals(db, class_literal, None) {
                return Some(enum_members.collect());
            }

            None
        }
        Type::Tuple(tuple_type) => {
            // Note: This should only account for tuples of known length, i.e., `tuple[bool, ...]`
            // should not be expanded here.
            let tuple = tuple_type.tuple(db);
            if !matches!(tuple, TupleSpec::Fixed(_)) {
                return None;
            }
            let expanded = tuple
                .all_elements()
                .map(|element| {
                    if let Some(expanded) = expand_type(db, *element) {
                        Either::Left(expanded.into_iter())
                    } else {
                        Either::Right(std::iter::once(*element))
                    }
                })
                .multi_cartesian_product()
                .map(|types| Type::heterogeneous_tuple(db, types))
                .collect::<Vec<_>>();
            if expanded.len() == 1 {
                // There are no elements in the tuple type that can be expanded.
                None
            } else {
                Some(expanded)
            }
        }
        Type::Union(union) => Some(union.iter(db).copied().collect()),
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
        let expected_types = [Type::BooleanLiteral(true), Type::BooleanLiteral(false)];
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
        let true_ty = Type::BooleanLiteral(true);
        let false_ty = Type::BooleanLiteral(false);

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
