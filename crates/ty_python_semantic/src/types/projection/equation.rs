use smallvec::SmallVec;

use crate::types::{DivergentType, KnownClass, Type, UnionType};
use crate::{Db, FxIndexMap, FxIndexSet};

use super::artifact::ProjectionPath;
use super::container::ProjectionContainer;
use super::evidence::ProjectionEvidenceSet;
use super::recovery::{ProjectionRecoverySlot, ProjectionRecoverySlotRole};
use super::term::ProjectionTerm;

/// A projection variable solved during cycle recovery.
///
/// `Projection(root, path)` is treated as an equation variable whose value is
/// derived from the container structure observed for `root`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(super) struct ProjectionVar<'db> {
    pub(super) root: DivergentType,
    pub(super) path: ProjectionPath<'db>,
}

#[derive(Debug, Clone)]
pub(super) struct CycleRootSet {
    roots: SmallVec<[DivergentType; 4]>,
}

impl CycleRootSet {
    pub(super) fn from_cycle(cycle: &salsa::Cycle) -> Self {
        Self {
            roots: cycle.head_ids().map(DivergentType::new).collect(),
        }
    }

    pub(super) fn single(root: DivergentType) -> Self {
        Self {
            roots: SmallVec::from_slice(&[root]),
        }
    }

    pub(super) fn from_roots(roots: impl IntoIterator<Item = DivergentType>) -> Self {
        Self {
            roots: roots.into_iter().collect(),
        }
    }

    pub(super) fn len(&self) -> usize {
        self.roots.len()
    }

    pub(super) fn contains(&self, root: DivergentType) -> bool {
        self.roots
            .iter()
            .any(|candidate| candidate.same_marker(root))
    }
}

/// Solved projection variables for one Salsa cycle recovery step.
pub(crate) struct ProjectionSolutions<'db> {
    solved: FxIndexMap<ProjectionVar<'db>, Type<'db>>,
}

impl<'db> ProjectionSolutions<'db> {
    pub(super) fn from_recovery_slots(
        db: &'db dyn Db,
        roots: &CycleRootSet,
        slots: &[ProjectionRecoverySlot<'db>],
        evidence: Option<&ProjectionEvidenceSet<'db>>,
    ) -> Option<Self> {
        ProjectionEquationSystem::from_recovery_slots(db, roots, slots, evidence)?.solve(db)
    }

    fn new(solved: FxIndexMap<ProjectionVar<'db>, Type<'db>>) -> Self {
        Self { solved }
    }

    fn contains_projection_artifact_in_roots(&self, db: &'db dyn Db) -> bool {
        let roots = self.roots();
        self.solved
            .values()
            .any(|ty| ty.mentions_projection_artifact_in_roots(db, &roots))
    }

    pub(super) fn roots(&self) -> CycleRootSet {
        let mut roots = SmallVec::new();
        for var in self.solved.keys() {
            if !roots
                .iter()
                .any(|candidate: &DivergentType| candidate.same_marker(var.root))
            {
                roots.push(var.root);
            }
        }
        CycleRootSet { roots }
    }

    pub(super) fn solved_type(
        &self,
        db: &'db dyn Db,
        var: &ProjectionVar<'db>,
    ) -> Option<Type<'db>> {
        if let Some(solved) = self.solved.get(var).copied() {
            debug_assert!(
                !solved.mentions_projection_artifact_in_roots(db, &CycleRootSet::single(var.root)),
                "projection solver must not return an unsolved projection artifact"
            );
            return Some(solved);
        }

        // Longer demanded paths are represented by the nearest solved prefix plus a
        // structural replay of the remaining operations.
        for prefix_len in (1..var.path.ops().len()).rev() {
            let prefix = ProjectionPath::from_ops(var.path.ops()[..prefix_len].iter().copied());
            let Some(solved) = self.solved.get(&ProjectionVar {
                root: var.root,
                path: prefix,
            }) else {
                continue;
            };
            let tail = ProjectionPath::from_ops(var.path.ops()[prefix_len..].iter().copied());
            if solved.same_divergent_marker(db, Type::Divergent(var.root)) {
                return Some(*solved);
            }
            let solved = ProjectionContainer::project_type_path(db, *solved, var.root, None, &tail)
                .map(|term| term.ty(db))?;
            debug_assert!(
                !solved.mentions_projection_artifact_in_roots(db, &CycleRootSet::single(var.root)),
                "projection solver must not return an unsolved projection artifact"
            );
            return Some(solved);
        }

        None
    }
}

/// Cycle-recovery-time equation system for projection variables.
///
/// Each [`ProjectionPath`] is a variable `A_p = Projection_p(root)`. Equations contain only
/// query-free facts collected from container arms: closed productive terms and flat dependencies on
/// other projection variables. Constructor-guarded recursion is filled with `Divergent`.
///
/// The system is solved only for the paths collected from the candidate type. If replay exposes a
/// dependency on a path outside that set, solving fails closed.
pub(super) struct ProjectionEquationSystem<'db> {
    equations: FxIndexMap<ProjectionVar<'db>, ProjectionEquation<'db>>,
}

impl<'db> ProjectionEquationSystem<'db> {
    pub(super) fn from_recovery_slots(
        db: &'db dyn Db,
        roots: &CycleRootSet,
        slots: &[ProjectionRecoverySlot<'db>],
        evidence: Option<&ProjectionEvidenceSet<'db>>,
    ) -> Option<Self> {
        if roots.len() <= 1 {
            return None;
        }

        let mut demands = FxIndexSet::default();
        for slot in slots {
            for (root, path) in slot.joined.projection_demands(db) {
                if roots.contains(root) {
                    demands.insert(ProjectionVar { root, path });
                }
            }
        }

        if demands.is_empty() {
            return None;
        }

        let mut candidates = RootCandidates::default();
        for slot in slots {
            let ProjectionRecoverySlotRole::Candidate { root_hint } = slot.role else {
                continue;
            };
            let joined_demands = slot.joined.projection_demands(db);
            if let Some(root) =
                root_hint.or_else(|| root_candidate_from_demands(&joined_demands, roots))
            {
                if !demands.iter().any(|var| var.root.same_marker(root)) {
                    continue;
                }
                if !is_plausible_root_candidate(db, root, slot.joined, evidence) {
                    continue;
                }
                candidates.insert(root, slot.joined);
                continue;
            }

            if slot.previous.is_none() {
                for (root, _) in joined_demands {
                    if demands.iter().any(|var| var.root.same_marker(root))
                        && is_plausible_root_candidate(db, root, slot.joined, evidence)
                    {
                        candidates.insert(root, slot.joined);
                    }
                }
            }
        }

        let demands = demands.into_iter().collect::<Vec<_>>();
        let mut equations = FxIndexMap::default();
        // A longer path whose prefix is also demanded is solved by replaying the tail
        // on the prefix solution. Keeping both as independent variables can build an
        // infinite chain such as `A_[0]^n -> B_[0]^(2n)`.
        let mut pending = demands
            .iter()
            .filter(|var| {
                !demands.iter().any(|prefix| {
                    prefix.root.same_marker(var.root) && prefix.path.is_strict_prefix_of(&var.path)
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        while let Some(var) = pending.pop() {
            if equations.contains_key(&var) {
                continue;
            }

            let mut equation = ProjectionEquation::default();
            let mut has_equation_terms = false;
            if let Some(candidates) = candidates.get(var.root) {
                for candidate in candidates {
                    let Some(candidate_equation) =
                        Self::build_equation(db, roots, *candidate, &var, evidence)
                    else {
                        continue;
                    };
                    has_equation_terms = true;
                    equation.merge(candidate_equation)?;
                }
            }
            if !has_equation_terms && let Some(evidence) = evidence {
                for fact in evidence.projection_facts(db) {
                    if fact.root.same_marker(var.root) && fact.path == var.path {
                        has_equation_terms = true;
                        equation.add_projection_term(db, roots, &var, fact.term, true)?;
                    }
                }
            }
            if !has_equation_terms {
                return None;
            }
            equation.wrap_in_list?;
            if equation.unsupported {
                return None;
            }
            for dependency in &equation.dependencies {
                if !equations.contains_key(dependency) {
                    pending.push(dependency.clone());
                }
            }
            equations.insert(var, equation);
        }

        Some(Self { equations })
    }

    pub(super) fn from_terms_by_op(
        db: &'db dyn Db,
        root: DivergentType,
        terms_by_op: &FxIndexMap<ProjectionPath<'db>, Vec<ProjectionTerm<'db>>>,
    ) -> Option<Self> {
        let roots = CycleRootSet::single(root);
        let mut equations = FxIndexMap::default();
        let mut pending = terms_by_op
            .keys()
            .map(|path| ProjectionVar {
                root,
                path: path.clone(),
            })
            .collect::<Vec<_>>();

        while let Some(var) = pending.pop() {
            if equations.contains_key(&var) {
                continue;
            }

            let terms = terms_by_op.get(&var.path)?;
            let mut equation = ProjectionEquation::default();
            for term in terms {
                equation.add_projection_term(db, &roots, &var, *term, true)?;
            }
            equation.wrap_in_list?;
            if equation.unsupported {
                return None;
            }
            for dependency in &equation.dependencies {
                if !equations.contains_key(dependency) {
                    pending.push(dependency.clone());
                }
            }
            equations.insert(var, equation);
        }

        Some(Self { equations })
    }

    fn build_equation(
        db: &'db dyn Db,
        roots: &CycleRootSet,
        candidate: Type<'db>,
        var: &ProjectionVar<'db>,
        evidence: Option<&ProjectionEvidenceSet<'db>>,
    ) -> Option<ProjectionEquation<'db>> {
        let elements = candidate.top_level_projection_union_elements(db);
        let mut equation = ProjectionEquation::default();

        for element in elements {
            if element.same_divergent_marker(db, Type::Divergent(var.root)) {
                continue;
            }

            let container = ProjectionContainer::try_from(db, var.root, element, evidence)?;
            let term = container.project_multi_root_path(db, var.root, evidence, &var.path)?;
            // Terms projected from an arm that already contains this root are recursive evidence,
            // not a productive base. Cross-root projection terms may still be productive because
            // they can be substituted once the other root is solved.
            let allow_productive = !element.mentions_cycle_artifact_direct(db, var.root);
            equation.add_projection_term(db, roots, var, term, allow_productive)?;
        }

        Some(equation)
    }

    pub(super) fn solve(self, db: &'db dyn Db) -> Option<ProjectionSolutions<'db>> {
        let Self { equations } = self;
        if equations.is_empty() || equations.values().any(|equation| equation.unsupported) {
            return None;
        }

        let vars = equations.keys().cloned().collect::<Vec<_>>();
        let var_indices = vars
            .iter()
            .enumerate()
            .map(|(index, var)| (var.clone(), index))
            .collect::<FxIndexMap<_, _>>();

        let mut graph = vec![Vec::new(); vars.len()];
        for (var, equation) in &equations {
            let source = var_indices[var];
            for dependency in &equation.dependencies {
                graph[source].push(*var_indices.get(dependency)?);
            }
        }

        let sccs = dependency_first_strongly_connected_components(&graph);
        let mut solutions = vec![None; vars.len()];
        for scc in sccs {
            let wrap_in_list = equations[&vars[*scc.first()?]].wrap_in_list?;
            for &index in &scc {
                if equations[&vars[index]].wrap_in_list != Some(wrap_in_list) {
                    return None;
                }
            }

            let scc_vars = scc
                .iter()
                .map(|&index| vars[index].clone())
                .collect::<FxIndexSet<_>>();
            let scc_is_divergent = scc.iter().any(|&index| {
                let equation = &equations[&vars[index]];
                equation.divergent
                    || equation
                        .productive
                        .iter()
                        .any(|term| term.mentions_projection_var_in(db, &scc_vars))
            });
            let solved_so_far = vars
                .iter()
                .cloned()
                .zip(solutions.iter().copied())
                .filter_map(|(var, solution)| Some((var, solution?)))
                .collect::<FxIndexMap<_, _>>();
            let solved_so_far = ProjectionSolutions::new(solved_so_far);

            if scc_is_divergent {
                for &index in &scc {
                    let mut base = Vec::new();
                    let equation = &equations[&vars[index]];
                    for term in &equation.productive {
                        if term.mentions_projection_var_in(db, &scc_vars) {
                            continue;
                        }
                        base.push(term.replace_solved_projection_vars(db, &solved_so_far)?);
                    }
                    for dependency in &equation.dependencies {
                        let dependency_index = var_indices[dependency];
                        if !scc.contains(&dependency_index) {
                            base.push(solutions[dependency_index]?);
                        }
                    }
                    let root = vars[index].root;
                    base.push(Type::Divergent(root));
                    let solved = match base.as_slice() {
                        [term] => *term,
                        _ => UnionType::from_elements_cycle_recovery(db, base),
                    };
                    solutions[index] = Some(if wrap_in_list {
                        KnownClass::List.to_specialized_instance(db, &[solved])
                    } else {
                        solved
                    });
                }
                continue;
            }

            let mut base = Vec::new();
            for &index in &scc {
                let equation = &equations[&vars[index]];
                for term in &equation.productive {
                    base.push(term.replace_solved_projection_vars(db, &solved_so_far)?);
                }
                for dependency in &equation.dependencies {
                    let dependency_index = var_indices[dependency];
                    if !scc.contains(&dependency_index) {
                        base.push(solutions[dependency_index]?);
                    }
                }
            }

            if base.is_empty() {
                for index in scc {
                    let root = vars[index].root;
                    let solved = if wrap_in_list {
                        KnownClass::List.to_specialized_instance(db, &[Type::Divergent(root)])
                    } else {
                        Type::Divergent(root)
                    };
                    solutions[index] = Some(solved);
                }
                continue;
            }

            let solved = match base.as_slice() {
                [term] => *term,
                _ => UnionType::from_elements_cycle_recovery(db, base),
            };
            let solved = if wrap_in_list {
                KnownClass::List.to_specialized_instance(db, &[solved])
            } else {
                solved
            };
            for index in scc {
                solutions[index] = Some(solved);
            }
        }

        let solved = vars
            .into_iter()
            .enumerate()
            .map(|(index, var)| Some((var, solutions[index]?)))
            .collect::<Option<FxIndexMap<_, _>>>()?;

        let solutions = ProjectionSolutions::new(solved);
        debug_assert!(
            !solutions.contains_projection_artifact_in_roots(db),
            "projection solver must not leave unsolved projection artifacts"
        );
        Some(solutions)
    }
}

#[derive(Default)]
struct RootCandidates<'db> {
    candidates: FxIndexMap<DivergentType, Vec<Type<'db>>>,
}

impl<'db> RootCandidates<'db> {
    fn insert(&mut self, root: DivergentType, ty: Type<'db>) {
        if let Some((_, candidates)) = self
            .candidates
            .iter_mut()
            .find(|(candidate, _)| candidate.same_marker(root))
        {
            if !candidates.contains(&ty) {
                candidates.push(ty);
            }
        } else {
            self.candidates.insert(root, vec![ty]);
        }
    }

    fn get(&self, root: DivergentType) -> Option<&[Type<'db>]> {
        self.candidates
            .iter()
            .find_map(|(candidate, ty)| candidate.same_marker(root).then_some(ty.as_slice()))
    }
}

#[derive(Default)]
struct ProjectionEquation<'db> {
    // Productive terms may still contain projection variables. Variables outside the SCC are
    // substituted with solved values; variables inside the SCC force divergent widening.
    productive: Vec<Type<'db>>,
    dependencies: FxIndexSet<ProjectionVar<'db>>,
    divergent: bool,
    unsupported: bool,
    wrap_in_list: Option<bool>,
}

impl<'db> ProjectionEquation<'db> {
    fn merge(&mut self, other: Self) -> Option<()> {
        match (self.wrap_in_list, other.wrap_in_list) {
            (Some(left), Some(right)) if left != right => return None,
            (None, Some(right)) => self.wrap_in_list = Some(right),
            _ => {}
        }

        self.productive.extend(other.productive);
        self.dependencies.extend(other.dependencies);
        self.divergent |= other.divergent;
        self.unsupported |= other.unsupported;
        Some(())
    }

    fn add_projection_term(
        &mut self,
        db: &'db dyn Db,
        roots: &CycleRootSet,
        var: &ProjectionVar<'db>,
        term: ProjectionTerm<'db>,
        allow_productive: bool,
    ) -> Option<()> {
        let wrap_in_list = matches!(term, ProjectionTerm::List(_));
        match self.wrap_in_list {
            Some(existing) if existing != wrap_in_list => return None,
            None => self.wrap_in_list = Some(wrap_in_list),
            Some(_) => {}
        }

        match term {
            ProjectionTerm::Exact(term) => {
                if term.same_divergent_marker(db, Type::Divergent(var.root)) {
                    self.dependencies.insert(var.clone());
                    return Some(());
                }

                if !term.is_matching_projection(db, var.root, &var.path)
                    && term.mentions_matching_projection(db, var.root, &var.path)
                    && let Some(projected) = ProjectionContainer::project_multi_root_type_path(
                        db, term, var.root, None, &var.path,
                    )
                {
                    if projected
                        .ty(db)
                        .is_matching_projection(db, var.root, &var.path)
                    {
                        self.divergent = true;
                        return Some(());
                    }
                    return self.add_projection_term(db, roots, var, projected, allow_productive);
                }

                self.add_type_term(db, roots, var, term, true, allow_productive)
            }
            ProjectionTerm::Homogeneous(term) => {
                if term.same_divergent_marker(db, Type::Divergent(var.root)) {
                    self.dependencies.insert(var.clone());
                    return Some(());
                }

                self.add_type_term(db, roots, var, term, true, allow_productive)
            }
            ProjectionTerm::List(term) => {
                self.add_type_term(db, roots, var, term, false, allow_productive)
            }
        }
    }

    fn add_type_term(
        &mut self,
        db: &'db dyn Db,
        roots: &CycleRootSet,
        var: &ProjectionVar<'db>,
        term: Type<'db>,
        allow_dependencies: bool,
        allow_productive: bool,
    ) -> Option<()> {
        if let Type::Union(union) = term {
            for element in union.elements(db) {
                self.add_type_term(
                    db,
                    roots,
                    var,
                    *element,
                    allow_dependencies,
                    allow_productive,
                )?;
            }
            return Some(());
        }

        if term.mentions_cycle_artifact_outside_roots(db, roots) {
            self.unsupported = true;
            return Some(());
        }

        if allow_dependencies {
            if let Type::Projection(projection) = term {
                let root = projection.root(db);
                if roots.contains(root) {
                    let dependency = ProjectionVar {
                        root,
                        path: projection.path(db),
                    };
                    if var.path.is_strict_prefix_of(&dependency.path) {
                        // A strict extension of the current path cannot be closed by
                        // adding another projection variable; widen this equation.
                        self.divergent = true;
                    } else {
                        self.dependencies.insert(dependency);
                    }
                } else {
                    self.unsupported = true;
                }
                return Some(());
            }

            if let Some(var) = term.matching_projection_narrowing_var_multi(db, roots) {
                self.dependencies.insert(var);
                return Some(());
            }
        }

        if allow_dependencies {
            let dependencies = term
                .projection_demands(db)
                .into_iter()
                .filter_map(|(root, path)| {
                    roots.contains(root).then_some(ProjectionVar { root, path })
                })
                .collect::<Vec<_>>();
            if !dependencies.is_empty() {
                if dependencies
                    .iter()
                    .any(|dependency| var.path.is_strict_prefix_of(&dependency.path))
                {
                    // A strict extension of the current path cannot be closed by
                    // adding another projection variable; widen this equation.
                    self.divergent = true;
                    return Some(());
                }
                self.dependencies.extend(dependencies);
                if allow_productive {
                    self.productive.push(term);
                }
                return Some(());
            }
        }

        if term.mentions_divergent_in_roots(db, roots)
            || term.mentions_cycle_artifact_in_roots(db, roots)
        {
            self.divergent = true;
            return Some(());
        }

        if allow_productive {
            self.productive.push(term);
        }
        Some(())
    }
}

pub(super) fn root_candidate_from_previous(
    db: &dyn Db,
    previous: Type<'_>,
    roots: &CycleRootSet,
) -> Option<DivergentType> {
    let mut candidates = previous
        .cycle_artifact_roots(db)
        .into_iter()
        .filter(|root| roots.contains(*root))
        .collect::<Vec<_>>();
    candidates.dedup_by(|left, right| left.same_marker(*right));
    match candidates.as_slice() {
        [root] => Some(*root),
        _ => None,
    }
}

fn root_candidate_from_demands(
    demands: &[(DivergentType, ProjectionPath<'_>)],
    roots: &CycleRootSet,
) -> Option<DivergentType> {
    let mut candidates = Vec::new();
    for (root, _) in demands {
        if roots.contains(*root) {
            Type::push_cycle_artifact_root(&mut candidates, *root);
        }
    }
    match candidates.as_slice() {
        [root] => Some(*root),
        _ => None,
    }
}

fn is_plausible_root_candidate<'db>(
    db: &'db dyn Db,
    root: DivergentType,
    ty: Type<'db>,
    evidence: Option<&ProjectionEvidenceSet<'db>>,
) -> bool {
    ty.top_level_projection_union_elements(db)
        .into_iter()
        .filter(|element| !element.same_divergent_marker(db, Type::Divergent(root)))
        .any(|element| ProjectionContainer::try_from(db, root, element, evidence).is_some())
}

/// Returns strongly connected components in dependency-first order for a graph
/// whose edges point from a projection variable to the variables it depends on.
fn dependency_first_strongly_connected_components(graph: &[Vec<usize>]) -> Vec<Vec<usize>> {
    struct SccState<'a> {
        graph: &'a [Vec<usize>],
        next_index: usize,
        indices: Vec<Option<usize>>,
        lowlinks: Vec<usize>,
        stack: Vec<usize>,
        on_stack: Vec<bool>,
        components: Vec<Vec<usize>>,
    }

    impl SccState<'_> {
        fn connect(&mut self, node: usize) {
            let index = self.next_index;
            self.next_index += 1;
            self.indices[node] = Some(index);
            self.lowlinks[node] = index;
            self.stack.push(node);
            self.on_stack[node] = true;

            for &dependency in &self.graph[node] {
                if self.indices[dependency].is_none() {
                    self.connect(dependency);
                    self.lowlinks[node] = self.lowlinks[node].min(self.lowlinks[dependency]);
                } else if self.on_stack[dependency]
                    && let Some(dependency_index) = self.indices[dependency]
                {
                    self.lowlinks[node] = self.lowlinks[node].min(dependency_index);
                }
            }

            let Some(node_index) = self.indices[node] else {
                return;
            };

            if self.lowlinks[node] == node_index {
                let mut component = Vec::new();
                while let Some(dependency) = self.stack.pop() {
                    self.on_stack[dependency] = false;
                    component.push(dependency);
                    if dependency == node {
                        break;
                    }
                }
                self.components.push(component);
            }
        }
    }

    let mut state = SccState {
        graph,
        next_index: 0,
        indices: vec![None; graph.len()],
        lowlinks: vec![0; graph.len()],
        stack: Vec::new(),
        on_stack: vec![false; graph.len()],
        components: Vec::new(),
    };

    for node in 0..graph.len() {
        if state.indices[node].is_none() {
            state.connect(node);
        }
    }

    state.components
}
