use ruff_python_ast as ast;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::ProgramFile;
use ty_python_core::definition::Definition;

use super::{SourceAccess, SourceEffects};

mod paramspec;
use crate::ProgramEnvironment;
use crate::types::generics::binding::TypeVarBindingEffects;
use crate::types::type_expression_conversion::TypeExpressionConversionEffects;
use crate::types::typevar::TypeVarInstance;
use crate::types::typevar::default::lazy::{
    LazyDefaultEffects, LazyDefaultExpression, LazyDefaultFacts, LazyDefaultKeywordCursor,
    LazyDefaultSource, lazy_default_recover_with, lazy_default_with,
};
use crate::types::{KnownClass, Type};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn infer_lazy_typevar_default(
        &self,
        variable: TypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        #[cfg(test)]
        crate::types::infer::source_runtime::tests::lazy_defaults::observe_query(
            self.db(),
            variable,
        );
        lazy_default_with(variable, LazyDefaultFacts, self).await
    }

    pub(in crate::types::infer) async fn recover_lazy_typevar_default(
        &self,
        cycle: &salsa::Cycle<'_>,
        previous: &Option<Type<'db>>,
        value: Option<Type<'db>>,
        variable: TypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        lazy_default_recover_with(cycle, *previous, value, variable, LazyDefaultFacts, self).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> LazyDefaultEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type Source = LazyDefaultSource<'db>;

    async fn definition(
        &self,
        variable: TypeVarInstance<'db>,
    ) -> RunResult<Option<Definition<'db>>> {
        TypeVarBindingEffects::typevar_definition(self, variable).await
    }

    async fn source(&self, definition: Definition<'db>) -> RunResult<Self::Source> {
        let file = self.definition_file(definition).await?;
        self.check_file_program(file).await?;
        let module = self.access.parsed_module(file).await?;
        let kind = self
            .field(
                definition
                    .read_fields(self.access.endpoint().field_request_context())
                    .kind(),
            )
            .await?;
        self.local(1, 0, || LazyDefaultSource::new(module, kind))
            .await
    }

    async fn select<'source>(
        &self,
        source: &'source Self::Source,
    ) -> RunResult<LazyDefaultExpression<'source>> {
        self.local(4, 0, || source.select()).await
    }

    async fn expression_type(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> RunResult<Type<'db>> {
        self.definition_expression_type(definition, expression)
            .await
    }

    async fn known_class(&self, ty: Type<'db>) -> RunResult<Option<KnownClass>> {
        let Some(class) = self.local(1, 0, || ty.as_class_literal()).await? else {
            return Ok(None);
        };
        TypeExpressionConversionEffects::class_known(self, class).await
    }

    async fn keyword_cursor<'source>(
        &self,
        call: &'source ast::ExprCall,
    ) -> RunResult<LazyDefaultKeywordCursor<'source>> {
        self.local(1, 0, || {
            LazyDefaultKeywordCursor::new(&call.arguments.keywords)
        })
        .await
    }

    async fn next_keyword<'source>(
        &self,
        cursor: &mut LazyDefaultKeywordCursor<'source>,
    ) -> RunResult<Option<&'source ast::Keyword>> {
        self.local(8, 0, || cursor.next()).await
    }

    async fn paramspec_value(&self, ty: Type<'db>) -> RunResult<Type<'db>> {
        self.boxed_future_with_fixed_transfers(
            Ok((11, size_of::<[(Type<'db>, &Self); 3]>())),
            || crate::types::typevar::default::lazy::paramspec::paramspec_default_with(ty, self),
        ).await?.await
    }

    async fn recovery_file(&self, variable: TypeVarInstance<'db>) -> RunResult<ProgramFile<'db>> {
        let definition = TypeVarBindingEffects::typevar_definition(self, variable)
            .await?
            .ok_or(RunError::Contract(
                "a lazy TypeVar default must have a source definition",
            ))?;
        self.definition_file(definition).await
    }

    async fn cycle_normalize(
        &self,
        default: Type<'db>,
        env: &ProgramEnvironment<'db>,
        previous: Type<'db>,
        cycle: &salsa::Cycle<'_>,
    ) -> RunResult<Type<'db>> {
        SourceEffects::cycle_normalize(self, env, default, previous, cycle).await
    }

    async fn recursive_normalize(
        &self,
        default: Type<'db>,
        env: &ProgramEnvironment<'db>,
        cycle: &salsa::Cycle<'_>,
    ) -> RunResult<Type<'db>> {
        self.normalize_cycle_heads(env, default, cycle).await
    }
}
