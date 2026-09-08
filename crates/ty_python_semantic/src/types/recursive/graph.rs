//! Finite graphs of closed type equations, with recursive references confined to private bodies.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Rc;

use rustc_hash::FxHashMap;
use salsa::plumbing::AsId;
use ty_python_core::definition::Definition;
use ty_python_core::place::ScopedPlaceId;
use ty_python_core::scope::FileScopeId;

use super::{
    RecursiveGraph, RecursiveMapping, RecursiveOrigin, RecursiveReferences, RecursiveSubstitution,
    RecursiveType, RecursiveVar,
};
use crate::types::graph::DependencyGraph;
use crate::types::set_theoretic::NegativeIntersectionElements;
use crate::types::tuple::{Tuple, VariableSegment};
use crate::types::visitor::TypeKind;
use crate::types::{
    ApplyTypeMappingVisitor, BoundTypeVarIdentity, InternedType, IntersectionType,
    LiteralValueTypeKind, Type, TypeContext, TypeMapping, UnionType,
};
use crate::{Db, FxIndexSet, FxOrderSet, ProgramEnvironment};

/// Maps closed input types to graph nodes; input order does not determine the final node order.
#[derive(Debug, Eq, PartialEq)]
pub(super) struct RecursiveGraphBuilder<'db> {
    inputs: RefCell<FxIndexSet<Type<'db>>>,
    equations: FxHashMap<Type<'db>, Type<'db>>,
    variables: FxHashMap<BoundTypeVarIdentity<'db>, Type<'db>>,
}

impl get_size2::GetSize for RecursiveGraphBuilder<'_> {}

/// Closed types for the input roots and whether any root belongs to a cycle.
pub(super) struct GraphSolution<'db> {
    pub(super) types: Vec<Type<'db>>,
    pub(super) recursive: bool,
}

impl<'db> RecursiveGraphBuilder<'db> {
    /// Keep atoms inline and register other closed types as graph dependencies.
    pub(super) fn reference(&self, db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        let ty = match ty {
            Type::TypeVar(variable) => match self.variables.get(&variable.identity(db)) {
                Some(ty) => *ty,
                None => return ty,
            },
            _ if matches!(TypeKind::from(ty), TypeKind::Atomic) => return ty,
            _ => ty,
        };
        let (index, _) = self.inputs.borrow_mut().insert_full(ty);
        Type::RecursiveVar(RecursiveVar::new_internal(db, 0, index, None))
    }

    /// Build and minimize the reachable regular graph, then close each cyclic component.
    /// Constructor operations see closed inputs throughout extraction and mapping.
    pub(super) fn solve(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        roots: &[(Type<'db>, Type<'db>)],
    ) -> Option<GraphSolution<'db>> {
        Self {
            inputs: RefCell::new(roots.iter().map(|(root, _)| *root).collect()),
            equations: roots.iter().copied().collect(),
            variables: roots
                .iter()
                .filter_map(|(root, _)| {
                    let Type::TypeVar(variable) = root else {
                        return None;
                    };
                    Some((variable.identity(db), *root))
                })
                .collect(),
        }
        .finish(db, env, roots.len())
    }

    fn finish(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        root_count: usize,
    ) -> Option<GraphSolution<'db>> {
        let builder = self;
        let mapping =
            TypeMapping::Recursive(RecursiveMapping(RecursiveSubstitution::Extract(&builder)));
        let visitor = ApplyTypeMappingVisitor::new(env);
        let mut bodies = Vec::new();
        loop {
            let input = builder.inputs.borrow().get_index(bodies.len()).copied();
            let Some(input) = input else {
                break;
            };
            let body = if let Some(body) = builder.equations.get(&input) {
                builder.reference(db, *body)
            } else if let Type::Recursive(recursive) = input
                && matches!(recursive.origin(db), RecursiveOrigin::ConstraintSolution(_))
            {
                recursive.map_type(db, env, |body| builder.reference(db, body))
            } else {
                input.apply_type_mapping_children(db, &mapping, TypeContext::default(), &visitor)
            };
            bodies.push(body);
        }
        let mut root_indices: Vec<_> = (0..root_count).collect();
        Self::remove_forwarding(db, env, &mut bodies, &mut root_indices)?;
        let unguarded = DependencyGraph::new(
            bodies
                .iter()
                .map(|body| {
                    if matches!(body, Type::Union(_) | Type::Intersection(_)) {
                        RecursiveReferences::indices(db, env, *body)
                    } else {
                        Vec::new()
                    }
                })
                .collect(),
        );
        if unguarded
            .components(0..bodies.len())
            .iter()
            .any(|component| unguarded.is_cyclic(component))
        {
            return None;
        }

        loop {
            let previous_len = bodies.len();
            Self::minimize(db, env, &mut bodies, &mut root_indices);
            Self::remove_forwarding(db, env, &mut bodies, &mut root_indices)?;
            if bodies.len() == previous_len {
                break;
            }
        }
        let graph = DependencyGraph::new(
            bodies
                .iter()
                .map(|body| RecursiveReferences::indices(db, env, *body))
                .collect(),
        );
        let mut closed = vec![Type::Never; bodies.len()];
        let mut recursive_roots = vec![false; bodies.len()];
        for mut component in graph.components(0..bodies.len()) {
            component.sort_unstable();
            let cyclic = graph.is_cyclic(&component);
            if cyclic {
                for (local, global) in component.iter().enumerate() {
                    closed[*global] =
                        Type::RecursiveVar(RecursiveVar::new_internal(db, 0, local, None));
                    recursive_roots[*global] = true;
                }
            }
            let mapping =
                TypeMapping::Recursive(RecursiveMapping(RecursiveSubstitution::Rebuild(&closed)));
            let visitor = ApplyTypeMappingVisitor::new(env);
            let mut definitions: Vec<_> = component
                .iter()
                .map(|index| {
                    bodies[*index].apply_type_mapping_impl(
                        db,
                        &mapping,
                        TypeContext::default(),
                        &visitor,
                    )
                })
                .collect();
            if cyclic {
                // Canonical numbering is local to the component, so unrelated roots
                // and already-closed dependencies cannot change its interned identity.
                let mut entries: Vec<_> = (0..component.len()).collect();
                Self::minimize(db, env, &mut definitions, &mut entries);
                let graph = RecursiveGraph::new_internal(db, definitions.into_boxed_slice());
                for (local, index) in component.iter().enumerate() {
                    closed[*index] = Type::Recursive(RecursiveType::new_internal(
                        db,
                        RecursiveOrigin::ConstraintSolution(env.program(db)),
                        graph,
                        entries[local],
                        None,
                        None,
                    ));
                }
            } else {
                closed[component[0]] = definitions[0];
            }
        }
        Some(GraphSolution {
            recursive: root_indices.iter().any(|index| recursive_roots[*index]),
            types: root_indices
                .into_iter()
                .map(|index| closed[index])
                .collect(),
        })
    }

    /// Forwarding nodes have no constructor; remove them before comparing node labels.
    fn remove_forwarding(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bodies: &mut Vec<Type<'db>>,
        roots: &mut [usize],
    ) -> Option<()> {
        let mut targets = vec![None; bodies.len()];
        for start in 0..bodies.len() {
            let mut path = FxIndexSet::default();
            let mut current = start;
            let target = loop {
                if let Some(target) = targets[current] {
                    break target;
                }
                let Type::RecursiveVar(reference) = bodies[current] else {
                    break current;
                };
                if !path.insert(current) {
                    return None;
                }
                debug_assert_eq!(reference.depth(db), 0);
                current = reference.index(db);
            };
            targets[current] = Some(target);
            for index in path {
                targets[index] = Some(target);
            }
        }
        let kept: Vec<_> = (0..bodies.len())
            .filter(|index| targets[*index] == Some(*index))
            .collect();
        let mut indices = vec![0; bodies.len()];
        for (new, old) in kept.iter().enumerate() {
            indices[*old] = new;
        }
        let indices: Vec<_> = targets
            .into_iter()
            .map(|target| indices[target.expect("every forwarding chain was resolved")])
            .collect();
        let mapping =
            TypeMapping::Recursive(RecursiveMapping(RecursiveSubstitution::Reindex(&indices)));
        let visitor = ApplyTypeMappingVisitor::new(env);
        *bodies = kept
            .into_iter()
            .map(|index| {
                bodies[index].apply_type_mapping_impl(
                    db,
                    &mapping,
                    TypeContext::default(),
                    &visitor,
                )
            })
            .collect();
        for root in roots {
            *root = indices[*root];
        }
        Some(())
    }

    /// Refine structural equivalence classes until no class splits. Equality uses
    /// complete labels; origins of inferred binders are absent. Each round splits a
    /// class or terminates, so a graph with N nodes needs at most N rounds.
    fn minimize(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bodies: &mut Vec<Type<'db>>,
        roots: &mut [usize],
    ) {
        let mut keys = FxHashMap::default();
        let mut classes = vec![0; bodies.len()];
        let mut class_count = 1;
        loop {
            let mapping =
                TypeMapping::Recursive(RecursiveMapping(RecursiveSubstitution::Reindex(&classes)));
            let visitor = ApplyTypeMappingVisitor::new(env);
            let signatures: Vec<_> = bodies
                .iter()
                .enumerate()
                .map(|(index, body)| {
                    (
                        classes[index],
                        Self::ordered_shape(
                            db,
                            env,
                            &mut keys,
                            body.apply_type_mapping_impl(
                                db,
                                &mapping,
                                TypeContext::default(),
                                &visitor,
                            ),
                        ),
                    )
                })
                .collect();
            let mut ordered_labels: Vec<_> = signatures
                .iter()
                .map(|(class, label)| {
                    (
                        (
                            *class,
                            ShapeKey::new(db, env, &mut keys, *label),
                            InternedType::new(db, *label).as_id(),
                        ),
                        (*class, *label),
                    )
                })
                .collect();
            ordered_labels.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
            let mut labels: Vec<_> = ordered_labels.into_iter().map(|(_, label)| label).collect();
            labels.dedup();
            let lookup: FxHashMap<_, _> = labels
                .iter()
                .enumerate()
                .map(|(index, label)| (*label, index))
                .collect();
            classes = signatures
                .iter()
                .map(|signature| lookup[signature])
                .collect();
            if labels.len() == class_count {
                break;
            }
            class_count = labels.len();
        }
        let mapping =
            TypeMapping::Recursive(RecursiveMapping(RecursiveSubstitution::Reindex(&classes)));
        let visitor = ApplyTypeMappingVisitor::new(env);
        let mut representatives = vec![0; class_count];
        for (index, class) in classes.iter().enumerate() {
            representatives[*class] = index;
        }
        *bodies = representatives
            .into_iter()
            .map(|index| {
                Self::ordered_shape(
                    db,
                    env,
                    &mut keys,
                    bodies[index].apply_type_mapping_impl(
                        db,
                        &mapping,
                        TypeContext::default(),
                        &visitor,
                    ),
                )
            })
            .collect();
        for root in roots {
            *root = classes[*root];
        }
    }

    /// Order open constructor shapes before interning them. Reference numbers and
    /// constructor contents take precedence over incidental interner allocation order.
    fn ordered_shape(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        keys: &mut FxHashMap<Type<'db>, ShapeKey<'db>>,
        ty: Type<'db>,
    ) -> Type<'db> {
        match ty {
            Type::Union(union) => {
                let mut elements = union.elements(db).to_vec();
                Self::order_elements(db, env, keys, &mut elements);
                elements.dedup();
                match elements.as_slice() {
                    [element] => *element,
                    _ => Type::Union(UnionType::new(
                        db,
                        elements.into_boxed_slice(),
                        union.recursively_defined(db),
                    )),
                }
            }
            Type::Intersection(intersection) => {
                let mut positive: Vec<_> = intersection.positive(db).iter().copied().collect();
                Self::order_elements(db, env, keys, &mut positive);
                let mut negative: Vec<_> = intersection.negative(db).into_iter().copied().collect();
                Self::order_elements(db, env, keys, &mut negative);
                let mut negatives = NegativeIntersectionElements::default();
                for ty in negative {
                    negatives.insert(ty);
                }
                Type::Intersection(IntersectionType::new(
                    db,
                    positive.into_iter().collect::<FxOrderSet<_>>(),
                    negatives,
                ))
            }
            _ => ty,
        }
    }

    fn order_elements(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        keys: &mut FxHashMap<Type<'db>, ShapeKey<'db>>,
        elements: &mut [Type<'db>],
    ) {
        elements.sort_by_cached_key(|element| {
            (
                ShapeKey::new(db, env, keys, *element),
                InternedType::new(db, *element).as_id(),
            )
        });
    }
}

/// Structural ordering of equation labels. This is not type equality: declarations with
/// identical names and opaque types still need a database-local identity tie-breaker.
#[derive(Eq, PartialEq, Ord, PartialOrd)]
enum ShapeKeyData<'db> {
    Reference(u32, usize),
    LiteralString,
    Bool(bool),
    Int(i64),
    String(&'db str),
    Bytes(&'db [u8]),
    Enum(&'db str, Option<DeclarationKey<'db>>, &'db str),
    TypeVar(
        &'db str,
        Option<DeclarationKey<'db>>,
        Option<DeclarationKey<'db>>,
    ),
    Object,
    VersionInfo,
    Tuple(
        Box<[ShapeKey<'db>]>,
        Option<ShapeKey<'db>>,
        Box<[ShapeKey<'db>]>,
    ),
    Instance(&'db str, Option<DeclarationKey<'db>>, Box<[ShapeKey<'db>]>),
    Class(&'db str, Option<DeclarationKey<'db>>),
    GenericClass(ShapeKey<'db>, Box<[ShapeKey<'db>]>),
    SubclassOf(ShapeKey<'db>),
    Union(Box<[ShapeKey<'db>]>),
    Intersection(Box<[ShapeKey<'db>]>, Box<[ShapeKey<'db>]>),
    Recursive(Box<[ShapeKey<'db>]>, usize),
    Opaque,
}

impl<'db> ShapeKeyData<'db> {
    /// Read stored constructor structure only; do not unfold aliases or request inference.
    fn new(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        cache: &mut FxHashMap<Type<'db>, ShapeKey<'db>>,
        ty: Type<'db>,
    ) -> Self {
        match ty {
            Type::RecursiveVar(reference) => {
                Self::Reference(reference.depth(db), reference.index(db))
            }
            Type::LiteralValue(literal) => match literal.kind() {
                LiteralValueTypeKind::LiteralString => Self::LiteralString,
                LiteralValueTypeKind::Bool(value) => Self::Bool(value),
                LiteralValueTypeKind::Int(value) => Self::Int(value.as_i64()),
                LiteralValueTypeKind::String(value) => Self::String(value.value(db)),
                LiteralValueTypeKind::Bytes(value) => Self::Bytes(value.value(db)),
                LiteralValueTypeKind::Enum(value) => Self::Enum(
                    value.enum_class(db).name(db).as_str(),
                    value
                        .enum_class(db)
                        .definition(db)
                        .map(|definition| DeclarationKey::new(db, definition)),
                    value.name(db).as_str(),
                ),
            },
            Type::TypeVar(variable) => {
                let identity = variable.identity(db);
                Self::TypeVar(
                    variable.name(db).as_str(),
                    identity
                        .identity
                        .definition(db)
                        .map(|definition| DeclarationKey::new(db, definition)),
                    identity
                        .binding_context
                        .definition()
                        .map(|definition| DeclarationKey::new(db, definition)),
                )
            }
            Type::NominalInstance(instance) => {
                if instance.is_object() {
                    return Self::Object;
                }
                if instance.is_sys_version_info() {
                    return Self::VersionInfo;
                }
                if let Some(tuple) = instance.own_tuple_spec(db) {
                    return match tuple.as_ref() {
                        Tuple::Fixed(tuple) => Self::Tuple(
                            Self::types(db, env, cache, tuple.iter_all_elements()),
                            None,
                            Box::new([]),
                        ),
                        Tuple::Variable(tuple) => {
                            let variable = match tuple.variable() {
                                VariableSegment::Homogeneous(ty) => ty,
                                VariableSegment::TypeVarTuple(variable) => Type::TypeVar(variable),
                            };
                            Self::Tuple(
                                Self::types(db, env, cache, tuple.iter_prefix_elements()),
                                Some(ShapeKey::new(db, env, cache, variable)),
                                Self::types(db, env, cache, tuple.iter_suffix_elements()),
                            )
                        }
                    };
                }
                let (class, arguments) =
                    instance.class(db, env).class_literal_and_specialization(db);
                Self::Instance(
                    class.name(db).as_str(),
                    class
                        .definition(db)
                        .map(|definition| DeclarationKey::new(db, definition)),
                    Self::types(
                        db,
                        env,
                        cache,
                        arguments
                            .into_iter()
                            .flat_map(|arguments| arguments.types(db).iter().copied()),
                    ),
                )
            }
            Type::ClassLiteral(class) => Self::Class(
                class.name(db).as_str(),
                class
                    .definition(db)
                    .map(|definition| DeclarationKey::new(db, definition)),
            ),
            Type::GenericAlias(alias) => Self::GenericClass(
                ShapeKey::new(db, env, cache, Type::from(alias.origin(db))),
                Self::types(
                    db,
                    env,
                    cache,
                    alias.specialization(db).types(db).iter().copied(),
                ),
            ),
            Type::SubclassOf(subclass) => {
                Self::SubclassOf(ShapeKey::new(db, env, cache, Type::from(subclass)))
            }
            Type::Union(union) => Self::Union(Self::types(
                db,
                env,
                cache,
                union.elements(db).iter().copied(),
            )),
            Type::Intersection(intersection) => Self::Intersection(
                Self::types(db, env, cache, intersection.positive(db).iter().copied()),
                Self::types(db, env, cache, intersection.negative(db).iter().copied()),
            ),
            Type::Recursive(recursive) => Self::Recursive(
                Self::types(
                    db,
                    env,
                    cache,
                    recursive.graph(db).bodies(db).iter().copied(),
                ),
                recursive.entry(db),
            ),
            _ => Self::Opaque,
        }
    }

    fn types(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        cache: &mut FxHashMap<Type<'db>, ShapeKey<'db>>,
        types: impl IntoIterator<Item = Type<'db>>,
    ) -> Box<[ShapeKey<'db>]> {
        types
            .into_iter()
            .map(|ty| ShapeKey::new(db, env, cache, ty))
            .collect()
    }
}

/// Preserve graph sharing while comparing keys; equal subgraphs need no recursive comparison.
#[derive(Clone, Eq, PartialEq)]
struct ShapeKey<'db>(Rc<ShapeKeyData<'db>>);

impl<'db> ShapeKey<'db> {
    fn new(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        cache: &mut FxHashMap<Type<'db>, Self>,
        ty: Type<'db>,
    ) -> Self {
        if let Some(key) = cache.get(&ty) {
            return key.clone();
        }
        let key = Self(Rc::new(ShapeKeyData::new(db, env, cache, ty)));
        cache.insert(ty, key.clone());
        key
    }
}

impl Ord for ShapeKey<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        if Rc::ptr_eq(&self.0, &other.0) {
            Ordering::Equal
        } else {
            self.0.cmp(&other.0)
        }
    }
}

impl PartialOrd for ShapeKey<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// File-local identities distinguish equally named declarations without inspecting their bodies.
#[derive(Eq, PartialEq, Ord, PartialOrd)]
struct DeclarationKey<'db> {
    file: &'db str,
    scope: FileScopeId,
    place: ScopedPlaceId,
}

impl<'db> DeclarationKey<'db> {
    fn new(db: &'db dyn Db, definition: Definition<'db>) -> Self {
        Self {
            file: definition.file(db).path(db).as_str(),
            scope: definition.file_scope(db),
            place: definition.place(db),
        }
    }
}
