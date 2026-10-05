use std::convert::Infallible;

use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_python_ast as ast;
use ty_python_core::ProgramFile;
use ty_python_core::definition::{Definition, DefinitionKind};

pub(in crate::types) mod paramspec;
use crate::types::typevar::TypeVarInstance;
use crate::types::{
    KnownClass, Type, definition_expression_type,
};
use crate::{Db, ProgramEnvironment};

pub(in crate::types) struct LazyDefaultFacts;

pub(in crate::types) enum LazyDefaultExpression<'source> {
    Missing,
    Direct(&'source ast::Expr, bool),
    Legacy(&'source ast::ExprCall),
}

pub(in crate::types) struct LazyDefaultSource<'db> {
    module: ParsedModuleRef,
    kind: &'db DefinitionKind<'db>,
}

impl<'db> LazyDefaultSource<'db> {
    pub(in crate::types) fn new(module: ParsedModuleRef, kind: &'db DefinitionKind<'db>) -> Self {
        Self { module, kind }
    }

    pub(in crate::types) fn select(&self) -> LazyDefaultExpression<'_> {
        match self.kind {
            // PEP 695 typevar
            DefinitionKind::TypeVar(typevar) => typevar
                .node(&self.module)
                .default
                .as_ref()
                .map_or(LazyDefaultExpression::Missing, |expression| {
                    LazyDefaultExpression::Direct(expression, false)
                }),
            // legacy typevar / ParamSpec
            DefinitionKind::Assignment(assignment) => {
                assignment.value(&self.module).as_call_expr().map_or(
                    LazyDefaultExpression::Missing,
                    LazyDefaultExpression::Legacy,
                )
            }
            // PEP 695 ParamSpec
            DefinitionKind::ParamSpec(paramspec) => paramspec
                .node(&self.module)
                .default
                .as_ref()
                .map_or(LazyDefaultExpression::Missing, |expression| {
                    LazyDefaultExpression::Direct(expression, true)
                }),
            // PEP 695 TypeVarTuple
            DefinitionKind::TypeVarTuple(typevartuple) => typevartuple
                .node(&self.module)
                .default
                .as_ref()
                .map_or(LazyDefaultExpression::Missing, |expression| {
                    LazyDefaultExpression::Direct(expression, false)
                }),
            _ => LazyDefaultExpression::Missing,
        }
    }
}

pub(in crate::types) struct LazyDefaultKeywordCursor<'source> {
    keywords: std::slice::Iter<'source, ast::Keyword>,
}

impl<'source> LazyDefaultKeywordCursor<'source> {
    pub(in crate::types) fn new(keywords: &'source [ast::Keyword]) -> Self {
        Self {
            keywords: keywords.iter(),
        }
    }

    pub(in crate::types) fn next(&mut self) -> Option<&'source ast::Keyword> {
        self.keywords.next()
    }
}

pub(in crate::types::typevar) struct OrdinaryLazyDefaultEffects<'db> {
    pub(in crate::types::typevar) db: &'db dyn Db,
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousLazyDefaultEffects)]
    pub(in crate::types) trait LazyDefaultEffects<'db> {
        type Error;
        type Source;

        #[operation(source)]
        async fn definition(&self, variable: TypeVarInstance<'db>) -> Result<Option<Definition<'db>>, Self::Error>;
        #[operation(source)]
        async fn source(&self, definition: Definition<'db>) -> Result<Self::Source, Self::Error>;
        #[operation(local)]
        async fn select<'source>(&self, source: &'source Self::Source) -> Result<LazyDefaultExpression<'source>, Self::Error>;
        #[operation(child)]
        async fn expression_type(&self, definition: Definition<'db>, expression: &ast::Expr) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn known_class(&self, ty: Type<'db>) -> Result<Option<KnownClass>, Self::Error>;
        #[operation(local)]
        async fn keyword_cursor<'source>(&self, call: &'source ast::ExprCall) -> Result<LazyDefaultKeywordCursor<'source>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_keyword<'source>(&self, cursor: &mut LazyDefaultKeywordCursor<'source>) -> Result<Option<&'source ast::Keyword>, Self::Error>;
        #[operation(child)]
        async fn paramspec_value(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn recovery_file(&self, variable: TypeVarInstance<'db>) -> Result<ProgramFile<'db>, Self::Error>;
        #[operation(child)]
        async fn cycle_normalize(&self, default: Type<'db>, env: &ProgramEnvironment<'db>, previous: Type<'db>, cycle: &salsa::Cycle<'_>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn recursive_normalize(&self, default: Type<'db>, env: &ProgramEnvironment<'db>, cycle: &salsa::Cycle<'_>) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl LazyDefaultFacts {
        fn is_paramspec(&self, class: Option<KnownClass>) -> bool {
            matches!(class, Some(KnownClass::ParamSpec | KnownClass::ExtensionsParamSpec))
        }

        fn is_default_keyword(&self, keyword: &ast::Keyword) -> bool {
            keyword.arg.as_deref() == Some("default")
        }

        fn file_environment<'db>(&self, file: ProgramFile<'db>) -> ProgramEnvironment<'db> {
            ProgramEnvironment::from_file(file)
        }
    }

    #[synchronous(lazy_default_sync)]
    #[capabilities(effects = LazyDefaultEffects, facts = LazyDefaultFacts)]
    #[passive_values()]
    pub(in crate::types) async fn lazy_default_with<'db, E: LazyDefaultEffects<'db>>(
        variable: TypeVarInstance<'db>,
        facts: LazyDefaultFacts,
        effects: &E,
    ) -> Result<Option<Type<'db>>, E::Error> {
        let Some(definition) = effects.definition(variable).await? else {
            return Ok(None);
        };
        let source = effects.source(definition).await?;
        match effects.select(&source).await? {
            LazyDefaultExpression::Missing => Ok(None),
            LazyDefaultExpression::Direct(expression, is_paramspec) => {
                let default = effects.expression_type(definition, expression).await?;
                let default = if is_paramspec {
                    effects.paramspec_value(default).await?
                } else {
                    default
                };
                Ok(Some(default))
            }
            LazyDefaultExpression::Legacy(call) => {
                let func_ty = effects.expression_type(definition, &call.func).await?;
                let known_class = effects.known_class(func_ty).await?;
                let mut cursor = effects.keyword_cursor(call).await?;
                #[cursor_loop]
                while let Some(keyword) = effects.next_keyword(&mut cursor).await? {
                    if facts.is_default_keyword(keyword) {
                        let default = effects.expression_type(definition, &keyword.value).await?;
                        let default = if facts.is_paramspec(known_class) {
                            effects.paramspec_value(default).await?
                        } else {
                            default
                        };
                        return Ok(Some(default));
                    }
                }
                Ok(None)
            }
        }
    }

    #[synchronous(lazy_default_recover_sync)]
    #[capabilities(effects = LazyDefaultEffects, facts = LazyDefaultFacts)]
    #[passive_values()]
    pub(in crate::types) async fn lazy_default_recover_with<'db, E: LazyDefaultEffects<'db>>(
        cycle: &salsa::Cycle<'_>,
        previous: Option<Type<'db>>,
        value: Option<Type<'db>>,
        variable: TypeVarInstance<'db>,
        facts: LazyDefaultFacts,
        effects: &E,
    ) -> Result<Option<Type<'db>>, E::Error> {
        // Normalize the default to ensure cycle convergence.
        let Some(default) = value else { return Ok(None); };
        let file = effects.recovery_file(variable).await?;
        let env = facts.file_environment(file);
        let default = match previous {
            Some(previous) => effects.cycle_normalize(default, &env, previous, cycle).await?,
            None => effects.recursive_normalize(default, &env, cycle).await?,
        };
        Ok(Some(default))
    }
}

impl<'db> SynchronousLazyDefaultEffects<'db> for OrdinaryLazyDefaultEffects<'db> {
    type Error = Infallible;
    type Source = LazyDefaultSource<'db>;

    fn definition(
        &self,
        variable: TypeVarInstance<'db>,
    ) -> Result<Option<Definition<'db>>, Self::Error> {
        Ok(variable.definition(self.db))
    }

    fn source(&self, definition: Definition<'db>) -> Result<Self::Source, Self::Error> {
        let program_file = definition.program_file(self.db);
        let python_file = program_file.python_file(self.db);
        let module = parsed_module(self.db, python_file).load(self.db);
        Ok(LazyDefaultSource::new(module, definition.kind(self.db)))
    }

    fn select<'source>(
        &self,
        source: &'source Self::Source,
    ) -> Result<LazyDefaultExpression<'source>, Self::Error> {
        Ok(source.select())
    }

    fn expression_type(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(definition_expression_type(self.db, definition, expression))
    }

    fn known_class(&self, ty: Type<'db>) -> Result<Option<KnownClass>, Self::Error> {
        Ok(ty.as_class_literal().and_then(|class| class.known(self.db)))
    }

    fn keyword_cursor<'source>(
        &self,
        call: &'source ast::ExprCall,
    ) -> Result<LazyDefaultKeywordCursor<'source>, Self::Error> {
        Ok(LazyDefaultKeywordCursor::new(&call.arguments.keywords))
    }

    fn next_keyword<'source>(
        &self,
        cursor: &mut LazyDefaultKeywordCursor<'source>,
    ) -> Result<Option<&'source ast::Keyword>, Self::Error> {
        Ok(cursor.next())
    }

    fn paramspec_value(&self, ty: Type<'db>) -> Result<Type<'db>, Infallible> {
        paramspec::paramspec_default_sync(ty, self)
    }

    fn recovery_file(
        &self,
        variable: TypeVarInstance<'db>,
    ) -> Result<ProgramFile<'db>, Self::Error> {
        Ok(variable
            .definition(self.db)
            .expect("a lazy TypeVar default must have a source definition")
            .program_file(self.db))
    }

    fn cycle_normalize(
        &self,
        default: Type<'db>,
        env: &ProgramEnvironment<'db>,
        previous: Type<'db>,
        cycle: &salsa::Cycle<'_>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(default.cycle_normalized(self.db, env, previous, cycle))
    }

    fn recursive_normalize(
        &self,
        default: Type<'db>,
        env: &ProgramEnvironment<'db>,
        cycle: &salsa::Cycle<'_>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(default.recursive_type_normalized(self.db, env, cycle))
    }
}

#[cfg(test)]
mod tests;
