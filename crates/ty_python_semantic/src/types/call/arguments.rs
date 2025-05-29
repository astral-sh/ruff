use std::borrow::Cow;
use std::ops::{Deref, DerefMut};

use itertools::{Either, Itertools};

use crate::Db;
use crate::types::{KnownClass, TupleType};

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

    pub(crate) fn expand(&self, db: &'db dyn Db) -> impl Iterator<Item = Vec<Vec<Type<'db>>>> + '_ {
        let mut index = 0;

        // TODO: Avoid cloning if there's no expansion needed
        std::iter::successors(Some(vec![self.types.clone()]), move |previous| {
            let expanded_types = loop {
                let arg_type = self.types.get(index)?;
                if let Some(expanded_types) = expand_type(db, *arg_type) {
                    break expanded_types;
                }
                index += 1;
            };

            // Generate all combinations by expanding the type at `index`
            let mut expanded_arg_types = Vec::with_capacity(expanded_types.len() * previous.len());

            for pre_expanded_types in previous {
                for subtype in &expanded_types {
                    let mut new_expanded_types = pre_expanded_types.clone();
                    new_expanded_types[index] = *subtype;
                    expanded_arg_types.push(new_expanded_types);
                }
            }

            index += 1;
            Some(expanded_arg_types)
        })
        .skip(1) // Skip the first iteration which is just the original types
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

fn expand_type<'db>(db: &'db dyn Db, ty: Type<'db>) -> Option<Vec<Type<'db>>> {
    // TODO: Expand enums to their variants
    Some(match ty {
        // We don't handle `type[A | B]` here because it's already stored in the expanded form
        // i.e., `type[A] | type[B]` which is handled by the `Type::Union` case.
        Type::NominalInstance(instance) if instance.class.is_known(db, KnownClass::Bool) => {
            vec![Type::BooleanLiteral(true), Type::BooleanLiteral(false)]
        }
        Type::Tuple(tuple) => {
            let expanded = tuple
                .iter(db)
                .map(|element| {
                    if let Some(expanded) = expand_type(db, element) {
                        Either::Left(expanded.into_iter())
                    } else {
                        Either::Right(std::iter::once(element))
                    }
                })
                // TODO: Use a custom implementation?
                .multi_cartesian_product()
                .map(|types| TupleType::from_elements(db, types))
                .collect::<Vec<_>>();
            if expanded.len() == 1 {
                return None;
            }
            expanded
        }
        Type::Union(union) => union.iter(db).copied().collect(),
        _ => return None,
    })
}
