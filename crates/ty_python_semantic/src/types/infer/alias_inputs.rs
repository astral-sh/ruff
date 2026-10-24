//! Reuse value-inference results when interpreting implicit and PEP 613 type aliases.
//!
//! ```python
//! Scalar = int | str
//! Value = Scalar | bytes
//!
//! def consume(value: Value) -> None: ...
//! ```
//!
//! We infer `Value = Scalar | bytes` as an ordinary assignment: `Scalar` refers to a runtime
//! union object and `bytes` to a class object. Interpreting `Value` in an annotation also infers
//! its right-hand side as a type expression, where it denotes instances of `int`, `str`, or
//! `bytes`. This interpretation still needs the runtime operand types to check that `|` is valid.
//!
//! The two computations use separate inference builders. This module lets the type-expression
//! builder reuse work from value inference through three steps:
//!
//! 1. **Collect during value inference.** [`AliasInferenceCache::Collecting`] owns an
//!    [`AliasInferenceInputsBuilder`]. It records the value type, source definition (when known),
//!    and lookup context of each resolved name or attribute, such as `Scalar` and `bytes`. It also
//!    records which runtime expression types can be reused for operand checks. Those types already
//!    live in the builder's expression table; the cache stores only their reuse contexts.
//! 2. **Retain the results.** When the builder finishes, it compacts the collected maps into
//!    [`AliasInferenceInputs`]. These are the value pass's outputs that become inputs to alias
//!    interpretation. They are stored alongside expression types in the Salsa-cached
//!    [`DefinitionInference`] or [`ExpressionInference`](super::ExpressionInference) result,
//!    so they outlive the builder that collected them.
//! 3. **Reuse during type-expression inference.** [`AliasInferenceCache::Reusing`] borrows the
//!    alias's value-inference [`DefinitionInference`]. The new builder can resolve `Scalar` and
//!    `bytes` from the saved lookups and read their runtime types from that result's expression
//!    table when checking `|`.
//!
//! Reuse requires compatible contexts: name and attribute lookups must agree on whether lookup
//! is deferred and which definition binds type variables. Runtime operand types are reusable
//! only for eager expressions inferred without an expected type, with the same type-variable
//! binding context. Missing or incompatible entries use ordinary inference.
//!
//! [`AliasInferenceCache::Disabled`] is used when a builder neither collects nor reuses these
//! results. Collected inputs are also discarded when the inferred value does not need further
//! alias interpretation. A builder never collects and reuses inputs at the same time.
//!
//! Reusing a value lookup still leaves type-expression conversion and cycle validation to the
//! alias inference builder. Simplifying a runtime union can remove recursive references, so the
//! normalized runtime value alone cannot establish that an alias is valid.

use crate::types::{KnownInstanceType, any_over_type};
use ruff_python_ast as ast;
use rustc_hash::FxHashMap;
use ty_python_core::ExpressionNodeKey;
use ty_python_core::definition::Definition;
use ty_python_core::frozen::FrozenMap;

use super::builder::DeferredExpressionState;
use super::{DefinitionInference, TypeContext};
use crate::types::Type;
use crate::{Db, ProgramEnvironment};

/// A name or attribute resolved before its value is interpreted as a type expression.
///
/// Retain the definition behind a normalized value, and reuse the resolution only when deferred
/// lookups and type-variable binding contexts agree.
#[derive(Debug, Clone, Copy, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
struct ResolvedReference<'db> {
    ty: Type<'db>,
    definition: Option<Definition<'db>>,
    deferred_state: DeferredExpressionState,
    typevar_binding_context: Option<Definition<'db>>,
}

/// Value-inference results retained for interpreting an alias's right-hand side as a type expression.
#[derive(Debug, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct AliasInferenceInputs<'db> {
    references: FrozenMap<ExpressionNodeKey, ResolvedReference<'db>>,
    /// Binding contexts of eager expressions with default type context. Their runtime types are
    /// stored in the inference region's expression table.
    runtime_values: FrozenMap<ExpressionNodeKey, Option<Definition<'db>>>,
}

impl<'db> AliasInferenceInputs<'db> {
    /// Normalize cached reference types alongside the inference result that owns them.
    /// Runtime operand types are normalized separately in that result's expression table.
    pub(super) fn normalize_cycle(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        previous: Option<&Self>,
        cycle: &salsa::Cycle,
    ) {
        for (expression, reference) in &mut self.references {
            reference.ty = match previous.and_then(|inputs| inputs.references.get(expression)) {
                Some(previous) => reference.ty.cycle_normalized(db, env, previous.ty, cycle),
                None => reference.ty.recursive_type_normalized(db, env, cycle),
            };
        }
    }
}

/// Mutable value-inference results, compacted into [`AliasInferenceInputs`] when inference finishes.
#[derive(Default)]
pub(super) struct AliasInferenceInputsBuilder<'db> {
    references: FxHashMap<ExpressionNodeKey, ResolvedReference<'db>>,
    runtime_values: FxHashMap<ExpressionNodeKey, Option<Definition<'db>>>,
}

impl<'db> AliasInferenceInputsBuilder<'db> {
    pub(super) fn is_empty(&self) -> bool {
        self.references.is_empty() && self.runtime_values.is_empty()
    }

    pub(super) fn finish(self) -> Option<Box<AliasInferenceInputs<'db>>> {
        (!self.is_empty()).then(|| {
            Box::new(AliasInferenceInputs {
                references: FrozenMap::from(self.references),
                runtime_values: FrozenMap::from(self.runtime_values),
            })
        })
    }
}

/// A region either collects alias inputs or reuses a completed value inference, never both.
#[derive(Default)]
pub(super) enum AliasInferenceCache<'db> {
    #[default]
    Disabled,
    Collecting(AliasInferenceInputsBuilder<'db>),
    Reusing(&'db DefinitionInference<'db>),
}

impl<'db> AliasInferenceCache<'db> {
    pub(super) fn new(collect: bool) -> Self {
        if collect {
            Self::Collecting(AliasInferenceInputsBuilder::default())
        } else {
            Self::Disabled
        }
    }

    /// Start a speculative region with the same reuse policy and no collected inputs.
    pub(super) fn speculate(&self) -> Self {
        match self {
            Self::Disabled => Self::Disabled,
            Self::Collecting(_) => Self::new(true),
            Self::Reusing(inference) => Self::Reusing(inference),
        }
    }

    pub(super) fn is_collecting(&self) -> bool {
        matches!(self, Self::Collecting(_))
    }

    pub(super) fn has_collected_inputs(&self) -> bool {
        matches!(self, Self::Collecting(inputs) if !inputs.is_empty())
    }

    pub(super) fn into_inputs(self) -> AliasInferenceInputsBuilder<'db> {
        match self {
            Self::Collecting(inputs) => inputs,
            Self::Disabled | Self::Reusing(_) => AliasInferenceInputsBuilder::default(),
        }
    }

    pub(super) fn record_reference(
        &mut self,
        expression: impl Into<ExpressionNodeKey>,
        ty: Type<'db>,
        definition: Option<Definition<'db>>,
        deferred_state: DeferredExpressionState,
        typevar_binding_context: Option<Definition<'db>>,
    ) {
        if let Self::Collecting(inputs) = self {
            inputs.references.insert(
                expression.into(),
                ResolvedReference {
                    ty,
                    definition,
                    deferred_state,
                    typevar_binding_context,
                },
            );
        }
    }

    /// Mark eager operands inferred without an expected type for reuse in runtime operator checks.
    pub(super) fn record_runtime_value(
        &mut self,
        expression: &ast::Expr,
        tcx: TypeContext<'db>,
        deferred_state: DeferredExpressionState,
        typevar_binding_context: Option<Definition<'db>>,
    ) {
        if let Self::Collecting(inputs) = self
            && tcx == TypeContext::default()
            && !deferred_state.is_deferred()
        {
            inputs
                .runtime_values
                .insert(expression.into(), typevar_binding_context);
        }
    }

    /// Reuse a value lookup only when deferred lookup and type-variable binding contexts match.
    /// The returned type retains its value semantics; alias interpretation still performs
    /// type-expression conversion and cycle validation.
    pub(super) fn reference(
        &self,
        expression: impl Into<ExpressionNodeKey>,
        deferred_state: DeferredExpressionState,
        typevar_binding_context: Option<Definition<'db>>,
    ) -> Option<(Type<'db>, Option<Definition<'db>>)> {
        let Self::Reusing(inference) = self else {
            return None;
        };
        let reference = inference
            .alias_inputs()?
            .references
            .get(&expression.into())?;
        (reference.deferred_state == deferred_state
            && reference.typevar_binding_context == typevar_binding_context)
            .then_some((reference.ty, reference.definition))
    }

    /// Read an eager operand's runtime type in the same binding context as its value inference.
    ///
    /// ```python
    /// Alias = int | str
    /// ```
    ///
    /// The runtime `|` check uses the class literal types of `int` and `str`, retained in value
    /// inference's expression table. Type-expression inference produces their instance types.
    pub(super) fn runtime_value(
        &self,
        expression: &ast::Expr,
        deferred_state: DeferredExpressionState,
        typevar_binding_context: Option<Definition<'db>>,
    ) -> Option<Type<'db>> {
        let Self::Reusing(inference) = self else {
            return None;
        };
        if deferred_state.is_deferred()
            || inference
                .alias_inputs()?
                .runtime_values
                .get(&expression.into())
                != Some(&typevar_binding_context)
        {
            return None;
        }
        inference.try_expression_type(expression)
    }

    pub(super) fn extend_cached(&mut self, inputs: Option<&AliasInferenceInputs<'db>>) {
        if let Self::Collecting(collected) = self
            && let Some(inputs) = inputs
        {
            collected
                .references
                .extend(inputs.references.iter().copied());
            collected
                .runtime_values
                .extend(inputs.runtime_values.iter().copied());
        }
    }

    pub(super) fn extend_collected(&mut self, inputs: &AliasInferenceInputsBuilder<'db>) {
        if let Self::Collecting(collected) = self {
            collected.references.extend(&inputs.references);
            collected.runtime_values.extend(&inputs.runtime_values);
        }
    }

    pub(super) fn merge(&mut self, other: Self) {
        if let Self::Collecting(collected) = self
            && let Self::Collecting(other) = other
        {
            collected.references.extend(other.references);
            collected.runtime_values.extend(other.runtime_values);
        }
    }
}

/// Whether an expression can supply cached inputs for a legacy type alias.
pub(super) fn is_implicit_alias_value(value: &ast::Expr) -> bool {
    matches!(
        value,
        ast::Expr::Name(_)
            | ast::Expr::Attribute(_)
            | ast::Expr::Subscript(_)
            | ast::Expr::BinOp(_)
            | ast::Expr::StringLiteral(_)
    )
}

/// Whether interpreting a value as an alias can reveal additional recursive references.
/// Value inference uses this to decide whether to retain inputs for later alias inference.
pub(super) fn may_require_implicit_alias_inference<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
) -> bool {
    any_over_type(db, env, ty, false, |ty| {
        matches!(
            ty,
            Type::Dynamic(_)
                | Type::Divergent(_)
                | Type::Recursive(_)
                | Type::TypeAlias(_)
                | Type::KnownInstance(
                    KnownInstanceType::UnionType(_) | KnownInstanceType::LiteralStringAlias(_)
                )
        )
    })
}

