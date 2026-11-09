//! Finite parametric proofs for recursively specialized aliases.
//!
//! Following [DeYoung et al.](https://www.cs.cmu.edu/~fp/papers/popl24.pdf),
//! "Parametric Subtyping for Structural Parametric Polymorphism" (POPL 2024),
//! we infer requirements on formal parameters and compose them with arguments.
//! States contain only operand and definition subexpressions, never
//! specialized bodies. In particular, following `F[list[T]]` does not construct
//! `F[list[list[T]]]`. Python unions add disjunction to the paper's requirements.
//!
//! Failure to find a parametric proof is inconclusive: this is a sufficient relation,
//! not a decision procedure for Python's structural subtyping.

use std::mem::discriminant;

use rustc_hash::{FxHashMap, FxHashSet};

use super::RecursiveType;
use crate::types::generics::{Specialization, specialization_variance};
use crate::types::tuple::{Tuple, TupleSpec, VariableSegment};
use crate::types::variance::TypeVarVariance;
use crate::types::{
    CallableType, ClassBase, ClassType, GenericContext, NominalInstanceType, Signature, Type,
    TypeAliasType,
};
use crate::{Db, ProgramEnvironment};

/// A normalized definition, independent of its actual arguments.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
enum Constructor<'db> {
    Alias(TypeAliasType<'db>),
    Recursive(RecursiveType<'db>),
}

impl<'db> Constructor<'db> {
    fn ty(self) -> Type<'db> {
        match self {
            Self::Alias(alias) => Type::TypeAlias(alias),
            Self::Recursive(recursive) => Type::Recursive(recursive),
        }
    }

    fn parameters(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        match self {
            Self::Alias(alias) => alias.generic_context(db),
            Self::Recursive(recursive) => recursive.parameters(db),
        }
    }

    fn body(self, db: &'db dyn Db) -> ScopedType<'db> {
        ScopedType {
            ty: match self {
                Self::Alias(alias) => alias.raw_value_type(db),
                Self::Recursive(recursive) => recursive.body(db),
            },
            scope: Some(self),
        }
    }
}

/// A subexpression and the constructor binding its formal parameters and graph references.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
struct ScopedType<'db> {
    ty: Type<'db>,
    scope: Option<Constructor<'db>>,
}

impl<'db> ScopedType<'db> {
    fn with_type(self, ty: Type<'db>) -> Self {
        Self { ty, ..self }
    }

    fn parameter(self, db: &'db dyn Db) -> Option<usize> {
        let Type::TypeVar(variable) = self.ty else {
            return None;
        };
        self.scope?
            .parameters(db)?
            .variable_index(db, variable.identity(db))
    }

    fn reference(self, db: &'db dyn Db) -> Option<(Constructor<'db>, Option<Specialization<'db>>)> {
        match self.ty {
            Type::TypeAlias(alias) if alias.materialization_kind(db).is_none() => Some((
                Constructor::Alias(alias.unspecialized(db)),
                alias.specialization(db).or_else(|| {
                    alias
                        .generic_context(db)
                        .map(|context| context.default_specialization(db, None))
                }),
            )),
            Type::Recursive(recursive) if recursive.materialization_kind(db).is_none() => Some((
                Constructor::Recursive(recursive.constructor(db)),
                recursive.arguments(db),
            )),
            Type::RecursiveVar(variable) if variable.materialization_kind(db).is_none() => {
                let Constructor::Recursive(scope) = self.scope? else {
                    return None;
                };
                let recursive = scope.reference(db, variable);
                Some((
                    Constructor::Recursive(recursive.constructor(db)),
                    recursive.arguments(db),
                ))
            }
            _ => None,
        }
    }

    fn identity_arguments(self, db: &'db dyn Db) -> Vec<Self> {
        self.scope
            .and_then(|scope| scope.parameters(db))
            .into_iter()
            .flat_map(|context| context.variables(db))
            .map(|variable| self.with_type(Type::TypeVar(variable)))
            .collect()
    }
}

/// `source[i] <= target[j]`, with source in the right scope when `reversed`.
/// Ordering by the directed indices keeps an atom adjacent to its scope reversal.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
struct Requirement {
    source: usize,
    target: usize,
    reversed: bool,
}

/// A positive Boolean formula in a shared, reduced ordered decision diagram.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
enum Requirements {
    Unproven,
    Always,
    Decision(usize),
}

impl Requirements {
    fn always() -> Self {
        Self::Always
    }

    fn unproven() -> Self {
        Self::Unproven
    }

    fn from_bool(proven: bool) -> Self {
        if proven {
            Self::always()
        } else {
            Self::unproven()
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
struct Decision {
    atom: Requirement,
    if_false: Requirements,
    if_true: Requirements,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
enum Connective {
    And,
    Or,
}

/// Canonical formulas avoid enumerating the Cartesian product of union alternatives.
#[derive(Default)]
struct RequirementFormulas {
    nodes: Vec<Decision>,
    indices: FxHashMap<Decision, Requirements>,
    combinations: FxHashMap<(Connective, Requirements, Requirements), Requirements>,
    reversals: FxHashMap<Requirements, Requirements>,
}

impl RequirementFormulas {
    fn decision(&mut self, node: Decision) -> Requirements {
        if node.if_false == node.if_true {
            return node.if_true;
        }
        *self.indices.entry(node).or_insert_with(|| {
            let result = Requirements::Decision(self.nodes.len());
            self.nodes.push(node);
            result
        })
    }

    fn parameter(&mut self, atom: Requirement) -> Requirements {
        self.decision(Decision {
            atom,
            if_false: Requirements::Unproven,
            if_true: Requirements::Always,
        })
    }

    fn and(&mut self, left: Requirements, right: Requirements) -> Requirements {
        self.combine(Connective::And, left, right)
    }

    fn or(&mut self, left: Requirements, right: Requirements) -> Requirements {
        self.combine(Connective::Or, left, right)
    }

    fn combine(
        &mut self,
        connective: Connective,
        left: Requirements,
        right: Requirements,
    ) -> Requirements {
        if left == right {
            return left;
        }
        let identity = match connective {
            Connective::And => Requirements::Always,
            Connective::Or => Requirements::Unproven,
        };
        let (left_node, right_node) = match (left, right) {
            (Requirements::Decision(left), Requirements::Decision(right)) => {
                (self.nodes[left], self.nodes[right])
            }
            (Requirements::Always | Requirements::Unproven, _) => {
                return if left == identity { right } else { left };
            }
            (_, Requirements::Always | Requirements::Unproven) => {
                return if right == identity { left } else { right };
            }
        };
        let key = (connective, left.min(right), left.max(right));
        if let Some(&result) = self.combinations.get(&key) {
            return result;
        }
        let atom = left_node.atom.min(right_node.atom);
        let branches = [(left, left_node), (right, right_node)].map(|(formula, node)| {
            if node.atom == atom {
                [node.if_false, node.if_true]
            } else {
                [formula, formula]
            }
        });
        let if_false = self.combine(connective, branches[0][0], branches[1][0]);
        let if_true = self.combine(connective, branches[0][1], branches[1][1]);
        let result = self.decision(Decision {
            atom,
            if_false,
            if_true,
        });
        self.combinations.insert(key, result);
        result
    }

    /// Children precede their parents in the arena, so index order is a topological order.
    fn reachable(&self, formula: Requirements) -> Vec<(Requirements, Decision)> {
        let mut pending = vec![formula];
        let mut seen = FxHashSet::default();
        while let Some(formula) = pending.pop() {
            if let Requirements::Decision(index) = formula
                && seen.insert(index)
            {
                let node = self.nodes[index];
                pending.extend([node.if_false, node.if_true]);
            }
        }
        let mut indices: Vec<_> = seen.into_iter().collect();
        indices.sort_unstable();
        indices
            .into_iter()
            .map(|index| (Requirements::Decision(index), self.nodes[index]))
            .collect()
    }

    fn substitute(
        &mut self,
        formula: Requirements,
        nodes: &[(Requirements, Decision)],
        substitutions: &FxHashMap<Requirement, Requirements>,
    ) -> Requirements {
        let mut results = FxHashMap::from_iter([
            (Requirements::Unproven, Requirements::Unproven),
            (Requirements::Always, Requirements::Always),
        ]);
        for &(id, node) in nodes {
            // For positive formulas, the false branch implies the true branch. Hence
            // (atom AND true_branch) OR false_branch performs substitution without negation.
            let if_true = self.and(substitutions[&node.atom], results[&node.if_true]);
            let result = self.or(if_true, results[&node.if_false]);
            results.insert(id, result);
        }
        results[&formula]
    }

    fn reverse(&mut self, formula: Requirements) -> Requirements {
        if let Some(&result) = self.reversals.get(&formula) {
            return result;
        }
        let nodes = self.reachable(formula);
        let mut substitutions = FxHashMap::default();
        for (_, node) in &nodes {
            substitutions.entry(node.atom).or_insert_with(|| {
                self.parameter(Requirement {
                    reversed: !node.atom.reversed,
                    ..node.atom
                })
            });
        }
        let result = self.substitute(formula, &nodes, &substitutions);
        self.reversals.insert(formula, result);
        self.reversals.insert(result, formula);
        result
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
struct TypePair<'db> {
    left: ScopedType<'db>,
    right: ScopedType<'db>,
}

impl<'db> TypePair<'db> {
    fn with_types(self, left: Type<'db>, right: Type<'db>) -> Self {
        Self {
            left: self.left.with_type(left),
            right: self.right.with_type(right),
        }
    }

    fn reverse(self) -> Self {
        Self {
            left: self.right,
            right: self.left,
        }
    }
}

struct ParametricRelation<'a, 'db> {
    db: &'db dyn Db,
    env: &'a ProgramEnvironment<'db>,
    indices: FxHashMap<TypePair<'db>, usize>,
    states: Vec<(TypePair<'db>, Requirements)>,
    formulas: RequirementFormulas,
}

impl<'db> ParametricRelation<'_, 'db> {
    fn require(&mut self, pair: TypePair<'db>) -> Requirements {
        let index = *self.indices.entry(pair).or_insert_with(|| {
            let index = self.states.len();
            self.states.push((pair, Requirements::always()));
            index
        });
        self.states[index].1
    }

    fn prove(&mut self, source: Type<'db>, target: Type<'db>) -> bool {
        self.require(TypePair {
            left: ScopedType {
                ty: source,
                scope: None,
            },
            right: ScopedType {
                ty: target,
                scope: None,
            },
        });
        // Start with the greatest candidate relation and monotonically strengthen its
        // requirements. There are finitely many scoped subexpressions and parameter pairs;
        // the decision diagram gives every positive Boolean formula a canonical representation.
        loop {
            let count = self.states.len();
            let mut changed = false;
            for index in 0..count {
                let requirements = self.decompose(self.states[index].0);
                if self.states[index].1 != requirements {
                    self.states[index].1 = requirements;
                    changed = true;
                }
            }
            if !changed && self.states.len() == count {
                // The root has no formal parameters, so its formula is a Boolean constant.
                return self.states[0].1 == Requirements::always();
            }
        }
    }

    fn compose(&mut self, pair: TypePair<'db>) -> Option<Requirements> {
        let db = self.db;
        let left = pair.left.reference(db);
        let right = pair.right.reference(db);
        if left.is_none() && right.is_none() {
            return None;
        }
        let mut bodies = pair;
        let mut arguments = [
            pair.left.identity_arguments(db),
            pair.right.identity_arguments(db),
        ];
        for (index, reference) in [left, right].into_iter().enumerate() {
            let Some((constructor, specialization)) = reference else {
                continue;
            };
            // Coinduction requires contractive definitions, including during Salsa recovery.
            if constructor.ty().has_unguarded_alias_cycle(db) {
                return Some(Requirements::unproven());
            }
            if let Some(specialization) = specialization
                && (specialization.materialization_kind(db).is_some()
                    || specialization.tuple(db).is_some())
            {
                return Some(Requirements::unproven());
            }
            let operand = if index == 0 { pair.left } else { pair.right };
            arguments[index] = specialization
                .into_iter()
                .flat_map(|specialization| specialization.types(db))
                .map(|&ty| operand.with_type(ty))
                .collect();
            if index == 0 {
                bodies.left = constructor.body(db);
            } else {
                bodies.right = constructor.body(db);
            }
        }
        let summary = self.require(bodies);
        let nodes = self.formulas.reachable(summary);
        let mut substitutions = FxHashMap::default();
        for (_, node) in &nodes {
            substitutions.entry(node.atom).or_insert_with(|| {
                let [source, target] = if node.atom.reversed { [1, 0] } else { [0, 1] };
                let (Some(&left), Some(&right)) = (
                    arguments[source].get(node.atom.source),
                    arguments[target].get(node.atom.target),
                ) else {
                    return Requirements::unproven();
                };
                let requirement = self.require(TypePair { left, right });
                if node.atom.reversed {
                    self.formulas.reverse(requirement)
                } else {
                    requirement
                }
            });
        }
        Some(self.formulas.substitute(summary, &nodes, &substitutions))
    }

    fn decompose(&mut self, pair: TypePair<'db>) -> Requirements {
        let db = self.db;
        let (left, right) = (pair.left.ty, pair.right.ty);
        if matches!(left, Type::Never)
            || right
                .as_nominal_instance()
                .is_some_and(NominalInstanceType::is_object)
        {
            return Requirements::always();
        }
        match (left, right) {
            (Type::Union(union), _) => {
                let mut result = Requirements::always();
                for &element in union.elements(db) {
                    let requirement = self.require(pair.with_types(element, right));
                    result = self.formulas.and(result, requirement);
                }
                result
            }
            (_, Type::Union(union)) => {
                let mut result = Requirements::unproven();
                for &element in union.elements(db) {
                    let requirement = self.require(pair.with_types(left, element));
                    result = self.formulas.or(result, requirement);
                }
                result
            }
            (_, Type::Intersection(intersection)) if intersection.negative(db).is_empty() => {
                let mut result = Requirements::always();
                for &element in intersection.positive(db) {
                    let requirement = self.require(pair.with_types(left, element));
                    result = self.formulas.and(result, requirement);
                }
                result
            }
            (Type::Intersection(intersection), _) => {
                let mut result = Requirements::unproven();
                for &element in intersection.positive(db) {
                    let requirement = self.require(pair.with_types(element, right));
                    result = self.formulas.or(result, requirement);
                }
                result
            }
            (_, _) if let Some(result) = self.compose(pair) => result,
            (Type::TypeVar(source), Type::TypeVar(target)) => {
                match (pair.left.parameter(db), pair.right.parameter(db)) {
                    (Some(left), Some(right)) => self.formulas.parameter(Requirement {
                        source: left,
                        target: right,
                        reversed: false,
                    }),
                    (None, None) => Requirements::from_bool(source.is_same_typevar_as(db, target)),
                    _ => Requirements::unproven(),
                }
            }
            (Type::NominalInstance(source), Type::NominalInstance(target)) => {
                match (
                    left.exact_tuple_instance_spec(db),
                    right.exact_tuple_instance_spec(db),
                ) {
                    (Some(left), Some(right)) => return self.tuples(pair, &left, &right),
                    (Some(_), None) | (None, Some(_)) => return Requirements::unproven(),
                    (None, None) => {}
                }
                let source = source.class(db, self.env);
                let target = target.class(db, self.env);
                if let (ClassType::Generic(source), ClassType::Generic(target)) = (source, target)
                    && source.origin(db) == target.origin(db)
                {
                    return self.specializations(
                        pair,
                        source.specialization(db),
                        target.specialization(db),
                    );
                }
                Requirements::from_bool(
                    matches!(target, ClassType::NonGeneric(_))
                        && source
                            .class_literal(db)
                            .iter_mro(db)
                            .filter_map(ClassBase::into_class)
                            .any(|base| base.class_literal(db) == target.class_literal(db)),
                )
            }
            (Type::Callable(source), Type::Callable(target)) => {
                self.callables(pair, source, target)
            }
            (Type::LiteralValue(_), Type::LiteralValue(_))
            | (Type::ClassLiteral(_), Type::ClassLiteral(_)) => {
                Requirements::from_bool(left == right)
            }
            (Type::LiteralValue(literal), Type::NominalInstance(_)) => {
                self.require(pair.with_types(literal.fallback_instance(db, self.env), right))
            }
            _ => Requirements::unproven(),
        }
    }

    fn callables(
        &mut self,
        pair: TypePair<'db>,
        left: CallableType<'db>,
        right: CallableType<'db>,
    ) -> Requirements {
        let db = self.db;
        if left.kind(db) != right.kind(db) {
            return Requirements::unproven();
        }
        let mut result = Requirements::always();
        for target in &right.signatures(db).overloads {
            let mut alternatives = Requirements::unproven();
            for source in &left.signatures(db).overloads {
                let requirement = self.signatures(pair, source, target);
                alternatives = self.formulas.or(alternatives, requirement);
            }
            result = self.formulas.and(result, alternatives);
        }
        result
    }

    fn signatures(
        &mut self,
        pair: TypePair<'db>,
        source: &Signature<'db>,
        target: &Signature<'db>,
    ) -> Requirements {
        if !source.is_non_generic()
            || !target.is_non_generic()
            || !source.supports_overload_aggregation()
            || !target.supports_overload_aggregation()
            || !source.parameters().is_standard()
            || !target.parameters().is_standard()
            || source.parameters().len() != target.parameters().len()
        {
            return Requirements::unproven();
        }
        let mut result = self.require(pair.with_types(source.return_ty, target.return_ty));
        for (source, target) in source.parameters().iter().zip(target.parameters()) {
            if discriminant(source.kind()) != discriminant(target.kind())
                || source.keyword_name() != target.keyword_name()
                || (target.has_default() && !source.has_default())
                || source.has_starred_annotation()
                || target.has_starred_annotation()
            {
                return Requirements::unproven();
            }
            let requirement = self.require(
                pair.with_types(source.annotated_type(), target.annotated_type())
                    .reverse(),
            );
            let requirement = self.formulas.reverse(requirement);
            result = self.formulas.and(result, requirement);
        }
        result
    }

    fn specializations(
        &mut self,
        pair: TypePair<'db>,
        left: Specialization<'db>,
        right: Specialization<'db>,
    ) -> Requirements {
        let db = self.db;
        if left.generic_context(db) != right.generic_context(db)
            || left.materialization_kind(db).is_some()
            || right.materialization_kind(db).is_some()
            || left.tuple(db).is_some()
            || right.tuple(db).is_some()
        {
            return Requirements::unproven();
        }
        let mut result = Requirements::always();
        for ((variable, &left), &right) in left
            .generic_context(db)
            .variables(db)
            .zip(left.types(db))
            .zip(right.types(db))
        {
            let pair = pair.with_types(left, right);
            let variance = specialization_variance(db, variable);
            if matches!(
                variance,
                TypeVarVariance::Covariant | TypeVarVariance::Invariant
            ) {
                let requirement = self.require(pair);
                result = self.formulas.and(result, requirement);
            }
            if matches!(
                variance,
                TypeVarVariance::Contravariant | TypeVarVariance::Invariant
            ) {
                let requirement = self.require(pair.reverse());
                let requirement = self.formulas.reverse(requirement);
                result = self.formulas.and(result, requirement);
            }
        }
        result
    }

    fn tuples(
        &mut self,
        pair: TypePair<'db>,
        left: &TupleSpec<'db>,
        right: &TupleSpec<'db>,
    ) -> Requirements {
        let (Some(left), Some(right)) = (TupleShape::new(left), TupleShape::new(right)) else {
            return Requirements::unproven();
        };
        if left.minimum() < right.minimum()
            || (right.middle.is_none()
                && (left.middle.is_some() || left.minimum() != right.minimum()))
        {
            return Requirements::unproven();
        }
        // Beyond the combined fixed portions, the only new overlap is middle/middle.
        let maximum = if left.middle.is_some() {
            left.minimum() + right.minimum() + 1
        } else {
            left.minimum()
        };
        let mut result = Requirements::always();
        for length in left.minimum()..=maximum {
            for (left, right) in left.elements(length).zip(right.elements(length)) {
                let requirement = self.require(pair.with_types(left, right));
                result = self.formulas.and(result, requirement);
            }
        }
        result
    }
}

struct TupleShape<'a, 'db> {
    prefix: &'a [Type<'db>],
    middle: Option<Type<'db>>,
    suffix: &'a [Type<'db>],
}

impl<'a, 'db> TupleShape<'a, 'db> {
    fn new(tuple: &'a TupleSpec<'db>) -> Option<Self> {
        Some(match tuple {
            Tuple::Fixed(tuple) => Self {
                prefix: tuple.all_elements(),
                middle: None,
                suffix: &[],
            },
            Tuple::Variable(tuple) => {
                let VariableSegment::Homogeneous(middle) = tuple.variable() else {
                    return None;
                };
                Self {
                    prefix: tuple.prefix_elements(),
                    middle: Some(middle),
                    suffix: tuple.suffix_elements(),
                }
            }
        })
    }

    fn minimum(&self) -> usize {
        self.prefix.len() + self.suffix.len()
    }

    fn elements(&self, length: usize) -> impl Iterator<Item = Type<'db>> + '_ {
        self.prefix
            .iter()
            .copied()
            .chain(
                self.middle
                    .into_iter()
                    .cycle()
                    .take(length - self.minimum()),
            )
            .chain(self.suffix.iter().copied())
    }
}

impl<'db> Type<'db> {
    /// Prove a sufficient subtype relation without reentering the semantic relation checker.
    ///
    /// Assignability cannot replace subtyping in the premises: separate comparisons may
    /// choose incompatible materializations of `Any`. This proof also deliberately leaves
    /// equivalence between gradual types to the ordinary relation checker.
    pub(in crate::types) fn is_parametric_subtype_of(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        target: Type<'db>,
    ) -> bool {
        ParametricRelation {
            db,
            env,
            indices: FxHashMap::default(),
            states: Vec::new(),
            formulas: RequirementFormulas::default(),
        }
        .prove(self, target)
    }
}
