use indexmap::{IndexMap, IndexSet};
use std::collections::hash_map::Keys;
use std::collections::{HashMap, HashSet, VecDeque};
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
    N: Clone + Eq + Hash + PartialEq,
    E: Clone,
{
    fn insert_node(&mut self, node: N) {
        self.outgoing_edges.entry(node.clone()).or_default();
        self.incoming_edges.entry(node).or_default();
    }

    fn insert_edge(&mut self, source: N, target: N, edge: E) {
        self.outgoing_edges
            .entry(source.clone())
            .or_default()
            .insert(target.clone(), edge.clone());
        self.incoming_edges
            .entry(target.clone())
            .or_default()
            .insert(source, edge);

        // outgoing_edges is used to track all nodes, so make sure the target exists there.
        self.outgoing_edges.entry(target).or_default();
    }
}

fn find_cycle<N, E>(graph: &Graph<N, E>) -> Option<Vec<&N>>
where
    N: Eq + Hash + PartialEq,
{
    let mut visited = HashSet::new();
    find_cycle_with_visited(graph, &mut visited)
}

fn find_cycle_with_visited<'a, N, E>(
    graph: &'a Graph<N, E>,
    visited: &mut HashSet<&'a N>,
) -> Option<Vec<&'a N>>
where
    N: Eq + Hash + PartialEq,
{
    for node in graph.nodes() {
        if visited.contains(node) {
            continue;
        }

        let mut path: IndexSet<&'a N> = IndexSet::new();

        if let Some(path) = find_cycle_with_path(graph, visited, &mut path, node) {
            return Some(path);
        }
    }

    None
}

fn find_cycle_with_path<'a, N, E>(
    graph: &'a Graph<N, E>,
    visited: &mut HashSet<&'a N>,
    path: &mut IndexSet<&'a N>,
    node: &'a N,
) -> Option<Vec<&'a N>>
where
    N: Eq + Hash + PartialEq,
{
    visited.insert(node);
    path.insert(node);

    for neighbor in graph.neighbors(node).unwrap() {
        if !visited.contains(neighbor) {
            if let Some(cycle) = find_cycle_with_path(graph, visited, path, neighbor) {
                visited.remove(node);
                return Some(cycle);
            }
        } else if let Some(index) = path.get_index_of(neighbor) {
            visited.remove(node);
            return Some(
                path.get_range(index..)
                    .unwrap()
                    .into_iter()
                    .copied()
                    .collect(),
            );
        }
    }

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
            HashSet::from_iter(&["a", "b", "c", "d", "e"])
        );
    }
}
