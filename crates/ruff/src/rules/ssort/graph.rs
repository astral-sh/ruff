use indexmap::{IndexMap, IndexSet};
use itertools::Itertools;
use std::collections::hash_map::Keys;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;

#[derive(Debug)]
pub(super) struct Graph<N, E> {
    outgoing_edges: HashMap<N, HashMap<N, E>>,
    incoming_edges: HashMap<N, HashMap<N, E>>,
}

impl<N, E> Graph<N, E> {
    fn new() -> Self {
        Graph {
            outgoing_edges: HashMap::new(),
            incoming_edges: HashMap::new(),
        }
    }
}

impl<N, E> Graph<N, E>
where
    N: Eq + Hash + PartialEq,
{
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
        self.outgoing_edges.contains_key(node)
    }

    fn nodes(&self) -> impl Iterator<Item = &N> {
        self.outgoing_edges.keys()
    }

    fn neighbors(&self, node: &N) -> Option<impl Iterator<Item = &N>> {
        self.outgoing_edges.get(node).map(|edges| edges.keys())
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

impl<N, E> Graph<N, E>
where
    N: Copy + Eq + Hash + PartialEq,
    E: Copy,
{
    fn insert_node(&mut self, node: N) {
        self.outgoing_edges.entry(node.clone()).or_default();
        self.incoming_edges.entry(node).or_default();
    }

    fn insert_edge(&mut self, source: N, target: N, edge: E) {
        self.outgoing_edges
            .entry(source)
            .or_default()
            .insert(target, edge);
        self.incoming_edges
            .entry(target)
            .or_default()
            .insert(source, edge);

        // outgoing_edges is used to track all nodes, so make sure the target exists there.
        self.outgoing_edges.entry(target).or_default();
    }
}

fn break_cycles<N, E>(graph: &mut Graph<N, E>)
where
    N: Copy + Eq + Hash + PartialEq,
    E: Ord + PartialOrd,
{
    let mut subgraph: HashSet<_> = graph.nodes().copied().collect();
    loop {
        let Some(cycle) = find_cycle_in_subgraph(graph, &mut subgraph) else { return };

        let cycle_len = cycle.len();
        let (source, target) = cycle
            .into_iter()
            .cycle()
            .tuple_windows()
            .take(cycle_len + 1)
            .max_by_key(|(source, target)| graph.edge(source, target))
            .unwrap();

        graph.remove_edge(&source, &target);
    }
}

fn find_cycle<N, E>(graph: &Graph<N, E>) -> Option<Vec<N>>
where
    N: Copy + Eq + Hash + PartialEq,
{
    let mut subgraph: HashSet<_> = graph.nodes().copied().collect();
    find_cycle_in_subgraph(graph, &mut subgraph)
}

fn find_cycle_in_subgraph<N, E>(graph: &Graph<N, E>, subgraph: &mut HashSet<N>) -> Option<Vec<N>>
where
    N: Copy + Eq + Hash + PartialEq,
{
    loop {
        let Some(node) = subgraph.iter().next() else { return None };

        let mut path = IndexSet::new();
        if let Some(path) = find_cycle_in_subgraph_with_path(graph, subgraph, &mut path, *node) {
            return Some(path);
        }
    }
}

fn find_cycle_in_subgraph_with_path<N, E>(
    graph: &Graph<N, E>,
    subgraph: &mut HashSet<N>,
    path: &mut IndexSet<N>,
    node: N,
) -> Option<Vec<N>>
where
    N: Copy + Eq + Hash + PartialEq,
{
    path.insert(node);

    for neighbor in graph.neighbors(&node).unwrap() {
        if let Some(index) = path.get_index_of(neighbor) {
            return Some(
                path.get_range(index..)
                    .unwrap()
                    .into_iter()
                    .copied()
                    .collect(),
            );
        } else if subgraph.contains(neighbor) {
            if let Some(cycle) = find_cycle_in_subgraph_with_path(graph, subgraph, path, *neighbor)
            {
                return Some(cycle);
            }
        }
    }

    subgraph.remove(&node);
    path.pop();
    None
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

    #[test]
    fn test_find_cycle() {
        let mut graph = Graph::<&str, i32>::new();
        graph.insert_edge("a", "b", 1);
        graph.insert_edge("b", "c", 1);
        graph.insert_edge("c", "d", 1);
        graph.insert_edge("d", "e", 1);
        graph.insert_edge("e", "a", 1);
        assert_eq!(
            find_cycle(&graph)
                .unwrap()
                .into_iter()
                .collect::<HashSet<_>>(),
            HashSet::from_iter(["a", "b", "c", "d", "e"])
        );
    }

    #[test]
    fn test_break_cycles() {
        let mut graph = Graph::<&str, i32>::new();
        graph.insert_edge("a", "b", 1);
        graph.insert_edge("b", "c", 2);
        graph.insert_edge("c", "d", 3);
        graph.insert_edge("d", "e", 4);
        graph.insert_edge("e", "a", 5);
        break_cycles(&mut graph);
        assert_eq!(graph.edge(&"a", &"b"), Some(&1));
        assert_eq!(graph.edge(&"b", &"c"), Some(&2));
        assert_eq!(graph.edge(&"c", &"d"), Some(&3));
        assert_eq!(graph.edge(&"d", &"e"), Some(&4));
        assert_eq!(graph.edge(&"e", &"a"), None);
    }
}
