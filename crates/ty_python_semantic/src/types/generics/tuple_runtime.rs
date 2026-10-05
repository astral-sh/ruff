//! Runtime tuple-element specialization with admission before local construction.

use std::convert::Infallible;

use super::{GenericContext, Specialization};
use crate::Db;
use crate::types::tuple::{TupleSpec, TupleType, VariableSegment};
use crate::types::{MaterializationKind, Type};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum TupleRuntimeWork {
    Inspect,
    Intern,
    Publish,
}

pub(in crate::types) trait TupleRuntimeControl {
    type Error;

    fn checkpoint(&self, work: TupleRuntimeWork) -> Result<(), Self::Error>;
}

pub(in crate::types) struct TupleRuntimeFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousTupleRuntimeEffects)]
    pub(in crate::types) trait TupleRuntimeEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self, work: TupleRuntimeWork) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn tuple_inner(&self, specialization: Specialization<'db>) -> Result<Option<TupleType<'db>>, Self::Error>;
        #[operation(source)]
        async fn tuple_spec(&self, tuple: TupleType<'db>) -> Result<&'db TupleSpec<'db>, Self::Error>;
        #[operation(source)]
        async fn generic_context(&self, specialization: Specialization<'db>) -> Result<GenericContext<'db>, Self::Error>;
        #[operation(source)]
        async fn materialization_kind(&self, specialization: Specialization<'db>) -> Result<Option<MaterializationKind>, Self::Error>;
        #[operation(source)]
        async fn intern(&self, context: GenericContext<'db>, types: [Type<'db>; 1], kind: Option<MaterializationKind>, tuple: Option<TupleType<'db>>) -> Result<Specialization<'db>, Self::Error>;
    }

    #[finite_capability]
    impl TupleRuntimeFacts {
        fn symbolic_pack(&self, tuple: &TupleSpec<'_>) -> bool {
            matches!(tuple, TupleSpec::Variable(tuple) if matches!(tuple.variable(), VariableSegment::TypeVarTuple(_)))
        }

        fn runtime_argument<'db>(&self) -> [Type<'db>; 1] {
            [Type::object()]
        }
    }

    #[synchronous(tuple_runtime_specialization_sync)]
    #[capabilities(effects = TupleRuntimeEffects, facts = TupleRuntimeFacts)]
    #[passive_values(TupleRuntimeWork::Inspect, TupleRuntimeWork::Intern, TupleRuntimeWork::Publish)]
    pub(in crate::types) async fn tuple_runtime_specialization_with<'db, E: TupleRuntimeEffects<'db>>(
        specialization: Specialization<'db>,
        effects: &E,
        facts: TupleRuntimeFacts,
    ) -> Result<Specialization<'db>, E::Error> {
        effects.checkpoint(TupleRuntimeWork::Inspect).await?;
        let symbolic_pack = match effects.tuple_inner(specialization).await? {
            Some(tuple) => {
                let tuple = effects.tuple_spec(tuple).await?;
                facts.symbolic_pack(tuple)
            }
            None => false,
        };
        let result = if symbolic_pack {
            // A symbolic pack contributes `object` to the runtime element type. Every fixed
            // prefix and suffix element is therefore absorbed, without inspecting those types.
            effects.checkpoint(TupleRuntimeWork::Intern).await?;
            let context = effects.generic_context(specialization).await?;
            let types = facts.runtime_argument();
            let kind = effects.materialization_kind(specialization).await?;
            effects.intern(context, types, kind, None).await?
        } else {
            // Ordinary tuple specializations already use their runtime element type as the tuple
            // class's generic argument. Rebuilding them would add allocation and interning work to
            // every tuple member and MRO lookup, both of which are hot paths in tuple-heavy programs.
            specialization
        };
        effects.checkpoint(TupleRuntimeWork::Publish).await?;
        Ok(result)
    }
}

pub(in crate::types) fn tuple_runtime_element_specialization_with<'db, C: TupleRuntimeControl>(
    db: &'db dyn Db,
    specialization: Specialization<'db>,
    control: &C,
) -> Result<Specialization<'db>, C::Error> {
    tuple_runtime_specialization_sync(
        specialization,
        &OrdinaryTupleRuntimeEffects { db, control },
        TupleRuntimeFacts,
    )
}

struct OrdinaryTupleRuntimeEffects<'a, 'db, C> {
    db: &'db dyn Db,
    control: &'a C,
}

impl<'db, C: TupleRuntimeControl> SynchronousTupleRuntimeEffects<'db>
    for OrdinaryTupleRuntimeEffects<'_, 'db, C>
{
    type Error = C::Error;

    fn checkpoint(&self, work: TupleRuntimeWork) -> Result<(), Self::Error> {
        self.control.checkpoint(work)
    }

    fn tuple_inner(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<Option<TupleType<'db>>, Self::Error> {
        Ok(specialization.tuple_inner(self.db))
    }

    fn tuple_spec(&self, tuple: TupleType<'db>) -> Result<&'db TupleSpec<'db>, Self::Error> {
        Ok(tuple.tuple(self.db))
    }

    fn generic_context(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<GenericContext<'db>, Self::Error> {
        Ok(specialization.generic_context(self.db))
    }

    fn materialization_kind(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<Option<MaterializationKind>, Self::Error> {
        Ok(specialization.materialization_kind(self.db))
    }

    fn intern(
        &self,
        context: GenericContext<'db>,
        types: [Type<'db>; 1],
        kind: Option<MaterializationKind>,
        tuple: Option<TupleType<'db>>,
    ) -> Result<Specialization<'db>, Self::Error> {
        Ok(Specialization::new(
            self.db,
            context,
            types.as_slice(),
            kind,
            tuple,
        ))
    }
}

pub(super) struct Unrestricted;

impl TupleRuntimeControl for Unrestricted {
    type Error = Infallible;

    fn checkpoint(&self, _work: TupleRuntimeWork) -> Result<(), Infallible> {
        Ok(())
    }
}
