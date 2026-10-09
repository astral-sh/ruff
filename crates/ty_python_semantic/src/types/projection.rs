use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use ruff_python_ast::name::Name;
use rustc_hash::FxHashMap;

use super::cyclic::TypeIdentity;
use super::generics::{ApplySpecialization, Specialization};
use super::recursive::RecursiveOperation;
use super::relation::RelationContext;
use super::tuple::{Tuple, TupleShapePosition, VariableSegment};
use super::visitor::{TypeKind, TypeVisitor, walk_non_atomic_type};
use super::{
    ApplyTypeMappingVisitor, BindingContext, CallableType, ClassBase, ClassLiteral, ClassType,
    Parameter, SelfBinding, StaticClassLiteral, SubclassOfInner, Type, TypeContext, TypeMapping,
    TypeVarBoundOrConstraints,
};
use crate::{Db, ProgramEnvironment};

/// A position in a structural type expression, independent of the closed value at that position.
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) enum ObservationEdge {
    Identity,
    TupleElement(usize),
    TupleSuffix(usize),
    TupleVariable,
    TuplePart(usize),
    TupleStoredPart(usize),
    DeferredArgument,
    DeferredDomain,
    TypeVarUpperBound,
    TypeVarConstraint(usize),
    UnionElement(usize),
    IntersectionPositive(usize),
    IntersectionNegative(usize),
    GenericArgument(usize),
    SpecializationTuple,
    FunctionImplementationCallable(usize),
    UnderlyingFunction,
    PropertyGetter,
    IntrinsicMember(Name),
    EnumMember(Name),
    ClassView,
    MetaType,
    ClassBase(usize),
    ClassMetaclassInstance,
    /// The guaranteed `type` base of an otherwise gradual metaclass.
    GradualMetaclassBase,
    SubclassInstance,
    TransposedSubclassVariable,
    CallableOverload(usize),
    CallableParameter {
        overload: usize,
        parameter: usize,
    },
    CallableReturn {
        overload: usize,
    },
    CallableReceiver {
        overload: usize,
        relation: usize,
        annotation: bool,
    },
    TypedDictField(Name),
    TypedDictExtraItems,
    ProtocolMemberRead {
        name: Name,
        class_access: bool,
    },
    ProtocolMemberWrite {
        name: Name,
        class_access: bool,
    },
}

impl ObservationEdge {
    fn is_contravariant(&self) -> bool {
        matches!(
            self,
            Self::CallableParameter { .. }
                | Self::IntersectionNegative(_)
                | Self::ProtocolMemberWrite { .. }
        )
    }
}

/// A finite body node, or a projection into the value substituted for that node.
/// Parameter projections describe descent into an argument, not a recursive edge of the body.
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct ExpressionNode<'db> {
    pub(super) template: Type<'db>,
    path: Box<[ObservationEdge]>,
}

impl<'db> From<Type<'db>> for ExpressionNode<'db> {
    fn from(template: Type<'db>) -> Self {
        Self {
            template,
            path: Box::default(),
        }
    }
}

/// The two distinct bindings performed when a previously captured callable is used.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct CallableSelfBinding<'db> {
    pub(super) receiver: Type<'db>,
    pub(super) self_type: Type<'db>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
enum CallableBindingMode {
    /// Apply a previously captured receiver to retained constraints.
    Apply,
    /// Consume the first parameter and retain any receiver constraint.
    Capture,
}

impl CallableBindingMode {
    fn bind<'db>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        callable: CallableType<'db>,
        binding: CallableSelfBinding<'db>,
    ) -> CallableType<'db> {
        match self {
            Self::Apply => {
                callable.apply_self_with_receiver(db, env, binding.receiver, binding.self_type)
            }
            Self::Capture => callable.bind_self(db, env, binding.receiver, binding.self_type),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
enum ObservedOperationKind<'db> {
    Mapping(RecursiveOperation<'db>),
    CallableBinding {
        callable: CallableType<'db>,
        binding: CallableSelfBinding<'db>,
        path_depth: usize,
        mode: CallableBindingMode,
    },
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct ObservedOperation<'db> {
    operation: ObservedOperationKind<'db>,
    contravariant: bool,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct ObservedTypeOrigin<'db> {
    pub(super) constructor: TypeIdentity<'db>,
    pub(super) application: Type<'db>,
    pub(super) node: ExpressionNode<'db>,
    pub(super) operations: Box<[ObservedOperation<'db>]>,
}

/// An immutable expression recipe carried by a stored inference bound.
/// It contains declaration paths and substitutions, but no proof session or assumptions.
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct ObservationRecipe<'db> {
    ty: Type<'db>,
    origin: Option<Arc<ObservedTypeOrigin<'db>>>,
    shape: ObservationRecipeShape<'db>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
enum ObservationRecipeShape<'db> {
    Root,
    Expression,
    Normalized(Arc<[ObservationRecipe<'db>]>),
    Merged(Arc<[ObservationRecipe<'db>]>),
    Constructed(Arc<[(ObservationEdge, ObservationRecipe<'db>)]>),
    Unresolved(Arc<[Arc<ObservedTypeOrigin<'db>>]>),
}

impl<'db> ObservationRecipe<'db> {
    pub(super) fn observe(&self) -> ObservedType<'db> {
        ObservationBuilder::default().observe(self)
    }
}

/// A stored bound retains exact expression recipes where available. An unknown derivative
/// retains only its input occurrence, independently of the bound's temporary output type.
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) enum BoundSourceRecipe<'db> {
    Expression(ObservationRecipe<'db>),
    Dependency(Arc<ObservedTypeOrigin<'db>>),
}

impl<'db> BoundSourceRecipe<'db> {
    pub(super) fn from_observed(observed: &ObservedType<'db>) -> Box<[Self]> {
        match &observed.shape {
            ObservedShape::Unresolved(origins) => origins
                .iter()
                .map(|origin| Self::Dependency(Arc::new((**origin).clone())))
                .collect(),
            _ => Box::new([Self::Expression(observed.recipe())]),
        }
    }

    pub(super) fn observe(&self, ty: Type<'db>) -> ObservedType<'db> {
        match self {
            Self::Expression(recipe) => recipe.observe().unchanged_or_unresolved(ty),
            Self::Dependency(origin) => ObservedType {
                ty,
                origin: None,
                shape: ObservedShape::Unresolved([Rc::new((**origin).clone())].into()),
            },
        }
    }
}

/// An explicit operand of a recursive proof. Child expressions are selected by an edge in
/// its declaration body; their identity is never inferred from an equal resulting type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ObservedType<'db> {
    pub(super) ty: Type<'db>,
    origin: Option<Rc<ObservedTypeOrigin<'db>>>,
    shape: ObservedShape<'db>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ObservedShape<'db> {
    Root,
    Expression,
    Normalized(Rc<[ObservedType<'db>]>),
    Merged(Rc<[ObservedType<'db>]>),
    /// A result assembled from separate observed inputs retains their exact stored positions.
    Constructed(Rc<[(ObservationEdge, ObservedType<'db>)]>),
    /// A semantic helper has not exposed the edge producing its result. Keep that dependency
    /// unresolved rather than pretending its result is an independent proof root.
    Unresolved(Rc<[Rc<ObservedTypeOrigin<'db>>]>),
}

type ObservedFields<'db> = [(ObservationEdge, ObservedType<'db>)];
type RecipeFields<'db> = [(ObservationEdge, ObservationRecipe<'db>)];

/// Preserve graph sharing when removing session-local observation state from stored bounds.
/// These keys identify allocations within this conversion, never types or proof identities.
#[derive(Default)]
struct RecipeBuilder<'db> {
    children: FxHashMap<*const [ObservedType<'db>], Arc<[ObservationRecipe<'db>]>>,
    fields: FxHashMap<*const ObservedFields<'db>, Arc<RecipeFields<'db>>>,
    origins: FxHashMap<*const ObservedTypeOrigin<'db>, Arc<ObservedTypeOrigin<'db>>>,
}

impl<'db> RecipeBuilder<'db> {
    fn recipe(&mut self, observed: &ObservedType<'db>) -> ObservationRecipe<'db> {
        let shape = match &observed.shape {
            ObservedShape::Root => ObservationRecipeShape::Root,
            ObservedShape::Expression => ObservationRecipeShape::Expression,
            ObservedShape::Normalized(children) => {
                ObservationRecipeShape::Normalized(self.children(children))
            }
            ObservedShape::Merged(children) => {
                ObservationRecipeShape::Merged(self.children(children))
            }
            ObservedShape::Constructed(fields) => {
                let key = Rc::as_ptr(fields);
                let fields = if let Some(recipes) = self.fields.get(&key) {
                    Arc::clone(recipes)
                } else {
                    let recipes: Arc<[_]> = fields
                        .iter()
                        .map(|(edge, field)| (edge.clone(), self.recipe(field)))
                        .collect();
                    self.fields.insert(key, Arc::clone(&recipes));
                    recipes
                };
                ObservationRecipeShape::Constructed(fields)
            }
            ObservedShape::Unresolved(origins) => ObservationRecipeShape::Unresolved(
                origins
                    .iter()
                    .map(|origin| Arc::new((**origin).clone()))
                    .collect(),
            ),
        };
        let origin = observed.origin.as_ref().map(|origin| {
            Arc::clone(
                self.origins
                    .entry(Rc::as_ptr(origin))
                    .or_insert_with(|| Arc::new((**origin).clone())),
            )
        });
        ObservationRecipe {
            ty: observed.ty,
            origin,
            shape,
        }
    }

    fn children(&mut self, children: &Rc<[ObservedType<'db>]>) -> Arc<[ObservationRecipe<'db>]> {
        let key = Rc::as_ptr(children);
        if let Some(recipes) = self.children.get(&key) {
            return Arc::clone(recipes);
        }
        let recipes: Arc<[_]> = children.iter().map(|child| self.recipe(child)).collect();
        self.children.insert(key, Arc::clone(&recipes));
        recipes
    }
}

/// Rehydrate a stored bound without expanding its shared expression graph into a tree.
#[derive(Default)]
struct ObservationBuilder<'db> {
    children: FxHashMap<*const [ObservationRecipe<'db>], Rc<[ObservedType<'db>]>>,
    fields: FxHashMap<*const RecipeFields<'db>, Rc<ObservedFields<'db>>>,
    origins: FxHashMap<*const ObservedTypeOrigin<'db>, Rc<ObservedTypeOrigin<'db>>>,
}

impl<'db> ObservationBuilder<'db> {
    fn observe(&mut self, recipe: &ObservationRecipe<'db>) -> ObservedType<'db> {
        let shape = match &recipe.shape {
            ObservationRecipeShape::Root => ObservedShape::Root,
            ObservationRecipeShape::Expression => ObservedShape::Expression,
            ObservationRecipeShape::Normalized(children) => {
                ObservedShape::Normalized(self.children(children))
            }
            ObservationRecipeShape::Merged(children) => {
                ObservedShape::Merged(self.children(children))
            }
            ObservationRecipeShape::Constructed(fields) => {
                let key = Arc::as_ptr(fields);
                let fields = if let Some(observed) = self.fields.get(&key) {
                    Rc::clone(observed)
                } else {
                    let observed: Rc<[_]> = fields
                        .iter()
                        .map(|(edge, field)| (edge.clone(), self.observe(field)))
                        .collect();
                    self.fields.insert(key, Rc::clone(&observed));
                    observed
                };
                ObservedShape::Constructed(fields)
            }
            ObservationRecipeShape::Unresolved(origins) => ObservedShape::Unresolved(
                origins
                    .iter()
                    .map(|origin| Rc::new((**origin).clone()))
                    .collect(),
            ),
        };
        let origin = recipe.origin.as_ref().map(|origin| {
            Rc::clone(
                self.origins
                    .entry(Arc::as_ptr(origin))
                    .or_insert_with(|| Rc::new((**origin).clone())),
            )
        });
        ObservedType {
            ty: recipe.ty,
            origin,
            shape,
        }
    }

    fn children(&mut self, children: &Arc<[ObservationRecipe<'db>]>) -> Rc<[ObservedType<'db>]> {
        let key = Arc::as_ptr(children);
        if let Some(observed) = self.children.get(&key) {
            return Rc::clone(observed);
        }
        let observed: Rc<[_]> = children.iter().map(|child| self.observe(child)).collect();
        self.children.insert(key, Rc::clone(&observed));
        observed
    }
}

impl<'db> ObservedType<'db> {
    /// The caller has constructed `ty` from these fields. Only the provided edges have exact
    /// occurrences; any other query on the result keeps the field dependencies unresolved.
    pub(super) fn constructed(
        ty: Type<'db>,
        fields: impl IntoIterator<Item = (ObservationEdge, Self)>,
    ) -> Self {
        Self {
            ty,
            origin: None,
            shape: ObservedShape::Constructed(fields.into_iter().collect()),
        }
    }
    /// Retain every contributing expression when a bound operation has no exact projection.
    pub(super) fn dependent_on(ty: Type<'db>, contributors: &[Self]) -> Self {
        let mut dependencies = Vec::new();
        for contributor in contributors {
            for origin in contributor.input_origins() {
                if !dependencies.contains(&origin) {
                    dependencies.push(origin);
                }
            }
        }
        Self {
            ty,
            origin: None,
            shape: ObservedShape::Unresolved(dependencies.into()),
        }
    }

    fn input_origins(&self) -> Vec<Rc<ObservedTypeOrigin<'db>>> {
        self.origin().map_or_else(
            || match &self.shape {
                ObservedShape::Root => self.root_expression().origin().into_iter().collect(),
                _ => self.dependency_origins(),
            },
            |origin| vec![origin],
        )
    }

    pub(super) fn recipe(&self) -> ObservationRecipe<'db> {
        RecipeBuilder::default().recipe(self)
    }

    /// Whether two operands name the same observed expression, including its substitutions.
    /// Equal resulting types with different declaration paths are different occurrences.
    pub(super) fn same_occurrence(&self, other: &Self) -> bool {
        self == other
    }

    pub(super) fn root(ty: Type<'db>) -> Self {
        Self {
            ty,
            origin: None,
            shape: ObservedShape::Root,
        }
    }

    fn root_expression(&self) -> Self {
        Self {
            ty: self.ty,
            origin: Some(Rc::new(ObservedTypeOrigin {
                constructor: TypeIdentity::Other(self.ty),
                application: self.ty,
                node: self.ty.into(),
                operations: Box::default(),
            })),
            shape: ObservedShape::Expression,
        }
    }

    /// Select a child and its expression identity through the same structural operation.
    pub(super) fn project(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        edge: ObservationEdge,
    ) -> Option<Self> {
        let position = match edge {
            ObservationEdge::TupleElement(index) => Some(TupleShapePosition::Element(index)),
            ObservationEdge::TupleSuffix(index) => Some(TupleShapePosition::Suffix(index)),
            ObservationEdge::TupleVariable => Some(TupleShapePosition::Variable),
            _ => None,
        };
        if let Some(position) = position {
            let tuple = self.ty.as_nominal_instance()?.own_tuple_type()?;
            let selected = tuple.observe_element(db, position)?;
            if let Some(part) = selected.source.part {
                let mut child = self.project(db, env, ObservationEdge::TuplePart(part))?;
                if let Some(position) = selected.source.position {
                    while let Some(unfolded) = child.unfold(db, env) {
                        child = unfolded;
                    }
                    let edge = match position {
                        TupleShapePosition::Element(index) => ObservationEdge::TupleElement(index),
                        TupleShapePosition::Suffix(index) => ObservationEdge::TupleSuffix(index),
                        TupleShapePosition::Variable => ObservationEdge::TupleVariable,
                    };
                    child = child.project(db, env, edge)?;
                }
                return Some(child.unchanged_or_unresolved(selected.ty));
            }
            return Some(self.child_at_impl(db, env, selected.ty, edge));
        }
        let ty = observation_child(db, env, self.ty, &edge)?;
        Some(self.child_at_impl(db, env, ty, edge))
    }

    /// Visit the operands stored in this expression, without expanding member declarations.
    /// Alias bodies and deferred operations are interpreted separately by the consumer.
    pub(super) fn stored_children(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Vec<Self> {
        let mut edges = Vec::new();
        match self.ty {
            Type::Union(union) => {
                edges.extend((0..union.elements(db).len()).map(ObservationEdge::UnionElement));
            }
            Type::Intersection(intersection) => {
                edges.extend(
                    (0..intersection.positive(db).len()).map(ObservationEdge::IntersectionPositive),
                );
                edges.extend(
                    intersection
                        .negative(db)
                        .iter()
                        .enumerate()
                        .map(|(index, _)| ObservationEdge::IntersectionNegative(index)),
                );
            }
            Type::NominalInstance(instance) if instance.own_tuple_type().is_some() => {
                return shallow_stored_children(db, env, self.ty)
                    .into_iter()
                    .enumerate()
                    .map(|(index, ty)| {
                        self.child_at(db, env, ty, ObservationEdge::TupleStoredPart(index))
                    })
                    .collect();
            }
            Type::TypeVar(variable) => {
                if variable.typevar(db).upper_bound(db, env).is_some() {
                    edges.push(ObservationEdge::TypeVarUpperBound);
                }
                if let Some(constraints) = variable.typevar(db).constraints(db, env) {
                    edges.extend((0..constraints.len()).map(ObservationEdge::TypeVarConstraint));
                }
            }
            Type::Deferred(_) => {
                edges.extend([
                    ObservationEdge::DeferredArgument,
                    ObservationEdge::DeferredDomain,
                ]);
            }
            Type::Callable(_) | Type::FunctionLiteral(_) => {
                let signatures = match self.ty {
                    Type::Callable(callable) => Some(callable.signatures(db)),
                    Type::FunctionLiteral(function) => function.updated_signature(db),
                    _ => None,
                };
                if let Some(signatures) = signatures {
                    for (overload, signature) in signatures.overloads.iter().enumerate() {
                        edges.extend((0..signature.parameters().len()).map(|parameter| {
                            ObservationEdge::CallableParameter {
                                overload,
                                parameter,
                            }
                        }));
                        if !signature.is_paramspec_value() {
                            edges.push(ObservationEdge::CallableReturn { overload });
                        }
                        for (relation, _) in signature.receiver_relation_slots() {
                            for annotation in [false, true] {
                                edges.push(ObservationEdge::CallableReceiver {
                                    overload,
                                    relation,
                                    annotation,
                                });
                            }
                        }
                    }
                }
                if let Type::FunctionLiteral(function) = self.ty
                    && let Some(callables) = function.updated_implementation_callables(db)
                {
                    edges.extend(
                        (0..callables.len()).map(ObservationEdge::FunctionImplementationCallable),
                    );
                }
            }
            Type::TypeAlias(_)
            | Type::Recursive(_)
            | Type::GenericAlias(_)
            | Type::NominalInstance(_)
            | Type::ProtocolInstance(_)
            | Type::TypedDict(_)
            | Type::SubclassOf(_) => {
                if let Some(specialization) = stored_specialization(db, env, self.ty) {
                    edges.extend(
                        (0..specialization.types(db).len()).map(ObservationEdge::GenericArgument),
                    );
                    if specialization.tuple_inner(db).is_some() {
                        edges.push(ObservationEdge::SpecializationTuple);
                    }
                } else if !matches!(
                    self.ty,
                    Type::TypeAlias(_) | Type::Recursive(_) | Type::ProtocolInstance(_)
                ) {
                    return shallow_stored_children(db, env, self.ty)
                        .into_iter()
                        .map(|ty| Self::dependent_on(ty, std::slice::from_ref(self)))
                        .collect();
                }
            }
            Type::RecursiveVar(_) => return Vec::new(),
            _ => {
                return shallow_stored_children(db, env, self.ty)
                    .into_iter()
                    .map(|ty| Self::dependent_on(ty, std::slice::from_ref(self)))
                    .collect();
            }
        }
        edges
            .into_iter()
            .filter_map(|edge| self.project(db, env, edge))
            .collect()
    }

    /// Select the occurrence of a declaring class in this operand's specialized MRO.
    pub(super) fn class_base(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        base: ClassLiteral<'db>,
    ) -> Option<Self> {
        let class_view = self.project(db, env, ObservationEdge::ClassView)?;
        let class = match class_view.ty {
            Type::ClassLiteral(class) => ClassType::NonGeneric(class),
            Type::GenericAlias(alias) => ClassType::Generic(alias),
            _ => return None,
        };
        let index = class.iter_mro(db).position(|candidate| {
            candidate
                .into_class()
                .is_some_and(|candidate| candidate.class_literal(db) == base)
        })?;
        class_view.project(db, env, ObservationEdge::ClassBase(index))
    }

    /// Select an overload by its position in the current callable. Its declaration index is
    /// diagnostic metadata and can differ after filtering or receiver specialization.
    pub(super) fn callable_overload(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        index: usize,
    ) -> Option<Self> {
        self.project(db, env, ObservationEdge::CallableOverload(index))
    }

    /// Observe a class-owned member before applying runtime receiver and lexical `Self` bindings.
    /// The declaring owner's specialization is recorded independently of the receiver's class,
    /// which can inherit the member through a differently specialized base.
    pub(super) fn declaration_member(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        owner: StaticClassLiteral<'db>,
        edge: ObservationEdge,
        raw_expression: Type<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Self {
        let observation =
            self.declaration_expression(TypeIdentity::Other(owner.into()), edge, raw_expression);
        let Some(specialization) = specialization else {
            return observation;
        };
        let substitution = ApplySpecialization::Specialization {
            specialization,
            specialize_self_domain: true,
        };
        let mapping = match specialization.materialization_kind(db) {
            None => TypeMapping::ApplySpecialization(substitution),
            Some(materialization_kind) => TypeMapping::ApplySpecializationWithMaterialization {
                specialization: substitution,
                materialization_kind,
            },
        };
        observation.apply_mapping(
            db,
            &mapping,
            &ApplyTypeMappingVisitor::new_for_type_construction(env),
        )
    }

    fn declaration_expression(
        &self,
        constructor: TypeIdentity<'db>,
        edge: ObservationEdge,
        raw_expression: Type<'db>,
    ) -> Self {
        Self {
            ty: raw_expression,
            origin: Some(Rc::new(ObservedTypeOrigin {
                constructor,
                application: self.ty,
                node: ExpressionNode {
                    template: raw_expression,
                    path: Box::new([edge]),
                },
                operations: Box::default(),
            })),
            shape: ObservedShape::Expression,
        }
    }

    /// A schema lookup enters a declaration body, just as a nominal member lookup does.
    /// Reset the finite field path at that boundary while retaining the closed application.
    fn typed_dict_schema_child(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        edge: &ObservationEdge,
    ) -> Option<Self> {
        if !matches!(
            edge,
            ObservationEdge::TypedDictField(_) | ObservationEdge::TypedDictExtraItems
        ) {
            return None;
        }
        let Type::TypedDict(typed_dict) = self.ty else {
            return None;
        };
        let class = typed_dict.defining_class()?;
        let (owner, specialization) = class.class_literal_and_specialization(db);
        let raw_schema = super::TypedDictType::Class(owner.identity_specialization(db));
        let raw_expression = raw_schema.observation_child(db, env, edge)?;
        let observation =
            self.declaration_expression(self.ty.to_type_identity(db), edge.clone(), raw_expression);
        let specialization = match owner {
            ClassLiteral::Static(class) => {
                class
                    .apply_optional_specialization(db, specialization)
                    .class_literal_and_specialization(db)
                    .1
            }
            _ => specialization,
        };
        let Some(specialization) = specialization else {
            return Some(observation);
        };
        let substitution = ApplySpecialization::specialization(specialization);
        let mapping = match specialization.materialization_kind(db) {
            None => TypeMapping::ApplySpecialization(substitution),
            Some(materialization_kind) => TypeMapping::ApplySpecializationWithMaterialization {
                specialization: substitution,
                materialization_kind,
            },
        };
        Some(observation.apply_mapping(
            db,
            &mapping,
            &ApplyTypeMappingVisitor::new_for_type_construction(env),
        ))
    }
    pub(super) fn is_normalized_union(&self) -> bool {
        matches!(self.shape, ObservedShape::Normalized(_))
    }
    pub(super) fn unresolved(&self) -> Self {
        Self {
            ty: self.ty,
            origin: None,
            shape: ObservedShape::Unresolved(self.input_origins().into()),
        }
    }
    pub(super) fn origin(&self) -> Option<Rc<ObservedTypeOrigin<'db>>> {
        match &self.shape {
            ObservedShape::Unresolved(_) => None,
            _ => self.origin.clone(),
        }
    }

    /// An unresolved derivative still depends on its nearest observed expression. This is
    /// evidence of a recursive dependency, not the identity of the derivative itself.
    pub(super) fn dependency_origins(&self) -> Vec<Rc<ObservedTypeOrigin<'db>>> {
        match &self.shape {
            ObservedShape::Unresolved(origins) => origins.to_vec(),
            ObservedShape::Merged(children) | ObservedShape::Normalized(children) => children
                .iter()
                .flat_map(|child| {
                    child
                        .origin()
                        .map_or_else(|| child.dependency_origins(), |origin| vec![origin])
                })
                .collect(),
            ObservedShape::Constructed(fields) => fields
                .iter()
                .flat_map(|(_, field)| field.input_origins())
                .collect(),
            _ => Vec::new(),
        }
    }
    #[cfg(test)]
    fn is_unresolved(&self) -> bool {
        matches!(self.shape, ObservedShape::Unresolved(_))
    }

    /// Keep an unchanged operand. A changed value requires an explicit structural edge.
    pub(super) fn unchanged_or_unresolved(&self, ty: Type<'db>) -> Self {
        if self.ty == ty {
            return self.clone();
        }
        Self {
            ty,
            ..self.unresolved()
        }
    }

    pub(super) fn child_at(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        edge: ObservationEdge,
    ) -> Self {
        if matches!(
            edge,
            ObservationEdge::TupleElement(_)
                | ObservationEdge::TupleSuffix(_)
                | ObservationEdge::TupleVariable
        ) {
            return self.project(db, env, edge).map_or_else(
                || self.unchanged_or_unresolved(ty),
                |child| child.unchanged_or_unresolved(ty),
            );
        }
        self.child_at_impl(db, env, ty, edge)
    }

    fn child_at_impl(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        edge: ObservationEdge,
    ) -> Self {
        if edge == ObservationEdge::Identity {
            return self.unchanged_or_unresolved(ty);
        }
        if let ObservationEdge::UnionElement(index) = edge {
            return self
                .union_children(db, env)
                .get(index)
                .filter(|child| child.ty == ty)
                .cloned()
                .unwrap_or_else(|| self.unchanged_or_unresolved(ty));
        }
        if let Some(child) = self.typed_dict_schema_child(db, env, &edge) {
            return child.unchanged_or_unresolved(ty);
        }
        match &self.shape {
            ObservedShape::Root => self.root_expression().child_at_impl(db, env, ty, edge),
            ObservedShape::Unresolved(_) => self.unchanged_or_unresolved(ty),
            ObservedShape::Normalized(_) => self.unchanged_or_unresolved(ty),
            ObservedShape::Constructed(fields) => fields
                .iter()
                .find(|(position, _)| position == &edge)
                .map_or_else(
                    || self.unchanged_or_unresolved(ty),
                    |(_, field)| field.unchanged_or_unresolved(ty),
                ),
            ObservedShape::Merged(children) => {
                let children = children
                    .iter()
                    .map(|child| child.child_at(db, env, ty, edge.clone()))
                    .collect();
                self.output_edge(ty, edge).normalized(ty, children)
            }
            ObservedShape::Expression => {
                let Some(origin) = &self.origin else {
                    return self.unchanged_or_unresolved(ty);
                };
                // Capturing a positional receiver shifts the remaining parameter positions.
                // Select that input position before recording the output edge.
                let mut input_edge = edge.clone();
                if let ObservationEdge::CallableParameter {
                    overload,
                    parameter,
                } = edge
                {
                    for operation in &origin.operations {
                        if let ObservedOperationKind::CallableBinding {
                            callable,
                            path_depth,
                            mode: CallableBindingMode::Capture,
                            ..
                        } = &operation.operation
                            && *path_depth == origin.node.path.len()
                            && callable
                                .signatures(db)
                                .overloads
                                .get(overload)
                                .and_then(|signature| signature.parameters().get(0))
                                .is_some_and(Parameter::is_positional)
                        {
                            input_edge = ObservationEdge::CallableParameter {
                                overload,
                                parameter: parameter + 1,
                            };
                        }
                    }
                }
                if let Some(template) =
                    observation_child(db, env, origin.node.template, &input_edge)
                {
                    let mut path = origin.node.path.to_vec();
                    path.push(edge.clone());
                    let child_origin = ObservedTypeOrigin {
                        node: ExpressionNode {
                            template,
                            path: path.into(),
                        },
                        ..(**origin).clone()
                    };
                    // The closed parent already encodes variance and invariant materialization
                    // families. Observe its value before replaying a lazy declaration expression.
                    if observation_child(db, env, self.ty, &edge) == Some(ty) {
                        return Self {
                            ty,
                            origin: Some(Rc::new(child_origin)),
                            shape: ObservedShape::Expression,
                        };
                    }

                    let Some(mapped) = Self::instantiate_node(db, env, &child_origin, template)
                    else {
                        return self.unchanged_or_unresolved(ty);
                    };
                    let child = Self {
                        ty: mapped,
                        origin: Some(Rc::new(child_origin)),
                        shape: ObservedShape::Expression,
                    };
                    return child.unchanged_or_unresolved(ty);
                }
                // Substitution and normalization can expose children absent from the original
                // shape. Select the actual output edge and retain its parent expression; this
                // also distinguishes descent into a substituted parameter from a body backedge.
                if observation_child(db, env, self.ty, &edge) == Some(ty) {
                    let mut path = origin.node.path.to_vec();
                    path.push(edge);
                    return Self {
                        ty,
                        origin: Some(Rc::new(ObservedTypeOrigin {
                            node: ExpressionNode {
                                template: origin.node.template,
                                path: path.into(),
                            },
                            ..(**origin).clone()
                        })),
                        shape: ObservedShape::Expression,
                    };
                }
                self.unchanged_or_unresolved(ty)
            }
        }
    }

    /// Attach normalized children in their output order while retaining the parent expression.
    pub(super) fn normalized(&self, ty: Type<'db>, children: Vec<Self>) -> Self {
        if let [child] = children.as_slice()
            && child.ty == ty
        {
            return child.clone();
        }
        let shape = if ty.is_union() {
            ObservedShape::Normalized(children.into())
        } else {
            ObservedShape::Merged(children.into())
        };
        Self {
            ty,
            origin: self.origin.clone(),
            shape,
        }
    }

    fn instantiate_node(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        origin: &ObservedTypeOrigin<'db>,
        node: Type<'db>,
    ) -> Option<Type<'db>> {
        let contravariant = origin
            .node
            .path
            .iter()
            .filter(|edge| edge.is_contravariant())
            .count()
            % 2
            == 1;
        let mut mapped = match origin.application {
            Type::TypeAlias(alias) => alias.observe_body_node(db, node, contravariant),
            Type::Recursive(recursive) => recursive.observe_body_node(db, env, node, contravariant),
            _ => node,
        };
        for recorded in &origin.operations {
            mapped = match &recorded.operation {
                ObservedOperationKind::Mapping(operation) => operation.with_mapping(|mapping| {
                    let mapping = if contravariant != recorded.contravariant {
                        mapping.flip()
                    } else {
                        mapping
                    };
                    let mut visitor = ApplyTypeMappingVisitor::new_for_type_construction(env);
                    if let RecursiveOperation::Materialize(_, bounds) = operation {
                        visitor.materialize_typevar_bounds_and_defaults = *bounds;
                    }
                    mapped.apply_type_mapping_impl(db, &mapping, TypeContext::default(), &visitor)
                }),
                ObservedOperationKind::CallableBinding {
                    callable,
                    binding,
                    path_depth,
                    mode,
                } => {
                    let edge = origin.node.path.get(*path_depth);
                    let (overload, receiver) = match edge {
                        None => {
                            let Type::Callable(callable) = mapped else {
                                return None;
                            };
                            mapped = Type::Callable(mode.bind(db, env, callable, *binding));
                            continue;
                        }
                        Some(
                            ObservationEdge::CallableParameter { overload, .. }
                            | ObservationEdge::CallableReturn { overload },
                        ) => (*overload, false),
                        Some(ObservationEdge::CallableReceiver { overload, .. }) => {
                            (*overload, true)
                        }
                        Some(ObservationEdge::CallableOverload(overload)) => {
                            match origin.node.path.get(*path_depth + 1) {
                                None => {
                                    let Type::Callable(callable) = mapped else {
                                        return None;
                                    };
                                    mapped = Type::Callable(mode.bind(db, env, callable, *binding));
                                    continue;
                                }
                                Some(
                                    ObservationEdge::CallableParameter { .. }
                                    | ObservationEdge::CallableReturn { .. },
                                ) => (*overload, false),
                                Some(ObservationEdge::CallableReceiver { .. }) => (*overload, true),
                                _ => return None,
                            }
                        }
                        _ => return None,
                    };
                    let signature = callable.signatures(db).overloads.get(overload)?;
                    // Runtime receiver substitution only acts on retained receiver obligations.
                    // Parameters and returns receive the lexical Self substitution alone.
                    if receiver {
                        let mapping = TypeMapping::BindSelf(SelfBinding::new(
                            db,
                            env,
                            binding.receiver,
                            Some(BindingContext::Synthetic(env.program(db))),
                        ));
                        let visitor = ApplyTypeMappingVisitor::new_for_type_construction(env);
                        mapped = mapped.apply_type_mapping_impl(
                            db,
                            &mapping,
                            TypeContext::default(),
                            &visitor,
                        );
                    }
                    let mapping = TypeMapping::BindSelf(SelfBinding::new(
                        db,
                        env,
                        binding.self_type,
                        signature.definition().map(BindingContext::Definition),
                    ));
                    let visitor = ApplyTypeMappingVisitor::new_for_type_construction(env);
                    mapped.apply_type_mapping_impl(db, &mapping, TypeContext::default(), &visitor)
                }
            };
        }
        Some(mapped)
    }

    /// Apply a structural substitution to the operand and retain the same operation for
    /// declaration children that have not been observed yet.
    pub(super) fn apply_mapping(
        &self,
        db: &'db dyn Db,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        if matches!(self.shape, ObservedShape::Root) {
            return self.root_expression().apply_mapping(db, mapping, visitor);
        }
        let mapping_visitor = visitor
            .for_new_mapping()
            .with_normalization(super::TypeNormalization::Structural);
        let ty =
            self.ty
                .apply_type_mapping_impl(db, mapping, TypeContext::default(), &mapping_visitor);
        if let ObservedShape::Constructed(fields) = &self.shape {
            return Self::constructed(
                ty,
                fields.iter().map(|(edge, field)| {
                    let field_mapping = if edge.is_contravariant() {
                        mapping.flip()
                    } else {
                        mapping.clone()
                    };
                    (
                        edge.clone(),
                        field.apply_mapping(db, &field_mapping, &mapping_visitor),
                    )
                }),
            );
        }
        if let ObservedShape::Unresolved(origins) = &self.shape {
            return Self {
                ty,
                origin: None,
                shape: ObservedShape::Unresolved(
                    origins
                        .iter()
                        .map(|origin| {
                            Rc::new(Self::map_dependency_origin(origin, mapping, visitor))
                        })
                        .collect(),
                ),
            };
        }
        let Some(origin) = &self.origin else {
            return self.unchanged_or_unresolved(ty);
        };
        let operation = match mapping {
            TypeMapping::Materialize(kind) => Some(RecursiveOperation::Materialize(
                *kind,
                visitor.materialize_typevar_bounds_and_defaults,
            )),
            _ => RecursiveOperation::substitution(mapping),
        };
        let Some(operation) = operation else {
            return self.unchanged_or_unresolved(ty);
        };
        let mut operations = origin.operations.to_vec();
        operations.push(ObservedOperation {
            operation: ObservedOperationKind::Mapping(operation),
            contravariant: origin
                .node
                .path
                .iter()
                .filter(|edge| edge.is_contravariant())
                .count()
                % 2
                == 1,
        });
        Self {
            ty,
            origin: Some(Rc::new(ObservedTypeOrigin {
                operations: operations.into(),
                ..(**origin).clone()
            })),
            shape: ObservedShape::Expression,
        }
    }

    /// Record a substitution on a dependency without replaying its enclosing type. An unknown
    /// derivative does not provide the structural edge needed to evaluate that input again.
    fn map_dependency_origin(
        origin: &ObservedTypeOrigin<'db>,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> ObservedTypeOrigin<'db> {
        let operation = match mapping {
            TypeMapping::Materialize(kind) => Some(RecursiveOperation::Materialize(
                *kind,
                visitor.materialize_typevar_bounds_and_defaults,
            )),
            _ => RecursiveOperation::substitution(mapping),
        };
        let mut operations = origin.operations.to_vec();
        if let Some(operation) = operation {
            operations.push(ObservedOperation {
                operation: ObservedOperationKind::Mapping(operation),
                contravariant: origin
                    .node
                    .path
                    .iter()
                    .filter(|edge| edge.is_contravariant())
                    .count()
                    % 2
                    == 1,
            });
        }
        ObservedTypeOrigin {
            operations: operations.into(),
            ..origin.clone()
        }
    }

    /// Bind a callable with the same domain-sensitive operation used by signature checking.
    /// Keep its input signatures so later parameter, return, and receiver edges replay exactly
    /// the substitutions that applied at their original signature position.
    pub(super) fn bind_callable_self(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        binding: CallableSelfBinding<'db>,
    ) -> Self {
        self.record_callable_binding(db, env, binding, CallableBindingMode::Apply)
    }

    pub(super) fn capture_callable_receiver(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        binding: CallableSelfBinding<'db>,
    ) -> Self {
        self.record_callable_binding(db, env, binding, CallableBindingMode::Capture)
    }

    fn record_callable_binding(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        binding: CallableSelfBinding<'db>,
        mode: CallableBindingMode,
    ) -> Self {
        if matches!(self.shape, ObservedShape::Root) {
            return self
                .root_expression()
                .record_callable_binding(db, env, binding, mode);
        }
        let Type::Callable(callable) = self.ty else {
            return self.unresolved();
        };
        let ty = Type::Callable(mode.bind(db, env, callable, binding));
        let Some(origin) = &self.origin else {
            return self.unchanged_or_unresolved(ty);
        };
        let mut operations = origin.operations.to_vec();
        operations.push(ObservedOperation {
            operation: ObservedOperationKind::CallableBinding {
                callable,
                binding,
                path_depth: origin.node.path.len(),
                mode,
            },
            contravariant: false,
        });
        Self {
            ty,
            origin: Some(Rc::new(ObservedTypeOrigin {
                operations: operations.into(),
                ..(**origin).clone()
            })),
            shape: self.shape.clone(),
        }
    }

    fn output_edge(&self, ty: Type<'db>, edge: ObservationEdge) -> Self {
        let origin = self
            .origin
            .as_ref()
            .map(|origin| {
                let mut path = origin.node.path.to_vec();
                path.push(edge);
                ObservedTypeOrigin {
                    node: ExpressionNode {
                        template: origin.node.template,
                        path: path.into(),
                    },
                    ..(**origin).clone()
                }
            })
            .map(Rc::new);
        Self {
            ty,
            origin,
            shape: self.shape.clone(),
        }
    }

    /// Build the output edges from the mapping's constituent expressions. Flattening or
    /// reordering does not change the identity of an originating node, and merged values
    /// preserve every input edge as an explicit compound observation.
    pub(super) fn union_children(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Vec<Self> {
        let Type::Union(union) = self.ty else {
            return Vec::new();
        };
        if let ObservedShape::Normalized(children) = &self.shape {
            return children.to_vec();
        }
        let mut candidates = Vec::new();
        if let ObservedShape::Expression = &self.shape
            && let Some(origin) = &self.origin
            && let Type::Union(template) = origin.node.template
        {
            for (index, &node) in template.elements(db).iter().enumerate() {
                let mut path = origin.node.path.to_vec();
                path.push(ObservationEdge::UnionElement(index));
                let child_origin = ObservedTypeOrigin {
                    node: ExpressionNode {
                        template: node,
                        path: path.into(),
                    },
                    ..(**origin).clone()
                };

                let Some(ty) = Self::instantiate_node(db, env, &child_origin, node) else {
                    // A pending operation could not be projected through this expression.
                    // The closed union still supplies every alternative; retain those values
                    // and their dependency rather than discarding an unobserved branch.
                    return union
                        .elements(db)
                        .iter()
                        .copied()
                        .enumerate()
                        .map(|(index, ty)| {
                            self.output_edge(ty, ObservationEdge::UnionElement(index))
                                .unresolved()
                        })
                        .collect();
                };
                let child = Self {
                    ty,
                    origin: Some(Rc::new(child_origin)),
                    shape: ObservedShape::Expression,
                };
                if child.ty.is_union() {
                    candidates.extend(child.union_children(db, env));
                } else {
                    candidates.push(child);
                }
            }
        } else {
            return union
                .elements(db)
                .iter()
                .copied()
                .enumerate()
                .map(|(index, ty)| match self.shape {
                    ObservedShape::Root => Self::root(ty),
                    _ => self.output_edge(ty, ObservationEdge::UnionElement(index)),
                })
                .collect();
        }
        union
            .elements(db)
            .iter()
            .copied()
            .enumerate()
            .map(|(index, ty)| {
                let contributors: Vec<_> = candidates
                    .iter()
                    .filter(|child| child.ty == ty)
                    .cloned()
                    .collect();
                let output = self.output_edge(ty, ObservationEdge::UnionElement(index));
                if contributors.is_empty() {
                    output
                } else {
                    output.normalized(ty, contributors)
                }
            })
            .collect()
    }

    pub(super) fn unfold(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Option<Self> {
        let (template, body) = match self.ty {
            Type::TypeAlias(alias) => (Some(alias.raw_value_type(db)), alias.value_type(db)),
            Type::Recursive(recursive) => (
                recursive.observation_body(db),
                recursive.unfold(db, env).into_unfolded()?,
            ),
            _ => return None,
        };
        let Some(template) = template else {
            // Opaque declaration constructors retain their closed application; their schema or
            // callable observer supplies member edges when those requirements are requested.
            return Some(Self {
                ty: body,
                origin: Some(Rc::new(ObservedTypeOrigin {
                    constructor: self.ty.to_type_identity(db),
                    application: self.ty,
                    node: match self.ty {
                        Type::Recursive(recursive) => {
                            Type::Recursive(recursive.constructor(db)).into()
                        }
                        _ => self.ty.into(),
                    },
                    operations: Box::default(),
                })),
                shape: ObservedShape::Expression,
            });
        };
        Some(Self {
            ty: body,
            origin: Some(Rc::new(ObservedTypeOrigin {
                constructor: self.ty.to_type_identity(db),
                application: self.ty,
                node: template.into(),
                operations: Box::default(),
            })),
            shape: ObservedShape::Expression,
        })
    }

    /// Interpret deferred operations within the proof that requested their result.
    pub(super) fn unfold_in_context(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        context: &RelationContext<'db>,
    ) -> Option<Self> {
        let Type::Deferred(deferred) = self.ty else {
            return self.unfold(db, env);
        };
        let operand = if self.origin.is_some() {
            self.clone()
        } else {
            Self {
                ty: self.ty,
                origin: Some(Rc::new(ObservedTypeOrigin {
                    constructor: TypeIdentity::Other(Type::Deferred(deferred.constructor(db))),
                    application: self.ty,
                    // Descending into the substituted argument is finite parameter descent,
                    // not a declaration backedge merely because its owner appears again.
                    node: Type::TypeVar(deferred.parameter(db)).into(),
                    operations: Box::default(),
                })),
                shape: ObservedShape::Expression,
            }
        };
        deferred.observe(db, env, &operand, context)
    }
}

/// The explicit expressions currently being compared. Reversing a comparison also reverses
/// these operands; a child can never borrow provenance from an equal value on the other side.
#[derive(Clone, Debug)]
pub(super) struct ObservedTypePair<'db> {
    pub(super) source: ObservedType<'db>,
    pub(super) target: ObservedType<'db>,
}

impl<'db> ObservedTypePair<'db> {
    pub(super) fn new(source: ObservedType<'db>, target: ObservedType<'db>) -> Self {
        Self { source, target }
    }

    /// Start an independent query. Derived obligations retain their existing operands instead.
    pub(super) fn roots(source: Type<'db>, target: Type<'db>) -> Self {
        Self::new(ObservedType::root(source), ObservedType::root(target))
    }
    pub(super) fn children(
        &self,
        source: Type<'db>,
        target: Type<'db>,
    ) -> (ObservedType<'db>, ObservedType<'db>) {
        (
            self.source.unchanged_or_unresolved(source),
            self.target.unchanged_or_unresolved(target),
        )
    }
    pub(super) fn children_at(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: Type<'db>,
        target: Type<'db>,
        source_edge: ObservationEdge,
        target_edge: ObservationEdge,
    ) -> (ObservedType<'db>, ObservedType<'db>) {
        (
            self.source.child_at(db, env, source, source_edge),
            self.target.child_at(db, env, target, target_edge),
        )
    }
    pub(super) fn map(
        &self,
        db: &'db dyn Db,
        source_mapping: Option<&TypeMapping<'_, 'db>>,
        target_mapping: Option<&TypeMapping<'_, 'db>>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        Self::new(
            source_mapping.map_or_else(
                || self.source.clone(),
                |mapping| self.source.apply_mapping(db, mapping, visitor),
            ),
            target_mapping.map_or_else(
                || self.target.clone(),
                |mapping| self.target.apply_mapping(db, mapping, visitor),
            ),
        )
    }
    pub(super) fn bind_callable_self(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source_binding: Option<CallableSelfBinding<'db>>,
        target_binding: Option<CallableSelfBinding<'db>>,
    ) -> Self {
        Self::new(
            source_binding.map_or_else(
                || self.source.clone(),
                |binding| self.source.bind_callable_self(db, env, binding),
            ),
            target_binding.map_or_else(
                || self.target.clone(),
                |binding| self.target.bind_callable_self(db, env, binding),
            ),
        )
    }

    pub(super) fn reversed(&self) -> Self {
        Self::new(self.target.clone(), self.source.clone())
    }
    pub(super) fn source_twice(&self) -> Self {
        Self::new(self.source.clone(), self.source.clone())
    }
    pub(super) fn target_twice(&self) -> Self {
        Self::new(self.target.clone(), self.target.clone())
    }
}

/// Retrieve application arguments without following a declaration's parameters or members.
fn stored_specialization<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
) -> Option<Specialization<'db>> {
    let class = match ty {
        Type::TypeAlias(alias) => return alias.specialization(db),
        Type::Recursive(recursive) => return recursive.arguments(db),
        Type::GenericAlias(alias) => return Some(alias.specialization(db)),
        Type::NominalInstance(instance) => instance.class(db, env),
        Type::ProtocolInstance(protocol) => *protocol.class_origin(db)?,
        Type::TypedDict(typed_dict) => typed_dict.defining_class()?,
        Type::SubclassOf(subclass) => match subclass.subclass_of() {
            SubclassOfInner::Class(class) => class,
            _ => return None,
        },
        _ => return None,
    };
    match class {
        ClassType::Generic(alias) => Some(alias.specialization(db)),
        ClassType::NonGeneric(_) => None,
    }
}

fn shallow_stored_children<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
) -> Vec<Type<'db>> {
    struct Children<'a, 'db> {
        env: &'a ProgramEnvironment<'db>,
        types: RefCell<Vec<Type<'db>>>,
    }
    impl<'db> TypeVisitor<'db> for Children<'_, 'db> {
        fn program_environment(&self) -> &ProgramEnvironment<'db> {
            self.env
        }
        fn should_visit_lazy_type_attributes(&self) -> bool {
            false
        }
        fn visit_type(&self, _db: &'db dyn Db, ty: Type<'db>) {
            self.types.borrow_mut().push(ty);
        }
        // Generic declarations own these binders; they are not operands of an application.
        fn visit_type_var_type(
            &self,
            _db: &'db dyn Db,
            _typevar: super::typevar::TypeVarInstance<'db>,
        ) {
        }
    }
    let children = Children {
        env,
        types: RefCell::default(),
    };
    if let TypeKind::NonAtomic(ty) = ty.into() {
        walk_non_atomic_type(db, ty, &children);
    }
    children.types.into_inner()
}

fn observation_child<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    template: Type<'db>,
    edge: &ObservationEdge,
) -> Option<Type<'db>> {
    match (template, edge) {
        (Type::TypeVar(variable), ObservationEdge::TypeVarUpperBound) => {
            variable.typevar(db).upper_bound(db, env)
        }
        (Type::TypeVar(variable), ObservationEdge::TypeVarConstraint(index)) => variable
            .typevar(db)
            .constraints(db, env)?
            .get(*index)
            .copied(),
        (Type::Deferred(deferred), ObservationEdge::DeferredArgument) => {
            Some(deferred.argument(db))
        }
        (Type::Deferred(deferred), ObservationEdge::DeferredDomain) => deferred.domain(db, env),
        (_, ObservationEdge::SpecializationTuple) => Some(Type::tuple(
            stored_specialization(db, env, template)?.tuple_inner(db)?,
        )),
        (
            Type::FunctionLiteral(function),
            ObservationEdge::FunctionImplementationCallable(index),
        ) => function
            .updated_implementation_callables(db)?
            .get(*index)
            .copied()
            .map(Type::Callable),
        (_, ObservationEdge::TuplePart(index)) => template
            .as_nominal_instance()?
            .own_tuple_type()?
            .expression_part(db, *index),
        (_, ObservationEdge::TupleStoredPart(index)) => {
            template.as_nominal_instance()?.own_tuple_type()?;
            shallow_stored_children(db, env, template)
                .get(*index)
                .copied()
        }
        (_, ObservationEdge::ClassMetaclassInstance) => {
            let class = match template {
                Type::ClassLiteral(class) => ClassType::NonGeneric(class),
                Type::GenericAlias(alias) => ClassType::Generic(alias),
                _ => return None,
            };
            // This is a storage lookup, not the public instance type of a metaclass.
            // A missing metaclass has arbitrary storage, even though its instances are classes.
            class.metaclass(db).to_instance_approximation(db, env)
        }
        (Type::SubclassOf(subclass), ObservationEdge::GradualMetaclassBase)
            if subclass.is_dynamic() =>
        {
            Some(super::KnownClass::Type.to_instance(db, env))
        }
        (Type::SubclassOf(subclass), ObservationEdge::SubclassInstance) => {
            Some(subclass.to_instance(db, env))
        }
        (Type::SubclassOf(subclass), ObservationEdge::TransposedSubclassVariable)
            if subclass.into_type_var().is_some() =>
        {
            let SubclassOfInner::TypeVar(variable) =
                subclass.subclass_of().with_transposed_type_var(db, env)
            else {
                return None;
            };
            Some(Type::TypeVar(variable))
        }
        (_, ObservationEdge::MetaType) => Some(template.to_meta_type(db, env)),
        (_, ObservationEdge::ClassBase(index)) => {
            let class = match template {
                Type::ClassLiteral(class) => ClassType::NonGeneric(class),
                Type::GenericAlias(alias) => ClassType::Generic(alias),
                _ => return None,
            };
            class
                .iter_mro(db)
                .nth(*index)
                .and_then(ClassBase::into_class)
                .map(Type::from)
        }
        (_, ObservationEdge::ClassView) => {
            let class = match template {
                Type::TypedDict(typed_dict) => typed_dict.defining_class()?,
                Type::ClassLiteral(class) => ClassType::NonGeneric(class),
                Type::GenericAlias(alias) => ClassType::Generic(alias),
                Type::SubclassOf(subclass) => match subclass.subclass_of() {
                    SubclassOfInner::Class(class) => class,
                    SubclassOfInner::Protocol(protocol) => *protocol.class_origin(db)?,
                    SubclassOfInner::TypeVar(variable) => {
                        // Keep the class namespace of a nominal bound before converting it to
                        // its meta-type: `type[object]` normalizes to an instance of `type`.
                        // Looking up that instance would lose the distinction between
                        // `object.__repr__` and `type.__repr__`.
                        let TypeVarBoundOrConstraints::UpperBound(Type::NominalInstance(bound)) =
                            variable.require_bound_or_constraints(db, env)
                        else {
                            return None;
                        };
                        bound.class(db, env)
                    }
                    _ => return None,
                },
                _ => template.nominal_class(db, env)?,
            };
            Some(class.into())
        }
        (Type::PropertyInstance(property), ObservationEdge::PropertyGetter) => property.getter(db),
        (ty, ObservationEdge::UnderlyingFunction) if ty.function_like_kind(db).is_some() => {
            Some(ty.underlying_function(db))
        }
        (ty, ObservationEdge::IntrinsicMember(name)) => ty
            .intrinsic_member(db, env, name)
            .and_then(|member| member.place.ignore_possibly_undefined()),
        (ty, ObservationEdge::EnumMember(name)) => ty.resolved_enum_member(db, env, name),
        (Type::Union(union), ObservationEdge::UnionElement(index)) => {
            union.elements(db).get(*index).copied()
        }
        (Type::Intersection(intersection), ObservationEdge::IntersectionPositive(index)) => {
            intersection.positive(db).get_index(*index).copied()
        }
        (Type::Intersection(intersection), ObservationEdge::IntersectionNegative(index)) => {
            intersection.negative(db).iter().nth(*index).copied()
        }
        (
            _,
            ObservationEdge::TupleElement(_)
            | ObservationEdge::TupleSuffix(_)
            | ObservationEdge::TupleVariable,
        ) => {
            let tuple = template.exact_tuple_instance_spec(db)?;
            match (tuple.as_ref(), edge) {
                (Tuple::Fixed(elements), ObservationEdge::TupleElement(index)) => {
                    elements.all_elements().get(*index).copied()
                }
                (Tuple::Fixed(elements), ObservationEdge::TupleSuffix(index)) => {
                    elements.all_elements().iter().rev().nth(*index).copied()
                }
                (Tuple::Variable(elements), ObservationEdge::TupleElement(index)) => {
                    elements.prefix_elements().get(*index).copied()
                }
                (Tuple::Variable(elements), ObservationEdge::TupleSuffix(index)) => {
                    elements.suffix_elements().iter().rev().nth(*index).copied()
                }
                (Tuple::Variable(elements), ObservationEdge::TupleVariable) => {
                    match elements.variable() {
                        VariableSegment::Homogeneous(ty) => Some(ty),
                        VariableSegment::TypeVarTuple(typevar) => Some(Type::TypeVar(typevar)),
                    }
                }
                _ => None,
            }
        }
        (_, ObservationEdge::GenericArgument(index)) => {
            let specialization = match template {
                Type::TypeAlias(alias) => alias.specialization(db)?,
                Type::Recursive(recursive) => recursive.arguments(db)?,
                Type::GenericAlias(alias) => alias.specialization(db),
                Type::SubclassOf(subclass) => match subclass.subclass_of() {
                    SubclassOfInner::Class(ClassType::Generic(alias)) => alias.specialization(db),
                    _ => return None,
                },
                _ => template.class_specialization(db, env)?.1,
            };
            specialization.types(db).get(*index).copied()
        }
        (
            _,
            ObservationEdge::CallableOverload(overload)
            | ObservationEdge::CallableReturn { overload }
            | ObservationEdge::CallableParameter { overload, .. }
            | ObservationEdge::CallableReceiver { overload, .. },
        ) => {
            let signatures = match template {
                Type::Callable(callable) => callable.signatures(db),
                Type::FunctionLiteral(function) => function.signature(db),
                Type::BoundMethod(method) => method.bound_signatures(db)?,
                _ => return None,
            };
            let signature = signatures.overloads.get(*overload)?;
            match edge {
                ObservationEdge::CallableOverload(_) => Some(Type::Callable(CallableType::new(
                    db,
                    super::signatures::CallableSignature::single(signature.clone()),
                    super::callable::CallableTypeKind::Regular,
                ))),
                ObservationEdge::CallableReturn { .. } => Some(signature.return_ty),
                ObservationEdge::CallableParameter { parameter, .. } => signature
                    .parameters()
                    .get(*parameter)
                    .map(Parameter::annotated_type),
                ObservationEdge::CallableReceiver {
                    relation,
                    annotation,
                    ..
                } => signature
                    .receiver_relation_at(*relation)
                    .map(|(receiver, expected)| if *annotation { expected } else { receiver }),
                _ => None,
            }
        }
        (
            Type::SubclassOf(subclass),
            ObservationEdge::ProtocolMemberRead {
                name,
                class_access: true,
            },
        ) if let SubclassOfInner::Protocol(protocol) = subclass.subclass_of() => protocol
            .interface(db)
            .meta_member(db, env, template, name)
            .and_then(|member| member.place.ignore_possibly_undefined()),
        (
            Type::SubclassOf(subclass),
            ObservationEdge::ProtocolMemberWrite {
                name,
                class_access: true,
            },
        ) if let SubclassOfInner::Protocol(protocol) = subclass.subclass_of() => protocol
            .interface(db)
            .meta_write_requirement(db, env, template, name)
            .and_then(|(domain, _)| domain),
        (
            _,
            edge @ (ObservationEdge::ProtocolMemberRead { .. }
            | ObservationEdge::ProtocolMemberWrite { .. }),
        ) => template
            .as_protocol_instance(db)?
            .interface(db)
            .observation_child(db, edge),
        (Type::TypedDict(typed_dict), edge) => typed_dict.observation_child(db, env, edge),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use ruff_db::files::system_path_to_file;
    use ruff_db::system::DbWithWritableSystem;
    use ty_python_core::ProgramFile;

    use super::{
        CallableSelfBinding, ObservationEdge, ObservedShape, ObservedType, ObservedTypeOrigin,
    };
    use crate::db::tests::setup_db;
    use crate::place::global_symbol;
    use crate::types::callable::CallableTypeKind;
    use crate::types::cyclic::TypeIdentity;
    use crate::types::signatures::CallableSignature;
    use crate::types::tuple::TupleType;
    use crate::types::{
        BindingContext, BoundTypeVarInstance, CallableType, KnownClass, Parameter, Parameters,
        Signature, Type,
    };

    #[test]
    fn stored_tuple_payload_preserves_elements_hidden_by_class_argument() {
        let db = setup_db();
        let env = db.program_environment();
        let int = KnownClass::Int.to_instance(&db, &env);
        let tuple = TupleType::heterogeneous(&db, &env, [Type::object(), int, int]);
        let class = ObservedType::root(tuple.to_class_type(&db).into());

        // The class argument is object, but the precise tuple still stores both int positions.
        let elements: Vec<_> = class
            .stored_children(&db, &env)
            .into_iter()
            .flat_map(|child| child.stored_children(&db, &env))
            .filter(|child| child.ty == int)
            .collect();
        assert_eq!(elements.len(), 2);
        assert!(elements[0].origin().is_some());
        assert!(elements[1].origin().is_some());
        assert_ne!(elements[0].origin(), elements[1].origin());
    }

    #[test]
    fn unresolved_recipes_keep_occurrences_without_parent_values() {
        let input = ObservedType::root(Type::object());
        let first = input.unchanged_or_unresolved(Type::int_literal(1));
        let second = first.unchanged_or_unresolved(Type::int_literal(2));
        assert_eq!(first.dependency_origins(), second.dependency_origins());
        assert!(second.origin().is_none());
        let restored = second.recipe().observe();
        assert_eq!(restored.ty, second.ty);
        assert_eq!(restored.dependency_origins(), second.dependency_origins());
    }

    #[test]
    fn mapping_an_unknown_derivative_does_not_replay_its_parent() {
        let db = setup_db();
        let env = db.program_environment();
        let int = KnownClass::Int.to_instance(&db, &env);
        let parent = ObservedType::root(Type::any()).root_expression();
        let derived = parent.unchanged_or_unresolved(int);
        let visitor = crate::types::ApplyTypeMappingVisitor::new_for_type_construction(&env);
        let top = derived.apply_mapping(
            &db,
            &crate::types::TypeMapping::Materialize(crate::types::MaterializationKind::Top),
            &visitor,
        );
        let bottom = derived.apply_mapping(
            &db,
            &crate::types::TypeMapping::Materialize(crate::types::MaterializationKind::Bottom),
            &visitor,
        );

        assert_eq!(top.ty, int);
        assert_eq!(bottom.ty, int);
        assert!(top.origin().is_none());
        assert!(bottom.origin().is_none());
        assert_eq!(top.dependency_origins()[0].application, Type::any());
        assert_eq!(bottom.dependency_origins()[0].application, Type::any());
        assert_ne!(
            super::BoundSourceRecipe::from_observed(&top),
            super::BoundSourceRecipe::from_observed(&bottom)
        );

        // The stored premises do not depend on an unknown operation's temporary output type.
        assert_eq!(
            super::BoundSourceRecipe::from_observed(&derived),
            super::BoundSourceRecipe::from_observed(
                &derived.unchanged_or_unresolved(Type::int_literal(1))
            ),
        );
    }

    #[test]
    fn overload_projection_keeps_occurrence_separate_from_diagnostic_index() {
        let db = setup_db();
        let env = db.program_environment();
        let signature = Signature::new(Parameters::standard([]), Type::object());
        let callable = Type::Callable(CallableType::new(
            &db,
            CallableSignature::from_overloads([
                signature.clone().with_source_overload_index(Some(3)),
                signature.with_source_overload_index(Some(7)),
            ]),
            CallableTypeKind::Regular,
        ));
        let observed = ObservedType::root(callable);
        let first = observed
            .callable_overload(&db, &env, 0)
            .unwrap()
            .project(&db, &env, ObservationEdge::CallableReturn { overload: 0 })
            .unwrap();
        let second = observed
            .callable_overload(&db, &env, 1)
            .unwrap()
            .project(&db, &env, ObservationEdge::CallableReturn { overload: 0 })
            .unwrap();
        assert_eq!(first.ty, second.ty);
        assert!(!first.is_unresolved());
        assert!(!second.is_unresolved());
        assert_ne!(first.origin(), second.origin());
        assert_eq!(second.recipe().observe().origin(), second.origin());
    }

    #[test]
    fn callable_binding_preserves_receiver_and_parameter_domains() {
        let db = setup_db();
        let env = db.program_environment();
        let self_type = Type::TypeVar(BoundTypeVarInstance::synthetic_self(
            &db,
            Type::object(),
            BindingContext::Synthetic(env.program(&db)),
        ));
        let signature = Signature::new(
            Parameters::standard([
                Parameter::positional_only(None)
                    .with_annotated_type(KnownClass::Type.to_instance(&db, &env)),
                Parameter::positional_only(None).with_annotated_type(self_type),
            ]),
            self_type,
        )
        .bind_self(&db, &env, None);
        assert_eq!(signature.receiver_relations().count(), 1);
        let callable = CallableType::new(
            &db,
            CallableSignature::single(signature),
            CallableTypeKind::Regular,
        );
        let ty = Type::Callable(callable);
        let observation = ObservedType {
            ty,
            origin: Some(Rc::new(ObservedTypeOrigin {
                constructor: TypeIdentity::Other(ty),
                application: ty,
                node: ty.into(),
                operations: Box::default(),
            })),
            shape: ObservedShape::Expression,
        };
        let instance = KnownClass::Int.to_instance(&db, &env);
        let class = KnownClass::Int.to_class_literal(&db, &env);
        let bound = observation.bind_callable_self(
            &db,
            &env,
            CallableSelfBinding {
                receiver: class,
                self_type: instance,
            },
        );
        for (edge, expected) in [
            (
                ObservationEdge::CallableParameter {
                    overload: 0,
                    parameter: 0,
                },
                instance,
            ),
            (ObservationEdge::CallableReturn { overload: 0 }, instance),
            (
                ObservationEdge::CallableReceiver {
                    overload: 0,
                    relation: 0,
                    annotation: false,
                },
                class,
            ),
        ] {
            let child = bound.child_at(&db, &env, expected, edge);
            assert!(!child.is_unresolved());
            let origin = child.origin().unwrap();
            assert_eq!(
                ObservedType::instantiate_node(&db, &env, &origin, self_type,),
                Some(expected),
            );
        }
    }

    #[test]
    fn discharging_a_receiver_keeps_later_expression_positions() {
        let db = setup_db();
        let env = db.program_environment();
        let class = KnownClass::Int.to_class_literal(&db, &env);
        let any_class = KnownClass::Type.to_instance(&db, &env);
        let signature = Signature::new(
            Parameters::standard([
                Parameter::positional_only(None).with_annotated_type(class),
                Parameter::positional_only(None).with_annotated_type(any_class),
            ]),
            Type::Never,
        )
        .bind_self(&db, &env, None)
        .bind_self(&db, &env, None);
        assert_eq!(signature.receiver_relations().count(), 2);
        let callable = CallableType::new(
            &db,
            CallableSignature::single(signature),
            CallableTypeKind::Regular,
        );
        let ty = Type::Callable(callable);
        let observation = ObservedType {
            ty,
            origin: Some(Rc::new(ObservedTypeOrigin {
                constructor: TypeIdentity::Other(ty),
                application: ty,
                node: ty.into(),
                operations: Box::default(),
            })),
            shape: ObservedShape::Expression,
        };
        let bound = observation.bind_callable_self(
            &db,
            &env,
            CallableSelfBinding {
                receiver: class,
                self_type: KnownClass::Int.to_instance(&db, &env),
            },
        );
        let discharged = ObservationEdge::CallableReceiver {
            overload: 0,
            relation: 0,
            annotation: true,
        };
        let surviving = ObservationEdge::CallableReceiver {
            overload: 0,
            relation: 1,
            annotation: true,
        };
        assert_eq!(
            super::observation_child(&db, &env, bound.ty, &discharged),
            None
        );
        assert_eq!(
            super::observation_child(&db, &env, bound.ty, &surviving),
            Some(any_class)
        );
        let child = bound.child_at(&db, &env, any_class, surviving);
        assert!(!child.is_unresolved());
        let origin = child.origin().unwrap();
        assert_eq!(origin.node.template, any_class);
        assert_eq!(
            ObservedType::instantiate_node(&db, &env, &origin, origin.node.template,),
            Some(any_class),
        );
    }

    #[test]
    fn nominal_class_views_preserve_recursive_argument_expressions() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            "from typing import TypedDict\nclass Node[T](TypedDict):\n    child: T\ntype Tree[T] = Node[Tree[list[T]]]\nvalue: Tree[int]\n",
        )
        .unwrap();
        let env = db.program_environment();
        let file = system_path_to_file(&db, "/src/a.py").unwrap();
        let file = ProgramFile::new(&db, file, env.program(&db));
        let value = global_symbol(&db, file, "value").place.expect_type();
        let first = ObservedType::root(value)
            .unfold(&db, &env)
            .unwrap()
            .project(&db, &env, ObservationEdge::ClassView)
            .unwrap()
            .project(&db, &env, ObservationEdge::GenericArgument(0))
            .unwrap();
        let second = first
            .unfold(&db, &env)
            .unwrap()
            .project(&db, &env, ObservationEdge::ClassView)
            .unwrap()
            .project(&db, &env, ObservationEdge::GenericArgument(0))
            .unwrap();

        assert_ne!(first.ty, second.ty);
        let first = first.origin().unwrap();
        let second = second.origin().unwrap();
        assert_eq!(first.constructor, second.constructor);
        assert_eq!(first.node, second.node);
        assert_ne!(first.application, second.application);
    }

    #[test]
    fn tuple_edges_distinguish_a_substituted_parameter_from_an_equal_literal() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            "type Pair[T] = tuple[T, int]\npair: Pair[int]\n",
        )
        .unwrap();
        let env = db.program_environment();
        let file = system_path_to_file(&db, "/src/a.py").unwrap();
        let file = ProgramFile::new(&db, file, env.program(&db));
        let pair = global_symbol(&db, file, "pair").place.expect_type();
        let observed = ObservedType::root(pair).unfold(&db, &env).unwrap();
        let int = KnownClass::Int.to_instance(&db, &env);
        let first = observed.child_at(&db, &env, int, ObservationEdge::TupleElement(0));
        let second = observed.child_at(&db, &env, int, ObservationEdge::TupleElement(1));
        assert_eq!(first.ty, second.ty);
        assert!(!first.is_unresolved());
        assert!(!second.is_unresolved());
        let first = first.origin().unwrap();
        let second = second.origin().unwrap();
        assert_ne!(first.node, second.node);
        assert!(first.node.template.is_type_var());
        assert_eq!(second.node.template, int);
    }

    #[test]
    fn materialized_callable_children_keep_their_opposite_variance() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            r#"
from typing import Any, Callable
from ty_extensions import Top

type Callback = Callable[[Any], Any]
callback: Top[Callback]
"#,
        )
        .unwrap();
        let env = db.program_environment();
        let file = system_path_to_file(&db, "/src/a.py").unwrap();
        let file = ProgramFile::new(&db, file, env.program(&db));
        let callback = global_symbol(&db, file, "callback").place.expect_type();
        let observed = ObservedType::root(callback).unfold(&db, &env).unwrap();
        let parameter = observed.child_at(
            &db,
            &env,
            Type::Never,
            ObservationEdge::CallableParameter {
                overload: 0,
                parameter: 0,
            },
        );
        let result = observed.child_at(
            &db,
            &env,
            Type::object(),
            ObservationEdge::CallableReturn { overload: 0 },
        );
        assert!(!parameter.is_unresolved());
        assert!(!result.is_unresolved());
        assert_ne!(
            parameter.origin().unwrap().node,
            result.origin().unwrap().node
        );
        assert_eq!(parameter.ty, Type::Never);
        assert_eq!(result.ty, Type::object());
    }

    #[test]
    fn generic_children_keep_declared_variance_and_invariant_families() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            r#"
from typing import Any, Generic, TypeVar
from ty_extensions import Top
T = TypeVar("T", contravariant=True)
class Consumer(Generic[T]):
    def consume(self, value: T) -> None: ...
class Cell[T]:
    value: T

type Consumed[T] = Consumer[T]
type Stored[T] = Cell[T]
consumer: Top[Consumed[Any]]
cell: Top[Stored[Any]]
"#,
        )
        .unwrap();
        let env = db.program_environment();
        let file = system_path_to_file(&db, "/src/a.py").unwrap();
        let file = ProgramFile::new(&db, file, env.program(&db));
        for (name, expected) in [("consumer", Type::Never), ("cell", Type::any())] {
            let ty = global_symbol(&db, file, name).place.expect_type();
            let observed = ObservedType::root(ty).unfold(&db, &env).unwrap();
            let child = observed.child_at(&db, &env, expected, ObservationEdge::GenericArgument(0));
            assert!(!child.is_unresolved(), "{name}");
            assert_eq!(child.ty, expected);
            if name == "cell" {
                assert!(
                    observed
                        .ty
                        .class_specialization(&db, &env)
                        .unwrap()
                        .1
                        .materialization_kind(&db)
                        .is_some()
                );
            }
        }
    }

    #[test]
    fn union_output_order_preserves_the_parameter_expression() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            "type First[T] = T | int\ntype Last[T] = int | T\nfirst: First[str]\nlast: Last[str]\n",
        )
        .unwrap();
        let env = db.program_environment();
        let file = system_path_to_file(&db, "/src/a.py").unwrap();
        let file = ProgramFile::new(&db, file, env.program(&db));
        for name in ["first", "last"] {
            let ty = global_symbol(&db, file, name).place.expect_type();
            let observed = ObservedType::root(ty).unfold(&db, &env).unwrap();
            let str = KnownClass::Str.to_instance(&db, &env);
            let children = observed.union_children(&db, &env);
            let child = children.iter().find(|child| child.ty == str).unwrap();
            assert!(child.origin().unwrap().node.template.is_type_var());
        }
    }
}
