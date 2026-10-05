use super::{Kind, Node, Script, fetch_chain};
use crate::DatabaseImpl;
use crate::function::execute::execution_run::tests::observation::{self, Event};

#[test]
fn cold_query_chains_share_one_native_task_poll() {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            for depth in [16, 512] {
                let db = DatabaseImpl::default();
                let mut root = Node::new(&db, None, 4);
                for _ in 1..depth {
                    root = Node::new(&db, Some(root), 0);
                }
                // No query is called while constructing this chain. Each body demands a
                // non-query child and then the next typed query on the same runtime stack.
                let script = Script::default();
                let (result, observed) = observation::collect(|| fetch_chain(&db, root, &script));
                assert_eq!(*result.scalar, (depth - 1) as u32);
                let bodies = script.bodies();
                assert_eq!(bodies.len(), depth);
                assert_eq!(bodies[0].0, Kind::Scalar);
                assert_eq!(observed.max_active_polls, 1);
                assert!(observed.polls > depth * 2);
                assert_eq!(
                    observed
                        .events
                        .iter()
                        .filter(|event| matches!(event, Event::Claim { .. }))
                        .count(),
                    depth
                );
            }
        })
        .expect("start a thread with a bounded native stack")
        .join()
        .expect("cold query chain completes");
}
