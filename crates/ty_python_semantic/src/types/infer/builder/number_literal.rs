//! Numeric literal selection shared by ordinary and suspended expression inference.

use std::convert::Infallible;

use ruff_python_ast as ast;
use ty_mapping_probe_macros::shared_semantic_family;

use super::TypeInferenceBuilder;
use crate::types::{KnownClass, Type};

pub(in crate::types::infer) struct NumberLiteralFacts;
pub(super) struct OrdinaryNumberLiteralEffects;

shared_semantic_family! {
    #[synchronous(SynchronousNumberLiteralEffects)]
    pub(in crate::types::infer) trait NumberLiteralEffects<'db, 'ast> {
        type Error;
        #[operation(source)]
        async fn value<'expr>(&self, literal: &'expr ast::ExprNumberLiteral) -> Result<&'expr ast::Number, Self::Error>;
        #[operation(local)]
        async fn integer(&self, value: &ast::Int) -> Result<Option<i64>, Self::Error>;
        #[operation(child)]
        async fn instance(&self, builder: &TypeInferenceBuilder<'db, 'ast>, class: KnownClass) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl NumberLiteralFacts {
        fn integer<'db>(&self, value: i64) -> Type<'db> { Type::int_literal(value) }
    }

    #[synchronous(infer_number_literal_sync)]
    #[capabilities(effects = NumberLiteralEffects, facts = NumberLiteralFacts)]
    #[passive_values(KnownClass::Int, KnownClass::Float, KnownClass::Complex)]
    pub(in crate::types::infer) async fn infer_number_literal_with<'db, 'ast, E: NumberLiteralEffects<'db, 'ast>>(
        builder: &TypeInferenceBuilder<'db, 'ast>,
        literal: &ast::ExprNumberLiteral,
        facts: NumberLiteralFacts,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        match effects.value(literal).await? {
            ast::Number::Int(value) => match effects.integer(value).await? {
                Some(value) => Ok(facts.integer(value)),
                None => effects.instance(builder, KnownClass::Int).await,
            },
            ast::Number::Float(_) => effects.instance(builder, KnownClass::Float).await,
            ast::Number::Complex { .. } => effects.instance(builder, KnownClass::Complex).await,
        }
    }
}

impl<'db, 'ast> SynchronousNumberLiteralEffects<'db, 'ast> for OrdinaryNumberLiteralEffects {
    type Error = Infallible;

    fn value<'expr>(
        &self,
        literal: &'expr ast::ExprNumberLiteral,
    ) -> Result<&'expr ast::Number, Infallible> {
        Ok(&literal.value)
    }

    fn integer(&self, value: &ast::Int) -> Result<Option<i64>, Infallible> {
        Ok(value.as_i64())
    }

    fn instance(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        class: KnownClass,
    ) -> Result<Type<'db>, Infallible> {
        Ok(class.to_instance(builder.db(), builder.program_environment()))
    }
}
