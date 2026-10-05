#![cfg(feature = "inventory")]

mod common;

use crate::common::{EventLoggerDatabase, LogDatabase};
use expect_test::expect;
use salsa::{Database, Setter};

#[derive(Clone, Debug, Eq, PartialEq, Hash, salsa::SalsaValue)]
struct Graph<'db> {
    nodes: Vec<Node<'db>>,
}

impl<'db> Graph<'db> {
    fn find_node(&self, db: &dyn salsa::Database, name: &str) -> Option<Node<'db>> {
        self.nodes
            .iter()
            .find(|node| node.name(db) == name)
            .copied()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, salsa::SalsaValue)]
struct Edge {
    // Index into `graph.nodes`
    to: usize,
    cost: usize,
}

#[salsa::tracked(debug)]
struct Node<'db> {
    #[returns(ref)]
    name: String,

    #[returns(deref)]
    #[tracked]
    edges: Vec<Edge>,

    #[returns(copy)]
    graph: GraphInput,
}

#[salsa::input(debug)]
struct GraphInput {
    #[returns(copy)]
    simple: bool,
    #[returns(copy)]
    fixpoint_variant: usize,
}

#[salsa::tracked(returns(ref))]
fn create_graph(db: &dyn salsa::Database, input: GraphInput) -> Graph<'_> {
    if input.simple(db) {
        let a = Node::new(db, "a".to_string(), vec![], input);
        let b = Node::new(db, "b".to_string(), vec![Edge { to: 0, cost: 20 }], input);
        let c = Node::new(db, "c".to_string(), vec![Edge { to: 1, cost: 2 }], input);

        Graph {
            nodes: vec![a, b, c],
        }
    } else {
        // ```
        // flowchart TD
        //
        // A("a")
        // B("b")
        // C("c")
        // D{"d"}
        //
        // B -- 20 --> D
        // C -- 4 --> D
        // D -- 4 --> A
        // D -- 4 --> B
        // ```
        let a = Node::new(db, "a".to_string(), vec![], input);
        let b = Node::new(db, "b".to_string(), vec![Edge { to: 3, cost: 20 }], input);
        let c = Node::new(db, "c".to_string(), vec![Edge { to: 3, cost: 4 }], input);
        let d = Node::new(
            db,
            "d".to_string(),
            vec![Edge { to: 0, cost: 4 }, Edge { to: 1, cost: 4 }],
            input,
        );

        Graph {
            nodes: vec![a, b, c, d],
        }
    }
}

/// Computes the minimum cost from the node with offset `0` to the given node.
#[salsa::tracked(returns(copy), cycle_initial=max_initial)]
fn cost_to_start<'db>(db: &'db dyn Database, node: Node<'db>) -> usize {
    let mut min_cost = usize::MAX;
    let graph = create_graph(db, node.graph(db));

    for edge in node.edges(db) {
        if edge.to == 0 {
            min_cost = min_cost.min(edge.cost);
        }

        let edge_cost_to_start = cost_to_start(db, graph.nodes[edge.to]);

        // We hit a cycle, never take this edge because it will always be more expensive than
        // any other edge
        if edge_cost_to_start == usize::MAX {
            continue;
        }

        min_cost = min_cost.min(edge.cost + edge_cost_to_start);
    }

    min_cost
}

fn max_initial(_db: &dyn Database, _id: salsa::Id, _node: Node) -> usize {
    usize::MAX
}

/// Tests for cycles where the cycle head is stored on a tracked struct
/// and that tracked struct is freed in a later revision.
#[test]
fn main() {
    let mut db = EventLoggerDatabase::default();

    let input = GraphInput::new(&db, false, 0);
    let graph = create_graph(&db, input);
    let c = graph.find_node(&db, "c").unwrap();

    // Query the cost from `c` to `a`.
    // There's a cycle between `b` and `d`, where `d` becomes the cycle head and `b` is a provisional, non finalized result.
    assert_eq!(cost_to_start(&db, c), 8);

    // Change the graph, this will remove `d`, leaving `b` pointing to a cycle head that's now collected.
    // Querying the cost from `c` to `a` should try to verify the result of `b` and it is important
    // that `b` doesn't try to dereference the cycle head (because its memo is now stored on a tracked
    // struct that has been freed).
    input.set_simple(&mut db).to(true);

    let graph = create_graph(&db, input);
    let c = graph.find_node(&db, "c").unwrap();

    assert_eq!(cost_to_start(&db, c), 22);

    db.assert_logs(expect![[r#"
        [
            "WillCheckCancellation",
            "WillExecute { database_key: create_graph(Id(0)) }",
            "WillCheckCancellation",
            "WillExecute { database_key: cost_to_start(Id(82)) }",
            "WillCheckCancellation",
            "WillCheckCancellation",
            "WillExecute { database_key: cost_to_start(Id(83)) }",
            "WillCheckCancellation",
            "WillCheckCancellation",
            "WillExecute { database_key: cost_to_start(Id(80)) }",
            "WillCheckCancellation",
            "WillCheckCancellation",
            "WillExecute { database_key: cost_to_start(Id(81)) }",
            "WillCheckCancellation",
            "WillCheckCancellation",
            "WillIterateCycle { database_key: cost_to_start(Id(83)), iteration: 1 }",
            "WillCheckCancellation",
            "WillCheckCancellation",
            "WillCheckCancellation",
            "WillExecute { database_key: cost_to_start(Id(81)) }",
            "WillCheckCancellation",
            "WillCheckCancellation",
            "DidFinalizeCycle { database_key: cost_to_start(Id(83)), iteration: 1 }",
            "DidSetCancellationFlag",
            "WillCheckCancellation",
            "WillExecute { database_key: create_graph(Id(0)) }",
            "WillDiscardStaleOutput { execute_key: create_graph(Id(0)), output_key: Node(Id(83)) }",
            "DidDiscard { key: Node(Id(83)) }",
            "DidDiscard { key: cost_to_start(Id(83)) }",
            "WillCheckCancellation",
            "WillCheckCancellation",
            "WillExecute { database_key: cost_to_start(Id(82)) }",
            "WillCheckCancellation",
            "WillCheckCancellation",
            "WillExecute { database_key: cost_to_start(Id(81)) }",
            "WillCheckCancellation",
            "WillCheckCancellation",
            "WillCheckCancellation",
            "WillExecute { database_key: cost_to_start(Id(80)) }",
            "WillCheckCancellation",
        ]"#]]);
}

#[salsa::tracked]
struct IterationNode<'db> {
    #[returns(ref)]
    name: String,
    #[returns(copy)]
    iteration: usize,
}

/// A cyclic query that creates more tracked structs in later fixpoint iterations.
///
/// The output depends on the input's fixpoint_variant:
/// - variant=0: Returns `[base]` (1 struct, no cycle)
/// - variant=1: Through fixpoint iteration, returns `[iter_0, iter_1, iter_2]` (3 structs)
/// - variant=2: Through fixpoint iteration, returns `[iter_0, iter_1]` (2 structs)
/// - variant>2: Through fixpoint iteration, returns `[iter_0, iter_1]` (2 structs, same as variant=2)
///
/// When variant > 0, the query creates a cycle by calling itself. The fixpoint iteration
/// proceeds as follows:
/// 1. Initial: returns empty vector
/// 2. First iteration: returns `[iter_0]`
/// 3. Second iteration: returns `[iter_0, iter_1]`
/// 4. Third iteration (only for variant=1): returns `[iter_0, iter_1, iter_2]`
/// 5. Further iterations: no change, fixpoint reached
#[salsa::tracked(returns(clone), cycle_initial=initial_with_structs)]
fn create_tracked_in_cycle(db: &dyn Database, input: GraphInput) -> Vec<IterationNode<'_>> {
    // Check if we should create more nodes based on the input.
    let variant = input.fixpoint_variant(db);

    if variant == 0 {
        // Base case - no cycle, just return a single node.
        vec![IterationNode::new(db, "base".to_string(), 0)]
    } else {
        // Create a cycle by calling ourselves.
        let previous = create_tracked_in_cycle(db, input);

        // In later iterations, create additional tracked structs.
        if previous.is_empty() {
            // First iteration - initial returns empty.
            vec![IterationNode::new(db, "iter_0".to_string(), 0)]
        } else {
            // Limit based on variant: variant=1 allows 3 nodes, variant=2 allows 2 nodes.
            let limit = if variant == 1 { 3 } else { 2 };

            if previous.len() < limit {
                // Subsequent iterations - add more nodes.
                let mut nodes = previous;
                nodes.push(IterationNode::new(
                    db,
                    format!("iter_{}", nodes.len()),
                    nodes.len(),
                ));
                nodes
            } else {
                // Reached the limit.
                previous
            }
        }
    }
}

fn initial_with_structs(
    _db: &dyn Database,
    _id: salsa::Id,
    _input: GraphInput,
) -> Vec<IterationNode<'_>> {
    vec![]
}

#[test_log::test]
fn test_cycle_with_fixpoint_structs() {
    let mut db = EventLoggerDatabase::default();

    // Create an input that will trigger the cyclic behavior.
    let input = GraphInput::new(&db, false, 1);

    // Initial query - this will create structs across multiple iterations.
    let nodes = create_tracked_in_cycle(&db, input);
    assert_eq!(nodes.len(), 3);
    // First iteration: previous is empty [], so we get [iter_0]
    // Second iteration: previous is [iter_0], so we get [iter_0, iter_1]
    // Third iteration: previous is [iter_0, iter_1], so we get [iter_0, iter_1, iter_2]
    assert_eq!(nodes[0].name(&db), "iter_0");
    assert_eq!(nodes[1].name(&db), "iter_1");
    assert_eq!(nodes[2].name(&db), "iter_2");

    // Clear logs to focus on the change.
    db.clear_logs();

    // Change the input to force re-execution with a different variant.
    // This will create 2 tracked structs instead of 3 (one fewer than before).
    input.set_fixpoint_variant(&mut db).to(2);

    // Re-query - this should handle the tracked struct changes properly.
    let nodes = create_tracked_in_cycle(&db, input);
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0].name(&db), "iter_0");
    assert_eq!(nodes[1].name(&db), "iter_1");

    // Check the logs to ensure proper execution and struct management.
    // We should see the third struct (iter_2) being discarded.
    db.assert_logs(expect![[r#"
        [
            "DidSetCancellationFlag",
            "WillCheckCancellation",
            "WillExecute { database_key: create_tracked_in_cycle(Id(0)) }",
            "WillCheckCancellation",
            "WillIterateCycle { database_key: create_tracked_in_cycle(Id(0)), iteration: 1 }",
            "WillCheckCancellation",
            "WillIterateCycle { database_key: create_tracked_in_cycle(Id(0)), iteration: 2 }",
            "WillCheckCancellation",
            "WillDiscardStaleOutput { execute_key: create_tracked_in_cycle(Id(0)), output_key: IterationNode(Id(82)) }",
            "DidDiscard { key: IterationNode(Id(82)) }",
            "DidFinalizeCycle { database_key: create_tracked_in_cycle(Id(0)), iteration: 2 }",
        ]"#]]);
}

#[salsa::tracked(debug)]
struct NameWithOffset<'db> {
    name: String,

    #[tracked]
    #[returns(copy)]
    offset: u32,
}

#[test]
fn cycle_tracked_struct_with_tracked_field() {
    #[salsa::tracked(returns(copy), cycle_initial=|_,_| 0)]
    fn query_a(db: &dyn salsa::Database) -> u32 {
        let offset = query_b(db);

        let tracked = NameWithOffset::new(db, "test".to_string(), offset);

        tracked.offset(db)
    }

    #[salsa::tracked(returns(copy))]
    fn query_b(db: &dyn salsa::Database) -> u32 {
        let base_offset = query_a(db);

        (base_offset + 1).min(5)
    }

    let db = salsa::DatabaseImpl::default();

    let result = query_a(&db);

    assert_eq!(result, 5);
}
