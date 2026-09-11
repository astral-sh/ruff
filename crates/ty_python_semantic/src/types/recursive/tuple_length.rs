//! Length equations distinguish recursive elements from recursive tuple expansions.
//! Constructors and alternative bindings form a finite graph. Unsupported operations
//! leave unknown lengths on the existing inference path.

use std::collections::BTreeSet;

use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;
use ty_python_core::definition::{DefinitionKind, DefinitionState};
use ty_python_core::place::PlaceExpr;
use ty_python_core::scope::ScopeId;
use ty_python_core::semantic_index;

use super::{InferenceQuery, InferenceSource};
use crate::place::Place;
use crate::place::loop_header_reachability;
use crate::place_load::{
    PlaceLoadMode, PlaceLoadResolutionStep, PlaceLoadSourceKind, resolve_place_load,
};
use crate::types::class::{ImplicitAttributeName, MethodDecorator};
use crate::types::class_base::ClassBase;
use crate::types::graph::DependencyGraph;
use crate::types::infer::{
    infer_definition_types, infer_expression_types, infer_expression_types_impl,
};
use crate::types::tuple::{TupleLength, TupleSpec, TupleType};
use crate::types::unpacker::unpacked_assignment_value;
use crate::types::{Type, TypeContext, UnionType};
use crate::{Db, FxIndexMap, ProgramEnvironment};

/// A finite system of nonnegative sums and maxima, before unpacking erases its dependencies.
#[derive(Default)]
pub(in crate::types) struct TupleLengthAnalysis<'db> {
    nodes: Vec<LengthNode>,
    inputs: FxIndexMap<InferenceSource<'db>, usize>,
}

#[derive(Debug)]
enum LengthNode {
    Constant(usize),
    Sum(Vec<usize>),
    Maximum(Vec<usize>),
    Unknown,
}

enum LengthBounds {
    Finite(BTreeSet<usize>),
    Unbounded,
    Unknown,
}

impl LengthNode {
    fn children(&self) -> &[usize] {
        match self {
            Self::Sum(children) | Self::Maximum(children) => children,
            Self::Constant(_) | Self::Unknown => &[],
        }
    }
}

impl<'db> TupleLengthAnalysis<'db> {
    /// Preserve each possible finite length, or widen an unbounded expansion while retaining its ends.
    pub(in crate::types) fn normalize(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        scope: ScopeId<'db>,
        tuple: &ast::ExprTuple,
        spec: TupleSpec<'db>,
        inferred_type: &impl Fn(&ast::Expr) -> Type<'db>,
    ) -> Type<'db> {
        let Some(prefix) = tuple.elts.iter().position(ast::Expr::is_starred_expr) else {
            return Type::tuple(TupleType::new(db, env, &spec));
        };
        let suffix = tuple
            .elts
            .iter()
            .rev()
            .position(ast::Expr::is_starred_expr)
            .unwrap_or(0);
        let mut analysis = Self::default();
        let root = analysis.sequence(db, scope, &tuple.elts, inferred_type);
        analysis.collect(db);
        match &analysis.bounds()[root] {
            LengthBounds::Unbounded => {
                let spec = spec
                    .resize(db, env, TupleLength::Variable(prefix, suffix))
                    .unwrap_or(spec);
                Type::tuple(TupleType::new(db, env, &spec))
            }
            LengthBounds::Finite(lengths) => {
                let alternatives: Vec<_> = lengths
                    .iter()
                    .filter_map(|length| {
                        spec.resize(db, env, TupleLength::Fixed(*length))
                            .ok()
                            .map(|spec| Type::tuple(TupleType::new(db, env, &spec)))
                    })
                    .collect();
                if alternatives.is_empty() {
                    Type::tuple(TupleType::new(db, env, &spec))
                } else {
                    UnionType::from_elements_leave_aliases(db, env, alternatives)
                }
            }
            LengthBounds::Unknown => Type::tuple(TupleType::new(db, env, &spec)),
        }
    }

    fn collect(&mut self, db: &'db dyn Db) {
        let mut cursor = 0;
        while let Some((&key, &node)) = self.inputs.get_index(cursor) {
            cursor += 1;
            let body = self.equation(db, key);
            self.nodes[node] = LengthNode::Maximum(vec![body]);
        }
    }

    fn push(&mut self, node: LengthNode) -> usize {
        let index = self.nodes.len();
        self.nodes.push(node);
        index
    }

    fn sequence(
        &mut self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
        elements: &[ast::Expr],
        inferred_type: &impl Fn(&ast::Expr) -> Type<'db>,
    ) -> usize {
        let mut children = Vec::new();
        let mut fixed = 0;
        for element in elements {
            if let ast::Expr::Starred(starred) = element {
                let length = match starred.value.as_ref() {
                    // A literal is consumed immediately. A list reached through a binding
                    // may have been mutated, so its initializer cannot establish its length.
                    ast::Expr::List(list) => self.sequence(db, scope, &list.elts, inferred_type),
                    value => self.expression(db, scope, value, inferred_type),
                };
                children.push(length);
            } else {
                // An entire recursive value still occupies one tuple position.
                fixed += 1;
            }
        }
        children.push(self.push(LengthNode::Constant(fixed)));
        self.push(LengthNode::Sum(children))
    }

    fn expression(
        &mut self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
        expression: &ast::Expr,
        inferred_type: &impl Fn(&ast::Expr) -> Type<'db>,
    ) -> usize {
        match expression {
            ast::Expr::Tuple(tuple) => self.sequence(db, scope, &tuple.elts, inferred_type),
            ast::Expr::If(if_expression) => {
                let body = self.expression(db, scope, &if_expression.body, inferred_type);
                let otherwise = self.expression(db, scope, &if_expression.orelse, inferred_type);
                self.push(LengthNode::Maximum(vec![body, otherwise]))
            }
            ast::Expr::Attribute(attribute) => {
                let env = ProgramEnvironment::from_scope(scope);
                let receiver = inferred_type(&attribute.value);
                // Stored fields retain their equation references before descriptor binding
                // and flow narrowing. A data descriptor's writes do not define its read type.
                if receiver.custom_getattribute_may_affect_lookup(
                    db,
                    &env,
                    receiver.try_member_lookup(db, &env, &attribute.attr),
                ) || receiver
                    .class_member(db, &env, &attribute.attr)
                    .place
                    .ignore_possibly_undefined()
                    .is_some_and(|ty| !ty.is_definitely_non_data_descriptor(db, &env))
                {
                    return self.type_length(db, inferred_type(expression));
                }
                if let Some(class) = receiver.nominal_class(db, &env) {
                    let mut children = Vec::new();
                    for base in class.iter_mro(db) {
                        let ClassBase::Class(base) = base else {
                            continue;
                        };
                        let member = base.own_instance_member(db, &env, &attribute.attr);
                        if let Place::Defined(place) = member.inner.place
                            && place.origin.is_declared()
                            && place.is_definitely_defined()
                        {
                            return self.type_length(db, place.ty);
                        }
                        if let Some((base, _)) = base.static_class_literal(db) {
                            let key = ImplicitAttributeName::new(
                                db,
                                base.body_scope(db),
                                &attribute.attr.id,
                                MethodDecorator::None,
                            );
                            if !key.bindings(db).is_empty() {
                                children.push(
                                    self.input(InferenceSource(InferenceQuery::Attribute(key))),
                                );
                            }
                        }
                    }
                    if !children.is_empty() {
                        if let Some(default) = receiver
                            .class_member(db, &env, &attribute.attr)
                            .place
                            .ignore_possibly_undefined()
                        {
                            children.push(self.type_length(db, default));
                        }
                        return self.push(LengthNode::Maximum(children));
                    }
                }
                let children: Vec<_> = [
                    receiver.static_member(db, &env, &attribute.attr),
                    receiver.instance_member(db, &env, &attribute.attr).place,
                ]
                .into_iter()
                .filter_map(|place| place.ignore_possibly_undefined())
                .map(|ty| self.type_length(db, ty))
                .collect();
                if children.is_empty() {
                    self.type_length(db, inferred_type(expression))
                } else {
                    self.push(LengthNode::Maximum(children))
                }
            }
            ast::Expr::Name(name) if name.ctx == ast::ExprContext::Load => {
                let index = semantic_index(db, scope.program_file(db));
                let resolution = resolve_place_load(
                    db,
                    index,
                    scope,
                    PlaceExpr::from_expr_name(name),
                    PlaceLoadMode::AtExpression(name.into()),
                );
                let mut children = Vec::new();
                for step in resolution {
                    let PlaceLoadResolutionStep::Source(source) = step else {
                        continue;
                    };
                    let bindings = match source.kind {
                        PlaceLoadSourceKind::Bindings(bindings) => bindings,
                        PlaceLoadSourceKind::DefinitionsFromOwningScope { scope, id } => {
                            semantic_index(db, scope.program_file(db))
                                .use_def_map(scope.file_scope_id(db))
                                .reachable_bindings(id)
                        }
                        PlaceLoadSourceKind::Implicit(_) => break,
                    };
                    let mut definitely_bound = true;
                    for binding in bindings {
                        if let DefinitionState::Defined(definition) = binding.binding {
                            children.push(
                                self.input(InferenceSource(InferenceQuery::Binding(definition))),
                            );
                        } else {
                            definitely_bound = false;
                        }
                    }
                    if definitely_bound && !children.is_empty() {
                        break;
                    }
                }
                if children.is_empty() {
                    self.type_length(db, inferred_type(expression))
                } else {
                    self.push(LengthNode::Maximum(children))
                }
            }
            // Opaque operations do not supply constructor equations. Asking their
            // owning inference query for a provisional result creates an extra cycle.
            _ => self.push(LengthNode::Unknown),
        }
    }

    fn input(&mut self, key: InferenceSource<'db>) -> usize {
        if let Some(node) = self.inputs.get(&key) {
            return *node;
        }
        let node = self.push(LengthNode::Unknown);
        self.inputs.insert(key, node);
        node
    }

    fn type_length(&mut self, db: &'db dyn Db, ty: Type<'db>) -> usize {
        match ty {
            Type::Recursive(recursive) => {
                if let Some(key) = recursive.inference_key(db) {
                    // Promotion preserves the outer tuple length.
                    self.input(key.source)
                } else {
                    // Reading stored syntax does not solve the recursive type. Its
                    // nested elements are irrelevant to the length of the outer tuple.
                    self.type_length(db, recursive.body(db))
                }
            }
            Type::Union(union) => {
                let children = union
                    .elements(db)
                    .iter()
                    .map(|ty| self.type_length(db, *ty))
                    .collect();
                self.push(LengthNode::Maximum(children))
            }
            _ => {
                let node = ty
                    .exact_tuple_instance_spec(db)
                    .and_then(|spec| spec.len().maximum())
                    .map_or(LengthNode::Unknown, LengthNode::Constant);
                self.push(node)
            }
        }
    }

    fn equation(&mut self, db: &'db dyn Db, key: InferenceSource<'db>) -> usize {
        match key.0 {
            InferenceQuery::Member(member) => {
                let ty = member.equation(db);
                self.type_length(db, ty)
            }
            InferenceQuery::Expression(input) => {
                let expression = input.into_inner(db).0;
                let module = parsed_module(db, expression.python_file(db)).load(db);
                self.expression(
                    db,
                    expression.scope(db),
                    expression.node_ref(db).node(&module),
                    &|expression| {
                        infer_expression_types_impl(db, input).raw_expression_type(expression)
                    },
                )
            }
            InferenceQuery::Binding(definition) => {
                if let DefinitionKind::Assignment(assignment) = definition.kind(db) {
                    let module = parsed_module(db, definition.python_file(db)).load(db);
                    if let Some(unpack) = assignment.unpack() {
                        if let Some(value) = unpacked_assignment_value(
                            unpack.target(db, &module),
                            assignment.value(&module),
                            assignment.target(&module),
                        ) {
                            return self.expression(
                                db,
                                definition.scope(db),
                                value,
                                &|expression| {
                                    infer_expression_types(
                                        db,
                                        unpack.value(db).expression(),
                                        TypeContext::default(),
                                    )
                                    .raw_expression_type(expression)
                                },
                            );
                        }
                    } else {
                        return self.expression(
                            db,
                            definition.scope(db),
                            assignment.value(&module),
                            &|expression| {
                                infer_definition_types(db, definition).expression_type(expression)
                            },
                        );
                    }
                }
                if let DefinitionKind::LoopHeader(_) = definition.kind(db) {
                    let children = loop_header_reachability(db, definition)
                        .reachable_bindings
                        .iter()
                        .map(|binding| {
                            self.input(InferenceSource(InferenceQuery::Binding(binding.definition)))
                        })
                        .collect();
                    return self.push(LengthNode::Maximum(children));
                }
                if let DefinitionKind::NamedExpression(named) = definition.kind(db) {
                    let module = parsed_module(db, definition.python_file(db)).load(db);
                    return self.expression(
                        db,
                        definition.scope(db),
                        &named.node(&module).value,
                        &|expression| {
                            infer_definition_types(db, definition).expression_type(expression)
                        },
                    );
                }
                if let DefinitionKind::NestedBindings(nested) = definition.kind(db) {
                    let index = semantic_index(db, definition.program_file(db));
                    let children = nested
                        .visible_binding_sources(index, definition.file_scope(db))
                        .flatten()
                        .filter_map(|binding| {
                            let DefinitionState::Defined(definition) = binding.binding else {
                                return None;
                            };
                            Some(self.input(InferenceSource(InferenceQuery::Binding(definition))))
                        })
                        .collect();
                    return self.push(LengthNode::Maximum(children));
                }
                self.push(LengthNode::Unknown)
            }
            InferenceQuery::Attribute(attribute) => {
                let children = attribute
                    .bindings(db)
                    .into_iter()
                    .map(|definition| {
                        self.input(InferenceSource(InferenceQuery::Binding(definition)))
                    })
                    .collect();
                self.push(LengthNode::Maximum(children))
            }
        }
    }

    /// A cycle grows exactly when a sum adds a positive sibling along an edge in that cycle.
    /// Positivity is solved first: duplicating an always-empty tuple does not grow its length.
    fn bounds(&self) -> Vec<LengthBounds> {
        let graph = DependencyGraph::new(
            self.nodes
                .iter()
                .map(|node| node.children().to_vec())
                .collect(),
        );
        let mut positive: Vec<_> = self
            .nodes
            .iter()
            .map(|node| matches!(node, LengthNode::Constant(length) if *length > 0))
            .collect();
        let mut pending: Vec<_> = (0..self.nodes.len())
            .filter(|node| positive[*node])
            .collect();
        while let Some(child) = pending.pop() {
            for parent in graph.dependents(child) {
                if !std::mem::replace(&mut positive[*parent], true) {
                    pending.push(*parent);
                }
            }
        }

        let components = graph.components(0..self.nodes.len());
        let mut membership = vec![0; self.nodes.len()];
        for (index, component) in components.iter().enumerate() {
            for node in component {
                membership[*node] = index;
            }
        }
        let mut unbounded = vec![false; self.nodes.len()];
        for (index, component) in components.iter().enumerate() {
            let growing = component.iter().any(|node| {
                if self.nodes[*node]
                    .children()
                    .iter()
                    .any(|child| unbounded[*child])
                {
                    return true;
                }
                let LengthNode::Sum(children) = &self.nodes[*node] else {
                    return false;
                };
                let positive_count = children.iter().filter(|child| positive[**child]).count();
                children.iter().any(|child| {
                    membership[*child] == index && positive_count > usize::from(positive[*child])
                })
            });
            for node in component {
                unbounded[*node] = growing;
            }
        }
        // In a bounded component, every internal edge equates the upper bounds:
        // any positive increase on such an edge would have made the component unbounded.
        let mut maxima = vec![Some(0_usize); self.nodes.len()];
        for component in &components {
            let maximum = if unbounded[component[0]] {
                None
            } else {
                component.iter().try_fold(0, |maximum, node| {
                    let bound = match &self.nodes[*node] {
                        LengthNode::Constant(length) => Some(*length),
                        LengthNode::Unknown => None,
                        LengthNode::Sum(children) => children
                            .iter()
                            .try_fold(0_usize, |sum, child| sum.checked_add(maxima[*child]?)),
                        LengthNode::Maximum(children) => children
                            .iter()
                            .try_fold(0, |bound, child| Some(bound.max(maxima[*child]?))),
                    }?;
                    Some(maximum.max(bound))
                })
            };
            for node in component {
                maxima[*node] = maximum;
            }
        }

        // Enumerate lengths only after establishing a finite upper bound. Starting
        // from empty sets also keeps unproductive cycles distinct from empty tuples.
        let mut lengths = vec![BTreeSet::new(); self.nodes.len()];
        for component in &components {
            if maxima[component[0]].is_none() {
                continue;
            }
            loop {
                let mut changed = false;
                for node in component {
                    let possible = match &self.nodes[*node] {
                        LengthNode::Constant(length) => BTreeSet::from([*length]),
                        LengthNode::Unknown => BTreeSet::new(),
                        LengthNode::Sum(children) => {
                            children
                                .iter()
                                .fold(BTreeSet::from([0_usize]), |sums, child| {
                                    sums.iter()
                                        .flat_map(|sum| {
                                            lengths[*child]
                                                .iter()
                                                .filter_map(|length| sum.checked_add(*length))
                                        })
                                        .collect()
                                })
                        }
                        LengthNode::Maximum(children) => children
                            .iter()
                            .flat_map(|child| lengths[*child].iter().copied())
                            .collect(),
                    };
                    let old_count = lengths[*node].len();
                    lengths[*node].extend(possible);
                    changed |= old_count != lengths[*node].len();
                }
                if !changed {
                    break;
                }
            }
        }
        unbounded
            .into_iter()
            .zip(lengths)
            .map(|(unbounded, lengths)| {
                if unbounded {
                    LengthBounds::Unbounded
                } else if lengths.is_empty() {
                    LengthBounds::Unknown
                } else {
                    LengthBounds::Finite(lengths)
                }
            })
            .collect()
    }
}
