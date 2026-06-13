use super::variance::VarianceInferable;
use super::{
    BoundTypeVarInstance, CycleDetector, IntersectionType, Type, TypeVarVariance, UnionType,
    visitor,
};
use crate::Db;

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct TypeFormType<'db> {
    pub(crate) type_argument: Type<'db>,
}

pub(super) fn walk_typeform_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    typeform_type: TypeFormType<'db>,
    visitor: &V,
) {
    visitor.visit_type(db, typeform_type.type_argument(db));
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
    pub(crate) fn project_type_form(self, db: &'db dyn Db) -> Type<'db> {
        struct TypeFormArgument;
        type TypeFormArgumentVisitor<'db> =
            CycleDetector<TypeFormArgument, Type<'db>, Option<Type<'db>>>;

        fn project<'db>(
            db: &'db dyn Db,
            ty: Type<'db>,
            visitor: &TypeFormArgumentVisitor<'db>,
        ) -> Option<Type<'db>> {
            match {
                let __ty_view_value = ty;
                (__ty_view_value, __ty_view_value.data())
            } {
                (_, crate::types::TypeData::TypeForm(type_form)) => {
                    Some(type_form.type_argument(db))
                }
                (_, crate::types::TypeData::TypeAlias(alias)) => {
                    visitor.visit(ty, || project(db, alias.value_type(db), visitor))
                }
                (_, crate::types::TypeData::Union(union)) => {
                    let mut elements = union
                        .elements(db)
                        .iter()
                        .filter_map(|element| project(db, *element, visitor))
                        .peekable();
                    elements.peek()?;
                    Some(UnionType::from_elements(db, elements))
                }
                (_, crate::types::TypeData::Intersection(intersection)) => {
                    let mut elements = intersection
                        .iter_positive(db)
                        .filter_map(|element| project(db, element, visitor))
                        .peekable();
                    elements.peek()?;
                    Some(IntersectionType::from_elements(db, elements))
                }
                (_, crate::types::TypeData::TypeVar(typevar)) => visitor.visit(ty, || {
                    typevar
                        .typevar(db)
                        .bound_or_constraints(db)
                        .and_then(|bound_or_constraints| {
                            project(db, bound_or_constraints.as_type(db), visitor)
                        })
                }),
                (_, crate::types::TypeData::SpecialForm(special_form)) => {
                    special_form.type_form_argument(db)
                }
                (_, crate::types::TypeData::KnownInstance(instance))
                    if instance.is_type_form_value() =>
                {
                    instance.type_form_argument(db)
                }
                (
                    _,
                    crate::types::TypeData::ClassLiteral(_)
                    | crate::types::TypeData::GenericAlias(_)
                    | crate::types::TypeData::SubclassOf(_),
                ) => ty.to_instance(db),
                (_, _) => None,
            }
        }

        project(db, self, &TypeFormArgumentVisitor::default()).unwrap_or(self)
    }
}

impl<'db> VarianceInferable<'db> for TypeFormType<'db> {
    // `TypeForm` is covariant in its type argument.
    fn variance_of(self, db: &'db dyn Db, typevar: BoundTypeVarInstance<'db>) -> TypeVarVariance {
        self.type_argument(db).variance_of(db, typevar)
    }
}
