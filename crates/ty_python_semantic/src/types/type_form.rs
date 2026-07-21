use super::variance::VarianceInferable;
use super::{
    BoundTypeVarIdentity, CycleDetector, IntersectionType, Type, TypeVarVariance, UnionType,
    visitor,
};
use crate::Db;
use crate::SemanticContext;

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct TypeFormType<'db> {
    #[returns(copy)]
    pub(crate) type_argument: Type<'db>,
}

pub(super) fn walk_typeform_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    ctx: &SemanticContext<'db>,
    typeform_type: TypeFormType<'db>,
    visitor: &V,
) {
    let db = ctx.db();
    visitor.visit_type(ctx, typeform_type.type_argument(db));
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
    pub(crate) fn project_type_form(self, ctx: &SemanticContext<'db>) -> Type<'db> {
        struct TypeFormArgument;
        type TypeFormArgumentVisitor<'db> =
            CycleDetector<'db, TypeFormArgument, Type<'db>, Option<Type<'db>>, 3>;

        fn project<'db>(
            ctx: &SemanticContext<'db>,
            ty: Type<'db>,
            visitor: &TypeFormArgumentVisitor<'db>,
        ) -> Option<Type<'db>> {
            let db = ctx.db();
            match ty {
                Type::TypeForm(type_form) => Some(type_form.type_argument(db)),
                Type::TypeAlias(alias) => {
                    visitor.visit(ctx, ty, || project(ctx, alias.value_type(ctx), visitor))
                }
                Type::Union(union) => {
                    let mut elements = union
                        .elements(db)
                        .iter()
                        .filter_map(|element| project(ctx, *element, visitor))
                        .peekable();
                    elements.peek()?;
                    Some(UnionType::from_elements(ctx, elements))
                }
                Type::Intersection(intersection) => {
                    let mut elements = intersection
                        .iter_positive(db)
                        .filter_map(|element| project(ctx, element, visitor))
                        .peekable();
                    elements.peek()?;
                    Some(IntersectionType::from_elements(ctx, elements))
                }
                Type::TypeVar(typevar) => visitor.visit(ctx, ty, || {
                    typevar
                        .typevar(db)
                        .bound_or_constraints(ctx)
                        .and_then(|bound_or_constraints| {
                            project(ctx, bound_or_constraints.as_type(ctx), visitor)
                        })
                }),
                Type::SpecialForm(special_form) => special_form.type_form_argument(ctx),
                Type::KnownInstance(instance) if instance.is_type_form_value() => {
                    instance.type_form_argument(ctx)
                }
                Type::ClassLiteral(_) | Type::GenericAlias(_) | Type::SubclassOf(_) => {
                    ty.to_instance_approximation(ctx)
                }
                _ => None,
            }
        }

        project(ctx, self, &TypeFormArgumentVisitor::default()).unwrap_or(self)
    }
}

impl<'db> VarianceInferable<'db> for TypeFormType<'db> {
    // `TypeForm` is covariant in its type argument.
    fn variance_of(
        self,
        ctx: &SemanticContext<'db>,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> TypeVarVariance {
        let db = ctx.db();
        self.type_argument(db).variance_of(ctx, typevar)
    }
}
