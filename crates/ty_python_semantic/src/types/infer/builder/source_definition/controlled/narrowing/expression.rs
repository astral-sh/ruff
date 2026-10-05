//! Admitted storage and source access for expression-predicate production.

use std::alloc::Layout;

use ruff_db::parsed::ParsedModuleRef;
use ruff_python_ast as ast;
use salsa::execution_probe::{ExecutionWork, RunError, RunResult, TaskEndpoint};
use smallvec::SmallVec;
use ty_python_core::SemanticIndex;
use ty_python_core::expression::Expression;
use ty_python_core::place::{PlaceExprRef, ScopedPlaceId};
use ty_python_core::predicate::PredicateNode;

use super::super::storage::{StorageQuote, sequence_merge};
use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::types::narrow::admission::{freeze_constraints, singleton_constraint};
use crate::types::narrow::expression::{
    BooleanMerge, BooleanOperands, CallResult, ExpressionNarrowingEffects,
    ExpressionNarrowingFacts, Frame, Traversal, evaluate_with, simple_with,
};
use crate::types::narrow::{
    FrozenNarrowingConstraints, NarrowingConstraint, NarrowingConstraints,
    NarrowingConstraintsBuilder,
};
use crate::types::{Truthiness, Type};
use crate::{Db, ProgramEnvironment};

fn checked<T>(value: Option<T>) -> RunResult<T> {
    value.ok_or(RunError::Contract(
        "expression narrowing storage quotation overflow",
    ))
}

fn admit(endpoint: &TaskEndpoint<'_, '_>, quote: StorageQuote) -> RunResult<()> {
    endpoint.admit_work(quote.work)?;
    if quote.bytes != 0 {
        endpoint.admit(ExecutionWork::Resource {
            requested_bytes: quote.bytes,
        })?;
    }
    endpoint.check_completion()
}

fn append_quote<T>(len: usize, capacity: usize) -> Option<(StorageQuote, usize)> {
    let mut quote = sequence_merge::<T>(len, capacity, 1)?;
    quote.work = quote.work.checked_add(size_of::<T>().checked_mul(2)?)?;
    let additional = if quote.bytes == 0 {
        0
    } else {
        let requested = quote.bytes.checked_div(size_of::<T>())?;
        Layout::array::<T>(requested).ok()?;
        quote.work = quote
            .work
            .checked_add(capacity.checked_mul(size_of::<T>())?)?
            .checked_add(quote.bytes.checked_mul(2)?)?;
        requested.checked_sub(len)?
    };
    Some((quote, additional))
}

fn take_frame<'db, 'ast>(frame: &mut Frame<'db, 'ast>) -> Frame<'db, 'ast> {
    // The unscheduled replacement owns no storage. A rejected admission leaves the original
    // frame in its calling future, where the runtime drains it with the other local owners.
    std::mem::replace(frame, Frame::MergeAlias { constraints: None })
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> ExpressionNarrowingEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn builder(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        module: &'ast ParsedModuleRef,
        expression: Expression<'db>,
        is_positive: bool,
    ) -> RunResult<NarrowingConstraintsBuilder<'db, 'ast>> {
        self.local(
            size_of::<NarrowingConstraintsBuilder<'db, 'ast>>() * 2 + 1,
            0,
            || {
                NarrowingConstraintsBuilder::new(
                    db,
                    env,
                    module,
                    PredicateNode::Expression(expression),
                    is_positive,
                )
            },
        )
        .await
    }

    async fn retire_builder(
        &self,
        builder: NarrowingConstraintsBuilder<'db, 'ast>,
    ) -> RunResult<()> {
        self.work(1).await?;
        drop(builder);
        Ok(())
    }

    async fn evaluate(
        &self,
        builder: &mut NarrowingConstraintsBuilder<'db, 'ast>,
        mut initial: Frame<'db, 'ast>,
    ) -> RunResult<Option<NarrowingConstraints<'db>>> {
        self.allocate_future(|| {
            evaluate_with(
                builder,
                take_frame(&mut initial),
                ExpressionNarrowingFacts,
                self,
            )
        })
        .await?
        .await
    }

    async fn freeze(
        &self,
        constraints: Option<NarrowingConstraints<'db>>,
    ) -> RunResult<Option<FrozenNarrowingConstraints<'db>>> {
        freeze_constraints(self.access.endpoint(), constraints).await
    }

    async fn start(&self, mut initial: Frame<'db, 'ast>) -> RunResult<Traversal<'db, 'ast>> {
        self.local(size_of::<Traversal<'db, 'ast>>() * 2 + 1, 0, || {
            let mut frames = SmallVec::new();
            frames.push(take_frame(&mut initial));
            Traversal {
                frames,
                result: None,
            }
        })
        .await
    }

    async fn next(
        &self,
        traversal: &mut Traversal<'db, 'ast>,
    ) -> RunResult<Option<Frame<'db, 'ast>>> {
        self.local(size_of::<Frame<'db, 'ast>>() + 1, 0, || {
            traversal.frames.pop()
        })
        .await
    }

    async fn push(
        &self,
        traversal: &mut Traversal<'db, 'ast>,
        mut frame: Frame<'db, 'ast>,
    ) -> RunResult<()> {
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                let (quote, additional) = checked(append_quote::<Frame<'db, 'ast>>(
                    traversal.frames.len(),
                    traversal.frames.capacity(),
                ))?;
                admit(endpoint, quote)?;
                traversal.frames.reserve_exact(additional);
                traversal.frames.push(take_frame(&mut frame));
                Ok(())
            })
            .await)
    }

    async fn take_result(
        &self,
        traversal: &mut Traversal<'db, 'ast>,
    ) -> RunResult<Option<NarrowingConstraints<'db>>> {
        self.local(
            size_of::<Option<NarrowingConstraints<'db>>>() + 1,
            0,
            || traversal.result.take(),
        )
        .await
    }

    async fn set_result(
        &self,
        traversal: &mut Traversal<'db, 'ast>,
        mut constraints: Option<NarrowingConstraints<'db>>,
    ) -> RunResult<()> {
        // Map creation and cloning prepay disposal before a map reaches a retained result slot.
        self.local(
            size_of::<Option<NarrowingConstraints<'db>>>() + 1,
            0,
            || {
                traversal.result = constraints.take();
            },
        )
        .await
    }

    async fn finish(
        &self,
        traversal: Traversal<'db, 'ast>,
    ) -> RunResult<Option<NarrowingConstraints<'db>>> {
        self.work(1).await?;
        Ok(traversal.result)
    }

    async fn node(
        &self,
        builder: &NarrowingConstraintsBuilder<'db, 'ast>,
        expression: Expression<'db>,
    ) -> RunResult<&'ast ast::Expr> {
        let node = self
            .field(expression.read_fields(self.db()).node_ref())
            .await?;
        self.local(1, 0, || node.node(builder.module)).await
    }

    async fn index(
        &self,
        expression: Expression<'db>,
        _builder: &NarrowingConstraintsBuilder<'db, 'ast>,
    ) -> RunResult<&'db SemanticIndex<'db>> {
        let file = self.expression_file(expression).await?;
        self.access.semantic_index(file).await
    }

    async fn simple(
        &self,
        builder: &mut NarrowingConstraintsBuilder<'db, 'ast>,
        node: &'ast ast::Expr,
        is_positive: bool,
    ) -> RunResult<Option<NarrowingConstraints<'db>>> {
        simple_with(builder, node, is_positive, self).await
    }

    async fn simple_place(
        &self,
        builder: &NarrowingConstraintsBuilder<'db, 'ast>,
        node: &ast::Expr,
    ) -> RunResult<Option<ScopedPlaceId>> {
        let Some(target) = self.construct_place(node.into()).await? else {
            return Ok(None);
        };
        let PredicateNode::Expression(expression) = builder.predicate else {
            return self.unavailable(SourceOperation::Narrowing).await;
        };
        let scope = self.expression_scope(expression).await?;
        let table = self.access.place_table(scope).await?;
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                let target = PlaceExprRef::from(&target);
                endpoint.admit_work(checked(table.lookup_work(target))?)?;
                endpoint.check_completion()?;
                table
                    .place_id(target)
                    .map(Some)
                    .ok_or(RunError::Contract("narrowing constraint place is missing"))
            })
            .await)
    }

    async fn negate(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.negate_type(env, ty).await
    }

    async fn singleton(
        &self,
        place: ScopedPlaceId,
        ty: Type<'db>,
    ) -> RunResult<NarrowingConstraints<'db>> {
        let constraint = self
            .local(size_of::<NarrowingConstraint<'db>>() * 2 + 1, 0, || {
                NarrowingConstraint::intersection(ty)
            })
            .await?;
        singleton_constraint(self.access.endpoint(), place, constraint).await
    }

    async fn alias(
        &self,
        _builder: &NarrowingConstraintsBuilder<'db, 'ast>,
        index: &'db SemanticIndex<'db>,
        node: &'ast ast::Expr,
        _name: &'ast ast::ExprName,
        _expression: Expression<'db>,
        _is_positive: bool,
    ) -> RunResult<Option<Expression<'db>>> {
        let absent = self
            .local(index.narrowing_alias_lookup_work(), 0, || {
                index.narrowing_alias_predicate(node).is_none()
            })
            .await?;
        if absent {
            Ok(None)
        } else {
            self.unavailable(SourceOperation::Narrowing).await
        }
    }

    async fn attribute(
        &self,
        _builder: &NarrowingConstraintsBuilder<'db, 'ast>,
        _node: &'ast ast::ExprAttribute,
        _expression: Expression<'db>,
        _is_positive: bool,
    ) -> RunResult<Option<NarrowingConstraints<'db>>> {
        self.unavailable(SourceOperation::Narrowing).await
    }

    async fn subscript(
        &self,
        _builder: &NarrowingConstraintsBuilder<'db, 'ast>,
        _node: &'ast ast::ExprSubscript,
        _expression: Expression<'db>,
        _is_positive: bool,
    ) -> RunResult<Option<NarrowingConstraints<'db>>> {
        self.unavailable(SourceOperation::Narrowing).await
    }

    async fn compare(
        &self,
        _builder: &mut NarrowingConstraintsBuilder<'db, 'ast>,
        _node: &'ast ast::ExprCompare,
        _expression: Expression<'db>,
        _is_positive: bool,
    ) -> RunResult<Option<NarrowingConstraints<'db>>> {
        self.unavailable(SourceOperation::Narrowing).await
    }

    async fn call(
        &self,
        _builder: &mut NarrowingConstraintsBuilder<'db, 'ast>,
        _node: &'ast ast::ExprCall,
        _expression: Expression<'db>,
        _is_positive: bool,
    ) -> RunResult<CallResult<'db, 'ast>> {
        self.unavailable(SourceOperation::Narrowing).await
    }

    async fn if_truthiness(
        &self,
        _builder: &NarrowingConstraintsBuilder<'db, 'ast>,
        _node: &'ast ast::ExprIf,
        _expression: Expression<'db>,
    ) -> RunResult<Truthiness> {
        self.unavailable(SourceOperation::Narrowing).await
    }

    async fn invalidate_named(
        &self,
        _builder: &NarrowingConstraintsBuilder<'db, 'ast>,
        _node: &'ast ast::ExprNamed,
        _constraints: &mut Option<NarrowingConstraints<'db>>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::Narrowing).await
    }

    async fn merge_and(
        &self,
        left: Option<NarrowingConstraints<'db>>,
        right: Option<NarrowingConstraints<'db>>,
    ) -> RunResult<Option<NarrowingConstraints<'db>>> {
        let result = self.unavailable(SourceOperation::Narrowing).await;
        drop(right);
        drop(left);
        result
    }

    async fn merge_or(
        &self,
        left: Option<NarrowingConstraints<'db>>,
        right: Option<NarrowingConstraints<'db>>,
    ) -> RunResult<Option<NarrowingConstraints<'db>>> {
        let result = self.unavailable(SourceOperation::Narrowing).await;
        drop(right);
        drop(left);
        result
    }

    async fn boolean(
        &self,
        _builder: &NarrowingConstraintsBuilder<'db, 'ast>,
        _node: &'ast ast::ExprBoolOp,
        _expression: Expression<'db>,
        _is_positive: bool,
    ) -> RunResult<BooleanOperands<'db, 'ast>> {
        self.unavailable(SourceOperation::Narrowing).await
    }

    async fn next_boolean_operand(
        &self,
        operands: &mut BooleanOperands<'db, 'ast>,
    ) -> RunResult<Option<&'ast ast::Expr>> {
        self.local(2, 0, || {
            let node = operands.node.values.get(operands.cursor);
            if node.is_some() {
                operands.cursor += 1;
            }
            node
        })
        .await
    }

    async fn boolean_truthiness(
        &self,
        _builder: &NarrowingConstraintsBuilder<'db, 'ast>,
        _operands: &BooleanOperands<'db, 'ast>,
        _node: &'ast ast::Expr,
    ) -> RunResult<Truthiness> {
        self.unavailable(SourceOperation::Narrowing).await
    }

    async fn push_boolean_constraint(
        &self,
        operands: &mut BooleanOperands<'db, 'ast>,
        mut constraints: Option<NarrowingConstraints<'db>>,
    ) -> RunResult<()> {
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                let (quote, additional) =
                    checked(append_quote::<Option<NarrowingConstraints<'db>>>(
                        operands.constraints.len(),
                        operands.constraints.capacity(),
                    ))?;
                admit(endpoint, quote)?;
                operands.constraints.reserve_exact(additional);
                operands.constraints.push(constraints.take());
                Ok(())
            })
            .await)
    }

    async fn begin_boolean_merge(
        &self,
        operands: BooleanOperands<'db, 'ast>,
    ) -> RunResult<BooleanMerge<'db>> {
        let mut operands = Some(operands);
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(size_of::<BooleanMerge<'db>>() * 2 + 1)?;
                endpoint.check_completion()?;
                let operands = operands.take().ok_or(RunError::Contract(
                    "narrowing Boolean operands already consumed",
                ))?;
                Ok(BooleanMerge {
                    remaining: operands.constraints.into_iter(),
                    result: None,
                    env: operands.env,
                })
            })
            .await)
    }

    async fn next_boolean_constraint(
        &self,
        merge: &mut BooleanMerge<'db>,
    ) -> RunResult<Option<Option<NarrowingConstraints<'db>>>> {
        self.local(
            size_of::<Option<NarrowingConstraints<'db>>>() + 1,
            0,
            || merge.remaining.next(),
        )
        .await
    }

    async fn take_boolean_result(
        &self,
        merge: &mut BooleanMerge<'db>,
    ) -> RunResult<Option<NarrowingConstraints<'db>>> {
        self.local(
            size_of::<Option<NarrowingConstraints<'db>>>() + 1,
            0,
            || merge.result.take(),
        )
        .await
    }

    async fn set_boolean_result(
        &self,
        merge: &mut BooleanMerge<'db>,
        mut constraints: Option<NarrowingConstraints<'db>>,
    ) -> RunResult<()> {
        self.local(
            size_of::<Option<NarrowingConstraints<'db>>>() + 1,
            0,
            || {
                merge.result = constraints.take();
            },
        )
        .await
    }

    async fn finish_boolean_merge(
        &self,
        merge: BooleanMerge<'db>,
    ) -> RunResult<Option<NarrowingConstraints<'db>>> {
        self.work(1).await?;
        Ok(merge.result)
    }

    async fn discard_boolean_merge(&self, merge: BooleanMerge<'db>) -> RunResult<()> {
        self.work(1).await?;
        drop(merge);
        Ok(())
    }
}
