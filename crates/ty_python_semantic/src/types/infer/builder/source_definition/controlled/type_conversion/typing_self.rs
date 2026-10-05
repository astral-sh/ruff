//! Resolve explicit `Self` through existing source, binding and nominal-relation children.

use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::scope::ScopeId;

use super::super::class_selection::FixedFieldBorrow;
use super::super::{FixedFieldCopy, SourceAccess, SourceEffects};
use crate::ProgramEnvironment;
use crate::types::class::type_conversion::type_to_class_type_with;
use crate::types::class::{KnownClassInstanceEffects, class_default_specialization_with};
use crate::types::infer::InferenceFlags;
use crate::types::local_transfer::generated_field_quote;
use crate::types::relation::source::resources::{ClassRelation, RelationResourceAccess};
use crate::types::type_expression_conversion::special_form::typing_self::SelfAnnotationEffects;
use crate::types::{
    BoundTypeVarInstance, ClassLiteral, ClassType, FunctionDecorators, InvalidTypeExpression,
    KnownClass, StaticClassLiteral, Type,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SelfAnnotationEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn begin(&self) -> RunResult<()> {
        // Fund finite dispatch and result assembly across child awaits. Semantic children
        // retain their own admissions; these carriers do not contain their owned payloads.
        self.local_with_fixed_transfers(
            32,
            size_of::<ProgramEnvironment<'db>>() * 2
                + size_of::<Option<StaticClassLiteral<'db>>>() * 2
                + size_of::<Option<BoundTypeVarInstance<'db>>>() * 4
                + size_of::<Option<Definition<'db>>>() * 2
                + size_of::<Option<ClassType<'db>>>() * 2
                + size_of::<ClassType<'db>>() * 3
                + size_of::<InferenceFlags>() * 2
                + size_of::<bool>() * 12
                + size_of::<Type<'db>>() * 2
                + size_of::<InvalidTypeExpression<'db>>() * 2
                + size_of::<Result<Type<'db>, InvalidTypeExpression<'db>>>() * 4,
            || (),
        )
        .await
    }

    async fn enclosing_class(
        &self,
        scope: ScopeId<'db>,
    ) -> RunResult<Option<StaticClassLiteral<'db>>> {
        let file = self.scope_file(scope).await?;
        self.check_file_program(file).await?;
        let index = self.access.semantic_index(file).await?;
        self.type_parameter_future(|| self.nearest_enclosing_class(index, scope))
            .await?
            .await
    }

    async fn bound_self(
        &self,
        scope: ScopeId<'db>,
        binding: Option<Definition<'db>>,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        self.type_parameter_future(|| {
            self.typing_self_source(scope, binding, ClassLiteral::Static(class))
        })
        .await?
        .await
    }

    async fn binding_definition(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<Definition<'db>>> {
        let quote = generated_field_quote(
            |variable: BoundTypeVarInstance<'db>, context| variable.field_requests(context),
            |variable: BoundTypeVarInstance<'db>, context| variable.identity_request(context),
        )
        .and_then(|(work, bytes)| {
            Ok((
                Self::checked(work.checked_add(3))?,
                Self::checked(bytes.checked_add(size_of::<Option<Definition<'db>>>() * 3))?,
            ))
        });
        let read = self
            .boxed_future_with_fixed_transfers(quote, || {
                self.access.endpoint().read_field(
                    variable.identity_request(self.access.endpoint().field_request_context()),
                    &FixedFieldCopy,
                )
            })
            .await?;
        Ok(read.await.binding_context.definition())
    }

    async fn is_function(&self, definition: Definition<'db>) -> RunResult<bool> {
        let quote = generated_field_quote(
            |definition: Definition<'db>, context| definition.read_fields(context),
            |definition: Definition<'db>, context| definition.read_fields(context).kind(),
        )
        .and_then(|(work, bytes)| {
            Ok((
                Self::checked(work.checked_add(2))?,
                Self::checked(bytes.checked_add(size_of::<bool>() * 3))?,
            ))
        });
        let read = self
            .boxed_future_with_fixed_transfers(quote, || {
                self.access.endpoint().read_field(
                    definition
                        .read_fields(self.access.endpoint().field_request_context())
                        .kind(),
                    &FixedFieldBorrow,
                )
            })
            .await?;
        Ok(matches!(read.await, DefinitionKind::Function(_)))
    }

    async fn is_dunder_new(&self, definition: Definition<'db>) -> RunResult<bool> {
        let file = self.definition_file(definition).await?;
        self.check_file_program(file).await?;
        let prepared = self
            .type_parameter_future(|| self.access.prepare_existing(file))
            .await?
            .await?;
        let quote = generated_field_quote(
            |definition: Definition<'db>, context| definition.read_fields(context),
            |definition: Definition<'db>, context| definition.read_fields(context).kind(),
        )
        .and_then(|(work, bytes)| {
            Ok((
                Self::checked(work.checked_add(14))?,
                Self::checked(
                    bytes.checked_add(size_of::<&str>() * 3 + size_of::<RunResult<bool>>() * 3),
                )?,
            ))
        });
        let read = self
            .boxed_future_with_fixed_transfers(quote, || {
                self.access.endpoint().read_field(
                    definition
                        .read_fields(self.access.endpoint().field_request_context())
                        .kind(),
                    &FixedFieldBorrow,
                )
            })
            .await?;
        match read.await {
            DefinitionKind::Function(function) => {
                Ok(function.node(&prepared.module).name.as_str() == "__new__")
            }
            _ => Err(RunError::Contract(
                "Self staticmethod check requires a function definition",
            )),
        }
    }

    async fn is_staticmethod(&self, definition: Definition<'db>) -> RunResult<bool> {
        let inference = self
            .type_parameter_future(|| self.access.function_known_decorators(definition))
            .await?
            .await?;
        self.local_with_fixed_transfers(3, 0, || {
            inference
                .known_decorators()
                .contains(FunctionDecorators::STATICMETHOD)
        })
        .await
    }

    async fn type_class(&self, env: &ProgramEnvironment<'db>) -> RunResult<Option<ClassType<'db>>> {
        self.environment_program(env).await?;
        let ty = self
            .type_parameter_future(|| {
                KnownClassInstanceEffects::class_literal(self, KnownClass::Type)
            })
            .await?
            .await?;
        self.type_parameter_future(|| type_to_class_type_with(ty, self))
            .await?
            .await
    }

    async fn class_default(&self, class: StaticClassLiteral<'db>) -> RunResult<ClassType<'db>> {
        self.type_parameter_future(|| class_default_specialization_with(class, self))
            .await?
            .await
    }

    async fn is_subclass(
        &self,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
        target: ClassType<'db>,
    ) -> RunResult<bool> {
        let resources = self
            .local_with_fixed_transfers(
                4,
                size_of::<ClassType<'db>>() * 2 + size_of::<ClassRelation>(),
                || self.access.resources(),
            )
            .await?;
        self.type_parameter_future(|| {
            resources.class_condition(
                self.db(),
                env,
                class,
                target,
                ClassRelation::Subtyping,
                self,
            )
        })
        .await?
        .await
    }
}
