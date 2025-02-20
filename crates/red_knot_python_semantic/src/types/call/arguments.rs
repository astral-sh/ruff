use ruff_python_ast::name::Name;

use crate::Db;

use super::Type;

/// Typed arguments for a single call, in source order.
#[salsa::interned]
pub(crate) struct CallArguments<'db> {
    args: Vec<Argument<'db>>,
}

impl<'a, 'db> CallArguments<'db> {
    /// Create a [`CallArguments`] with no arguments.
    pub(crate) fn none(db: &'db dyn Db) -> Self {
        CallArguments::new(db, Vec::new())
    }

    /// Create a [`CallArguments`] from an iterator over non-variadic positional argument types.
    pub(crate) fn positional(
        db: &'db dyn Db,
        positional_tys: impl IntoIterator<Item = Type<'db>>,
    ) -> Self {
        CallArguments::new(
            db,
            positional_tys
                .into_iter()
                .map(Argument::Positional)
                .collect::<Vec<_>>(),
        )
    }

    /// Prepend an extra positional argument.
    pub(crate) fn with_self(&self, db: &'db dyn Db, self_ty: Type<'db>) -> Self {
        let mut arguments = Vec::with_capacity(self.args(db).len() + 1);
        arguments.push(Argument::Synthetic(self_ty));
        arguments.extend_from_slice(&self.args(db));
        CallArguments::new(db, arguments)
    }

    pub(crate) fn args_iter(&self, db: &'db dyn Db) -> impl IntoIterator<Item = Argument<'db>> {
        self.args(db).into_iter()
    }

    // TODO this should be eliminated in favor of [`bind_call`]
    pub(crate) fn first_argument(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        self.args(db).first().map(Argument::ty)
    }

    // TODO this should be eliminated in favor of [`bind_call`]
    pub(crate) fn second_argument(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        self.args(db).get(1).map(Argument::ty)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, salsa::Update)]
pub(crate) enum Argument<'db> {
    /// The synthetic `self` or `cls` argument, which doesn't appear explicitly at the call site.
    Synthetic(Type<'db>),
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
            Self::Synthetic(ty) => *ty,
            Self::Positional(ty) => *ty,
            Self::Variadic(ty) => *ty,
            Self::Keyword { name: _, ty } => *ty,
            Self::Keywords(ty) => *ty,
        }
    }
}
