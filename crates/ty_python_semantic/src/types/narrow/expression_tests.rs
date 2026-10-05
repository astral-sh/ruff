use anyhow::Context;
use ruff_db::files::system_path_to_file;
use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;
use ruff_text_size::Ranged;
use ty_python_core::predicate::PredicateNode;
use ty_python_core::{ExpressionNodeKey, ProgramFile, place_table, semantic_index};

use super::{NarrowingConstraint, NarrowingConstraints, NarrowingConstraintsBuilder};
use crate::db::tests::TestDbBuilder;
use crate::types::Type;

struct PredicateChain(Option<ast::Expr>);

impl Drop for PredicateChain {
    fn drop(&mut self) {
        // Recursive AST destruction would obscure the predicate traversal's stack usage,
        // including when an assertion unwinds through this owner.
        // Assignment targets in this fixture are names, so their destruction is shallow.
        while let Some(node) = self.0.take() {
            self.0 = match node {
                ast::Expr::UnaryOp(unary) => Some(*unary.operand),
                ast::Expr::Named(named) => Some(*named.value),
                _ => None,
            };
        }
    }
}

#[test]
fn deep_named_predicate_preserves_constraints() -> anyhow::Result<()> {
    const DEPTH: usize = 10_001;

    let db = TestDbBuilder::new()
        .with_file("/src/predicate.py", "if (target := value):\n    pass\n")
        .build()?;
    let env = db.program_environment();
    let file = system_path_to_file(&db, "/src/predicate.py")?;
    let file = ProgramFile::new(&db, file, env.program(&db));
    let module = parsed_module(&db, file.python_file(&db)).load(&db);
    let condition = module
        .suite()
        .first()
        .and_then(ast::Stmt::as_if_stmt)
        .map(|statement| statement.test.as_ref())
        .context("expected an if condition")?;
    let index = semantic_index(&db, file);
    let expression = index
        .try_expression(condition)
        .context("expected an indexed condition")?;
    let named = condition
        .as_named_expr()
        .context("expected an assignment expression")?;
    assert!(named.target.is_name_expr());
    assert!(named.value.is_name_expr());
    let places = place_table(&db, expression.scope(&db));
    let target_place = places
        .symbol_id("target")
        .context("expected the assignment's target name")?
        .into();
    let value_place = places
        .symbol_id("value")
        .context("expected the assignment's value name")?
        .into();

    // Build around indexed names to isolate predicate traversal from parser and indexer
    // depth limits. Cloning preserves their node keys, so the names still use their real
    // scope and semantic-index entries. The synthetic wrappers need no index lookups.
    let leaf_clone = (*named.value).clone();
    let target_clone = named.target.clone();
    assert_eq!(
        ExpressionNodeKey::from(&leaf_clone),
        ExpressionNodeKey::from(&named.value),
    );
    assert_eq!(
        ExpressionNodeKey::from(&target_clone),
        ExpressionNodeKey::from(&named.target),
    );
    assert!(index.narrowing_alias_predicate(&named.value).is_none());

    // Each nested assignment retains target constraints while evaluating its value,
    // growing the continuation stack beyond its inline capacity.
    let mut chain = PredicateChain(Some(leaf_clone));
    for _ in 0..DEPTH {
        chain.0 = chain.0.take().map(|operand| {
            ast::Expr::Named(ast::ExprNamed {
                node_index: ast::AtomicNodeIndex::default(),
                range: condition.range(),
                target: target_clone.clone(),
                value: Box::new(ast::Expr::UnaryOp(ast::ExprUnaryOp {
                    node_index: ast::AtomicNodeIndex::default(),
                    range: condition.range(),
                    op: ast::UnaryOp::Not,
                    operand: Box::new(operand),
                })),
            })
        });
    }
    let root = chain
        .0
        .as_ref()
        .context("expected a nested assignment expression")?;
    let mut builder = NarrowingConstraintsBuilder::new(
        &db,
        &env,
        &module,
        PredicateNode::Expression(expression),
        true,
    );
    for is_positive in [true, false] {
        // Odd depth reverses the value's polarity. Rebinding the target removes the
        // inner assignment's opposite constraint before adding the outer constraint.
        let (target_excluded, value_excluded) = if is_positive {
            (Type::AlwaysFalsy, Type::AlwaysTruthy)
        } else {
            (Type::AlwaysTruthy, Type::AlwaysFalsy)
        };
        let expected = NarrowingConstraints::from_iter([
            (
                target_place,
                NarrowingConstraint::intersection(target_excluded.negate(&db, &env)),
            ),
            (
                value_place,
                NarrowingConstraint::intersection(value_excluded.negate(&db, &env)),
            ),
        ]);
        assert_eq!(
            builder.evaluate_expression_node_predicate(root, expression, is_positive),
            Some(expected),
        );
    }
    Ok(())
}
