//! Ordered expression-predicate production with flat continuation ownership.
//!
//! Semantic leaves and constraint operations remain explicit effects. In particular, Boolean
//! operands all finish producing their constraints before their maps are merged.

use std::convert::Infallible;
use std::vec::IntoIter;

use ruff_db::parsed::ParsedModuleRef;
use ruff_python_ast::{self as ast, BoolOp};
use smallvec::{SmallVec, smallvec};
use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::expression::Expression;
use ty_python_core::place::{PlaceExpr, ScopedPlaceId};
use ty_python_core::predicate::PredicateNode;
use ty_python_core::{SemanticIndex, semantic_index};

use super::{
    ExpressionNarrowingConstraints, FrozenNarrowingConstraints, NarrowingConstraint,
    NarrowingConstraints, NarrowingConstraintsBuilder,
};
use crate::types::infer::ExpressionInference;
use crate::types::{KnownClass, Truthiness, Type, TypeContext, infer_expression_types};
use crate::{Db, ProgramEnvironment};

pub(in crate::types) struct ExpressionNarrowingFacts;
pub(super) struct OrdinaryExpressionNarrowingEffects;

enum NodeKind<'ast> {
    Name(&'ast ast::ExprName),
    Attribute(&'ast ast::ExprAttribute),
    Subscript(&'ast ast::ExprSubscript),
    Compare(&'ast ast::ExprCompare),
    Call(&'ast ast::ExprCall),
    Not(&'ast ast::Expr),
    Boolean(&'ast ast::ExprBoolOp),
    If(&'ast ast::ExprIf),
    Named(&'ast ast::ExprNamed),
    Other,
}

pub(in crate::types) enum CallResult<'db, 'ast> {
    Complete(Option<NarrowingConstraints<'db>>),
    Child(&'ast ast::Expr),
}

pub(in crate::types) struct BooleanOperands<'db, 'ast> {
    pub(in crate::types) expression: Expression<'db>,
    pub(in crate::types) node: &'ast ast::ExprBoolOp,
    pub(in crate::types) is_positive: bool,
    pub(in crate::types) inference: &'db ExpressionInference<'db>,
    pub(in crate::types) env: ProgramEnvironment<'db>,
    pub(in crate::types) cursor: usize,
    pub(in crate::types) constraints: Vec<Option<NarrowingConstraints<'db>>>,
}

pub(in crate::types) struct BooleanMerge<'db> {
    pub(in crate::types) remaining: IntoIter<Option<NarrowingConstraints<'db>>>,
    pub(in crate::types) result: Option<NarrowingConstraints<'db>>,
    pub(in crate::types) env: ProgramEnvironment<'db>,
}

/// Frames borrow the prepared AST and own only flat maps, vectors, and iterators.
pub(in crate::types) enum Frame<'db, 'ast> {
    Expression {
        expression: Expression<'db>,
        is_positive: bool,
    },
    Node {
        node: &'ast ast::Expr,
        expression: Expression<'db>,
        is_positive: bool,
    },
    MergeAlias {
        constraints: Option<NarrowingConstraints<'db>>,
    },
    BooleanNext(BooleanOperands<'db, 'ast>),
    BooleanAfter(BooleanOperands<'db, 'ast>),
    BooleanMerge {
        merge: BooleanMerge<'db>,
        conjunction: bool,
        first: bool,
    },
    IfAfterTest {
        node: &'ast ast::ExprIf,
        expression: Expression<'db>,
        is_positive: bool,
    },
    IfAfterBody {
        node: &'ast ast::ExprIf,
        expression: Expression<'db>,
        is_positive: bool,
        test_constraints: Option<NarrowingConstraints<'db>>,
    },
    IfAfterNegativeTest {
        node: &'ast ast::ExprIf,
        expression: Expression<'db>,
        is_positive: bool,
        body_constraints: Option<NarrowingConstraints<'db>>,
    },
    IfAfterElse {
        body_constraints: Option<NarrowingConstraints<'db>>,
        test_constraints: Option<NarrowingConstraints<'db>>,
    },
    NamedAfter {
        node: &'ast ast::ExprNamed,
        target_constraints: Option<NarrowingConstraints<'db>>,
    },
}

pub(in crate::types) struct Traversal<'db, 'ast> {
    pub(in crate::types) frames: SmallVec<[Frame<'db, 'ast>; 4]>,
    pub(in crate::types) result: Option<NarrowingConstraints<'db>>,
}

shared_semantic_family! {
    #[synchronous(SynchronousExpressionNarrowingEffects)]
    pub(in crate::types) trait ExpressionNarrowingEffects<'db, 'ast> {
        type Error;

        // Owned arguments stay outside rejectable admission closures until acceptance. Every
        // allocation or clone prepays its disposal, including a later refusal or native unwind.
        #[operation(local)]
        async fn builder(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>, module: &'ast ParsedModuleRef, expression: Expression<'db>, is_positive: bool) -> Result<NarrowingConstraintsBuilder<'db, 'ast>, Self::Error>;
        #[operation(local)]
        async fn retire_builder(&self, builder: NarrowingConstraintsBuilder<'db, 'ast>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn evaluate(&self, builder: &mut NarrowingConstraintsBuilder<'db, 'ast>, initial: Frame<'db, 'ast>) -> Result<Option<NarrowingConstraints<'db>>, Self::Error>;
        #[operation(local)]
        async fn freeze(&self, constraints: Option<NarrowingConstraints<'db>>) -> Result<Option<FrozenNarrowingConstraints<'db>>, Self::Error>;

        #[operation(local)]
        async fn start(&self, initial: Frame<'db, 'ast>) -> Result<Traversal<'db, 'ast>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next(&self, traversal: &mut Traversal<'db, 'ast>) -> Result<Option<Frame<'db, 'ast>>, Self::Error>;
        #[operation(local)]
        async fn push(&self, traversal: &mut Traversal<'db, 'ast>, frame: Frame<'db, 'ast>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn take_result(&self, traversal: &mut Traversal<'db, 'ast>) -> Result<Option<NarrowingConstraints<'db>>, Self::Error>;
        #[operation(local)]
        async fn set_result(&self, traversal: &mut Traversal<'db, 'ast>, constraints: Option<NarrowingConstraints<'db>>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn finish(&self, traversal: Traversal<'db, 'ast>) -> Result<Option<NarrowingConstraints<'db>>, Self::Error>;

        #[operation(local)]
        async fn node(&self, builder: &NarrowingConstraintsBuilder<'db, 'ast>, expression: Expression<'db>) -> Result<&'ast ast::Expr, Self::Error>;
        #[operation(local)]
        async fn index(&self, expression: Expression<'db>, builder: &NarrowingConstraintsBuilder<'db, 'ast>) -> Result<&'db SemanticIndex<'db>, Self::Error>;
        #[operation(source)]
        async fn simple(&self, builder: &mut NarrowingConstraintsBuilder<'db, 'ast>, node: &'ast ast::Expr, is_positive: bool) -> Result<Option<NarrowingConstraints<'db>>, Self::Error>;
        #[operation(source)]
        async fn simple_place(&self, builder: &NarrowingConstraintsBuilder<'db, 'ast>, node: &ast::Expr) -> Result<Option<ScopedPlaceId>, Self::Error>;
        #[operation(source)]
        async fn negate(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn singleton(&self, place: ScopedPlaceId, ty: Type<'db>) -> Result<NarrowingConstraints<'db>, Self::Error>;
        #[operation(source)]
        async fn alias(&self, builder: &NarrowingConstraintsBuilder<'db, 'ast>, index: &'db SemanticIndex<'db>, node: &'ast ast::Expr, name: &'ast ast::ExprName, expression: Expression<'db>, is_positive: bool) -> Result<Option<Expression<'db>>, Self::Error>;
        #[operation(source)]
        async fn attribute(&self, builder: &NarrowingConstraintsBuilder<'db, 'ast>, node: &'ast ast::ExprAttribute, expression: Expression<'db>, is_positive: bool) -> Result<Option<NarrowingConstraints<'db>>, Self::Error>;
        #[operation(source)]
        async fn subscript(&self, builder: &NarrowingConstraintsBuilder<'db, 'ast>, node: &'ast ast::ExprSubscript, expression: Expression<'db>, is_positive: bool) -> Result<Option<NarrowingConstraints<'db>>, Self::Error>;
        #[operation(source)]
        async fn compare(&self, builder: &mut NarrowingConstraintsBuilder<'db, 'ast>, node: &'ast ast::ExprCompare, expression: Expression<'db>, is_positive: bool) -> Result<Option<NarrowingConstraints<'db>>, Self::Error>;
        #[operation(source)]
        async fn call(&self, builder: &mut NarrowingConstraintsBuilder<'db, 'ast>, node: &'ast ast::ExprCall, expression: Expression<'db>, is_positive: bool) -> Result<CallResult<'db, 'ast>, Self::Error>;
        #[operation(source)]
        async fn if_truthiness(&self, builder: &NarrowingConstraintsBuilder<'db, 'ast>, node: &'ast ast::ExprIf, expression: Expression<'db>) -> Result<Truthiness, Self::Error>;
        #[operation(source)]
        async fn invalidate_named(&self, builder: &NarrowingConstraintsBuilder<'db, 'ast>, node: &'ast ast::ExprNamed, constraints: &mut Option<NarrowingConstraints<'db>>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn merge_and(&self, left: Option<NarrowingConstraints<'db>>, right: Option<NarrowingConstraints<'db>>) -> Result<Option<NarrowingConstraints<'db>>, Self::Error>;
        #[operation(source)]
        async fn merge_or(&self, left: Option<NarrowingConstraints<'db>>, right: Option<NarrowingConstraints<'db>>) -> Result<Option<NarrowingConstraints<'db>>, Self::Error>;

        #[operation(source)]
        async fn boolean(&self, builder: &NarrowingConstraintsBuilder<'db, 'ast>, node: &'ast ast::ExprBoolOp, expression: Expression<'db>, is_positive: bool) -> Result<BooleanOperands<'db, 'ast>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_boolean_operand(&self, operands: &mut BooleanOperands<'db, 'ast>) -> Result<Option<&'ast ast::Expr>, Self::Error>;
        #[operation(source)]
        async fn boolean_truthiness(&self, builder: &NarrowingConstraintsBuilder<'db, 'ast>, operands: &BooleanOperands<'db, 'ast>, node: &'ast ast::Expr) -> Result<Truthiness, Self::Error>;
        #[operation(local)]
        async fn push_boolean_constraint(&self, operands: &mut BooleanOperands<'db, 'ast>, constraints: Option<NarrowingConstraints<'db>>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn begin_boolean_merge(&self, operands: BooleanOperands<'db, 'ast>) -> Result<BooleanMerge<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_boolean_constraint(&self, merge: &mut BooleanMerge<'db>) -> Result<Option<Option<NarrowingConstraints<'db>>>, Self::Error>;
        #[operation(local)]
        async fn take_boolean_result(&self, merge: &mut BooleanMerge<'db>) -> Result<Option<NarrowingConstraints<'db>>, Self::Error>;
        #[operation(local)]
        async fn set_boolean_result(&self, merge: &mut BooleanMerge<'db>, constraints: Option<NarrowingConstraints<'db>>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn finish_boolean_merge(&self, merge: BooleanMerge<'db>) -> Result<Option<NarrowingConstraints<'db>>, Self::Error>;
        #[operation(local)]
        async fn discard_boolean_merge(&self, merge: BooleanMerge<'db>) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl ExpressionNarrowingFacts {
        fn kind<'ast>(&self, node: &'ast ast::Expr) -> NodeKind<'ast> {
            match node {
                ast::Expr::Name(name) => NodeKind::Name(name),
                ast::Expr::Attribute(attribute) => NodeKind::Attribute(attribute),
                ast::Expr::Subscript(subscript) => NodeKind::Subscript(subscript),
                ast::Expr::Compare(compare) => NodeKind::Compare(compare),
                ast::Expr::Call(call) => NodeKind::Call(call),
                ast::Expr::UnaryOp(unary) if unary.op == ast::UnaryOp::Not => NodeKind::Not(&unary.operand),
                ast::Expr::BoolOp(boolean) => NodeKind::Boolean(boolean),
                ast::Expr::If(conditional) => NodeKind::If(conditional),
                ast::Expr::Named(named) => NodeKind::Named(named),
                _ => NodeKind::Other,
            }
        }

        fn include_boolean_operand(&self, op: BoolOp, truthiness: Truthiness) -> bool {
            truthiness != match op {
                BoolOp::And => Truthiness::AlwaysTrue,
                BoolOp::Or => Truthiness::AlwaysFalse,
            }
        }

        fn conjunction(&self, op: BoolOp, is_positive: bool) -> bool {
            matches!((op, is_positive), (BoolOp::And, true) | (BoolOp::Or, false))
        }
    }

    #[synchronous(simple_sync)]
    #[capabilities(effects = ExpressionNarrowingEffects)]
    #[passive_values(Type::AlwaysFalsy, Type::AlwaysTruthy)]
    pub(in crate::types) async fn simple_with<'db, 'ast, E: ExpressionNarrowingEffects<'db, 'ast>>(
        builder: &mut NarrowingConstraintsBuilder<'db, 'ast>,
        node: &ast::Expr,
        is_positive: bool,
        effects: &E,
    ) -> Result<Option<NarrowingConstraints<'db>>, E::Error> {
        let Some(place) = effects.simple_place(builder, node).await? else {
            return Ok(None);
        };
        let excluded = if is_positive { Type::AlwaysFalsy } else { Type::AlwaysTruthy };
        let ty = effects.negate(builder.db, &builder.env, excluded).await?;
        Ok(Some(effects.singleton(place, ty).await?))
    }

    #[synchronous(produce_sync)]
    #[capabilities(effects = ExpressionNarrowingEffects)]
    #[passive_values(ExpressionNarrowingConstraints, Frame::Expression)]
    pub(in crate::types) async fn produce_with<'db, 'ast, E: ExpressionNarrowingEffects<'db, 'ast>>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        module: &'ast ParsedModuleRef,
        expression: Expression<'db>,
        effects: &E,
    ) -> Result<ExpressionNarrowingConstraints<'db>, E::Error> {
        let mut builder = effects.builder(db, env, module, expression, true).await?;
        let constraints = effects.evaluate(&mut builder, Frame::Expression { expression, is_positive: true }).await?;
        let positive = effects.freeze(constraints).await?;
        effects.retire_builder(builder).await?;
        let mut builder = effects.builder(db, env, module, expression, false).await?;
        let constraints = effects.evaluate(&mut builder, Frame::Expression { expression, is_positive: false }).await?;
        let negative = effects.freeze(constraints).await?;
        effects.retire_builder(builder).await?;
        Ok(ExpressionNarrowingConstraints { positive, negative })
    }

    #[synchronous(evaluate_sync)]
    #[capabilities(effects = ExpressionNarrowingEffects, facts = ExpressionNarrowingFacts)]
    #[passive_values(Frame::Expression, Frame::Node, Frame::MergeAlias, Frame::BooleanNext, Frame::BooleanAfter, Frame::BooleanMerge, Frame::IfAfterTest, Frame::IfAfterBody, Frame::IfAfterNegativeTest, Frame::IfAfterElse, Frame::NamedAfter)]
    pub(in crate::types) async fn evaluate_with<'db, 'ast, E: ExpressionNarrowingEffects<'db, 'ast>>(
        builder: &mut NarrowingConstraintsBuilder<'db, 'ast>,
        initial: Frame<'db, 'ast>,
        facts: ExpressionNarrowingFacts,
        effects: &E,
    ) -> Result<Option<NarrowingConstraints<'db>>, E::Error> {
        let mut traversal = effects.start(initial).await?;
        #[cursor_loop]
        while let Some(frame) = effects.next(&mut traversal).await? {
            match frame {
                Frame::Expression { expression, is_positive } => {
                    let node = effects.node(builder, expression).await?;
                    effects.push(&mut traversal, Frame::Node { node, expression, is_positive }).await?;
                }
                Frame::Node { node, expression, is_positive } => {
                    match facts.kind(node) {
                        NodeKind::Name(name) => {
                            let index = effects.index(expression, builder).await?;
                            let constraints = effects.simple(builder, node, is_positive).await?;
                            if let Some(alias) = effects.alias(builder, index, node, name, expression, is_positive).await? {
                                effects.push(&mut traversal, Frame::MergeAlias { constraints }).await?;
                                effects.push(&mut traversal, Frame::Expression { expression: alias, is_positive }).await?;
                            } else {
                                effects.set_result(&mut traversal, constraints).await?;
                            }
                        }
                        NodeKind::Attribute(attribute) => {
                            let constraints = effects.simple(builder, node, is_positive).await?;
                            let nominal_constraints = effects.attribute(builder, attribute, expression, is_positive).await?;
                            let constraints = effects.merge_and(constraints, nominal_constraints).await?;
                            effects.set_result(&mut traversal, constraints).await?;
                        }
                        NodeKind::Subscript(subscript) => {
                            let constraints = effects.simple(builder, node, is_positive).await?;
                            let typeddict_constraints = effects.subscript(builder, subscript, expression, is_positive).await?;
                            let constraints = effects.merge_and(constraints, typeddict_constraints).await?;
                            effects.set_result(&mut traversal, constraints).await?;
                        }
                        NodeKind::Compare(compare) => {
                            let constraints = effects.compare(builder, compare, expression, is_positive).await?;
                            effects.set_result(&mut traversal, constraints).await?;
                        }
                        NodeKind::Call(call) => {
                            match effects.call(builder, call, expression, is_positive).await? {
                                CallResult::Complete(constraints) => effects.set_result(&mut traversal, constraints).await?,
                                CallResult::Child(node) => effects.push(&mut traversal, Frame::Node { node, expression, is_positive }).await?,
                            }
                        }
                        NodeKind::Not(node) => {
                            effects.push(&mut traversal, Frame::Node { node, expression, is_positive: !is_positive }).await?;
                        }
                        NodeKind::Boolean(node) => {
                            let operands = effects.boolean(builder, node, expression, is_positive).await?;
                            effects.push(&mut traversal, Frame::BooleanNext(operands)).await?;
                        }
                        NodeKind::If(node) => {
                            match effects.if_truthiness(builder, node, expression).await? {
                                Truthiness::AlwaysTrue => effects.push(&mut traversal, Frame::Node { node: &node.body, expression, is_positive }).await?,
                                Truthiness::AlwaysFalse => effects.push(&mut traversal, Frame::Node { node: &node.orelse, expression, is_positive }).await?,
                                Truthiness::Ambiguous => {
                                    effects.push(&mut traversal, Frame::IfAfterTest { node, expression, is_positive }).await?;
                                    effects.push(&mut traversal, Frame::Node { node: &node.test, expression, is_positive: true }).await?;
                                }
                            }
                        }
                        NodeKind::Named(node) => {
                            let target_constraints = effects.simple(builder, &node.target, is_positive).await?;
                            effects.push(&mut traversal, Frame::NamedAfter { node, target_constraints }).await?;
                            effects.push(&mut traversal, Frame::Node { node: &node.value, expression, is_positive }).await?;
                        }
                        NodeKind::Other => effects.set_result(&mut traversal, None).await?,
                    }
                }
                Frame::MergeAlias { constraints } => {
                    let aliased_constraints = effects.take_result(&mut traversal).await?;
                    // For example, suppose we have an alias `is_none = x is None`.
                    // When this alias is used for narrowing, that is, within a block like `if is_none: ...`,
                    // both the constraint `is_none: Literal[True]` and the constraint `x: None` should be imposed.
                    // The former is `constraints` and the latter is `aliased_constraints`.
                    let constraints = effects.merge_and(constraints, aliased_constraints).await?;
                    effects.set_result(&mut traversal, constraints).await?;
                }
                Frame::BooleanNext(mut operands) => {
                    if let Some(node) = effects.next_boolean_operand(&mut operands).await? {
                        let truthiness = effects.boolean_truthiness(builder, &operands, node).await?;
                        // filter our arms with statically known truthiness
                        if facts.include_boolean_operand(operands.node.op, truthiness) {
                            let expression = operands.expression;
                            let is_positive = operands.is_positive;
                            effects.push(&mut traversal, Frame::BooleanAfter(operands)).await?;
                            effects.push(&mut traversal, Frame::Node { node, expression, is_positive }).await?;
                        } else {
                            effects.push(&mut traversal, Frame::BooleanNext(operands)).await?;
                        }
                    } else {
                        let conjunction = facts.conjunction(operands.node.op, operands.is_positive);
                        let merge = effects.begin_boolean_merge(operands).await?;
                        effects.push(&mut traversal, Frame::BooleanMerge { merge, conjunction, first: true }).await?;
                    }
                }
                Frame::BooleanAfter(mut operands) => {
                    let constraints = effects.take_result(&mut traversal).await?;
                    effects.push_boolean_constraint(&mut operands, constraints).await?;
                    effects.push(&mut traversal, Frame::BooleanNext(operands)).await?;
                }
                Frame::BooleanMerge { mut merge, conjunction, first } => {
                    if let Some(constraints) = effects.next_boolean_constraint(&mut merge).await? {
                        if conjunction {
                            if let Some(constraints) = constraints {
                                let previous = effects.take_boolean_result(&mut merge).await?;
                                let constraints = effects.merge_and(previous, Some(constraints)).await?;
                                effects.set_boolean_result(&mut merge, constraints).await?;
                            }
                            effects.push(&mut traversal, Frame::BooleanMerge { merge, conjunction, first: false }).await?;
                        } else if let Some(constraints) = constraints {
                            if first {
                                effects.set_boolean_result(&mut merge, Some(constraints)).await?;
                            } else {
                                let previous = effects.take_boolean_result(&mut merge).await?;
                                let constraints = effects.merge_or(previous, Some(constraints)).await?;
                                effects.set_boolean_result(&mut merge, constraints).await?;
                            }
                            effects.push(&mut traversal, Frame::BooleanMerge { merge, conjunction, first: false }).await?;
                        } else {
                            effects.discard_boolean_merge(merge).await?;
                            effects.set_result(&mut traversal, None).await?;
                        }
                    } else {
                        let constraints = effects.finish_boolean_merge(merge).await?;
                        effects.set_result(&mut traversal, constraints).await?;
                    }
                }
                Frame::IfAfterTest { node, expression, is_positive } => {
                    let test_constraints = effects.take_result(&mut traversal).await?;
                    effects.push(&mut traversal, Frame::IfAfterBody { node, expression, is_positive, test_constraints }).await?;
                    effects.push(&mut traversal, Frame::Node { node: &node.body, expression, is_positive }).await?;
                }
                Frame::IfAfterBody { node, expression, is_positive, test_constraints } => {
                    let body_constraints = effects.take_result(&mut traversal).await?;
                    let body_constraints = effects.merge_and(test_constraints, body_constraints).await?;
                    effects.push(&mut traversal, Frame::IfAfterNegativeTest { node, expression, is_positive, body_constraints }).await?;
                    effects.push(&mut traversal, Frame::Node { node: &node.test, expression, is_positive: false }).await?;
                }
                Frame::IfAfterNegativeTest { node, expression, is_positive, body_constraints } => {
                    let test_constraints = effects.take_result(&mut traversal).await?;
                    effects.push(&mut traversal, Frame::IfAfterElse { body_constraints, test_constraints }).await?;
                    effects.push(&mut traversal, Frame::Node { node: &node.orelse, expression, is_positive }).await?;
                }
                Frame::IfAfterElse { body_constraints, test_constraints } => {
                    let orelse_constraints = effects.take_result(&mut traversal).await?;
                    let orelse_constraints = effects.merge_and(test_constraints, orelse_constraints).await?;
                    // `a if c else b` is equivalent to `(c and a) or (not c and b)`.
                    let constraints = effects.merge_or(body_constraints, orelse_constraints).await?;
                    effects.set_result(&mut traversal, constraints).await?;
                }
                Frame::NamedAfter { node, target_constraints } => {
                    let mut value_constraints = effects.take_result(&mut traversal).await?;
                    effects.invalidate_named(builder, node, &mut value_constraints).await?;
                    let constraints = effects.merge_and(target_constraints, value_constraints).await?;
                    effects.set_result(&mut traversal, constraints).await?;
                }
            }
        }
        effects.finish(traversal).await
    }
}

impl<'db, 'ast> SynchronousExpressionNarrowingEffects<'db, 'ast>
    for OrdinaryExpressionNarrowingEffects
{
    type Error = Infallible;

    fn builder(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        module: &'ast ParsedModuleRef,
        expression: Expression<'db>,
        is_positive: bool,
    ) -> Result<NarrowingConstraintsBuilder<'db, 'ast>, Self::Error> {
        Ok(NarrowingConstraintsBuilder::new(
            db,
            env,
            module,
            PredicateNode::Expression(expression),
            is_positive,
        ))
    }

    fn retire_builder(
        &self,
        builder: NarrowingConstraintsBuilder<'db, 'ast>,
    ) -> Result<(), Self::Error> {
        drop(builder);
        Ok(())
    }

    fn evaluate(
        &self,
        builder: &mut NarrowingConstraintsBuilder<'db, 'ast>,
        initial: Frame<'db, 'ast>,
    ) -> Result<Option<NarrowingConstraints<'db>>, Self::Error> {
        evaluate_sync(builder, initial, ExpressionNarrowingFacts, self)
    }

    fn freeze(
        &self,
        constraints: Option<NarrowingConstraints<'db>>,
    ) -> Result<Option<FrozenNarrowingConstraints<'db>>, Self::Error> {
        Ok(constraints.map(FrozenNarrowingConstraints::from))
    }

    fn start(&self, initial: Frame<'db, 'ast>) -> Result<Traversal<'db, 'ast>, Self::Error> {
        Ok(Traversal {
            frames: smallvec![initial],
            result: None,
        })
    }

    fn next(
        &self,
        traversal: &mut Traversal<'db, 'ast>,
    ) -> Result<Option<Frame<'db, 'ast>>, Self::Error> {
        Ok(traversal.frames.pop())
    }

    fn push(
        &self,
        traversal: &mut Traversal<'db, 'ast>,
        frame: Frame<'db, 'ast>,
    ) -> Result<(), Self::Error> {
        traversal.frames.push(frame);
        Ok(())
    }

    fn take_result(
        &self,
        traversal: &mut Traversal<'db, 'ast>,
    ) -> Result<Option<NarrowingConstraints<'db>>, Self::Error> {
        Ok(traversal.result.take())
    }

    fn set_result(
        &self,
        traversal: &mut Traversal<'db, 'ast>,
        constraints: Option<NarrowingConstraints<'db>>,
    ) -> Result<(), Self::Error> {
        traversal.result = constraints;
        Ok(())
    }

    fn finish(
        &self,
        traversal: Traversal<'db, 'ast>,
    ) -> Result<Option<NarrowingConstraints<'db>>, Self::Error> {
        Ok(traversal.result)
    }

    fn node(
        &self,
        builder: &NarrowingConstraintsBuilder<'db, 'ast>,
        expression: Expression<'db>,
    ) -> Result<&'ast ast::Expr, Self::Error> {
        Ok(expression.node_ref(builder.db).node(builder.module))
    }

    fn index(
        &self,
        expression: Expression<'db>,
        builder: &NarrowingConstraintsBuilder<'db, 'ast>,
    ) -> Result<&'db SemanticIndex<'db>, Self::Error> {
        Ok(semantic_index(
            builder.db,
            expression.program_file(builder.db),
        ))
    }

    fn simple(
        &self,
        builder: &mut NarrowingConstraintsBuilder<'db, 'ast>,
        node: &'ast ast::Expr,
        is_positive: bool,
    ) -> Result<Option<NarrowingConstraints<'db>>, Self::Error> {
        Ok(builder.evaluate_simple_expr(node, is_positive))
    }

    fn simple_place(
        &self,
        builder: &NarrowingConstraintsBuilder<'db, 'ast>,
        node: &ast::Expr,
    ) -> Result<Option<ScopedPlaceId>, Self::Error> {
        Ok(PlaceExpr::try_from_expr(node).map(|target| builder.expect_place(&target)))
    }

    fn negate(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(ty.negate(db, env))
    }

    fn singleton(
        &self,
        place: ScopedPlaceId,
        ty: Type<'db>,
    ) -> Result<NarrowingConstraints<'db>, Self::Error> {
        Ok(NarrowingConstraints::from_iter([(
            place,
            NarrowingConstraint::intersection(ty),
        )]))
    }

    fn alias(
        &self,
        builder: &NarrowingConstraintsBuilder<'db, 'ast>,
        index: &'db SemanticIndex<'db>,
        node: &'ast ast::Expr,
        name: &'ast ast::ExprName,
        expression: Expression<'db>,
        is_positive: bool,
    ) -> Result<Option<Expression<'db>>, Self::Error> {
        Ok(index
            .narrowing_alias_predicate(node)
            .filter(|alias| builder.is_valid_alias(name, expression, alias.expression, is_positive))
            .map(|alias| alias.expression))
    }

    fn attribute(
        &self,
        builder: &NarrowingConstraintsBuilder<'db, 'ast>,
        node: &'ast ast::ExprAttribute,
        expression: Expression<'db>,
        is_positive: bool,
    ) -> Result<Option<NarrowingConstraints<'db>>, Self::Error> {
        let inference = infer_expression_types(builder.db, expression, TypeContext::default());
        Ok(builder
            .narrow_nominal_attribute_by_truthiness(
                inference.expression_type(&*node.value),
                &node.value,
                node.attr.id(),
                is_positive,
            )
            .map(|(place, constraint)| NarrowingConstraints::from_iter([(place, constraint)])))
    }

    fn subscript(
        &self,
        builder: &NarrowingConstraintsBuilder<'db, 'ast>,
        node: &'ast ast::ExprSubscript,
        expression: Expression<'db>,
        is_positive: bool,
    ) -> Result<Option<NarrowingConstraints<'db>>, Self::Error> {
        let inference = infer_expression_types(builder.db, expression, TypeContext::default());
        Ok(builder
            .narrow_typeddict_subscript_by_truthiness(
                inference.expression_type(&*node.value),
                &node.value,
                inference.expression_type(&*node.slice),
                is_positive,
            )
            .map(|(place, constraint)| NarrowingConstraints::from_iter([(place, constraint)])))
    }

    fn compare(
        &self,
        builder: &mut NarrowingConstraintsBuilder<'db, 'ast>,
        node: &'ast ast::ExprCompare,
        expression: Expression<'db>,
        is_positive: bool,
    ) -> Result<Option<NarrowingConstraints<'db>>, Self::Error> {
        Ok(builder.evaluate_expr_compare(node, expression, is_positive))
    }

    fn call(
        &self,
        builder: &mut NarrowingConstraintsBuilder<'db, 'ast>,
        node: &'ast ast::ExprCall,
        expression: Expression<'db>,
        is_positive: bool,
    ) -> Result<CallResult<'db, 'ast>, Self::Error> {
        let db = builder.db;
        let inference = infer_expression_types(db, expression, TypeContext::default());
        if let Some(constraints) = builder.evaluate_type_guard_call(inference, node, is_positive) {
            return Ok(CallResult::Complete(Some(constraints)));
        }
        let callable_ty = inference.expression_type(&*node.func);
        match callable_ty {
            // for the expression `bool(E)`, we further narrow the type based on `E`
            Type::ClassLiteral(class_type)
                if node.arguments.args.len() == 1
                    && node.arguments.keywords.is_empty()
                    && class_type.is_known(db, KnownClass::Bool) =>
            {
                Ok(CallResult::Child(&node.arguments.args[0]))
            }
            _ => Ok(CallResult::Complete(
                builder.evaluate_expr_call_constraints(node, inference, callable_ty, is_positive),
            )),
        }
    }

    fn if_truthiness(
        &self,
        builder: &NarrowingConstraintsBuilder<'db, 'ast>,
        node: &'ast ast::ExprIf,
        expression: Expression<'db>,
    ) -> Result<Truthiness, Self::Error> {
        Ok(
            infer_expression_types(builder.db, expression, TypeContext::default())
                .expression_type(&node.test)
                .bool(builder.db, &builder.env),
        )
    }

    fn invalidate_named(
        &self,
        builder: &NarrowingConstraintsBuilder<'db, 'ast>,
        node: &'ast ast::ExprNamed,
        constraints: &mut Option<NarrowingConstraints<'db>>,
    ) -> Result<(), Self::Error> {
        builder.invalidate_named_constraints(node, constraints);
        Ok(())
    }

    fn merge_and(
        &self,
        left: Option<NarrowingConstraints<'db>>,
        right: Option<NarrowingConstraints<'db>>,
    ) -> Result<Option<NarrowingConstraints<'db>>, Self::Error> {
        Ok(NarrowingConstraintsBuilder::merge_optional_constraints_and(
            left, right,
        ))
    }

    fn merge_or(
        &self,
        left: Option<NarrowingConstraints<'db>>,
        right: Option<NarrowingConstraints<'db>>,
    ) -> Result<Option<NarrowingConstraints<'db>>, Self::Error> {
        Ok(NarrowingConstraintsBuilder::merge_optional_constraints_or(
            left, right,
        ))
    }

    fn boolean(
        &self,
        builder: &NarrowingConstraintsBuilder<'db, 'ast>,
        node: &'ast ast::ExprBoolOp,
        expression: Expression<'db>,
        is_positive: bool,
    ) -> Result<BooleanOperands<'db, 'ast>, Self::Error> {
        let inference = infer_expression_types(builder.db, expression, TypeContext::default());
        Ok(BooleanOperands {
            expression,
            node,
            is_positive,
            inference,
            env: builder.env.clone(),
            cursor: 0,
            constraints: Vec::new(),
        })
    }

    fn next_boolean_operand(
        &self,
        operands: &mut BooleanOperands<'db, 'ast>,
    ) -> Result<Option<&'ast ast::Expr>, Self::Error> {
        let node = operands.node.values.get(operands.cursor);
        if node.is_some() {
            operands.cursor += 1;
        }
        Ok(node)
    }

    fn boolean_truthiness(
        &self,
        builder: &NarrowingConstraintsBuilder<'db, 'ast>,
        operands: &BooleanOperands<'db, 'ast>,
        node: &'ast ast::Expr,
    ) -> Result<Truthiness, Self::Error> {
        Ok(operands
            .inference
            .expression_type(node)
            .bool(builder.db, &operands.env))
    }

    fn push_boolean_constraint(
        &self,
        operands: &mut BooleanOperands<'db, 'ast>,
        constraints: Option<NarrowingConstraints<'db>>,
    ) -> Result<(), Self::Error> {
        operands.constraints.push(constraints);
        Ok(())
    }

    fn begin_boolean_merge(
        &self,
        operands: BooleanOperands<'db, 'ast>,
    ) -> Result<BooleanMerge<'db>, Self::Error> {
        Ok(BooleanMerge {
            remaining: operands.constraints.into_iter(),
            result: None,
            env: operands.env,
        })
    }

    fn next_boolean_constraint(
        &self,
        merge: &mut BooleanMerge<'db>,
    ) -> Result<Option<Option<NarrowingConstraints<'db>>>, Self::Error> {
        Ok(merge.remaining.next())
    }

    fn take_boolean_result(
        &self,
        merge: &mut BooleanMerge<'db>,
    ) -> Result<Option<NarrowingConstraints<'db>>, Self::Error> {
        Ok(merge.result.take())
    }

    fn set_boolean_result(
        &self,
        merge: &mut BooleanMerge<'db>,
        constraints: Option<NarrowingConstraints<'db>>,
    ) -> Result<(), Self::Error> {
        merge.result = constraints;
        Ok(())
    }

    fn finish_boolean_merge(
        &self,
        merge: BooleanMerge<'db>,
    ) -> Result<Option<NarrowingConstraints<'db>>, Self::Error> {
        let result = merge.result;
        drop(merge.remaining);
        drop(merge.env);
        Ok(result)
    }

    fn discard_boolean_merge(&self, merge: BooleanMerge<'db>) -> Result<(), Self::Error> {
        drop(merge);
        Ok(())
    }
}
