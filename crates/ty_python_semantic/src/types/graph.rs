/// Indexed dependencies shared by constraint solving and recursive shape analysis.
/// Each node is an index into `dependencies`; reverse edges retain their insertion order.
pub(super) struct DependencyGraph {
    dependencies: Vec<Vec<usize>>,
    dependents: Vec<Vec<usize>>,
}

impl DependencyGraph {
    pub(super) fn new(dependencies: Vec<Vec<usize>>) -> Self {
        let mut dependents = vec![Vec::new(); dependencies.len()];
        for (index, edges) in dependencies.iter().enumerate() {
            for &dependency in edges {
                dependents[dependency].push(index);
            }
        }
        Self {
            dependencies,
            dependents,
        }
    }

    /// Nodes that directly depend on `node`, in insertion order.
    pub(super) fn dependents(&self, node: usize) -> &[usize] {
        &self.dependents[node]
    }

    /// Whether a nonempty component contains a cycle, including a self-reference.
    pub(super) fn is_cyclic(&self, component: &[usize]) -> bool {
        component.len() > 1 || self.dependencies[component[0]].contains(&component[0])
    }

    /// Find strongly connected components of the subgraph induced by `nodes`, dependencies first.
    /// Edges to excluded nodes are ignored. Both depth-first walks are iterative and linear;
    /// callers choose the order of members within each component.
    pub(super) fn components(&self, nodes: impl IntoIterator<Item = usize>) -> Vec<Vec<usize>> {
        // Excluded nodes start visited in both walks.
        let mut visited = vec![true; self.dependencies.len()];
        for node in nodes {
            visited[node] = false;
        }
        let mut assigned = visited.clone();
        let mut order = Vec::new();
        for start in 0..visited.len() {
            if std::mem::replace(&mut visited[start], true) {
                continue;
            }
            let mut pending = vec![(start, 0)];
            while let Some((current, next)) = pending.pop() {
                if let Some(&dependent) = self.dependents[current].get(next) {
                    pending.push((current, next + 1));
                    if !std::mem::replace(&mut visited[dependent], true) {
                        pending.push((dependent, 0));
                    }
                } else {
                    order.push(current);
                }
            }
        }
        let mut components = Vec::new();
        for start in order.into_iter().rev() {
            if std::mem::replace(&mut assigned[start], true) {
                continue;
            }
            let mut component = Vec::new();
            let mut pending = vec![start];
            while let Some(current) = pending.pop() {
                component.push(current);
                for &dependency in &self.dependencies[current] {
                    if !std::mem::replace(&mut assigned[dependency], true) {
                        pending.push(dependency);
                    }
                }
            }
            components.push(component);
        }
        components
    }
}
