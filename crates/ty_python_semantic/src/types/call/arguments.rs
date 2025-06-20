use std::borrow::Cow;
use std::ops::{Deref, DerefMut};

use itertools::{Either, Itertools};

use crate::Db;
use crate::types::KnownClass;
use crate::types::tuple::{TupleSpec, TupleType};

use super::Type;

/// Arguments for a single call, in source order.
#[derive(Clone, Debug, Default)]
pub(crate) struct CallArguments<'a>(Vec<Argument<'a>>);

impl<'a> CallArguments<'a> {
    /// Prepend an optional extra synthetic argument (for a `self` or `cls` parameter) to the front
    /// of this argument list. (If `bound_self` is none, we return the argument list
    /// unmodified.)
    pub(crate) fn with_self(&self, bound_self: Option<Type<'_>>) -> Cow<Self> {
        if bound_self.is_some() {
            let arguments = std::iter::once(Argument::Synthetic)
                .chain(self.0.iter().copied())
                .collect();
            Cow::Owned(CallArguments(arguments))
        } else {
            Cow::Borrowed(self)
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = Argument<'a>> + '_ {
        self.0.iter().copied()
    }
}

impl<'a> FromIterator<Argument<'a>> for CallArguments<'a> {
    fn from_iter<T: IntoIterator<Item = Argument<'a>>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Argument<'a> {
    /// The synthetic `self` or `cls` argument, which doesn't appear explicitly at the call site.
    Synthetic,
    /// A positional argument.
    Positional,
    /// A starred positional argument (e.g. `*args`).
    Variadic,
    /// A keyword argument (e.g. `a=1`).
    Keyword(&'a str),
    /// The double-starred keywords argument (e.g. `**kwargs`).
    Keywords,
}

/// Arguments for a single call, in source order, along with inferred types for each argument.
#[derive(Clone, Debug, Default)]
pub(crate) struct CallArgumentTypes<'a, 'db> {
    arguments: CallArguments<'a>,
    types: Vec<Type<'db>>,
}

impl<'a, 'db> CallArgumentTypes<'a, 'db> {
    /// Create a [`CallArgumentTypes`] with no arguments.
    pub(crate) fn none() -> Self {
        Self::default()
    }

    /// Create a [`CallArgumentTypes`] from an iterator over non-variadic positional argument
    /// types.
    pub(crate) fn positional(positional_tys: impl IntoIterator<Item = Type<'db>>) -> Self {
        let types: Vec<_> = positional_tys.into_iter().collect();
        let arguments = CallArguments(vec![Argument::Positional; types.len()]);
        Self { arguments, types }
    }

    /// Create a new [`CallArgumentTypes`] to store the inferred types of the arguments in a
    /// [`CallArguments`]. Uses the provided callback to infer each argument type.
    pub(crate) fn new<F>(arguments: CallArguments<'a>, mut f: F) -> Self
    where
        F: FnMut(usize, Argument<'a>) -> Type<'db>,
    {
        let types = arguments
            .iter()
            .enumerate()
            .map(|(idx, argument)| f(idx, argument))
            .collect();
        Self { arguments, types }
    }

    pub(crate) fn types(&self) -> &[Type<'db>] {
        &self.types
    }

    /// Prepend an optional extra synthetic argument (for a `self` or `cls` parameter) to the front
    /// of this argument list. (If `bound_self` is none, we return the argument list
    /// unmodified.)
    pub(crate) fn with_self(&self, bound_self: Option<Type<'db>>) -> Cow<Self> {
        if let Some(bound_self) = bound_self {
            let arguments = CallArguments(
                std::iter::once(Argument::Synthetic)
                    .chain(self.arguments.0.iter().copied())
                    .collect(),
            );
            let types = std::iter::once(bound_self)
                .chain(self.types.iter().copied())
                .collect();
            Cow::Owned(CallArgumentTypes { arguments, types })
        } else {
            Cow::Borrowed(self)
        }
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (Argument<'a>, Type<'db>)> + '_ {
        self.arguments.iter().zip(self.types.iter().copied())
    }

    /// Returns an iterator on performing [argument type expansion].
    ///
    /// Each element of the iterator represents a set of argument lists, where each argument list
    /// contains the same arguments, but with one or more of the argument types expanded.
    ///
    /// [argument type expansion]: https://typing.python.org/en/latest/spec/overload.html#argument-type-expansion
    pub(crate) fn expand(&self, db: &'db dyn Db) -> impl Iterator<Item = Vec<Vec<Type<'db>>>> + '_ {
        /// Represents the state of the expansion process.
        ///
        /// This is useful to avoid cloning the initial types vector if none of the types can be
        /// expanded.
        enum State<'a, 'db> {
            Initial(&'a Vec<Type<'db>>),
            Expanded(Vec<Vec<Type<'db>>>),
        }

        impl<'db> State<'_, 'db> {
            fn len(&self) -> usize {
                match self {
                    State::Initial(_) => 1,
                    State::Expanded(expanded) => expanded.len(),
                }
            }

            fn iter(&self) -> impl Iterator<Item = &Vec<Type<'db>>> + '_ {
                match self {
                    State::Initial(types) => std::slice::from_ref(*types).iter(),
                    State::Expanded(expanded) => expanded.iter(),
                }
            }
        }

        let mut index = 0;

        std::iter::successors(Some(State::Initial(&self.types)), move |previous| {
            // Find the next type that can be expanded.
            let expanded_types = loop {
                let arg_type = self.types.get(index)?;
                if let Some(expanded_types) = expand_type(db, *arg_type) {
                    break expanded_types;
                }
                index += 1;
            };

            let mut expanded_arg_types = Vec::with_capacity(expanded_types.len() * previous.len());

            for pre_expanded_types in previous.iter() {
                for subtype in &expanded_types {
                    let mut new_expanded_types = pre_expanded_types.clone();
                    new_expanded_types[index] = *subtype;
                    expanded_arg_types.push(new_expanded_types);
                }
            }

            // Increment the index to move to the next argument type for the next iteration.
            index += 1;

            Some(State::Expanded(expanded_arg_types))
        })
        .skip(1) // Skip the initial state, which has no expanded types.
        .map(|state| match state {
            State::Initial(_) => unreachable!("initial state should be skipped"),
            State::Expanded(expanded) => expanded,
        })
    }
}

impl<'a> Deref for CallArgumentTypes<'a, '_> {
    type Target = CallArguments<'a>;
    fn deref(&self) -> &CallArguments<'a> {
        &self.arguments
    }
}

impl<'a> DerefMut for CallArgumentTypes<'a, '_> {
    fn deref_mut(&mut self) -> &mut CallArguments<'a> {
        &mut self.arguments
    }
}

/// Expands a type into its possible subtypes, if applicable.
///
/// Returns [`None`] if the type cannot be expanded.
fn expand_type<'db>(db: &'db dyn Db, ty: Type<'db>) -> Option<Vec<Type<'db>>> {
    // TODO: Expand enums to their variants
    match ty {
        Type::NominalInstance(instance) if instance.class.is_known(db, KnownClass::Bool) => {
            Some(vec![
                Type::BooleanLiteral(true),
                Type::BooleanLiteral(false),
            ])
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
                    if let Some(expanded) = expand_type(db, element) {
                        Either::Left(expanded.into_iter())
                    } else {
                        Either::Right(std::iter::once(element))
                    }
                })
                .multi_cartesian_product()
                .map(|types| TupleType::from_elements(db, types))
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
        let empty_tuple = TupleType::empty(&db);
        let expanded = expand_type(&db, empty_tuple);
        assert!(expanded.is_none());

        // None of the elements can be expanded.
        let tuple_type1 = TupleType::from_elements(&db, [int_ty, str_ty]);
        let expanded = expand_type(&db, tuple_type1);
        assert!(expanded.is_none());

        // All elements can be expanded.
        let tuple_type2 = TupleType::from_elements(
            &db,
            [
                bool_ty,
                UnionType::from_elements(&db, [int_ty, str_ty, bytes_ty]),
            ],
        );
        let expected_types = [
            TupleType::from_elements(&db, [true_ty, int_ty]),
            TupleType::from_elements(&db, [true_ty, str_ty]),
            TupleType::from_elements(&db, [true_ty, bytes_ty]),
            TupleType::from_elements(&db, [false_ty, int_ty]),
            TupleType::from_elements(&db, [false_ty, str_ty]),
            TupleType::from_elements(&db, [false_ty, bytes_ty]),
        ];
        let expanded = expand_type(&db, tuple_type2).unwrap();
        assert_eq!(expanded, expected_types);

        // Mixed set of elements where some can be expanded while others cannot be.
        let tuple_type3 = TupleType::from_elements(
            &db,
            [
                bool_ty,
                int_ty,
                UnionType::from_elements(&db, [str_ty, bytes_ty]),
                str_ty,
            ],
        );
        let expected_types = [
            TupleType::from_elements(&db, [true_ty, int_ty, str_ty, str_ty]),
            TupleType::from_elements(&db, [true_ty, int_ty, bytes_ty, str_ty]),
            TupleType::from_elements(&db, [false_ty, int_ty, str_ty, str_ty]),
            TupleType::from_elements(&db, [false_ty, int_ty, bytes_ty, str_ty]),
        ];
        let expanded = expand_type(&db, tuple_type3).unwrap();
        assert_eq!(expanded, expected_types);

        // Variable-length tuples are not expanded.
        let variable_length_tuple = TupleType::mixed(
            &db,
            [bool_ty],
            int_ty,
            [UnionType::from_elements(&db, [str_ty, bytes_ty]), str_ty],
        );
        let expanded = expand_type(&db, variable_length_tuple);
        assert!(expanded.is_none());
    }
}
