//! Fixed quotations for borrowed context cursors and class-scan handle comparisons.

use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::definition::DefinitionNodeKey;
use ty_python_core::scope::{NodeWithScopeKind, Scope};

use crate::types::generics::binding::{BindingFacts, BindingNode};
use crate::types::generics::context_construction::ContextVariables;
use crate::types::infer::builder::post_inference::static_class::generic_checks::default_references::{VariableCursor, VariableRange};
use crate::types::local_transfer::collections::{CALL_1, CALL_2, event_quote};
use crate::types::local_transfer::context_variables::{context_variable_at_quote, context_variable_count_quote};
use crate::types::typevar::{BoundTypeVarIdentity, TypeVarInstance};
use crate::types::BoundTypeVarInstance;

// The cursor adds its end test, position retention, successful `?`, increment and return.
// The same admission also covers destructuring and loop decisions in the shared scan.
const CURSOR_CONSTRUCTION: usize = CALL_2 + 14;
const CURSOR_STEP: usize = CALL_1 + 28;

// Salsa's interned handle compares Id(index, generation) and PhantomData. The index is
// NonZeroU32, whose equality reads both inner values; primitive comparisons are included.
const INSTANCE_EQUALITY: usize = 6 * CALL_2 + 2 * CALL_1 + 16;

/// Quotes classification of a scope's stored node and copying its definition key.
pub(super) const fn binding_node_quote() -> RunResult<(usize, usize)> {
    // BindingFacts::node/node_kind, Scope::node, Into/From, NodeKey::from_node_ref
    // and AstNodeRef::index only select a variant and copy stored index metadata.
    match event_quote(
        2 * CALL_2 + 5 * CALL_1 + 14,
        &[
            size_of::<(&BindingFacts, &Scope)>(),
            size_of::<(&BindingFacts, &NodeWithScopeKind)>(),
            size_of::<DefinitionNodeKey>(),
            size_of::<BindingNode>(),
            size_of::<ruff_python_ast::NodeIndex>(),
        ],
    ) {
        Some(quote) => Ok(quote),
        None => Err(RunError::Contract(
            "scope node classification quotation overflow",
        )),
    }
}

/// Quotes construction of a borrowed variable cursor, including the context length chain.
pub(super) const fn cursor_quote() -> RunResult<(usize, usize)> {
    let (count_work, count_bytes) = match context_variable_count_quote() {
        Ok(quote) => quote,
        Err(error) => return Err(error),
    };
    match event_quote(
        CURSOR_CONSTRUCTION,
        &[
            size_of::<(&ContextVariables<'static>, VariableRange)>(),
            size_of::<VariableCursor<'static, 'static>>(),
            size_of::<(usize, usize)>(),
            size_of::<(&usize, &usize)>(),
            size_of::<bool>(),
        ],
    ) {
        Some((work, bytes)) => match (work.checked_add(count_work), bytes.checked_add(count_bytes))
        {
            (Some(work), Some(bytes)) => Ok((work, bytes)),
            _ => Err(RunError::Contract(
                "class variable cursor quotation overflow",
            )),
        },
        None => Err(RunError::Contract(
            "class variable cursor quotation overflow",
        )),
    }
}

/// Quotes one ordered lookup and cursor step without allocating or cloning the context.
pub(super) const fn cursor_step_quote() -> RunResult<(usize, usize)> {
    let (lookup_work, lookup_bytes) = match context_variable_at_quote() {
        Ok(quote) => quote,
        Err(error) => return Err(error),
    };
    match event_quote(
        CURSOR_STEP,
        &[
            size_of::<(&ContextVariables<'static>, usize)>(),
            size_of::<
                Option<(
                    &BoundTypeVarIdentity<'static>,
                    &BoundTypeVarInstance<'static>,
                )>,
            >(),
            size_of::<Option<(usize, BoundTypeVarInstance<'static>)>>(),
            size_of::<&[()]>(),
            size_of::<(*const (), usize, usize, bool)>(),
            size_of::<Vec<()>>(),
        ],
    ) {
        Some((work, bytes)) => match (
            work.checked_add(lookup_work),
            bytes.checked_add(lookup_bytes),
        ) {
            (Some(work), Some(bytes)) => Ok((work, bytes)),
            _ => Err(RunError::Contract("class variable step quotation overflow")),
        },
        None => Err(RunError::Contract("class variable step quotation overflow")),
    }
}

/// Quotes equality of full interned variable-instance handles, including their Salsa ids.
pub(super) const fn instance_equality_quote() -> RunResult<(usize, usize)> {
    match event_quote(
        INSTANCE_EQUALITY,
        &[
            size_of::<(&TypeVarInstance<'static>, &TypeVarInstance<'static>)>(),
            size_of::<(&salsa::Id, &salsa::Id)>(),
            size_of::<(u32, u32)>(),
            size_of::<bool>(),
        ],
    ) {
        Some(quote) => Ok(quote),
        None => Err(RunError::Contract(
            "class variable comparison quotation overflow",
        )),
    }
}
