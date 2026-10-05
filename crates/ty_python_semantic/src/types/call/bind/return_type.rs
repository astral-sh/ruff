//! Call return types retain the callable union/intersection structure during assembly.

use std::convert::Infallible;
use std::slice;

use super::{Bindings, BindingsElement, CallableItem};
use crate::types::Type;
use crate::types::set_theoretic::assembly::{
    InlineTypeAssembly, TypeAssemblyEffects, TypeElements, intersection_from_elements,
    union_from_elements,
};
use crate::{Db, ProgramEnvironment};

pub(in crate::types) trait ReturnTypeEffects<'db>:
    TypeAssemblyEffects<'db>
{
    async fn local<T>(
        &self,
        work: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;

    async fn constructor_return<T>(&self, action: impl FnOnce() -> T) -> Result<T, Self::Error>;
}

pub(super) struct InlineReturnTypeEffects;

impl<'db> ReturnTypeEffects<'db> for InlineReturnTypeEffects {
    async fn local<T>(
        &self,
        _work: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Infallible> {
        Ok(action())
    }

    async fn constructor_return<T>(&self, action: impl FnOnce() -> T) -> Result<T, Infallible> {
        Ok(action())
    }
}

impl<'db> TypeAssemblyEffects<'db> for InlineReturnTypeEffects {
    type Error = Infallible;

    async fn union<I: TypeElements<'db, Error = Infallible>>(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        first: I::Item,
        second: I::Item,
        remaining: &mut I,
    ) -> Result<Type<'db>, Infallible> {
        InlineTypeAssembly
            .union(db, env, first, second, remaining)
            .await
    }

    async fn intersection<I: TypeElements<'db, Error = Infallible>>(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        first: I::Item,
        second: I::Item,
        remaining: &mut I,
    ) -> Result<Type<'db>, Infallible> {
        InlineTypeAssembly
            .intersection(db, env, first, second, remaining)
            .await
    }
}

impl<'db> Bindings<'db> {
    pub(in crate::types) async fn return_type_with<E: ReturnTypeEffects<'db>>(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        let mut elements = ElementReturnTypes {
            db,
            env,
            remaining: self.elements.iter(),
            effects,
        };
        union_from_elements(db, env, &mut elements, effects).await
    }
}

impl<'db> BindingsElement<'db> {
    pub(super) async fn return_type_with<E: ReturnTypeEffects<'db>>(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        if effects
            .local(self.items.len().checked_add(1), || self.is_callable())
            .await?
        {
            let mut items = ItemReturnTypes {
                db,
                env,
                remaining: self.items.iter(),
                effects,
            };
            intersection_from_elements(db, env, &mut items, effects).await
        } else {
            Ok(Type::unknown())
        }
    }
}

impl<'db> CallableItem<'db> {
    pub(super) async fn return_type_with<E: ReturnTypeEffects<'db>>(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        match self {
            CallableItem::Regular(binding) => {
                let work = effects
                    .local(binding.overloads.len().checked_add(1), || {
                        binding.overloads.iter().try_fold(4usize, |work, overload| {
                            work.checked_add(overload.errors.len())?.checked_add(2)
                        })
                    })
                    .await?;
                effects.local(work, || binding.return_type()).await
            }
            CallableItem::Constructor(binding) => {
                effects
                    .constructor_return(|| binding.return_type(db, env))
                    .await
            }
        }
    }
}

struct ElementReturnTypes<'a, 'db, E> {
    db: &'db dyn Db,
    env: &'a ProgramEnvironment<'db>,
    remaining: slice::Iter<'a, BindingsElement<'db>>,
    effects: &'a E,
}

impl<'db, E: ReturnTypeEffects<'db>> TypeElements<'db> for ElementReturnTypes<'_, 'db, E> {
    type Error = E::Error;
    type Item = Type<'db>;

    async fn next(&mut self) -> Result<Option<Type<'db>>, E::Error> {
        let element = self
            .effects
            .local(Some(2), || self.remaining.next())
            .await?;
        match element {
            Some(element) => element
                .return_type_with(self.db, self.env, self.effects)
                .await
                .map(Some),
            None => Ok(None),
        }
    }
}

struct ItemReturnTypes<'a, 'db, E> {
    db: &'db dyn Db,
    env: &'a ProgramEnvironment<'db>,
    remaining: slice::Iter<'a, CallableItem<'db>>,
    effects: &'a E,
}

impl<'db, E: ReturnTypeEffects<'db>> TypeElements<'db> for ItemReturnTypes<'_, 'db, E> {
    type Error = E::Error;
    type Item = Type<'db>;

    async fn next(&mut self) -> Result<Option<Type<'db>>, E::Error> {
        loop {
            let item = self
                .effects
                .local(Some(3), || {
                    self.remaining.next().map(|item| (item, item.is_callable()))
                })
                .await?;
            match item {
                Some((item, true)) => {
                    return item
                        .return_type_with(self.db, self.env, self.effects)
                        .await
                        .map(Some);
                }
                Some((_, false)) => {}
                None => return Ok(None),
            }
        }
    }
}
