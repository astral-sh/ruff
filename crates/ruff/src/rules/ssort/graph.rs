use itertools::Itertools;
use petgraph::algo::tarjan_scc;
use petgraph::graph::EdgeReference;
use petgraph::visit::EdgeRef;
use petgraph::Graph;
use std::collections::HashSet;

pub(super) fn break_cycles<'a, N, E: 'a, F, K>(graph: &'a mut Graph<N, E>, edge_cost: F)
where
    F: Fn(&EdgeReference<E>) -> K,
    K: Ord,
{
    loop {
        let mut found_cycle = false;
        for strongly_connected_component in tarjan_scc(&*graph) {
            if strongly_connected_component.len() < 2 {
                continue;
            }

            found_cycle = true;

            let nodes: HashSet<_> = strongly_connected_component.iter().copied().collect();

            let edges_to_remove = strongly_connected_component
                .into_iter()
                .flat_map(|node| {
                    graph
                        .edges(node)
                        .filter(|edge| nodes.contains(&edge.target()))
                        .group_by(|edge| edge.target())
                        .into_iter()
                        .map(|(_, edges)| edges.collect::<Vec<_>>())
                        .collect::<Vec<_>>()
                })
                .max_by_key(|edges| edges.iter().map(&edge_cost).min().unwrap())
                .unwrap()
                .into_iter()
                .map(|edge| edge.id())
                .collect::<Vec<_>>();

            for edge in edges_to_remove {
                graph.remove_edge(edge);
            }
        }

        if !found_cycle {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_break_cycles_with_two_nodes() {
        let mut graph = Graph::<&str, i32>::new();
        let a = graph.add_node("a");
        let b = graph.add_node("b");
        graph.extend_with_edges(&[(a, b, 1), (b, a, 2)]);
        assert!(graph.contains_edge(a, b));
        assert!(graph.contains_edge(b, a));
        break_cycles(&mut graph, |edge| *edge.weight());
        assert!(graph.contains_edge(a, b));
        assert!(!graph.contains_edge(b, a));
    }

    #[test]
    fn test_break_cycles_with_three_nodes() {
        let mut graph = Graph::<&str, i32>::new();
        let a = graph.add_node("a");
        let b = graph.add_node("b");
        let c = graph.add_node("c");
        graph.extend_with_edges(&[
            (a, b, 1),
            (a, c, 2),
            (b, a, 4),
            (b, c, 5),
            (c, a, 6),
            (c, b, 7),
        ]);
        assert!(graph.contains_edge(a, b));
        assert!(graph.contains_edge(a, c));
        assert!(graph.contains_edge(b, a));
        assert!(graph.contains_edge(b, c));
        assert!(graph.contains_edge(c, a));
        assert!(graph.contains_edge(c, b));
        break_cycles(&mut graph, |edge| *edge.weight());
        assert!(graph.contains_edge(a, b));
        assert!(graph.contains_edge(a, c));
        assert!(!graph.contains_edge(b, a));
        assert!(graph.contains_edge(b, c));
        assert!(!graph.contains_edge(c, a));
        assert!(!graph.contains_edge(c, b));
    }
}
