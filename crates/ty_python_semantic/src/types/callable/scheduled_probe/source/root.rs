//! Tracked preparation and result transport for the admitted source-header operations.
//!
//! This entry requests a declaration's generic context, not its inferred binding. Selecting a
//! unique top-level declaration is syntax preparation; it does not implement import semantics.

mod tests;

use ruff_db::parsed::parsed_module;
use ruff_python_ast::{self as ast, name::Name};
use std::cell::RefCell;
use ty_module_resolver::{ImportingFile, ModuleName, resolve_module};
use ty_python_core::definition::Definition;
use ty_python_core::{ProgramFile, semantic_index};

use super::{PreparedSources, Router};
use crate::types::callable::scheduled_probe::{Boundary, run_with};
use crate::types::generics::GenericContext;
use crate::types::infer::type_parameter_header::TypeParameterHeader;
use crate::{Db, ProgramEnvironment};

thread_local! {
    // Test-only cancellation is consumed once, so retrying the same query can finish.
    static CANCEL_NEXT_ROOT: RefCell<Option<(usize, salsa::CancellationToken)>> = const { RefCell::new(None) };
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, salsa::SalsaValue)]
struct RootPolicy {
    budget: usize,
    reverse_execution: bool,
    reverse_merge: bool,
}

#[salsa::interned(debug)]
struct ContextRootRequest<'db> {
    #[returns(copy)]
    importing_file: ProgramFile<'db>,
    #[returns(ref)]
    module: ModuleName,
    #[returns(ref)]
    declaration: Name,
    #[returns(copy)]
    policy: RootPolicy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, salsa::SalsaValue)]
enum PreparationFailure {
    MissingModule,
    NamespacePackage,
    MissingDeclaration,
    AmbiguousDeclaration,
    MissingTypeParameters,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, salsa::SalsaValue)]
enum Incomplete {
    Allowance,
    Preparation(PreparationFailure),
    Source(Boundary),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, salsa::SalsaValue)]
enum Completion<T> {
    Complete(T),
    Incomplete(Incomplete),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, salsa::SalsaValue)]
enum SourceObligation<'db> {
    Header(Definition<'db>),
    GenericContext(Definition<'db>),
}

/// Exported evidence contains checked values, not session-local source-support capabilities.
#[derive(Clone, Debug, Eq, PartialEq, salsa::SalsaValue)]
struct ContextRootOutcome<'db> {
    source: Option<ProgramFile<'db>>,
    owner: Option<Definition<'db>>,
    context: Completion<GenericContext<'db>>,
    headers: Box<[(Definition<'db>, TypeParameterHeader<'db>)]>,
    outstanding: Box<[SourceObligation<'db>]>,
    work: usize,
}

impl<'db> ContextRootOutcome<'db> {
    fn unfinished(source: ProgramFile<'db>, owner: Definition<'db>, reason: Incomplete) -> Self {
        Self {
            source: Some(source),
            owner: Some(owner),
            context: Completion::Incomplete(reason),
            headers: Box::default(),
            outstanding: Box::new([SourceObligation::GenericContext(owner)]),
            work: 0,
        }
    }

    fn failed(failure: PreparationFailure, source: Option<ProgramFile<'db>>) -> Self {
        Self {
            source,
            owner: None,
            context: Completion::Incomplete(Incomplete::Preparation(failure)),
            headers: Box::default(),
            outstanding: Box::default(),
            work: 0,
        }
    }
}

#[salsa::tracked(returns(ref))]
fn evaluate_prepared_context_root<'db>(
    db: &'db dyn Db,
    request: ContextRootRequest<'db>,
) -> ContextRootOutcome<'db> {
    let env = ProgramEnvironment::from_file(request.importing_file(db));
    let importing_file = ImportingFile::File(
        request.importing_file(db).file(db),
        env.resolver_environment(db),
    );
    let Some(module) = resolve_module(db, importing_file, request.module(db)) else {
        return ContextRootOutcome::failed(PreparationFailure::MissingModule, None);
    };
    let Some(file) = module.file(db) else {
        return ContextRootOutcome::failed(PreparationFailure::NamespacePackage, None);
    };
    let source = ProgramFile::new(db, file, env.program(db));
    let parsed = parsed_module(db, source.python_file(db)).load(db);
    let index = semantic_index(db, source);

    // These reads stay under this tracked entry so edits and resolver changes invalidate it.
    let mut matches = parsed
        .suite()
        .iter()
        .filter_map(|statement| match statement {
            ast::Stmt::FunctionDef(function) if function.name.id == *request.declaration(db) => {
                Some((
                    index.expect_single_definition(function),
                    function.type_params.as_deref(),
                ))
            }
            ast::Stmt::ClassDef(class) if class.name.id == *request.declaration(db) => Some((
                index.expect_single_definition(class),
                class.type_params.as_deref(),
            )),
            _ => None,
        });
    let Some((owner, parameters)) = matches.next() else {
        return ContextRootOutcome::failed(PreparationFailure::MissingDeclaration, Some(source));
    };
    if matches.next().is_some() {
        return ContextRootOutcome::failed(PreparationFailure::AmbiguousDeclaration, Some(source));
    }
    let Some(parameters) = parameters else {
        return ContextRootOutcome::failed(PreparationFailure::MissingTypeParameters, Some(source));
    };
    let mut prepared = PreparedSources::default();
    prepared.insert_type_params(index, owner, parameters);
    let policy = request.policy(db);
    // Reserve the ordered export's copies, support checks and result/obligation records before
    // evaluating children. Parsing and constructing prepared inputs are a separate phase.
    let Some(transport_work) = parameters
        .len()
        .checked_mul(4)
        .and_then(|work| work.checked_add(4))
    else {
        return ContextRootOutcome::unfinished(
            source,
            owner,
            Incomplete::Source(Boundary::CostOverflow),
        );
    };
    let Some(semantic_budget) = policy.budget.checked_sub(transport_work) else {
        return ContextRootOutcome::unfinished(source, owner, Incomplete::Allowance);
    };
    let definitions = prepared.generic_contexts[&owner].clone();
    let router = Router::with_sources(prepared);
    router.cancellation_probe.replace(CANCEL_NEXT_ROOT.take());
    let result = run_with(
        db,
        &env,
        &router,
        semantic_budget,
        policy.reverse_execution,
        policy.reverse_merge,
        |router| async { router.consumer_generic_context_demand(owner).await },
    );
    let snapshot = match result {
        Ok(snapshot) => snapshot,
        Err(boundary) => {
            return ContextRootOutcome {
                source: Some(source),
                owner: Some(owner),
                context: Completion::Incomplete(Incomplete::Source(boundary)),
                headers: Box::default(),
                outstanding: Box::new([SourceObligation::GenericContext(owner)]),
                work: transport_work,
            };
        }
    };
    let mut outcome = ContextRootOutcome {
        source: Some(source),
        owner: Some(owner),
        context: match snapshot.graph.generic_context_values.get(&owner).copied() {
            Some(Ok(fact)) => match fact.value(&router) {
                Ok(context) => Completion::Complete(context),
                Err(boundary) => Completion::Incomplete(Incomplete::Source(boundary)),
            },
            Some(Err(boundary)) => Completion::Incomplete(Incomplete::Source(boundary)),
            None => Completion::Incomplete(Incomplete::Allowance),
        },
        headers: Box::default(),
        outstanding: Box::default(),
        work: snapshot.graph.work + transport_work,
    };
    let mut headers = Vec::new();
    let mut outstanding = Vec::new();
    for definition in definitions {
        match snapshot.graph.header_values.get(&definition) {
            Some(Ok(fact)) => match fact.value(&router) {
                Ok(header) => headers.push((definition, header)),
                Err(boundary) => {
                    outcome.context = Completion::Incomplete(Incomplete::Source(boundary));
                    outstanding.push(SourceObligation::Header(definition));
                }
            },
            Some(Err(boundary)) => {
                outcome.context = Completion::Incomplete(Incomplete::Source(*boundary));
                outstanding.push(SourceObligation::Header(definition));
            }
            None => outstanding.push(SourceObligation::Header(definition)),
        }
    }
    if !matches!(outcome.context, Completion::Complete(_)) {
        outstanding.push(SourceObligation::GenericContext(owner));
    }
    outcome.headers = headers.into_boxed_slice();
    outcome.outstanding = outstanding.into_boxed_slice();
    db.unwind_if_revision_cancelled();
    outcome
}
