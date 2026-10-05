//! Tuple specifications preserve exact tuple storage and nominal subclass lookup.

use std::borrow::Cow;
use std::convert::Infallible;

use ruff_python_ast::PythonVersion;

use super::{NominalInstanceClass, NominalInstanceInner, NominalInstanceType};
use crate::types::class::{ClassType, GenericAlias, KnownClass};
use crate::types::mro::MroIterator;
use crate::types::set_theoretic::RecursivelyDefined;
use crate::types::tuple::{TupleSpec, TupleType};
use crate::types::{ClassBase, Type, UnionType};
use crate::{Db, ProgramEnvironment};

#[cfg(feature = "experimental-analysis")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TupleSpecOperation {
    Mro,
}

pub(in crate::types) struct TupleSpecFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousTupleSpecEffects)]
    pub(in crate::types) trait TupleSpecEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn nominal_spec(&self, env: &ProgramEnvironment<'db>, instance: NominalInstanceType<'db>) -> Result<Option<Cow<'db, TupleSpec<'db>>>, Self::Error>;
        #[operation(local)]
        async fn exact_spec(&self, tuple: TupleType<'db>) -> Result<&'db TupleSpec<'db>, Self::Error>;
        #[operation(child)]
        async fn version_info(&self, env: &ProgramEnvironment<'db>) -> Result<TupleSpec<'db>, Self::Error>;
        #[operation(local)]
        async fn non_tuple_class(&self, class: NominalInstanceClass<'db>) -> Result<ClassType<'db>, Self::Error>;
        #[operation(local)]
        async fn class_known(&self, class: ClassType<'db>) -> Result<Option<KnownClass>, Self::Error>;
        #[operation(child)]
        async fn mro(&self, class: ClassType<'db>) -> Result<MroIterator<'db>, Self::Error>;
        #[operation(child)]
        #[progress]
        async fn next_mro(&self, mro: &mut MroIterator<'db>) -> Result<Option<ClassBase<'db>>, Self::Error>;
        #[operation(local)]
        async fn retire_mro(&self, mro: MroIterator<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn specialization_tuple(&self, alias: GenericAlias<'db>) -> Result<Option<&'db TupleSpec<'db>>, Self::Error>;
        #[operation(local)]
        async fn unknown_tuple(&self) -> Result<TupleSpec<'db>, Self::Error>;
        #[operation(source)]
        async fn python_version(&self, env: &ProgramEnvironment<'db>) -> Result<PythonVersion, Self::Error>;
        #[operation(child)]
        async fn known_instance(&self, env: &ProgramEnvironment<'db>, class: KnownClass) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn string_literal(&self, value: &str) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn release_elements(&self) -> Result<Vec<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn append_release_element(&self, elements: &mut Vec<Type<'db>>, element: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn release_union(&self, elements: Vec<Type<'db>>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn fixed_tuple(&self, elements: [Type<'db>; 5]) -> Result<TupleSpec<'db>, Self::Error>;
    }

    #[finite_capability]
    impl TupleSpecFacts {
        fn nominal<'db>(&self, ty: Type<'db>) -> Option<NominalInstanceType<'db>> { ty.as_nominal_instance() }
        fn inner<'db>(&self, instance: NominalInstanceType<'db>) -> NominalInstanceInner<'db> { instance.0 }
        fn cannot_be_tuple(&self, class: KnownClass) -> bool { !class.is_tuple_subclass() }
        fn base_class<'db>(&self, base: ClassBase<'db>) -> Option<ClassType<'db>> { base.into_class() }
        fn generic_alias<'db>(&self, class: ClassType<'db>) -> Option<GenericAlias<'db>> { class.into_generic_alias() }
        fn version_elements<'db>(&self, version: PythonVersion, int_instance: Type<'db>, release_level: Type<'db>) -> [Type<'db>; 5] {
            [
                Type::int_literal(version.major.into()),
                Type::int_literal(version.minor.into()),
                int_instance,
                release_level,
                int_instance,
            ]
        }
    }

    #[synchronous(tuple_instance_spec_sync)]
    #[capabilities(effects = TupleSpecEffects, facts = TupleSpecFacts)]
    #[passive_values()]
    pub(in crate::types) async fn tuple_instance_spec_with<'db, E: TupleSpecEffects<'db>>(
        ty: Type<'db>, env: &ProgramEnvironment<'db>, facts: TupleSpecFacts, effects: &E,
    ) -> Result<Option<Cow<'db, TupleSpec<'db>>>, E::Error> {
        effects.checkpoint().await?;
        match facts.nominal(ty) {
            Some(instance) => effects.nominal_spec(env, instance).await,
            None => Ok(None),
        }
    }

    #[synchronous(nominal_tuple_spec_sync)]
    #[capabilities(effects = TupleSpecEffects, facts = TupleSpecFacts)]
    #[passive_values(Cow::Borrowed, Cow::Owned)]
    pub(in crate::types) async fn nominal_tuple_spec_with<'db, E: TupleSpecEffects<'db>>(
        instance: NominalInstanceType<'db>, env: &ProgramEnvironment<'db>, facts: TupleSpecFacts, effects: &E,
    ) -> Result<Option<Cow<'db, TupleSpec<'db>>>, E::Error> {
        effects.checkpoint().await?;
        match facts.inner(instance) {
            NominalInstanceInner::ExactTuple(tuple) => Ok(Some(Cow::Borrowed(effects.exact_spec(tuple).await?))),
            NominalInstanceInner::SysVersionInfo => Ok(Some(Cow::Owned(effects.version_info(env).await?))),
            NominalInstanceInner::Object => Ok(None),
            NominalInstanceInner::NonTuple(class) => {
                let class = effects.non_tuple_class(class).await?;
                // Avoid an expensive MRO traversal for common stdlib classes.
                if let Some(known) = effects.class_known(class).await?
                    && facts.cannot_be_tuple(known)
                {
                    return Ok(None);
                }
                let mut mro = effects.mro(class).await?;
                #[cursor_loop]
                while let Some(base) = effects.next_mro(&mut mro).await? {
                    if let Some(class) = facts.base_class(base)
                        && let Some(KnownClass::Tuple) = effects.class_known(class).await?
                    {
                        let tuple = if let Some(alias) = facts.generic_alias(class)
                            && let Some(tuple) = effects.specialization_tuple(alias).await?
                        {
                            Cow::Borrowed(tuple)
                        } else {
                            Cow::Owned(effects.unknown_tuple().await?)
                        };
                        effects.retire_mro(mro).await?;
                        return Ok(Some(tuple));
                    }
                }
                effects.retire_mro(mro).await?;
                Ok(None)
            }
        }
    }

    #[synchronous(version_info_spec_sync)]
    #[capabilities(effects = TupleSpecEffects, facts = TupleSpecFacts)]
    #[passive_values(KnownClass::Int)]
    pub(in crate::types) async fn version_info_spec_with<'db, E: TupleSpecEffects<'db>>(
        env: &ProgramEnvironment<'db>, facts: TupleSpecFacts, effects: &E,
    ) -> Result<TupleSpec<'db>, E::Error> {
        let python_version = effects.python_version(env).await?;
        let int_instance_ty = effects.known_instance(env, KnownClass::Int).await?;

        // TODO: just grab this type from typeshed (it's a `sys._ReleaseLevel` type alias there)
        let mut elements = effects.release_elements().await?;
        let alpha = effects.string_literal("alpha").await?;
        effects.append_release_element(&mut elements, alpha).await?;
        let beta = effects.string_literal("beta").await?;
        effects.append_release_element(&mut elements, beta).await?;
        let candidate = effects.string_literal("candidate").await?;
        effects.append_release_element(&mut elements, candidate).await?;
        let final_level = effects.string_literal("final").await?;
        effects.append_release_element(&mut elements, final_level).await?;

        // For most unions, it's better to go via `UnionType::from_elements` or use `UnionBuilder`;
        // those techniques ensure that union elements are deduplicated and unions are eagerly simplified
        // into other types where necessary. Here, however, we know that there are no duplicates
        // in this union, so it's probably more efficient to use `UnionType::new()` directly.
        let release_level_ty = effects.release_union(elements).await?;
        effects.fixed_tuple(facts.version_elements(python_version, int_instance_ty, release_level_ty)).await
    }
}

pub(in crate::types) struct OrdinaryTupleSpecEffects<'db> {
    pub(in crate::types) db: &'db dyn Db,
}

impl<'db> SynchronousTupleSpecEffects<'db> for OrdinaryTupleSpecEffects<'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn nominal_spec(
        &self,
        env: &ProgramEnvironment<'db>,
        instance: NominalInstanceType<'db>,
    ) -> Result<Option<Cow<'db, TupleSpec<'db>>>, Self::Error> {
        nominal_tuple_spec_sync(instance, env, TupleSpecFacts, self)
    }

    fn exact_spec(&self, tuple: TupleType<'db>) -> Result<&'db TupleSpec<'db>, Self::Error> {
        Ok(tuple.tuple(self.db))
    }

    fn version_info(&self, env: &ProgramEnvironment<'db>) -> Result<TupleSpec<'db>, Self::Error> {
        Ok(TupleSpec::version_info_spec(self.db, env))
    }

    fn non_tuple_class(
        &self,
        class: NominalInstanceClass<'db>,
    ) -> Result<ClassType<'db>, Self::Error> {
        Ok(class.class(self.db))
    }

    fn class_known(&self, class: ClassType<'db>) -> Result<Option<KnownClass>, Self::Error> {
        Ok(class.known(self.db))
    }

    fn mro(&self, class: ClassType<'db>) -> Result<MroIterator<'db>, Self::Error> {
        Ok(class.iter_mro(self.db))
    }

    fn next_mro(&self, mro: &mut MroIterator<'db>) -> Result<Option<ClassBase<'db>>, Self::Error> {
        Ok(mro.next())
    }

    fn retire_mro(&self, _mro: MroIterator<'db>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn specialization_tuple(
        &self,
        alias: GenericAlias<'db>,
    ) -> Result<Option<&'db TupleSpec<'db>>, Self::Error> {
        Ok(alias.specialization(self.db).tuple(self.db))
    }

    fn unknown_tuple(&self) -> Result<TupleSpec<'db>, Self::Error> {
        Ok(TupleSpec::homogeneous(Type::unknown()))
    }

    fn python_version(&self, env: &ProgramEnvironment<'db>) -> Result<PythonVersion, Self::Error> {
        Ok(env.python_version(self.db))
    }

    fn known_instance(
        &self,
        env: &ProgramEnvironment<'db>,
        class: KnownClass,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(class.to_instance(self.db, env))
    }

    fn string_literal(&self, value: &str) -> Result<Type<'db>, Self::Error> {
        Ok(Type::string_literal(self.db, value))
    }

    fn release_elements(&self) -> Result<Vec<Type<'db>>, Self::Error> {
        Ok(Vec::with_capacity(4))
    }

    fn append_release_element(
        &self,
        elements: &mut Vec<Type<'db>>,
        element: Type<'db>,
    ) -> Result<(), Self::Error> {
        elements.push(element);
        Ok(())
    }

    fn release_union(&self, elements: Vec<Type<'db>>) -> Result<Type<'db>, Self::Error> {
        Ok(Type::Union(UnionType::new(
            self.db,
            elements.into_boxed_slice(),
            RecursivelyDefined::No,
        )))
    }

    fn fixed_tuple(&self, elements: [Type<'db>; 5]) -> Result<TupleSpec<'db>, Self::Error> {
        Ok(TupleSpec::heterogeneous(elements))
    }
}
