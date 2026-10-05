//! Enum inheritance classification retains the caller's program environment.

use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects};
use crate::ProgramEnvironment;
use crate::types::class::{
    KnownClassSubclassEffects, class_default_specialization_with, interpret_class_literal_lookup,
    known_class_to_subclass_of_with,
};
use crate::types::enums::inheritance::{EnumInheritanceEffects, is_enum_class_by_inheritance_with};
use crate::types::relation::source::subtyping_condition;
use crate::types::subclass_of::{SubclassConstructionFacts, SubclassOfInner, subclass_from_with};
use crate::types::{ClassType, KnownClass, StaticClassLiteral, Type};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer::builder) async fn is_enum_class_by_inheritance_source(
        &self,
        class: StaticClassLiteral<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<bool> {
        let env = ProgramEnvironment::from_program(self.environment_program(env).await?);
        let effects = self
            .local(1, 0, || EnumInheritanceSourceEffects { source: self, env: &env })
            .await?;
        is_enum_class_by_inheritance_with(class, &effects).await
    }
}

struct EnumInheritanceSourceEffects<'env, 'access, 'run, 'db: 'run, A> {
    source: &'env SourceEffects<'access, 'run, 'db, A>,
    env: &'env ProgramEnvironment<'db>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> KnownClassSubclassEffects<'db>
    for EnumInheritanceSourceEffects<'_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn lookup(&self, class: KnownClass) -> RunResult<Option<StaticClassLiteral<'db>>> {
        let program = self.source.environment_program(self.env).await?;
        let result = self
            .source
            .access
            .known_class_lookup(program, class)
            .await?;
        self.source
            .local(1, 0, || interpret_class_literal_lookup(result))
            .await
    }

    async fn default_specialization(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<ClassType<'db>> {
        class_default_specialization_with(class, self.source).await
    }

    async fn subclass_of(&self, class: ClassType<'db>) -> RunResult<Type<'db>> {
        subclass_from_with(
            SubclassOfInner::Class(class),
            SubclassConstructionFacts,
            self.source,
        )
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> EnumInheritanceEffects<'db>
    for EnumInheritanceSourceEffects<'_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn known_subclass(&self, class: KnownClass) -> RunResult<Type<'db>> {
        known_class_to_subclass_of_with(class, self).await
    }

    async fn is_subtype(&self, source: Type<'db>, target: Type<'db>) -> RunResult<bool> {
        subtyping_condition(self.source.db(), self.env, source, target, self.source).await
    }

    async fn metaclass(&self, class: StaticClassLiteral<'db>) -> RunResult<Type<'db>> {
        self.source.infer_static_metaclass(class).await
    }
}
