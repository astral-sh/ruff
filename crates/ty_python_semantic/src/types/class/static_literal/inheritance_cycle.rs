use std::convert::Infallible;

use super::{InheritanceCycle, StaticClassLiteral};
use crate::types::Type;
use crate::{Db, FxIndexSet};

pub(in crate::types) struct Frame<'db> {
    bases: &'db [Type<'db>],
    next: usize,
    cyclic: bool,
    introduced_base: bool,
}

#[cfg(feature = "experimental-analysis")]
#[derive(Default)]
pub(in crate::types) struct Backing {
    pub table_slots: usize,
    pub ordered_capacity: usize,
    pub peak_attempt: usize,
}

#[derive(Default)]
pub(in crate::types) struct Traversal<'db> {
    pub frames: Vec<Frame<'db>>,
    pub active: FxIndexSet<StaticClassLiteral<'db>>,
    pub visited: FxIndexSet<StaticClassLiteral<'db>>,
    #[cfg(feature = "experimental-analysis")]
    pub active_backing: Backing,
    #[cfg(feature = "experimental-analysis")]
    pub visited_backing: Backing,
    cyclic: bool,
    #[cfg(all(test, feature = "experimental-analysis"))]
    pub retirement_observer: Option<fn()>,
}

#[cfg(all(test, feature = "experimental-analysis"))]
impl Drop for Traversal<'_> {
    fn drop(&mut self) {
        if let Some(observer) = self.retirement_observer {
            observer();
        }
    }
}

pub(in crate::types) enum Step<'db> {
    Base(Type<'db>),
    Continue,
}

pub(in crate::types) enum Enter {
    Descend,
    Revisited,
    Active,
}

impl<'db> Traversal<'db> {
    pub(in crate::types) fn push(&mut self, bases: &'db [Type<'db>], introduced_base: bool) {
        self.frames.push(Frame {
            bases,
            next: 0,
            cyclic: false,
            introduced_base,
        });
    }

    pub(in crate::types) fn next(&mut self) -> Option<Step<'db>> {
        if let Some(frame) = self.frames.last_mut()
            && let Some(base) = frame.bases.get(frame.next)
        {
            frame.next += 1;
            return Some(Step::Base(*base));
        }
        let Some(frame) = self.frames.pop() else {
            return None;
        };
        if frame.introduced_base {
            self.active.pop();
        }
        if let Some(parent) = self.frames.last_mut() {
            // If we find a cycle, keep searching to check if we can reach the starting
            // class.
            parent.cyclic |= frame.cyclic;
            Some(Step::Continue)
        } else {
            self.cyclic = frame.cyclic;
            None
        }
    }

    pub(in crate::types) fn enter(&mut self, class: StaticClassLiteral<'db>) -> Enter {
        if !self.active.insert(class) {
            if let Some(frame) = self.frames.last_mut() {
                frame.cyclic = true;
                frame.next = frame.bases.len();
            }
            return Enter::Active;
        }
        if self.visited.insert(class) {
            Enter::Descend
        } else {
            self.active.pop();
            Enter::Revisited
        }
    }

    pub(in crate::types) fn classify(
        &self,
        root: StaticClassLiteral<'db>,
    ) -> Option<InheritanceCycle> {
        if !self.cyclic {
            None
        } else if self.visited.contains(&root) {
            Some(InheritanceCycle::Participant)
        } else {
            Some(InheritanceCycle::Inherited)
        }
    }
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousCycleTraversalEffects)]
    pub(in crate::types) trait CycleTraversalEffects<'db> {
        type Error;

        #[operation(local)]
        async fn start(&self) -> Result<Traversal<'db>, Self::Error>;
        #[operation(child)]
        async fn explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<&'db [Type<'db>], Self::Error>;
        #[operation(source)]
        async fn base_class(&self, base: Type<'db>) -> Result<Option<StaticClassLiteral<'db>>, Self::Error>;
        #[operation(local)]
        async fn push(&self, traversal: &mut Traversal<'db>, bases: &'db [Type<'db>], introduced_base: bool) -> Result<(), Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next(&self, traversal: &mut Traversal<'db>) -> Result<Option<Step<'db>>, Self::Error>;
        #[operation(local)]
        async fn enter(&self, traversal: &mut Traversal<'db>, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn classify(&self, traversal: &Traversal<'db>, root: StaticClassLiteral<'db>) -> Result<Option<InheritanceCycle>, Self::Error>;
    }

    #[synchronous(inheritance_cycle_inner_sync)]
    #[capabilities(effects = CycleTraversalEffects)]
    #[passive_values()]
    pub(in crate::types) async fn inheritance_cycle_inner_with<'db, E: CycleTraversalEffects<'db>>(
        class: StaticClassLiteral<'db>,
        effects: &E,
    ) -> Result<Option<InheritanceCycle>, E::Error> {
        let mut traversal = effects.start().await?;
        let bases = effects.explicit_bases(class).await?;
        effects.push(&mut traversal, bases, false).await?;
        #[cursor_loop]
        while let Some(step) = effects.next(&mut traversal).await? {
            match step {
                Step::Base(base) => {
                    if let Some(class) = effects.base_class(base).await?
                        && effects.enter(&mut traversal, class).await?
                    {
                        let bases = effects.explicit_bases(class).await?;
                        effects.push(&mut traversal, bases, true).await?;
                    }
                }
                Step::Continue => {}
            }
        }
        effects.classify(&traversal, class).await
    }
}

pub(super) struct OrdinaryCycleTraversal<'db>(pub &'db dyn Db);

impl<'db> SynchronousCycleTraversalEffects<'db> for OrdinaryCycleTraversal<'db> {
    type Error = Infallible;

    fn start(&self) -> Result<Traversal<'db>, Infallible> {
        Ok(Traversal::default())
    }

    fn explicit_bases(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db [Type<'db>], Infallible> {
        Ok(class.explicit_bases(self.0))
    }

    fn base_class(&self, base: Type<'db>) -> Result<Option<StaticClassLiteral<'db>>, Infallible> {
        Ok(match base {
            Type::ClassLiteral(class) => class.as_static(),
            Type::GenericAlias(alias) => Some(alias.origin(self.0)),
            _ => None,
        })
    }

    fn push(
        &self,
        traversal: &mut Traversal<'db>,
        bases: &'db [Type<'db>],
        introduced_base: bool,
    ) -> Result<(), Infallible> {
        traversal.push(bases, introduced_base);
        Ok(())
    }

    fn next(&self, traversal: &mut Traversal<'db>) -> Result<Option<Step<'db>>, Infallible> {
        Ok(traversal.next())
    }

    fn enter(
        &self,
        traversal: &mut Traversal<'db>,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, Infallible> {
        Ok(matches!(traversal.enter(class), Enter::Descend))
    }

    fn classify(
        &self,
        traversal: &Traversal<'db>,
        root: StaticClassLiteral<'db>,
    ) -> Result<Option<InheritanceCycle>, Infallible> {
        Ok(traversal.classify(root))
    }
}
