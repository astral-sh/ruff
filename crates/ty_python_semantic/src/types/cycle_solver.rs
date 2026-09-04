//! Solves the equations that define the cycle variables mentioned by a query's outputs.
//!
//! A query's output can mention cycle markers whose values are defined elsewhere: the root marker
//! of another cycle participant stands for that query's current result, and a derived marker
//! stands for an operation the owning query deferred. Gathering those definitions yields a system
//! of equations. Solving it replaces the markers whose values the current cycle iteration already
//! determines, instead of leaving them for later iterations of the Salsa fixed point, which cannot
//! resolve a marker nested inside a constructed type at all.
//!
//! The solver never substitutes a value into a nested position while iterating: a marker nested
//! inside a type stays a reference to its variable, and only a marker that is the whole input, or
//! a top-level element of a union input, is replaced by the variable's current value. Values
//! therefore never grow in depth, so the iteration terminates without a budget. The final
//! expansion substitutes nested references once, keeping the markers of mutually recursive
//! variables as the cut points that cycle recovery already knows how to bound.

use std::cell::RefCell;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::Db;
use crate::types::class::implicit_attributes::implicit_attribute_value;
use crate::types::cycle_equations::{CycleEquations, Operation, opaque_passthrough};
use crate::types::cycle_variable::{CycleOwner, CycleVariable};
use crate::types::infer::{
    InferenceRegion, infer_deferred_types, infer_definition_types, infer_expression_types,
    infer_function_default_types, infer_statement_types_impl, infer_unpack_types,
};
use crate::types::visitor::any_over_type_for_cycle_markers;
use crate::types::{
    DivergentType, ProgramEnvironment, Type, TypeContext, TypeMapping, UnionType,
    member_lookup_value,
};

/// The resolved values of cycle variables, ready to substitute for their markers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CycleSolution<'db>(FxHashMap<CycleVariable<'db>, Type<'db>>);

impl<'db> CycleSolution<'db> {
    pub(crate) fn get(&self, variable: CycleVariable<'db>) -> Option<Type<'db>> {
        self.0.get(&variable).copied()
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Substitutes the resolved values of `solution` for the markers in `ty`.
pub(crate) fn resolve_cycle_variables<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    solution: &CycleSolution<'db>,
    ty: Type<'db>,
) -> Type<'db> {
    let mentions_resolved = any_over_type_for_cycle_markers(db, env, ty, |ty| {
        matches!(
            ty,
            Type::Divergent(marker)
                if marker.variable().is_some_and(|variable| solution.get(variable).is_some())
        )
    });
    if !mentions_resolved {
        return ty;
    }
    ty.apply_type_mapping(
        db,
        env,
        &TypeMapping::ResolveCycleVariables(solution),
        TypeContext::default(),
    )
}

/// How a system defines the value of one variable.
#[derive(Clone, Debug)]
enum SystemEquation<'db> {
    /// The value the owning query has inferred for its root. It can mention markers, including
    /// the root's own.
    Value(Type<'db>),
    /// An operation the owning query deferred.
    Operation(Operation<'db>),
}

impl<'db> SystemEquation<'db> {
    fn inputs(&self) -> Vec<Type<'db>> {
        match self {
            SystemEquation::Value(ty) => vec![*ty],
            SystemEquation::Operation(operation) => operation.inputs().collect(),
        }
    }
}

/// Resolves the cycle markers in the outputs of one query.
pub(crate) struct CycleResolver<'a, 'db> {
    db: &'db dyn Db,
    env: &'a ProgramEnvironment<'db>,
    /// The query whose outputs are resolved.
    owner: CycleOwner<'db>,
    /// The operations the query has deferred so far.
    local: &'a FxHashMap<CycleVariable<'db>, Operation<'db>>,
    /// The value the query's root marker stands for, for a query with one main output.
    root: Option<Type<'db>>,
}

impl<'a, 'db> CycleResolver<'a, 'db> {
    pub(crate) fn new(
        db: &'db dyn Db,
        env: &'a ProgramEnvironment<'db>,
        owner: CycleOwner<'db>,
        local: &'a FxHashMap<CycleVariable<'db>, Operation<'db>>,
        root: Option<Type<'db>>,
    ) -> Self {
        Self {
            db,
            env,
            owner,
            local,
            root,
        }
    }

    /// Solves the equations reachable from `outputs`.
    ///
    /// Root markers are never part of the solution. A root stands for a query's whole result,
    /// and substituting that result where its marker is nested would unfold the recursion by
    /// one level on every cycle iteration, without the marker that lets cycle recovery cut the
    /// growth. Root values only feed the evaluation of the derived variables.
    ///
    /// Returns `None` when no marker in `outputs` can be resolved.
    pub(crate) fn solve(
        &self,
        outputs: impl IntoIterator<Item = Type<'db>>,
    ) -> Option<CycleSolution<'db>> {
        let db = self.db;
        let mut system = System::default();
        let mut worklist: Vec<_> = outputs
            .into_iter()
            .flat_map(|output| unmaterialized_markers(db, self.env, output))
            .collect();
        if worklist.is_empty() {
            return None;
        }
        let mut visited = FxHashSet::default();
        while let Some(variable) = worklist.pop() {
            if !visited.insert(variable) {
                continue;
            }
            let Some(equation) = self.equation(variable) else {
                continue;
            };
            worklist.extend(
                equation
                    .inputs()
                    .into_iter()
                    .flat_map(|input| unmaterialized_markers(db, self.env, input)),
            );
            system.insert(variable, equation);
        }
        if system.is_empty() {
            return None;
        }
        let mut solution = system.solve(db, self.env);
        solution.0.retain(|variable, _| !variable.is_root(db));
        (!solution.is_empty()).then_some(solution)
    }

    /// The definition of `variable`, if the query that owns it has recorded one.
    fn equation(&self, variable: CycleVariable<'db>) -> Option<SystemEquation<'db>> {
        let db = self.db;
        let owner = variable.owner(db);
        let is_root = variable.is_root(db);
        if owner == self.owner {
            return if is_root {
                self.root.map(SystemEquation::Value)
            } else {
                self.local
                    .get(&variable)
                    .cloned()
                    .map(SystemEquation::Operation)
            };
        }
        if is_root {
            owner
                .root_value(db)
                // A query that has only its cycle-initial marker so far defines nothing.
                .filter(|value| *value != Type::divergent_variable(variable))
                .map(SystemEquation::Value)
        } else {
            owner
                .equations(db)
                .and_then(|equations| equations.get(&variable).cloned())
                .map(SystemEquation::Operation)
        }
    }
}

impl<'db> CycleOwner<'db> {
    /// The current result that this owner's root marker stands for.
    ///
    /// Reading the result of a query that is still being inferred yields its provisional value,
    /// which is how a cycle participant learns what the other participants know so far.
    fn root_value(self, db: &'db dyn Db) -> Option<Type<'db>> {
        match self {
            CycleOwner::Region(InferenceRegion::Definition(definition)) => {
                infer_definition_types(db, definition).root_value(definition)
            }
            CycleOwner::Region(InferenceRegion::Deferred(definition)) => {
                infer_deferred_types(db, definition).root_value(definition)
            }
            CycleOwner::Region(InferenceRegion::Expression(expression, tcx)) => {
                infer_expression_types(db, expression, tcx)
                    .try_expression_type(expression.node_ref(db))
            }
            CycleOwner::Attribute(attribute) => implicit_attribute_value(db, attribute),
            CycleOwner::Member(key, receiver) => member_lookup_value(db, key, receiver),
            // These queries have no single main output.
            CycleOwner::Region(
                InferenceRegion::Statement(_)
                | InferenceRegion::Scope(..)
                | InferenceRegion::FunctionDecorators(_)
                | InferenceRegion::FunctionDefaults(_),
            )
            | CycleOwner::Unpack(_)
            | CycleOwner::Query(_) => None,
        }
    }

    /// The operations this owner has deferred, as recorded by its current result.
    fn equations(self, db: &'db dyn Db) -> Option<&'db CycleEquations<'db>> {
        match self {
            CycleOwner::Region(InferenceRegion::Definition(definition)) => {
                infer_definition_types(db, definition).equations()
            }
            CycleOwner::Region(InferenceRegion::Deferred(definition)) => {
                infer_deferred_types(db, definition).equations()
            }
            CycleOwner::Region(InferenceRegion::FunctionDefaults(definition)) => {
                infer_function_default_types(db, definition).equations()
            }
            CycleOwner::Region(InferenceRegion::Expression(expression, tcx)) => {
                infer_expression_types(db, expression, tcx).equations()
            }
            CycleOwner::Region(InferenceRegion::Statement(statement)) => {
                infer_statement_types_impl(db, statement).equations()
            }
            CycleOwner::Unpack(unpack) => Some(infer_unpack_types(db, unpack).equations()),
            // Depending on a whole scope's inference from another query would create needless
            // cycles, and the remaining owners never defer operations.
            CycleOwner::Region(
                InferenceRegion::Scope(..) | InferenceRegion::FunctionDecorators(_),
            )
            | CycleOwner::Attribute(_)
            | CycleOwner::Member(..)
            | CycleOwner::Query(_) => None,
        }
    }
}

/// The variables appearing in `ty` whose markers still stand for values to be resolved.
fn unmaterialized_markers<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
) -> Vec<CycleVariable<'db>> {
    let found = RefCell::new(Vec::new());
    any_over_type_for_cycle_markers(db, env, ty, |ty| {
        if let Type::Divergent(marker) = ty
            && marker.materialization_kind().is_none()
            && let Some(variable) = marker.variable()
        {
            found.borrow_mut().push(variable);
        }
        false
    });
    found.into_inner()
}

/// The equations gathered for one resolution, indexed for solving.
#[derive(Default)]
struct System<'db> {
    variables: Vec<CycleVariable<'db>>,
    index: FxHashMap<CycleVariable<'db>, usize>,
    equations: Vec<SystemEquation<'db>>,
}

impl<'db> System<'db> {
    fn is_empty(&self) -> bool {
        self.variables.is_empty()
    }

    fn insert(&mut self, variable: CycleVariable<'db>, equation: SystemEquation<'db>) {
        self.index.insert(variable, self.variables.len());
        self.variables.push(variable);
        self.equations.push(equation);
    }

    fn index_of(&self, marker: DivergentType<'db>) -> Option<usize> {
        if marker.materialization_kind().is_some() {
            return None;
        }
        self.index.get(&marker.variable()?).copied()
    }

    /// The variables of this system mentioned anywhere in `ty`.
    fn mentioned(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Vec<usize> {
        let found = RefCell::new(Vec::new());
        any_over_type_for_cycle_markers(db, env, ty, |ty| {
            if let Type::Divergent(marker) = ty
                && let Some(index) = self.index_of(marker)
            {
                found.borrow_mut().push(index);
            }
            false
        });
        found.into_inner()
    }

    fn solve(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> CycleSolution<'db> {
        let dependencies: Vec<Vec<usize>> = self
            .equations
            .iter()
            .map(|equation| {
                let mut dependencies: Vec<_> = equation
                    .inputs()
                    .into_iter()
                    .flat_map(|input| self.mentioned(db, env, input))
                    .collect();
                dependencies.sort_unstable();
                dependencies.dedup();
                dependencies
            })
            .collect();
        let mut dependents = vec![Vec::new(); self.variables.len()];
        for (variable, dependencies) in dependencies.iter().enumerate() {
            for &dependency in dependencies {
                dependents[dependency].push(variable);
            }
        }

        let mut solver = Solver {
            db,
            env,
            system: &self,
            values: vec![None; self.variables.len()],
        };
        // Components come out with their dependencies first, so every variable a component reads
        // from outside itself already has its final value.
        for component in strongly_connected_components(&dependencies) {
            solver.solve_component(&component, &dependents);
        }
        solver.expand()
    }
}

struct Solver<'a, 'db> {
    db: &'db dyn Db,
    env: &'a ProgramEnvironment<'db>,
    system: &'a System<'db>,
    /// The current value of each variable; `None` until an evaluation produces one.
    values: Vec<Option<Type<'db>>>,
}

impl<'db> Solver<'_, 'db> {
    /// Iterates the variables of one strongly connected component to a fixed point.
    ///
    /// Each value only ever gains union elements drawn from the inputs of the component's
    /// equations and the results of its operations on them, and nested markers are never
    /// expanded, so the iteration terminates.
    fn solve_component(&mut self, component: &[usize], dependents: &[Vec<usize>]) {
        let in_component: FxHashSet<usize> = component.iter().copied().collect();
        let mut queued: Vec<bool> = vec![false; self.system.variables.len()];
        let mut worklist: Vec<usize> = component.to_vec();
        for &variable in component {
            queued[variable] = true;
        }
        while let Some(variable) = worklist.pop() {
            queued[variable] = false;
            let Some(new) = self.evaluate(variable) else {
                continue;
            };
            let changed = match self.values[variable] {
                None => {
                    self.values[variable] = Some(new);
                    true
                }
                Some(old) if old == new => false,
                Some(old) => {
                    let joined =
                        UnionType::from_elements_cycle_recovery(self.db, self.env, [old, new]);
                    let changed = joined != old;
                    self.values[variable] = Some(joined);
                    changed
                }
            };
            if changed {
                for &dependent in &dependents[variable] {
                    if in_component.contains(&dependent) && !queued[dependent] {
                        queued[dependent] = true;
                        worklist.push(dependent);
                    }
                }
            }
        }
    }

    fn evaluate(&self, variable: usize) -> Option<Type<'db>> {
        match &self.system.equations[variable] {
            SystemEquation::Value(ty) => self.resolve_top(variable, *ty),
            SystemEquation::Operation(operation) => {
                let operation = operation.map_inputs(|input| self.resolve_top(variable, input))?;
                self.resolve_top(variable, operation.evaluate(self.db, self.env))
            }
        }
    }

    /// Replaces the markers at the top level of `ty` by the values of their variables.
    ///
    /// A marker of the variable being evaluated adds nothing at the top level and is dropped.
    /// A union element that mentions a variable without a value yet is dropped as well, so the
    /// remaining elements form the productive base; the element returns once the variable has a
    /// value and the equation is re-evaluated. Returns `None` when nothing remains.
    fn resolve_top(&self, variable: usize, ty: Type<'db>) -> Option<Type<'db>> {
        match ty {
            Type::Divergent(marker) => match self.system.index_of(marker) {
                Some(index) if index == variable => None,
                Some(index) => self.values[index],
                None => Some(ty),
            },
            Type::Union(union) => {
                let mut elements = Vec::new();
                for &element in union.elements(self.db) {
                    match element {
                        Type::Divergent(marker)
                            if let Some(index) = self.system.index_of(marker) =>
                        {
                            if index != variable
                                && let Some(value) = self.values[index]
                            {
                                elements.push(value);
                            }
                        }
                        _ => {
                            if !self.mentions_pending(variable, element) {
                                elements.push(element);
                            }
                        }
                    }
                }
                if elements.is_empty() {
                    None
                } else {
                    Some(UnionType::from_elements_cycle_recovery(
                        self.db, self.env, elements,
                    ))
                }
            }
            _ => Some(ty),
        }
    }

    /// Whether `ty` mentions a variable other than `variable` that has no value yet.
    ///
    /// A nested mention of `variable` itself is a recursive reference, kept as it is: it becomes
    /// the cut point that bounds the value's depth.
    fn mentions_pending(&self, variable: usize, ty: Type<'db>) -> bool {
        any_over_type_for_cycle_markers(self.db, self.env, ty, |ty| {
            if let Type::Divergent(marker) = ty
                && let Some(index) = self.system.index_of(marker)
            {
                index != variable && self.values[index].is_none()
            } else {
                false
            }
        })
    }

    /// Substitutes the nested references to derived variables in every resolved value.
    ///
    /// References between mutually recursive values are left as markers, so the expansion is
    /// finite and the markers remain for cycle recovery to cut. References to roots are left as
    /// markers too, for the reason given on [`CycleResolver::solve`].
    fn expand(self) -> CycleSolution<'db> {
        let references: Vec<Vec<usize>> = self
            .values
            .iter()
            .map(|value| {
                value.map_or_else(Vec::new, |value| {
                    let mut references: Vec<_> = self
                        .system
                        .mentioned(self.db, self.env, value)
                        .into_iter()
                        .filter(|&index| {
                            self.values[index].is_some()
                                && !self.system.variables[index].is_root(self.db)
                        })
                        .collect();
                    references.sort_unstable();
                    references.dedup();
                    references
                })
            })
            .collect();
        let mut solution = CycleSolution::default();
        for component in strongly_connected_components(&references) {
            let expanded: Vec<_> = component
                .iter()
                .filter_map(|&index| {
                    let value = self.values[index]?;
                    Some((
                        self.system.variables[index],
                        resolve_cycle_variables(self.db, self.env, &solution, value),
                    ))
                })
                .collect();
            solution.0.extend(expanded);
        }
        solution
    }
}

impl<'db> Operation<'db> {
    /// Evaluates the operation on its inputs, which contain no markers at the top level.
    ///
    /// Errors are not reported: the query that recorded the operation reports them once it
    /// re-infers the operation on the resolved operand.
    fn evaluate(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        let result = match self {
            Operation::Subscript { value, key } => value
                .subscript(db, env, *key, ruff_python_ast::ExprContext::Load)
                .unwrap_or_else(|_| Type::unknown()),
            Operation::Member {
                value,
                name,
                policy,
            } => value
                .member_lookup_with_policy(db, env, name, *policy)
                .place
                .ignore_possibly_undefined()
                .unwrap_or_else(Type::unknown),
            Operation::Iterate { value, mode } => value
                .try_iterate_with_mode(db, env, *mode)
                .map(|tuple| tuple.homogeneous_element_type(db, env))
                .unwrap_or_else(|error| error.fallback_element_type(db, env)),
            Operation::Unpack {
                value,
                length,
                index,
            } => value
                .unpacked_target(db, env, *length, *index)
                .unwrap_or_else(Type::unknown),
            Operation::Enter { value, mode } => value
                .try_enter_with_mode(db, env, *mode)
                .unwrap_or_else(|error| error.fallback_enter_type(db, env)),
            Operation::Await { value } => {
                value.try_await(db, env).unwrap_or_else(|_| Type::unknown())
            }
        };
        // A marker outside the system passes through like any dynamic type; the result must not
        // keep naming the operand.
        opaque_passthrough(db, env, self.inputs(), result)
    }
}

/// Tarjan's algorithm, returning components with their dependencies first.
fn strongly_connected_components(edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    struct Tarjan<'a> {
        edges: &'a [Vec<usize>],
        index: Vec<Option<usize>>,
        low_link: Vec<usize>,
        on_stack: Vec<bool>,
        stack: Vec<usize>,
        next_index: usize,
        components: Vec<Vec<usize>>,
    }

    impl Tarjan<'_> {
        fn visit(&mut self, root: usize) {
            // An explicit stack of (node, next edge position) keeps deep systems off the call
            // stack.
            let mut call_stack = vec![(root, 0)];
            self.enter(root);
            while let Some(&mut (node, ref mut position)) = call_stack.last_mut() {
                if let Some(&successor) = self.edges[node].get(*position) {
                    *position += 1;
                    if self.index[successor].is_none() {
                        self.enter(successor);
                        call_stack.push((successor, 0));
                    } else if self.on_stack[successor] {
                        self.low_link[node] = self.low_link[node].min(self.low_link[successor]);
                    }
                    continue;
                }
                call_stack.pop();
                if let Some(&(parent, _)) = call_stack.last() {
                    self.low_link[parent] = self.low_link[parent].min(self.low_link[node]);
                }
                if Some(self.low_link[node]) == self.index[node] {
                    let mut component = Vec::new();
                    loop {
                        let member = self.stack.pop().expect("node is on the stack");
                        self.on_stack[member] = false;
                        component.push(member);
                        if member == node {
                            break;
                        }
                    }
                    component.sort_unstable();
                    self.components.push(component);
                }
            }
        }

        fn enter(&mut self, node: usize) {
            self.index[node] = Some(self.next_index);
            self.low_link[node] = self.next_index;
            self.next_index += 1;
            self.stack.push(node);
            self.on_stack[node] = true;
        }
    }

    let mut tarjan = Tarjan {
        edges,
        index: vec![None; edges.len()],
        low_link: vec![0; edges.len()],
        on_stack: vec![false; edges.len()],
        stack: Vec::new(),
        next_index: 0,
        components: Vec::new(),
    };
    for node in 0..edges.len() {
        if tarjan.index[node].is_none() {
            tarjan.visit(node);
        }
    }
    tarjan.components
}

#[cfg(test)]
mod tests {
    use ruff_db::files::system_path_to_file;
    use ruff_db::parsed::parsed_module;
    use ruff_db::system::DbWithWritableSystem;
    use ty_python_core::ExpressionNodeKey;

    use super::*;
    use crate::db::tests::{TestDb, setup_db};
    use crate::semantic_model::HasDefinition;
    use crate::types::CycleSlot;
    use crate::types::class::KnownClass;
    use crate::types::tuple::{TupleLength, TupleType};
    use crate::{ProgramFile, SemanticModel};

    /// A database with two definitions whose queries can own cycle variables.
    fn db_with_owners() -> TestDb {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            r#"
def first(): ...
def second(): ...
"#,
        )
        .unwrap();
        db
    }

    /// The query inferring the `index`th function definition of the test module.
    fn owner<'db>(db: &'db TestDb, env: &ProgramEnvironment<'db>, index: usize) -> CycleOwner<'db> {
        let file = system_path_to_file(db, "/src/a.py").unwrap();
        let program_file = ProgramFile::new(db, file, env.program(db));
        let module = parsed_module(db, program_file.python_file(db)).load(db);
        let function = module.syntax().body[index]
            .as_function_def_stmt()
            .expect("test module consists of function definitions");
        let model = SemanticModel::new(db, program_file);
        CycleOwner::Region(InferenceRegion::Definition(function.definition(&model)))
    }

    /// A slot for a derived variable; the expression it names does not matter here.
    fn expression_slot() -> CycleSlot {
        let parsed = ruff_python_parser::parse_expression("x").expect("valid expression");
        CycleSlot::Expression(ExpressionNodeKey::from(parsed.expr()))
    }

    #[test]
    fn components_come_after_their_dependencies() {
        // 0 -> 1 -> 2 -> 1, 0 -> 3
        let edges = vec![vec![1, 3], vec![2], vec![1], vec![]];
        let components = strongly_connected_components(&edges);
        let position = |node: usize| {
            components
                .iter()
                .position(|component| component.contains(&node))
                .unwrap()
        };
        assert_eq!(components.len(), 3);
        assert!(components.contains(&vec![1, 2]));
        assert!(position(1) < position(0));
        assert!(position(3) < position(0));
    }

    #[test]
    fn rebuilds_a_tuple_from_its_productive_alternative() {
        // x = (x[0],) or x = (1,): the marker nested in the first alternative is the value the
        // unpacking of `x` yields, which the productive alternative determines.
        let db = db_with_owners();
        let env = db.program_environment();
        let root = CycleVariable::root(&db, salsa::plumbing::Id::from_bits(1), owner(&db, &env, 0));
        let element = CycleVariable::derived(&db, owner(&db, &env, 1), expression_slot(), root);
        let int = KnownClass::Int.to_instance(&db, &env);
        let output = UnionType::from_elements(
            &db,
            &env,
            [
                Type::tuple(TupleType::heterogeneous(
                    &db,
                    &env,
                    [Type::divergent_variable(element)],
                )),
                Type::tuple(TupleType::heterogeneous(&db, &env, [int])),
            ],
        );
        let mut system = System::default();
        system.insert(root, SystemEquation::Value(output));
        system.insert(
            element,
            SystemEquation::Operation(Operation::Unpack {
                value: Type::divergent_variable(root),
                length: TupleLength::Fixed(1),
                index: 0,
            }),
        );
        let solution = system.solve(&db, &env);
        assert_eq!(solution.get(element), Some(int));
        assert_eq!(
            solution.get(root),
            Some(Type::tuple(TupleType::heterogeneous(&db, &env, [int])))
        );
    }

    #[test]
    fn keeps_a_recursive_reference_as_a_cut_point() {
        // x = [x] or x = 1: the recursive alternative is kept with its marker rather than
        // unfolded, so the result stays finite.
        let db = db_with_owners();
        let env = db.program_environment();
        let root = CycleVariable::root(&db, salsa::plumbing::Id::from_bits(1), owner(&db, &env, 0));
        let int = KnownClass::Int.to_instance(&db, &env);
        let list_of_root = KnownClass::List.to_specialized_instance(
            &db,
            &env,
            vec![Type::divergent_variable(root)],
        );
        let output = UnionType::from_elements(&db, &env, [int, list_of_root]);
        let mut system = System::default();
        system.insert(root, SystemEquation::Value(output));
        let solution = system.solve(&db, &env);
        assert_eq!(solution.get(root), Some(output));
    }

    #[test]
    fn leaves_an_unproductive_variable_unresolved() {
        // x = (x,): nothing determines the element, so no value is produced.
        let db = db_with_owners();
        let env = db.program_environment();
        let root = CycleVariable::root(&db, salsa::plumbing::Id::from_bits(1), owner(&db, &env, 0));
        let element = CycleVariable::derived(&db, owner(&db, &env, 1), expression_slot(), root);
        let output = Type::tuple(TupleType::heterogeneous(
            &db,
            &env,
            [Type::divergent_variable(element)],
        ));
        let mut system = System::default();
        system.insert(root, SystemEquation::Value(output));
        system.insert(
            element,
            SystemEquation::Operation(Operation::Unpack {
                value: Type::divergent_variable(root),
                length: TupleLength::Fixed(1),
                index: 0,
            }),
        );
        let solution = system.solve(&db, &env);
        assert!(solution.get(element).is_none());
        assert_eq!(solution.get(root), Some(output));
    }

    #[test]
    fn solves_mutually_dependent_values() {
        // a = int or b; b = str or a.
        let db = db_with_owners();
        let env = db.program_environment();
        let a = CycleVariable::root(&db, salsa::plumbing::Id::from_bits(1), owner(&db, &env, 0));
        let b = CycleVariable::root(&db, salsa::plumbing::Id::from_bits(2), owner(&db, &env, 1));
        let int = KnownClass::Int.to_instance(&db, &env);
        let str = KnownClass::Str.to_instance(&db, &env);
        let mut system = System::default();
        system.insert(
            a,
            SystemEquation::Value(UnionType::from_elements(
                &db,
                &env,
                [int, Type::divergent_variable(b)],
            )),
        );
        system.insert(
            b,
            SystemEquation::Value(UnionType::from_elements(
                &db,
                &env,
                [str, Type::divergent_variable(a)],
            )),
        );
        let solution = system.solve(&db, &env);
        let both = FxHashSet::from_iter([int, str]);
        assert_eq!(
            solution.get(a).map(|ty| elements(&db, ty)),
            Some(both.clone())
        );
        assert_eq!(solution.get(b).map(|ty| elements(&db, ty)), Some(both));
    }

    /// The elements of a union, ignoring their order.
    fn elements<'db>(db: &'db dyn Db, ty: Type<'db>) -> FxHashSet<Type<'db>> {
        match ty {
            Type::Union(union) => union.elements(db).iter().copied().collect(),
            ty => FxHashSet::from_iter([ty]),
        }
    }
}
