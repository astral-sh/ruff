use itertools::Either;
use smallvec::{SmallVec, smallvec};

use crate::Db;
use crate::types::Type;

/// Describes the contents of a tuple.
///
/// At runtime, a Python tuple is a fixed-length immutable list of values. There is no restriction
/// on the types of the elements of a tuple value. In the type system, we want to model both
/// "heterogeneous" tuples that have elements of a fixed sequence of specific types, and
/// "homogenous" tuples that have an unknown number of elements of the same single type. And in
/// fact, we want to model tuples that are a combination of the two, with a heterogeneous prefix
/// and/or suffix, and a homogeneous portion of unknown length in between those.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum Tuple<'db> {
    FixedSize(SmallVec<[Type<'db>; 4]>),
    VariableSize {
        prefix: Vec<Type<'db>>,
        variable: Type<'db>,
        suffix: Vec<Type<'db>>,
    },
}

impl<'db> Tuple<'db> {
    pub(crate) fn empty() -> Self {
        Tuple::FixedSize(smallvec![])
    }

    pub(crate) fn fixed_size(types: impl IntoIterator<Item = Type<'db>>) -> Self {
        Tuple::FixedSize(types.into_iter().collect())
    }

    /// Returns an iterator of all of the element types of this tuple. Does not deduplicate the
    /// tuples, and does not distinguish between fixed- and variable-length elements.
    pub(crate) fn elements(&self) -> impl Iterator<Item = Type<'db>> + '_ {
        match self {
            Tuple::FixedSize(elements) => Either::Left(elements.iter().copied()),
            Tuple::VariableSize {
                prefix,
                variable,
                suffix,
            } => Either::Right(
                (prefix.iter())
                    .chain(std::iter::once(variable))
                    .chain(suffix)
                    .copied(),
            ),
        }
    }

    /// Returns the minimum and maximum length of this tuple. (The maximum length will be `None`
    /// for a tuple with a variable-length portion.)
    pub(crate) fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Tuple::FixedSize(elements) => {
                let len = elements.len();
                (len, Some(len))
            }
            Tuple::VariableSize { prefix, suffix, .. } => (prefix.len() + suffix.len(), None),
        }
    }

    /// Adds a sequence of fixed elements to the end of this tuple.
    pub(crate) fn extend(&mut self, types: impl Iterator<Item = Type<'db>>) {
        match self {
            Tuple::FixedSize(elements) => elements.extend(types),
            Tuple::VariableSize { suffix, .. } => suffix.extend(types),
        }
    }

    /// Adds a homogeneous, variable-sized element to the end of this tuple. Returns an error if
    /// the tuple already contains a homogeneous element of a different type, or if it contains a
    /// fixed-length suffix after an existing homogeneous element.
    pub(crate) fn extend_homogeneous(
        &mut self,
        variable: Type<'db>,
    ) -> Result<(), TupleError<'db>> {
        match self {
            Tuple::FixedSize(elements) => {
                *self = Tuple::VariableSize {
                    prefix: std::mem::take(elements).into_vec(),
                    variable,
                    suffix: vec![],
                };
                Ok(())
            }

            Tuple::VariableSize {
                variable: existing,
                suffix,
                ..
            } => {
                if *existing != variable {
                    return Err(TupleError::IncompatibleVariableLengthElements {
                        existing: *existing,
                        new: variable,
                    });
                }
                if !suffix.is_empty() {
                    return Err(TupleError::SuffixAfterVariableLengthElement);
                }
                Ok(())
            }
        }
    }

    #[must_use]
    pub(crate) fn normalized(&self, db: &'db dyn Db) -> Self {
        match self {
            Tuple::FixedSize(elements) => {
                Tuple::fixed_size(elements.iter().map(|ty| ty.normalized(db)))
            }

            Tuple::VariableSize {
                prefix,
                variable,
                suffix,
            } => Tuple::VariableSize {
                prefix: prefix.iter().map(|ty| ty.normalized(db)).collect(),
                variable: variable.normalized(db),
                suffix: suffix.iter().map(|ty| ty.normalized(db)).collect(),
            },
        }
    }

    pub(crate) fn is_equivalent_to(&self, db: &'db dyn Db, other: &Self) -> bool {
        match (self, other) {
            (Tuple::FixedSize(self_elements), Tuple::FixedSize(other_elements)) => {
                self_elements.len() == other_elements.len()
                    && (self_elements.iter())
                        .zip(other_elements)
                        .all(|(self_ty, other_ty)| self_ty.is_equivalent_to(db, *other_ty))
            }

            (
                Tuple::VariableSize {
                    prefix: self_prefix,
                    variable: self_variable,
                    suffix: self_suffix,
                },
                Tuple::VariableSize {
                    prefix: other_prefix,
                    variable: other_variable,
                    suffix: other_suffix,
                },
            ) => {
                self_prefix.len() == other_prefix.len()
                    && self_suffix.len() == other_suffix.len()
                    && (self_prefix.iter())
                        .zip(other_prefix)
                        .all(|(self_ty, other_ty)| self_ty.is_equivalent_to(db, *other_ty))
                    && self_variable.is_equivalent_to(db, *other_variable)
                    && (self_suffix.iter())
                        .zip(other_suffix)
                        .all(|(self_ty, other_ty)| self_ty.is_equivalent_to(db, *other_ty))
            }

            (Tuple::FixedSize(_), Tuple::VariableSize { .. })
            | (Tuple::VariableSize { .. }, Tuple::FixedSize(_)) => false,
        }
    }

    pub(crate) fn is_gradual_equivalent_to(&self, db: &'db dyn Db, other: &Self) -> bool {
        match (self, other) {
            (Tuple::FixedSize(self_elements), Tuple::FixedSize(other_elements)) => {
                self_elements.len() == other_elements.len()
                    && (self_elements.iter())
                        .zip(other_elements)
                        .all(|(self_ty, other_ty)| self_ty.is_gradual_equivalent_to(db, *other_ty))
            }

            (
                Tuple::VariableSize {
                    prefix: self_prefix,
                    variable: self_variable,
                    suffix: self_suffix,
                },
                Tuple::VariableSize {
                    prefix: other_prefix,
                    variable: other_variable,
                    suffix: other_suffix,
                },
            ) => {
                self_prefix.len() == other_prefix.len()
                    && self_suffix.len() == other_suffix.len()
                    && (self_prefix.iter())
                        .zip(other_prefix)
                        .all(|(self_ty, other_ty)| self_ty.is_gradual_equivalent_to(db, *other_ty))
                    && self_variable.is_gradual_equivalent_to(db, *other_variable)
                    && (self_suffix.iter())
                        .zip(other_suffix)
                        .all(|(self_ty, other_ty)| self_ty.is_gradual_equivalent_to(db, *other_ty))
            }

            (Tuple::FixedSize(_), Tuple::VariableSize { .. })
            | (Tuple::VariableSize { .. }, Tuple::FixedSize(_)) => false,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum TupleError<'db> {
    IncompatibleVariableLengthElements { existing: Type<'db>, new: Type<'db> },
    SuffixAfterVariableLengthElement,
}
