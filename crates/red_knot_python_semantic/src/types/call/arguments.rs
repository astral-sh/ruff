use std::collections::VecDeque;
use std::ops::{Deref, DerefMut};

use super::Type;

/// Arguments for a single call, in source order.
#[derive(Clone, Debug, Default)]
pub(crate) struct CallArguments<'a>(VecDeque<Argument<'a>>);

impl<'a> CallArguments<'a> {
    /// Invoke a function with an optional extra synthetic argument (for a `self` or `cls`
    /// parameter) prepended to the front of this argument list. (If `bound_self` is none, the
    /// function is invoked with the unmodified argument list.)
    pub(crate) fn with_self<F, R>(&mut self, bound_self: Option<Type<'_>>, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        if bound_self.is_some() {
            self.0.push_front(Argument::Synthetic);
        }
        let result = f(self);
        if bound_self.is_some() {
            self.0.pop_front();
        }
        result
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
pub(crate) struct CallArgumentTypes<'a, 'db> {
    arguments: CallArguments<'a>,
    types: VecDeque<Type<'db>>,
}

impl<'a, 'db> CallArgumentTypes<'a, 'db> {
    /// Create a [`CallArgumentTypes`] with no arguments.
    pub(crate) fn none() -> Self {
        let arguments = CallArguments::default();
        let types = VecDeque::default();
        Self { arguments, types }
    }

    /// Create a [`CallArgumentTypes`] from an iterator over non-variadic positional argument
    /// types.
    pub(crate) fn positional(positional_tys: impl IntoIterator<Item = Type<'db>>) -> Self {
        let types: VecDeque<_> = positional_tys.into_iter().collect();
        let arguments = CallArguments(vec![Argument::Positional; types.len()].into());
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

    /// Invoke a function with an optional extra synthetic argument (for a `self` or `cls`
    /// parameter) prepended to the front of this argument list. (If `bound_self` is none, the
    /// function is invoked with the unmodified argument list.)
    pub(crate) fn with_self<F, R>(&mut self, bound_self: Option<Type<'db>>, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        if let Some(bound_self) = bound_self {
            self.arguments.0.push_front(Argument::Synthetic);
            self.types.push_front(bound_self);
        }
        let result = f(self);
        if bound_self.is_some() {
            self.arguments.0.pop_front();
            self.types.pop_front();
        }
        result
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (Argument<'a>, Type<'db>)> + '_ {
        self.arguments.iter().zip(self.types.iter().copied())
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
