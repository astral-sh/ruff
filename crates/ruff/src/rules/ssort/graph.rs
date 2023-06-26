use indexmap::{IndexMap, IndexSet};
use std::collections::hash_map::Keys;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;

#[derive(Debug)]
pub(super) struct Graph<N, E> {
    outgoing_edges: HashMap<N, HashMap<N, E>>,
    incoming_edges: HashMap<N, HashMap<N, E>>,
}

impl<N, E> Graph<N, E>
where
    N: Clone + Eq + Hash + PartialEq,
    E: Clone,
{
    fn new() -> Self {
        Graph {
            outgoing_edges: HashMap::new(),
            incoming_edges: HashMap::new(),
        }
    }

    fn insert_node(&mut self, node: N) {
        self.outgoing_edges.entry(node.clone()).or_default();
        self.incoming_edges.entry(node).or_default();
    }

    fn remove_node(&mut self, node: &N) {
        let outgoing_edges = self.outgoing_edges.remove(node).unwrap_or_default();
        let incoming_edges = self.incoming_edges.remove(node).unwrap_or_default();

        for outgoing_node in outgoing_edges.keys() {
            self.incoming_edges
                .get_mut(outgoing_node)
                .map(|edges| edges.remove(node));
        }

        for incoming_node in incoming_edges.keys() {
            self.outgoing_edges
                .get_mut(incoming_node)
                .map(|edges| edges.remove(node));
        }
    }

    fn contains_node(&self, node: &N) -> bool {
        self.outgoing_edges.contains_key(node) || self.incoming_edges.contains_key(node)
    }

    fn insert_edge(&mut self, source: N, target: N, edge: E) {
        self.outgoing_edges
            .entry(source.clone())
            .or_default()
            .insert(target.clone(), edge.clone());
        self.incoming_edges
            .entry(target)
            .or_default()
            .insert(source, edge);
    }

    fn remove_edge(&mut self, source: &N, target: &N) {
        self.outgoing_edges
            .get_mut(source)
            .map(|edges| edges.remove(target));
        self.incoming_edges
            .get_mut(target)
            .map(|edges| edges.remove(source));
    }

    fn edge(&self, source: &N, target: &N) -> Option<&E> {
        self.outgoing_edges
            .get(source)
            .map(|edges| edges.get(target))
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_graph_insert_node() {
        let mut graph = Graph::<&str, i32>::new();
        graph.insert_node("a");
        assert!(graph.contains_node(&"a"));
    }

    #[test]
    fn test_graph_insert_edge() {
        let mut graph = Graph::<&str, i32>::new();
        graph.insert_edge("a", "b", 1);
        assert_eq!(graph.edge(&"a", &"b"), Some(&1));
    }

    #[test]
    fn test_graph_remove_node() {
        let mut graph = Graph::<&str, i32>::new();
        graph.insert_edge("a", "b", 1);
        graph.insert_edge("a", "c", 2);
        graph.insert_edge("b", "c", 3);
        graph.remove_node(&"b");
        assert!(graph.contains_node(&"a"));
        assert!(!graph.contains_node(&"b"));
        assert!(graph.contains_node(&"c"));
        assert_eq!(graph.edge(&"a", &"b"), None);
        assert_eq!(graph.edge(&"a", &"c"), Some(&2));
        assert_eq!(graph.edge(&"b", &"c"), None);
    }

    #[test]
    fn test_graph_remove_edge() {
        let mut graph = Graph::<&str, i32>::new();
        graph.insert_edge("a", "b", 1);
        graph.insert_edge("a", "c", 2);
        graph.insert_edge("b", "c", 3);
        graph.remove_edge(&"b", &"c");
        assert!(graph.contains_node(&"a"));
        assert!(graph.contains_node(&"b"));
        assert!(graph.contains_node(&"c"));
        assert_eq!(graph.edge(&"a", &"b"), Some(&1));
        assert_eq!(graph.edge(&"a", &"c"), Some(&2));
        assert_eq!(graph.edge(&"b", &"c"), None);
    }
}
