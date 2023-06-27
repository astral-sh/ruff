use indexmap::{IndexMap, IndexSet};
use itertools::Itertools;
use std::cmp::Reverse;
use std::collections::hash_map::Keys;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash;

#[derive(Clone, Debug)]
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

    fn node_count(&self) -> usize {
        self.outgoing_edges.len()
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

    fn outgoing_neighbors(&self, node: &N) -> Option<impl Iterator<Item = &N>> {
        self.outgoing_edges.get(node).map(|edges| edges.keys())
    }

    fn incoming_neighbors(&self, node: &N) -> Option<impl Iterator<Item = &N>> {
        self.incoming_edges.get(node).map(|edges| edges.keys())
    }

    fn outgoing_neighbor_count(&self, node: &N) -> usize {
        self.outgoing_edges
            .get(node)
            .map(|edges| edges.len())
            .unwrap_or(0)
    }

    fn incoming_neighbor_count(&self, node: &N) -> usize {
        self.incoming_edges
            .get(node)
            .map(|edges| edges.len())
            .unwrap_or(0)
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

        self.outgoing_edges.entry(target).or_default();
        self.incoming_edges.entry(source).or_default();
    }
}

pub(super) fn topological_sort<N, E>(graph: &Graph<N, E>) -> Vec<N>
where
    N: Copy + Eq + Hash + Ord + PartialEq + PartialOrd,
    E: Copy + Ord + PartialOrd,
{
    let mut graph = graph.clone();
    break_cycles(&mut graph);

    let mut pending: BinaryHeap<Reverse<N>> = graph
        .nodes()
        .filter(|node| graph.incoming_neighbors(node).unwrap().next().is_none())
        .map(|node| Reverse(*node))
        .collect();

    let mut result: Vec<N> = Vec::new();
    loop {
        let Some(Reverse(node)) = pending.pop() else {break};
        for neighbor in graph.outgoing_neighbors(&node).unwrap() {
            if graph.incoming_neighbor_count(neighbor) == 1 {
                pending.push(Reverse(*neighbor));
            }
        }
        graph.remove_node(&node);
        result.push(node);
    }

    assert!(graph.node_count() == 0);
    result
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

    for neighbor in graph.outgoing_neighbors(&node).unwrap() {
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
    fn graph_insert_node() {
        let mut graph = Graph::<&str, i32>::new();
        graph.insert_node("a");
        assert!(graph.contains_node(&"a"));
    }

    #[test]
    fn graph_insert_edge() {
        let mut graph = Graph::<&str, i32>::new();
        graph.insert_edge("a", "b", 1);
        assert_eq!(graph.edge(&"a", &"b"), Some(&1));
    }

    #[test]
    fn graph_remove_node() {
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
    fn graph_remove_edge() {
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
    fn find_cycle_with_empty_graph() {
        let mut graph = Graph::<&str, i32>::new();
        assert_eq!(find_cycle(&graph), None);
    }

    #[test]
    fn find_cycle_with_no_cycles() {
        let mut graph = Graph::<&str, i32>::new();
        graph.insert_edge("a", "b", 1);
        graph.insert_edge("b", "c", 2);
        graph.insert_edge("c", "d", 3);
        assert_eq!(find_cycle(&graph), None);
    }

    #[test]
    fn find_cycle_with_one_cycle() {
        let mut graph = Graph::<&str, i32>::new();
        graph.insert_edge("a", "b", 1);
        graph.insert_edge("b", "c", 2);
        graph.insert_edge("c", "a", 3);
        assert_eq!(
            find_cycle(&graph)
                .unwrap()
                .into_iter()
                .collect::<HashSet<_>>(),
            HashSet::from_iter(["a", "b", "c"])
        );
    }

    #[test]
    fn find_cycle_with_self_cycle() {
        let mut graph = Graph::<&str, i32>::new();
        graph.insert_edge("a", "a", 1);
        assert_eq!(
            find_cycle(&graph)
                .unwrap()
                .into_iter()
                .collect::<HashSet<_>>(),
            HashSet::from_iter(["a"])
        );
    }

    #[test]
    fn break_cycles_with_empty_graph() {
        let mut graph = Graph::<&str, i32>::new();
        break_cycles(&mut graph);
        assert_eq!(graph.node_count(), 0);
    }

    #[test]
    fn break_cycles_with_no_cycles() {
        let mut graph = Graph::<&str, i32>::new();
        graph.insert_edge("a", "b", 1);
        graph.insert_edge("b", "c", 2);
        graph.insert_edge("c", "d", 3);
        break_cycles(&mut graph);
        assert_eq!(graph.edge(&"a", &"b"), Some(&1));
        assert_eq!(graph.edge(&"b", &"c"), Some(&2));
        assert_eq!(graph.edge(&"c", &"d"), Some(&3));
    }

    #[test]
    fn break_cycles_with_one_cycle() {
        let mut graph = Graph::<&str, i32>::new();
        graph.insert_edge("a", "b", 1);
        graph.insert_edge("b", "c", 2);
        graph.insert_edge("c", "a", 3);
        break_cycles(&mut graph);
        assert_eq!(graph.edge(&"a", &"b"), Some(&1));
        assert_eq!(graph.edge(&"b", &"c"), Some(&2));
        assert_eq!(graph.edge(&"c", &"a"), None);
    }

    #[test]
    fn break_cycles_with_self_cycle() {
        let mut graph = Graph::<&str, i32>::new();
        graph.insert_edge("a", "a", 1);
        break_cycles(&mut graph);
        assert_eq!(graph.edge(&"a", &"a"), None);
    }

    #[test]
    fn topological_sort_with_empty_graph() {
        let mut graph = Graph::<&str, i32>::new();
        assert_eq!(topological_sort(&graph), [] as [&str; 0]);
    }

    #[test]
    fn topological_sort_with_no_cycles() {
        let mut graph = Graph::<&str, i32>::new();
        graph.insert_edge("a", "b", 1);
        graph.insert_edge("b", "c", 2);
        graph.insert_edge("c", "d", 3);
        assert_eq!(topological_sort(&graph), ["a", "b", "c", "d"]);
    }

    #[test]
    fn topological_sort_with_one_cycle() {
        let mut graph = Graph::<&str, i32>::new();
        graph.insert_edge("a", "b", 3);
        graph.insert_edge("b", "c", 2);
        graph.insert_edge("c", "a", 1);
        assert_eq!(topological_sort(&graph), ["b", "c", "a"]);
    }

    #[test]
    fn topological_sort_with_self_cycle() {
        let mut graph = Graph::<&str, i32>::new();
        graph.insert_edge("a", "a", 1);
        assert_eq!(topological_sort(&graph), ["a"]);
    }
}
