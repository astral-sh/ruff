//! Widen inferred types that can grow even after their recursive marker has disappeared.
//! For example, if `Box[T].next` is a `Box[tuple[T]]`, repeated reads starting at
//! `Box[int]` produce ever deeper specializations. Keep the outer constructors and use
//! a marked `Unknown` at the depth limit. A second query that wraps that result can then
//! apply the same bound even when Salsa chose another query as the cycle head.

use std::cell::{Cell, RefCell};
use std::collections::hash_map::Entry;

use rustc_hash::FxHashMap;

use super::{
    ApplyTypeMappingVisitor, BoundTypeVarInstance, ClassType, DynamicType, KnownBoundMethodType,
    KnownInstanceType, RecursiveType, SubclassOfInner, Type, TypeAliasType, TypeContext,
    TypeMapping, UnionBuilder, WideningLimit,
    generics::walk_specialization_types,
    known_instance::walk_known_instance_type,
    signatures::Parameter,
    type_alias::walk_type_alias_arguments,
    visitor::{TypeKind, TypeVisitor, walk_non_atomic_type},
};
use crate::{Db, ProgramEnvironment};

const MAX_CONSTRUCTOR_DEPTH: u8 = 16;
const MAX_UNION_ELEMENTS: usize = 64;

impl<'db> Type<'db> {
    /// A deep annotation can move through a cycle without growing. Keep exact types until
    /// the structure grows beyond a limit and the projection can no longer distinguish
    /// successive results. Two types that differ only at a leaf can still form a finite union.
    pub(super) fn widen_growing_inference(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        previous: Self,
    ) -> Self {
        if self == previous || !has_widening_children(db, self) {
            return self;
        }
        let cache = RefCell::default();
        let widening = CycleWidening::new(&cache);
        let visitor = ApplyTypeMappingVisitor::new(env);
        let bounded = widening.project(db, self, &visitor);
        if bounded == self {
            return self;
        }
        let shapes = ShapeVisitor::new(env);
        let current = shapes.measure(db, self);
        if current.widened
            || (bounded == widening.project(db, previous, &visitor)
                && current.grew_from(shapes.measure(db, previous)))
        {
            bounded
        } else {
            self
        }
    }

    /// Reapply the depth limit to a binding that carries a previously widened type.
    pub(super) fn propagate_widened_inference(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Self {
        if has_widening_children(db, self) && ShapeVisitor::new(env).measure(db, self).widened {
            CycleWidening::new(&RefCell::default()).project(
                db,
                self,
                &ApplyTypeMappingVisitor::new(env),
            )
        } else {
            self
        }
    }
}

#[derive(Clone, Copy, Default)]
struct InferenceShape {
    depth: usize,
    union_width: usize,
    widened: bool,
}

impl InferenceShape {
    fn grew_from(self, previous: Self) -> bool {
        self.depth > previous.depth
            || (self.union_width > MAX_UNION_ELEMENTS && self.union_width > previous.union_width)
    }
}

/// Measure stored arguments, receivers and callable defaults without evaluating declarations.
struct ShapeVisitor<'env, 'db> {
    env: &'env ProgramEnvironment<'db>,
    children: Cell<InferenceShape>,
    cache: RefCell<FxHashMap<Type<'db>, InferenceShape>>,
}

impl<'env, 'db> ShapeVisitor<'env, 'db> {
    fn new(env: &'env ProgramEnvironment<'db>) -> Self {
        Self {
            env,
            children: Cell::default(),
            cache: RefCell::default(),
        }
    }

    fn measure(&self, db: &'db dyn Db, ty: Type<'db>) -> InferenceShape {
        if !has_widening_children(db, ty) {
            return InferenceShape {
                widened: matches!(ty, Type::Dynamic(DynamicType::Widened(_))),
                ..InferenceShape::default()
            };
        }
        match self.cache.borrow_mut().entry(ty) {
            Entry::Occupied(entry) => return *entry.get(),
            Entry::Vacant(entry) => {
                // A back edge adds no new structure to the current path.
                entry.insert(InferenceShape::default());
            }
        }
        let outer = self.children.take();
        if let TypeKind::NonAtomic(compound) = TypeKind::from(ty) {
            walk_non_atomic_type(db, compound, self);
        }
        let mut shape = self.children.replace(outer);
        match ty {
            Type::Union(union) => {
                shape.union_width = shape.union_width.max(union.elements(db).len());
            }
            Type::Intersection(_) => {}
            _ => shape.depth += 1,
        }
        self.cache.borrow_mut().insert(ty, shape);
        shape
    }
}

impl<'db> TypeVisitor<'db> for ShapeVisitor<'_, 'db> {
    fn program_environment(&self) -> &ProgramEnvironment<'db> {
        self.env
    }

    fn should_visit_lazy_type_attributes(&self) -> bool {
        false
    }

    fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
        let child = self.measure(db, ty);
        let siblings = self.children.get();
        self.children.set(InferenceShape {
            depth: child.depth.max(siblings.depth),
            union_width: child.union_width.max(siblings.union_width),
            widened: child.widened || siblings.widened,
        });
    }

    fn visit_parameter(&self, db: &'db dyn Db, parameter: &Parameter<'db>) {
        self.visit_type(db, parameter.annotated_type());
        if let Some(default) = parameter.eager_default_type() {
            self.visit_type(db, default);
        }
    }

    // Type parameter declarations are shared by all specializations. Only the supplied
    // arguments of a specialization can acquire a marker when it is widened.
    fn visit_bound_type_var_type(&self, _db: &'db dyn Db, _typevar: BoundTypeVarInstance<'db>) {}

    fn visit_type_alias_type(&self, db: &'db dyn Db, alias: TypeAliasType<'db>) {
        walk_type_alias_arguments(db, alias, self);
    }

    fn visit_recursive_type(&self, db: &'db dyn Db, recursive: RecursiveType<'db>) {
        if let Some(arguments) = recursive.arguments(db) {
            walk_specialization_types(db, arguments, self);
        }
    }

    fn visit_known_instance_type(&self, db: &'db dyn Db, instance: KnownInstanceType<'db>) {
        match instance {
            KnownInstanceType::TypeAliasType(alias) => self.visit_type_alias_type(db, alias),
            KnownInstanceType::FunctoolsPartial(partial)
            | KnownInstanceType::FunctoolsPartialCall(partial) => {
                self.visit_type(db, partial.wrapped(db).inner(db));
                self.visit_callable_type(db, partial.partial(db));
            }
            _ => walk_known_instance_type(db, instance, self),
        }
    }
}

fn has_widening_children<'db>(db: &'db dyn Db, ty: Type<'db>) -> bool {
    // Declarations stay opaque. Their stored type arguments can grow across a recursive
    // assignment, but looking up their bases, alias bodies, or method signatures here can
    // introduce a new query cycle while Salsa is recovering the current one.
    match ty {
        Type::NominalInstance(instance) => instance.is_definition_generic(db),
        Type::ProtocolInstance(protocol) => protocol
            .class_origin(db)
            .is_none_or(|class| class.is_generic()),
        Type::TypedDict(dict) => dict.defining_class().is_none_or(ClassType::is_generic),
        Type::TypeAlias(alias) | Type::KnownInstance(KnownInstanceType::TypeAliasType(alias)) => {
            alias.specialization(db).is_some()
        }
        Type::Recursive(recursive) => recursive.arguments(db).is_some(),
        Type::SubclassOf(subclass) => match subclass.subclass_of() {
            SubclassOfInner::Class(class) => class.is_generic(),
            SubclassOfInner::Protocol(protocol) => protocol
                .class_origin(db)
                .is_none_or(|class| class.is_generic()),
            SubclassOfInner::Dynamic(_) | SubclassOfInner::TypeVar(_) => false,
        },
        Type::Union(_)
        | Type::Intersection(_)
        | Type::GenericAlias(_)
        | Type::Callable(_)
        | Type::FunctionLiteral(_)
        | Type::BoundMethod(_)
        | Type::BoundSuper(_)
        | Type::TypeForm(_)
        | Type::TypeGuard(_)
        | Type::TypeIs(_)
        | Type::PropertyInstance(_)
        | Type::SlotDescriptor(_)
        | Type::KnownBoundMethod(
            KnownBoundMethodType::FunctionTypeDunderGet(_)
            | KnownBoundMethodType::MethodTypeDunderGet(_)
            | KnownBoundMethodType::DunderCall(_)
            | KnownBoundMethodType::PropertyDunderGet(_)
            | KnownBoundMethodType::PropertyDunderSet(_)
            | KnownBoundMethodType::PropertyDunderDelete(_),
        )
        | Type::KnownInstance(
            KnownInstanceType::Annotated(_)
            | KnownInstanceType::Callable(_)
            | KnownInstanceType::Field(_)
            | KnownInstanceType::FunctoolsPartial(_)
            | KnownInstanceType::FunctoolsPartialCall(_)
            | KnownInstanceType::MethodWrapper(_)
            | KnownInstanceType::TypeGenericAlias(_),
        ) => true,
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, get_size2::GetSize)]
pub struct CycleWidening<'a, 'db> {
    remaining: u8,
    cache: &'a RefCell<FxHashMap<(Type<'db>, u8), Type<'db>>>,
}

impl<'a, 'db> CycleWidening<'a, 'db> {
    fn new(cache: &'a RefCell<FxHashMap<(Type<'db>, u8), Type<'db>>>) -> Self {
        Self {
            remaining: MAX_CONSTRUCTOR_DEPTH,
            cache,
        }
    }

    pub(super) fn project(
        self,
        db: &'db dyn Db,
        ty: Type<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        if !has_widening_children(db, ty) {
            return ty;
        }

        let key = (ty, self.remaining);
        if let Some(&widened) = self.cache.borrow().get(&key) {
            return widened;
        }
        let widened = if let Type::Union(union) = ty {
            // A chain such as `x = x.next` can add a different class on every visit without
            // ever nesting a constructor. Once widened, that position absorbs later alternatives
            // so an initializer cannot restart the same chain.
            let elements = union.elements(db);
            let widened = Type::Dynamic(DynamicType::Widened(WideningLimit::UnionWidth));
            if elements.len() > MAX_UNION_ELEMENTS {
                return widened;
            }
            // A loop such as `x = [x]` can accumulate a union of all earlier results in
            // each new element type. Those nested alternatives grow exponentially in size.
            // A flat union of unrelated alternatives does not add any constructor nesting.
            let branch = if self.remaining < MAX_CONSTRUCTOR_DEPTH {
                Self {
                    remaining: self.remaining / 2,
                    ..self
                }
            } else {
                self
            };
            let mut mapped = None;
            for (index, &element) in elements.iter().enumerate() {
                let projected = branch.project(db, element, visitor);
                if projected == widened {
                    return widened;
                }
                if mapped.is_none() && projected != element {
                    let builder = UnionBuilder::new(db, visitor.env)
                        .cycle_recovery(true)
                        .or_recursively_defined(union.recursively_defined(db));
                    mapped = Some(
                        elements[..index]
                            .iter()
                            .copied()
                            .fold(builder, UnionBuilder::add),
                    );
                }
                if let Some(builder) = &mut mapped {
                    builder.add_in_place(projected);
                }
            }
            mapped.map_or(ty, UnionBuilder::build)
        } else {
            let remaining = if matches!(ty, Type::Intersection(_)) {
                self.remaining
            } else if let Some(remaining) = self.remaining.checked_sub(1) {
                remaining
            } else {
                return Type::Dynamic(DynamicType::Widened(WideningLimit::Depth));
            };
            let child = Self { remaining, ..self };
            if let Type::BoundSuper(bound_super) = ty {
                bound_super
                    .widen(db, |ty| child.project(db, ty, visitor))
                    .map_or(
                        Type::Dynamic(DynamicType::Widened(WideningLimit::Depth)),
                        Type::BoundSuper,
                    )
            } else {
                ty.map_type(
                    db,
                    &TypeMapping::WidenRecursiveInference(child),
                    TypeContext::default(),
                    visitor,
                )
            }
        };
        self.cache.borrow_mut().insert(key, widened);
        widened
    }
}
