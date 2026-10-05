//! Queued source facts obtained from prepared syntax without entering inference queries.

pub(super) mod declarations;
mod definition_root;

pub(super) use declarations::PreparedDeclarations;

mod root;
mod tests;

use ruff_python_ast::{self as ast, name::Name};
use rustc_hash::FxHashMap;
use ty_python_core::SemanticIndex;
use ty_python_core::definition::Definition;

use super::{Boundary, GenericContextAnswer, HeaderAnswer, Key, Router};
use crate::types::generics::GenericContext;
use crate::types::generics::context_construction::{ContextConstructionControl, ContextConstructionWork, InlineContextConstruction};
use crate::types::{BoundTypeVarInstance, TypeVarInstance};
use crate::types::generics::header_effects::{
    TypeParameterDeclaration, TypeParameterEffects, sealed,
};
use crate::types::infer::type_parameter_header::{
    TypeParameterBoundHeader, TypeParameterHeaderInput, infer_type_parameter_header,
};
use crate::types::typevar::TypeVarKind;
use crate::{Db, ProgramEnvironment};

/// Structural inputs captured before semantic evaluation, with no inferred values.
///
/// Parsing and index construction belong to the preparation phase. Owning these compact
/// fields keeps the evaluator independent of later accesses to a file's AST or index.
#[derive(Clone, Default)]
pub(crate) struct PreparedSources<'db> {
    headers: FxHashMap<Definition<'db>, PreparedHeader>,
    generic_contexts: FxHashMap<Definition<'db>, Box<[Definition<'db>]>>,
}

impl<'db> PreparedSources<'db> {
    pub(super) fn insert_type_params(
        &mut self,
        index: &SemanticIndex<'db>,
        binding_context: Definition<'db>,
        parameters: &ast::TypeParams,
    ) {
        let definitions = parameters
            .iter()
            .map(|parameter| {
                let definition = match parameter {
                    ast::TypeParam::TypeVar(node) => index.expect_single_definition(node),
                    ast::TypeParam::ParamSpec(node) => index.expect_single_definition(node),
                    ast::TypeParam::TypeVarTuple(node) => index.expect_single_definition(node),
                };
                let input = TypeParameterHeaderInput::from(parameter);
                self.headers.insert(
                    definition,
                    PreparedHeader {
                        name: input.name.clone(),
                        kind: input.kind,
                        bound: input.bound,
                        has_default: input.has_default,
                    },
                );
                definition
            })
            .collect();
        self.generic_contexts.insert(binding_context, definitions);
    }
}

#[derive(Clone)]
struct PreparedHeader {
    name: Name,
    kind: TypeVarKind,
    bound: Option<TypeParameterBoundHeader>,
    has_default: bool,
}

impl PreparedHeader {
    fn input(&self) -> TypeParameterHeaderInput<'_> {
        TypeParameterHeaderInput {
            name: &self.name,
            kind: self.kind,
            bound: self.bound,
            has_default: self.has_default,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SourceKey<'db> {
    Header(Definition<'db>),
    GenericContext(Definition<'db>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SourceSupport<'db> {
    domain: usize,
    key: SourceKey<'db>,
    logical_debit: usize,
}

/// A value whose complete producer is recorded in this root's committed source table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SourceFact<'db, T> {
    value: T,
    support: SourceSupport<'db>,
}

impl<'db, T: Copy> SourceFact<'db, T> {
    pub(super) fn value(self, router: &Router<'db, '_>) -> Result<T, Boundary> {
        if router.evaluation_domain.0 != Some(self.support.domain) {
            return Err(Boundary::SourceSupport);
        }
        let published = match self.support.key {
            SourceKey::Header(definition) => router
                .headers
                .borrow()
                .get(&definition)
                .and_then(|entry| entry.answer.as_ref())
                .is_some_and(|answer| {
                    answer
                        .as_ref()
                        .is_ok_and(|fact| fact.support == self.support)
                }),
            SourceKey::GenericContext(definition) => router
                .generic_contexts
                .borrow()
                .get(&definition)
                .and_then(|entry| entry.answer.as_ref())
                .is_some_and(|answer| {
                    answer
                        .as_ref()
                        .is_ok_and(|fact| fact.support == self.support)
                }),
        };
        if published {
            Ok(self.value)
        } else {
            Err(Boundary::SourceSupport)
        }
    }
}

/// Reserves bounded header records and variable-sized name/map work on the first poll.
///
/// These units count logical operations, including the lengths presented to interners;
/// they do not bound allocator time. Source preparation is an explicit earlier phase.
pub(super) fn payload_debit(router: &Router<'_, '_>, key: Key<'_>) -> Result<usize, Boundary> {
    match key {
        Key::Header(definition) => {
            router
                .prepared_sources
                .headers
                .get(&definition)
                .map_or(Ok(0), |header| {
                    header
                        .name
                        .len()
                        .checked_add(6)
                        .ok_or(Boundary::CostOverflow)
                })
        }
        Key::GenericContext(definition) => router
            .prepared_sources
            .generic_contexts
            .get(&definition)
            .map_or(Ok(0), |definitions| {
                definitions
                    .len()
                    .checked_mul(8)
                    .and_then(|work| work.checked_add(4))
                    .ok_or(Boundary::CostOverflow)
            }),
        _ => Ok(0),
    }
}

fn support<'db>(
    router: &Router<'db, '_>,
    key: SourceKey<'db>,
) -> Result<SourceSupport<'db>, Boundary> {
    Ok(SourceSupport {
        domain: router.evaluation_domain.0.ok_or(Boundary::SourceSupport)?,
        key,
        logical_debit: payload_debit(
            router,
            match key {
                SourceKey::Header(definition) => Key::Header(definition),
                SourceKey::GenericContext(definition) => Key::GenericContext(definition),
            },
        )?,
    })
}

pub(super) fn evaluate_header<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    router: &Router<'db, '_>,
    definition: Definition<'db>,
) -> HeaderAnswer<'db> {
    if definition.program(db) != env.program(db) {
        return Err(Boundary::ProgramDomain);
    }
    let header = router
        .prepared_sources
        .headers
        .get(&definition)
        .ok_or(Boundary::SourcePreparation)?;
    let support = support(router, SourceKey::Header(definition))?;
    Ok(SourceFact {
        value: infer_type_parameter_header(db, definition, header.input()),
        support,
    })
}

pub(super) async fn evaluate_generic_context<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    router: &Router<'db, '_>,
    definition: Definition<'db>,
) -> GenericContextAnswer<'db> {
    if definition.program(db) != env.program(db) {
        return Err(Boundary::ProgramDomain);
    }
    let definitions = router
        .prepared_sources
        .generic_contexts
        .get(&definition)
        .ok_or(Boundary::SourcePreparation)?;
    let support = support(router, SourceKey::GenericContext(definition))?;
    let effects = QueuedTypeParameterEffects { router, parent: definition };
    let context = InlineContextConstruction::<_, std::iter::Empty<BoundTypeVarInstance<'db>>>::new(
        db, &effects, env.program(db),
    );
    let value = GenericContext::from_type_param_definitions_with(
        db,
        env,
        definition,
        definitions.iter().copied(),
        &effects,
        &context,
    )
    .await?;
    Ok(SourceFact { value, support })
}

struct QueuedTypeParameterEffects<'eval, 'db, 'c> {
    router: &'eval Router<'db, 'c>,
    parent: Definition<'db>,
}

impl sealed::Sealed for QueuedTypeParameterEffects<'_, '_, '_> {}

impl ContextConstructionControl for QueuedTypeParameterEffects<'_, '_, '_> {
    type Error = Boundary;

    fn checkpoint(&self, _: ContextConstructionWork) -> Result<(), Self::Error> { Ok(()) }
}

impl<'db> TypeParameterEffects<'db> for QueuedTypeParameterEffects<'_, 'db, '_> {
    type Error = Boundary;

    async fn prepare_headers<I>(&self, definitions: &I) -> Result<usize, Self::Error>
    where I: ExactSizeIterator<Item = Definition<'db>> + Clone {
        for definition in definitions.clone() {
            self.router.declare_header(definition);
        }
        Ok(definitions.len())
    }

    async fn next_definition<I>(&self, definitions: &mut I) -> Result<Option<Definition<'db>>, Self::Error>
    where I: Iterator<Item = Definition<'db>> {
        Ok(definitions.next())
    }

    async fn bind(&self, db: &'db dyn Db, variable: TypeVarInstance<'db>, binding: Definition<'db>) -> Result<BoundTypeVarInstance<'db>, Self::Error> {
        Ok(variable.with_binding_context(db, binding))
    }

    async fn type_parameter(
        &self,
        _db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> Result<TypeParameterDeclaration<'db>, Self::Error> {
        let fact = self
            .router
            .generic_context_header_demand(self.parent, definition)
            .await?;
        Ok(TypeParameterDeclaration::Variable(
            fact.value(self.router)?.variable,
        ))
    }
}
