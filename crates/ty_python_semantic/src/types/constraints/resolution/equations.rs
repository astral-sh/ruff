//! Solve finite recursive equations by identifying states with the same unfolding.

use std::cell::{Cell, RefCell};

use rustc_hash::FxHashMap;

use crate::types::RecursivelyDefined;
use crate::types::generics::{ApplySpecialization, GenericContext};
use crate::types::structural::StructuralTypeMapper;
use crate::types::visitor::any_over_type_including_alias_arguments;
use crate::types::{
    BoundTypeVarIdentity, BoundTypeVarInstance, RecursiveType, RecursiveVar, Type, TypeAliasType,
    TypeContext, TypeMapping, UnionType,
};
use crate::{Db, FxIndexSet, ProgramEnvironment};

/// Each stored subexpression is a state; equation variables transparently refer to their roots.
pub(super) struct EquationGraph<'db> {
    variables: FxHashMap<BoundTypeVarIdentity<'db>, Type<'db>>,
    nodes: RefCell<FxIndexSet<Type<'db>>>,
}

impl<'db> EquationGraph<'db> {
    /// Solves the equations `variable = body`, whose bodies refer to each other's variables, by
    /// closing them into recursive types. Returns `None` if some reference is not guarded by a
    /// type constructor and cannot be eliminated, as in `T = T & tuple[T]`: unfolding it would
    /// never reach a type to inspect.
    pub(super) fn solve(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        equations: &[(BoundTypeVarInstance<'db>, Type<'db>)],
    ) -> Option<Vec<Type<'db>>> {
        let UnguardedReferences { bodies, equal_to } =
            Self::inline_unguarded_references(db, env, equations)?;

        // Refer to variables that are equal by a single one of them, and solve for that one.
        let is_retained = |entry: &usize| equal_to[*entry] == *entry;
        let renamed: Vec<_> = (0..equations.len())
            .filter(|entry| !is_retained(entry))
            .collect();
        let bodies = if renamed.is_empty() {
            bodies
        } else {
            let context = GenericContext::from_typevar_instances(
                db,
                env,
                renamed.iter().map(|entry| equations[*entry].0),
            );
            let names: Vec<_> = renamed
                .iter()
                .map(|entry| Type::TypeVar(equations[equal_to[*entry]].0))
                .collect();
            let renaming = TypeMapping::ApplySpecialization(ApplySpecialization::Partial {
                generic_context: context,
                types: &names,
                skip: None,
            });
            bodies
                .iter()
                .map(|body| body.apply_type_mapping(db, env, &renaming, TypeContext::default()))
                .collect()
        };
        let retained: Vec<_> = (0..equations.len()).filter(is_retained).collect();
        let variables: Vec<_> = retained.iter().map(|entry| equations[*entry].0).collect();
        let bodies: Vec<_> = retained.iter().map(|entry| bodies[*entry]).collect();

        let solution = Self::close(db, env, &variables, &bodies)?;
        (0..equations.len())
            .map(|entry| {
                let position = retained
                    .iter()
                    .position(|other| *other == equal_to[entry])?;
                Some(solution[position])
            })
            .collect()
    }

    /// Replaces each variable that a body exposes outside a type constructor, as `U` in
    /// `T = U | int`, with the body of its equation. Every remaining reference is then guarded.
    ///
    /// Variables that expose each other, as in `T = U | int`, `U = T | str`, contain each other
    /// and are therefore equal. The least type that solves their equations is the union of what
    /// each of them contains besides the others: `T = U = int | str`.
    ///
    /// Returns `None` if an unguarded reference passes through a type alias, if variables
    /// expose each other anywhere but in a union, or if they contain nothing else, as in
    /// `T = U`, `U = T`, which every type solves.
    fn inline_unguarded_references(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        equations: &[(BoundTypeVarInstance<'db>, Type<'db>)],
    ) -> Option<UnguardedReferences<'db>> {
        let count = equations.len();
        // The equations whose variables each body exposes.
        let exposed: Vec<Vec<usize>> = equations
            .iter()
            .map(|(_, body)| {
                body.unguarded_typevars(db)
                    .iter()
                    .filter_map(|variable| {
                        equations
                            .iter()
                            .position(|(other, _)| other.is_same_typevar_as(db, *variable))
                    })
                    .collect()
            })
            .collect();
        let mut references = UnguardedReferences {
            bodies: equations.iter().map(|(_, body)| *body).collect(),
            equal_to: (0..count).collect(),
        };
        if exposed.iter().all(Vec::is_empty) {
            return Some(references);
        }

        // Whether following exposed variables leads from one equation to another. There are
        // only as many equations as mutually dependent type variables in one call.
        let mut reaches = vec![vec![false; count]; count];
        for (from, exposed) in exposed.iter().enumerate() {
            for to in exposed {
                reaches[from][*to] = true;
            }
        }
        for through in 0..count {
            for from in 0..count {
                for to in 0..count {
                    reaches[from][to] |= reaches[from][through] && reaches[through][to];
                }
            }
        }
        let exposing_each_other = |entry: usize| -> Vec<usize> {
            (0..count)
                .filter(|other| {
                    *other == entry || (reaches[entry][*other] && reaches[*other][entry])
                })
                .collect()
        };

        // Solve each group of variables once everything else that it exposes is solved. Such a
        // group always exists, since the groups do not expose each other mutually.
        let mut is_solved = vec![false; count];
        while let Some(group) = (0..count)
            .filter(|entry| !is_solved[*entry])
            .map(exposing_each_other)
            .find(|group| {
                group.iter().all(|member| {
                    exposed[*member]
                        .iter()
                        .all(|other| is_solved[*other] || group.contains(other))
                })
            })
        {
            let contents: Option<Vec<_>> = group
                .iter()
                .map(|member| {
                    let replacements: Vec<_> = exposed[*member]
                        .iter()
                        .map(|other| {
                            let replacement =
                                (!group.contains(other)).then(|| references.bodies[*other]);
                            (equations[*other].0, replacement)
                        })
                        .collect();
                    equations[*member]
                        .1
                        .replace_unguarded_typevars(db, env, &replacements)
                })
                .collect();
            let contents = contents?;
            let first = *group.first()?;
            if reaches[first][first] {
                let body = UnionType::from_elements(db, env, contents);
                if body.is_never() {
                    return None;
                }
                for member in &group {
                    references.bodies[*member] = body;
                    references.equal_to[*member] = first;
                }
            } else {
                references.bodies[first] = *contents.first()?;
            }
            for member in group {
                is_solved[member] = true;
            }
        }
        Some(references)
    }

    fn close(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variables: &[BoundTypeVarInstance<'db>],
        bodies: &[Type<'db>],
    ) -> Option<Vec<Type<'db>>> {
        let graph = Self {
            variables: variables
                .iter()
                .zip(bodies)
                .map(|(variable, body)| (variable.identity(db), *body))
                .collect(),
            nodes: RefCell::new(bodies.iter().copied().collect()),
        };
        let roots: Vec<_> = bodies
            .iter()
            .map(|body| graph.nodes.borrow().get_index_of(body))
            .collect::<Option<_>>()?;
        let mut nodes = Vec::new();
        let mut expansions = Vec::new();
        let mut sizes = Vec::new();
        let mut index = 0;
        loop {
            let Some(body) = graph.nodes.borrow().get_index(index).copied() else {
                break;
            };
            let expanded = match body {
                Type::TypeVar(variable) => graph
                    .variables
                    .get(&variable.identity(db))
                    .map(|body| graph.map_type(db, *body)),
                Type::TypeAlias(alias) if !Self::alias_is_recursive(db, env, alias) => {
                    Some(graph.map_type(db, alias.value_type(db)))
                }
                _ => None,
            };
            expansions.push(expanded.and_then(|ty| match ty {
                Type::RecursiveVar(variable) => variable.equation_entry(db),
                _ => None,
            }));
            let size = Cell::new(0);
            any_over_type_including_alias_arguments(db, env, body, |_| {
                size.set(size.get() + 1);
                false
            });
            sizes.push(size.get());
            nodes.push(body.map_structure(db, &graph));
            index += 1;
        }

        // Refine the coarsest partition. Constructors and stored metadata distinguish states;
        // union members and each sign of an intersection are unordered sets of child states.
        let mut classes = vec![0; nodes.len()];
        loop {
            let mut heads = HeadSignatures::new(&nodes, &classes, &expansions);
            let mut signatures = FxIndexSet::default();
            let refined: Vec<_> = nodes
                .iter()
                .enumerate()
                .map(|(index, _)| {
                    signatures
                        .insert_full((classes[index], heads.signature(db, index)))
                        .0
                })
                .collect();
            if refined == classes {
                break;
            }
            classes = refined;
        }
        let mapping = Quotient {
            nodes: &nodes,
            classes: &classes,
            expansions: &expansions,
        };
        let mut representatives: Vec<Option<usize>> =
            vec![None; classes.iter().max().copied().unwrap_or(0) + 1];
        for (index, class) in classes.iter().enumerate() {
            let representative = &mut representatives[*class];
            // Equality is already established. Prefer a constructor over a transparent set
            // wrapper, then retain the shortest original spelling of that same state.
            let rank = |index: usize| {
                (
                    expansions[index].is_some(),
                    matches!(nodes[index], Type::Union(_) | Type::Intersection(_)),
                    sizes[index],
                )
            };
            if representative.is_none_or(|previous| rank(index) < rank(previous)) {
                *representative = Some(index);
            }
        }
        let nodes: Vec<_> = representatives
            .into_iter()
            .map(|index| Some(nodes[index?].map_structure(db, &mapping)))
            .collect::<Option<_>>()?;
        let roots: Vec<_> = roots.into_iter().map(|root| classes[root]).collect();

        let mut incoming = vec![Vec::new(); nodes.len()];
        let mut remaining = Vec::with_capacity(nodes.len());
        for (parent, body) in nodes.iter().enumerate() {
            let children = References::default();
            body.map_structure(db, &children);
            let children = children.0.into_inner();
            remaining.push(children.len());
            for child in children {
                incoming[child].push(parent);
            }
        }
        // Remove finite subgraphs. The remaining nodes can reach a cycle.
        let mut finite: Vec<_> = remaining
            .iter()
            .enumerate()
            .filter_map(|(index, count)| (*count == 0).then_some(index))
            .collect();
        while let Some(child) = finite.pop() {
            for parent in &incoming[child] {
                remaining[*parent] -= 1;
                if remaining[*parent] == 0 {
                    finite.push(*parent);
                }
            }
        }
        let mut retained = FxIndexSet::default();
        retained.extend(roots.iter().copied().filter(|root| remaining[*root] != 0));
        retained.extend(
            (0..nodes.len()).filter(|index| remaining[*index] != 0 && incoming[*index].len() > 1),
        );
        let entries: Vec<_> = (0..nodes.len())
            .map(|index| retained.get_index_of(&index))
            .collect();
        let reconstruction = Reconstruction {
            nodes: &nodes,
            entries: &entries,
            expanded: RefCell::default(),
        };
        let recursive = RecursiveType::bind_equations(
            db,
            env.program(db),
            retained
                .iter()
                .map(|index| nodes[*index].map_structure(db, &reconstruction))
                .collect(),
        )?;
        let solution: Vec<_> = roots
            .iter()
            .map(|root| {
                entries[*root].map_or_else(
                    || reconstruction.expand(db, *root),
                    |entry| Type::Recursive(recursive[entry]),
                )
            })
            .collect();
        // Only complete, closed types may participate in semantic verification.
        let context = GenericContext::from_typevar_instances(db, env, variables.iter().copied());
        let substitution = TypeMapping::ApplySpecialization(ApplySpecialization::Partial {
            generic_context: context,
            types: &solution,
            skip: None,
        });
        bodies
            .iter()
            .zip(&solution)
            .all(|(body, solution)| {
                let substituted =
                    body.apply_type_mapping(db, env, &substitution, TypeContext::default());
                substituted == *solution || substituted.is_equivalent_to(db, env, *solution)
            })
            .then_some(solution)
    }

    /// An acyclic alias is transparent in the graph. Recursive aliases remain parameterized
    /// constructors, so aliases whose arguments grow do not generate an infinite graph.
    fn alias_is_recursive(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        alias: TypeAliasType<'db>,
    ) -> bool {
        let target = alias.definition(db);
        let mut seen = FxIndexSet::default();
        let mut pending = vec![alias];
        while let Some(next) = pending.pop() {
            if !seen.insert(next.definition(db)) {
                continue;
            }
            let found = RefCell::new(Vec::new());
            if any_over_type_including_alias_arguments(db, env, next.raw_value_type(db), |ty| {
                if let Type::TypeAlias(child) = ty {
                    if child.definition(db) == target {
                        return true;
                    }
                    found.borrow_mut().push(child);
                }
                false
            }) {
                return true;
            }
            pending.extend(found.into_inner());
        }
        false
    }
}

impl<'db> StructuralTypeMapper<'db> for EquationGraph<'db> {
    fn map_type(&self, db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        // Keep references distinct from literal alias occurrences. Substituting a variable's
        // body here would repeatedly reconstruct the alias in `T = Container[T]`.
        let index = self.nodes.borrow_mut().insert_full(ty).0;
        RecursiveVar::equation(db, index)
    }
}

struct Partition<'a> {
    classes: &'a [usize],
}

/// Preserve an alias occurrence's stored arguments when its value shares another state.
struct Quotient<'a, 'db> {
    nodes: &'a [Type<'db>],
    classes: &'a [usize],
    expansions: &'a [Option<usize>],
}

impl<'db> StructuralTypeMapper<'db> for Quotient<'_, 'db> {
    fn map_type(&self, db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        match ty {
            Type::RecursiveVar(variable) if let Some(index) = variable.equation_entry(db) => {
                if let Type::TypeAlias(alias) = self.nodes[index]
                    && self.expansions[index].is_some()
                {
                    Type::TypeAlias(alias.map_structure(db, self))
                } else {
                    RecursiveVar::equation(db, self.classes[index])
                }
            }
            _ => ty.map_structure(db, self),
        }
    }
}

#[derive(Clone, Eq, PartialEq, Hash)]
enum HeadSignature<'db> {
    Constructor(Type<'db>),
    Union(Vec<usize>, RecursivelyDefined),
    Intersection(Vec<usize>, Vec<usize>),
}

/// Compare sets of constructor heads rather than introducing a distinct state for `A | A`.
/// Traversal stops at constructors, whose children refer to the preceding partition.
struct HeadSignatures<'a, 'db> {
    nodes: &'a [Type<'db>],
    expansions: &'a [Option<usize>],
    partition: Partition<'a>,
    signatures: FxIndexSet<HeadSignature<'db>>,
    cached: Vec<Option<usize>>,
}

impl<'a, 'db> HeadSignatures<'a, 'db> {
    fn new(nodes: &'a [Type<'db>], classes: &'a [usize], expansions: &'a [Option<usize>]) -> Self {
        Self {
            nodes,
            expansions,
            partition: Partition { classes },
            signatures: FxIndexSet::default(),
            cached: vec![None; nodes.len()],
        }
    }

    fn reference(&mut self, db: &'db dyn Db, ty: Type<'db>) -> usize {
        match ty {
            Type::RecursiveVar(variable) if let Some(index) = variable.equation_entry(db) => {
                self.signature(db, index)
            }
            _ => {
                self.signatures
                    .insert_full(HeadSignature::Constructor(ty))
                    .0
            }
        }
    }

    fn signature(&mut self, db: &'db dyn Db, index: usize) -> usize {
        if let Some(cached) = self.cached[index] {
            return cached;
        }
        if let Some(expanded) = self.expansions[index] {
            let signature = self.signature(db, expanded);
            self.cached[index] = Some(signature);
            return signature;
        }
        let signature = match self.nodes[index] {
            Type::Union(union) => {
                let mut members = Vec::new();
                let mut recursively_defined = union.recursively_defined(db);
                for ty in union.elements(db) {
                    let child = self.reference(db, *ty);
                    if let HeadSignature::Union(children, recursive) = &self.signatures[child] {
                        members.extend(children);
                        recursively_defined = recursively_defined.or(*recursive);
                    } else {
                        members.push(child);
                    }
                }
                members.sort_unstable();
                members.dedup();
                if let [only] = members.as_slice() {
                    self.cached[index] = Some(*only);
                    return *only;
                }
                HeadSignature::Union(members, recursively_defined)
            }
            Type::Intersection(intersection) => {
                let mut positive = Vec::new();
                let mut negative = Vec::new();
                for ty in intersection.positive(db) {
                    let child = self.reference(db, *ty);
                    if let HeadSignature::Intersection(positives, negatives) =
                        &self.signatures[child]
                    {
                        positive.extend(positives);
                        negative.extend(negatives);
                    } else {
                        positive.push(child);
                    }
                }
                for ty in intersection.negative(db) {
                    negative.push(self.reference(db, *ty));
                }
                positive.sort_unstable();
                positive.dedup();
                negative.sort_unstable();
                negative.dedup();
                if let ([only], []) = (positive.as_slice(), negative.as_slice()) {
                    self.cached[index] = Some(*only);
                    return *only;
                }
                HeadSignature::Intersection(positive, negative)
            }
            body => HeadSignature::Constructor(body.map_structure(db, &self.partition)),
        };
        let signature = self.signatures.insert_full(signature).0;
        self.cached[index] = Some(signature);
        signature
    }
}

impl<'db> StructuralTypeMapper<'db> for Partition<'_> {
    fn map_type(&self, db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        match ty {
            Type::RecursiveVar(variable) if let Some(index) = variable.equation_entry(db) => {
                RecursiveVar::equation(db, self.classes[index])
            }
            _ => ty,
        }
    }
}

#[derive(Default)]
struct References(RefCell<Vec<usize>>);

impl<'db> StructuralTypeMapper<'db> for References {
    fn map_type(&self, db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        if let Type::RecursiveVar(variable) = ty
            && let Some(index) = variable.equation_entry(db)
        {
            self.0.borrow_mut().push(index);
            ty
        } else {
            ty.map_structure(db, self)
        }
    }
}

struct Reconstruction<'a, 'db> {
    nodes: &'a [Type<'db>],
    entries: &'a [Option<usize>],
    expanded: RefCell<FxHashMap<usize, Type<'db>>>,
}

impl<'db> Reconstruction<'_, 'db> {
    fn expand(&self, db: &'db dyn Db, index: usize) -> Type<'db> {
        if let Some(expanded) = self.expanded.borrow().get(&index) {
            return *expanded;
        }
        let expanded = self.nodes[index].map_structure(db, self);
        self.expanded.borrow_mut().insert(index, expanded);
        expanded
    }
}

impl<'db> StructuralTypeMapper<'db> for Reconstruction<'_, 'db> {
    fn map_type(&self, db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        match ty {
            Type::RecursiveVar(variable) if let Some(index) = variable.equation_entry(db) => {
                self.entries[index].map_or_else(
                    || self.expand(db, index),
                    |entry| RecursiveVar::equation(db, entry),
                )
            }
            _ => ty.map_structure(db, self),
        }
    }
}

/// The bodies of a group of equations once no variable of the group occurs outside of a type
/// constructor.
struct UnguardedReferences<'db> {
    bodies: Vec<Type<'db>>,
    /// For each equation, the first of the equations whose variables are equal to its own.
    equal_to: Vec<usize>,
}

impl<'db> Type<'db> {
    /// Substitutes types for the unguarded occurrences of type variables. A variable without a
    /// replacement is removed from the union that it is an element of. Returns `None` if a
    /// variable occurs where that is not possible: in a type alias, whose definition is fixed,
    /// negated in an intersection, or, for a variable to remove, anywhere but in a union.
    fn replace_unguarded_typevars(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        replacements: &[(BoundTypeVarInstance<'db>, Option<Type<'db>>)],
    ) -> Option<Type<'db>> {
        let replacement_of = |exposed: BoundTypeVarInstance<'db>| {
            replacements
                .iter()
                .find(|(variable, _)| variable.is_same_typevar_as(db, exposed))
                .map(|(_, replacement)| *replacement)
        };
        let exposes_replaced = |ty: Type<'db>| {
            ty.unguarded_typevars(db)
                .iter()
                .any(|exposed| replacement_of(*exposed).is_some())
        };
        match self {
            _ if replacements.is_empty() => Some(self),
            Type::TypeVar(exposed) => replacement_of(exposed).unwrap_or(Some(self)),
            Type::Union(union) => {
                let elements: Option<Vec<_>> = union
                    .elements(db)
                    .iter()
                    .filter(|element| {
                        element
                            .as_typevar()
                            .is_none_or(|exposed| replacement_of(exposed) != Some(None))
                    })
                    .map(|element| element.replace_unguarded_typevars(db, env, replacements))
                    .collect();
                Some(UnionType::from_elements(db, env, elements?))
            }
            Type::Intersection(intersection) => {
                if intersection.iter_negative(db).any(exposes_replaced) {
                    return None;
                }
                let positive: Option<Vec<_>> = intersection
                    .iter_positive(db)
                    .map(|element| element.replace_unguarded_typevars(db, env, replacements))
                    .collect();
                let mut positive = positive?.into_iter();
                Some(
                    intersection
                        .map_positive(db, env, |_| positive.next().unwrap_or_else(Type::object)),
                )
            }
            _ => (!exposes_replaced(self)).then_some(self),
        }
    }
}
