use super::Type;

/// Typed arguments for a single call, in source order.
#[derive(Clone, Debug, Default)]
pub(crate) struct CallArguments<'a, 'db>(Vec<Argument<'a, 'db>>);

impl<'a, 'db> CallArguments<'a, 'db> {
    /// Create a [`CallArguments`] from an iterator over non-variadic positional argument types.
    pub(crate) fn positional(positional_tys: impl IntoIterator<Item = Type<'db>>) -> Self {
        positional_tys
            .into_iter()
            .map(Argument::Positional)
            .collect()
    }

    /// Prepend an extra positional argument.
    pub(crate) fn with_self(&self, self_ty: Type<'db>) -> Self {
        let mut arguments = Vec::with_capacity(self.0.len() + 1);
        arguments.push(Argument::Synthetic(self_ty));
        arguments.extend_from_slice(&self.0);
        Self(arguments)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &Argument<'a, 'db>> {
        self.0.iter()
    }

    // TODO this should be eliminated in favor of [`bind_call`]
    pub(crate) fn first_argument(&self) -> Option<Type<'db>> {
        self.0.first().map(Argument::ty)
    }
}

impl<'db, 'a, 'b> IntoIterator for &'b CallArguments<'a, 'db> {
    type Item = &'b Argument<'a, 'db>;
    type IntoIter = std::slice::Iter<'b, Argument<'a, 'db>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'a, 'db> FromIterator<Argument<'a, 'db>> for CallArguments<'a, 'db> {
    fn from_iter<T: IntoIterator<Item = Argument<'a, 'db>>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Argument<'a, 'db> {
    /// The synthetic `self` or `cls` argument, which doesn't appear explicitly at the call site.
    Synthetic(Type<'db>),
    /// A positional argument.
    Positional(Type<'db>),
    /// A starred positional argument (e.g. `*args`).
    Variadic(Type<'db>),
    /// A keyword argument (e.g. `a=1`).
    Keyword { name: &'a str, ty: Type<'db> },
    /// The double-starred keywords argument (e.g. `**kwargs`).
    Keywords(Type<'db>),
}

impl<'db> Argument<'_, 'db> {
    fn ty(&self) -> Type<'db> {
        match self {
            Self::Synthetic(ty) => *ty,
            Self::Positional(ty) => *ty,
            Self::Variadic(ty) => *ty,
            Self::Keyword { name: _, ty } => *ty,
            Self::Keywords(ty) => *ty,
        }
    }
}
