use super::Type;
use ruff_python_ast::name::Name;
use std::collections::VecDeque;

/// Typed arguments for a single call, in source order.
#[derive(Clone, Debug, Default)]
pub(crate) struct CallArguments<'db>(VecDeque<Argument<'db>>);

impl<'db> CallArguments<'db> {
    /// Create a [`CallArguments`] from an iterator over [`Argument`]s.
    pub(crate) fn from_arguments(arguments: impl IntoIterator<Item = Argument<'db>>) -> Self {
        Self(arguments.into_iter().collect())
    }

    /// Create a [`CallArguments`] from an iterator over non-variadic positional argument types.
    pub(crate) fn positional(positional_tys: impl IntoIterator<Item = Type<'db>>) -> Self {
        Self::from_arguments(positional_tys.into_iter().map(Argument::Positional))
    }

    /// Prepend an extra positional argument.
    pub(crate) fn with_self(mut self, self_ty: Type<'db>) -> Self {
        self.0.push_front(Argument::Positional(self_ty));
        self
    }

    // TODO this should be eliminated in favor of [`bind_call`]
    pub(crate) fn first_argument(&self) -> Option<Type<'db>> {
        self.0.front().map(Argument::ty)
    }
}

impl<'db, 'a> IntoIterator for &'a CallArguments<'db> {
    type Item = &'a Argument<'db>;
    type IntoIter = std::collections::vec_deque::Iter<'a, Argument<'db>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Argument<'db> {
    /// A positional argument.
    Positional(Type<'db>),
    /// A starred positional argument (e.g. `*args`).
    Variadic(Type<'db>),
    /// A keyword argument (e.g. `a=1`).
    Keyword { name: Name, ty: Type<'db> },
    /// The double-starred keywords argument (e.g. `**kwargs`).
    Keywords(Type<'db>),
}

impl<'db> Argument<'db> {
    fn ty(&self) -> Type<'db> {
        match self {
            Self::Positional(ty) => *ty,
            Self::Variadic(ty) => *ty,
            Self::Keyword { name: _, ty } => *ty,
            Self::Keywords(ty) => *ty,
        }
    }
}
