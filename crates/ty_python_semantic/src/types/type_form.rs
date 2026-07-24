use super::variance::VarianceInferable;
use super::{
    BoundTypeVarIdentity, CycleDetector, IntersectionType, Type, TypeVarVariance, UnionType,
    visitor,
};
use crate::Db;
use crate::SemanticEnvironment;

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct TypeFormType<'db> {
    #[returns(copy)]
    pub(crate) type_argument: Type<'db>,
}

pub(super) fn walk_typeform_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    env: &SemanticEnvironment<'db>,
    typeform_type: TypeFormType<'db>,
    visitor: &V,
) {
    let db = env.db();
    visitor.visit_type(env, typeform_type.type_argument(db));
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for TypeFormType<'_> {}

impl<'db> TypeFormType<'db> {
    pub(crate) fn from_type_expression(db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        Type::TypeForm(Self::new(db, ty))
    }
}

impl<'db> Type<'db> {
    /// Projects type-form values to the static types that they represent.
    ///
    /// This projects `TypeForm[T]` to `T` and runtime class forms to their instance types. It also
    /// recursively projects type aliases, unions, positive intersection elements, and type-variable
    /// bounds or constraints, using cycle detection for recursive types. Union and intersection
    /// elements that do not represent type forms are ignored, as are negative intersection
    /// elements. If no type-form component can be projected, this returns the original type.
    pub(crate) fn project_type_form(self, env: &SemanticEnvironment<'db>) -> Type<'db> {
        struct TypeFormArgument;
        type TypeFormArgumentVisitor<'db> =
            CycleDetector<'db, TypeFormArgument, Type<'db>, Option<Type<'db>>, 3>;

        fn project<'db>(
            env: &SemanticEnvironment<'db>,
            ty: Type<'db>,
            visitor: &TypeFormArgumentVisitor<'db>,
        ) -> Option<Type<'db>> {
            let db = env.db();
            match ty {
                Type::TypeForm(type_form) => Some(type_form.type_argument(db)),
                Type::TypeAlias(alias) => {
                    visitor.visit(env, ty, || project(env, alias.value_type(env), visitor))
                }
                Type::Union(union) => {
                    let mut elements = union
                        .elements(db)
                        .iter()
                        .filter_map(|element| project(env, *element, visitor))
                        .peekable();
                    elements.peek()?;
                    Some(UnionType::from_elements(env, elements))
                }
                Type::Intersection(intersection) => {
                    let mut elements = intersection
                        .iter_positive(db)
                        .filter_map(|element| project(env, element, visitor))
                        .peekable();
                    elements.peek()?;
                    Some(IntersectionType::from_elements(env, elements))
                }
                Type::TypeVar(typevar) => visitor.visit(env, ty, || {
                    typevar
                        .typevar(db)
                        .bound_or_constraints(env)
                        .and_then(|bound_or_constraints| {
                            project(env, bound_or_constraints.as_type(env), visitor)
                        })
                }),
                Type::SpecialForm(special_form) => special_form.type_form_argument(env),
                Type::KnownInstance(instance) if instance.is_type_form_value() => {
                    instance.type_form_argument(env)
                }
                Type::ClassLiteral(_) | Type::GenericAlias(_) | Type::SubclassOf(_) => {
                    ty.to_instance_approximation(env)
                }
                _ => None,
            }
        }

        project(env, self, &TypeFormArgumentVisitor::default()).unwrap_or(self)
    }
}

impl<'db> VarianceInferable<'db> for TypeFormType<'db> {
    // `TypeForm` is covariant in its type argument.
    fn variance_of(
        self,
        env: &SemanticEnvironment<'db>,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> TypeVarVariance {
        let db = env.db();
        self.type_argument(db).variance_of(env, typevar)
    }
}
