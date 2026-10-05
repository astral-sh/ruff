use std::convert::Infallible;

use super::{NominalInstanceClass, NominalInstanceInner, NominalInstanceType};
use crate::ProgramEnvironment;
use crate::types::Type;
use crate::types::normalization::OrdinaryNormalizationEffects;
use crate::types::tuple::TupleType;

pub(in crate::types) struct NominalNormalizationFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousNominalNormalizationEffects)]
    pub(in crate::types) trait NominalNormalizationEffects<'db> {
        type Error;

        #[operation(child)]
        async fn exact_tuple(&self, tuple: TupleType<'db>, env: &ProgramEnvironment<'db>, divergent: Type<'db>, nested: bool) -> Result<Option<TupleType<'db>>, Self::Error>;
        #[operation(child)]
        async fn non_tuple(&self, class: NominalInstanceClass<'db>, env: &ProgramEnvironment<'db>, divergent: Type<'db>, nested: bool) -> Result<Option<NominalInstanceClass<'db>>, Self::Error>;
    }

    #[finite_capability]
    impl NominalNormalizationFacts {
        fn inner<'db>(&self, instance: NominalInstanceType<'db>) -> NominalInstanceInner<'db> {
            instance.0
        }

        fn instance<'db>(&self, inner: NominalInstanceInner<'db>) -> NominalInstanceType<'db> {
            NominalInstanceType(inner)
        }
    }

    #[synchronous(nominal_normalize_sync)]
    #[capabilities(effects = NominalNormalizationEffects, facts = NominalNormalizationFacts)]
    #[passive_values(NominalInstanceInner::ExactTuple, NominalInstanceInner::NonTuple)]
    pub(in crate::types) async fn nominal_normalize_with<'db, E: NominalNormalizationEffects<'db>>(
        instance: NominalInstanceType<'db>,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        nested: bool,
        effects: &E,
        facts: NominalNormalizationFacts,
    ) -> Result<Option<NominalInstanceType<'db>>, E::Error> {
        match facts.inner(instance) {
            NominalInstanceInner::ExactTuple(tuple) => {
                match effects.exact_tuple(tuple, env, divergent, nested).await? {
                    Some(tuple) => Ok(Some(facts.instance(NominalInstanceInner::ExactTuple(tuple)))),
                    None => Ok(None),
                }
            }
            NominalInstanceInner::SysVersionInfo | NominalInstanceInner::Object => Ok(Some(instance)),
            NominalInstanceInner::NonTuple(class) => {
                match effects.non_tuple(class, env, divergent, nested).await? {
                    Some(class) => Ok(Some(facts.instance(NominalInstanceInner::NonTuple(class)))),
                    None => Ok(None),
                }
            }
        }
    }
}

impl<'db> SynchronousNominalNormalizationEffects<'db> for OrdinaryNormalizationEffects<'db> {
    type Error = Infallible;

    fn exact_tuple(
        &self,
        tuple: TupleType<'db>,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        nested: bool,
    ) -> Result<Option<TupleType<'db>>, Infallible> {
        Ok(tuple.recursive_type_normalized_impl(self.db, env, divergent, nested))
    }

    fn non_tuple(
        &self,
        class: NominalInstanceClass<'db>,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        nested: bool,
    ) -> Result<Option<NominalInstanceClass<'db>>, Infallible> {
        Ok(class
            .class(self.db)
            .recursive_type_normalized_impl(self.db, env, divergent, nested)
            .map(|transformed| class.with_class(self.db, transformed)))
    }
}
