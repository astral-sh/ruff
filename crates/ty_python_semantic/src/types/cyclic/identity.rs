//! Compose recursion identities through stored type structure.

use super::TypeIdentity;
use crate::Db;
use crate::types::callable::{CallableTypeKind, SignatureSource};
use crate::types::function::OverloadLiteral;
use crate::types::generics::{GenericContext, Specialization};
use crate::types::set_theoretic::RecursivelyDefined;
use crate::types::signatures::SignatureIdentityShape;
use crate::types::tuple::{Tuple, TupleSpec, VariableSegment};
use crate::types::{
    CallableType, ClassType, MaterializationKind, StaticClassLiteral, Type, TypeAliasType,
};
use rustc_hash::FxHashMap;

/// The shape surrounding a growing recursive reference.
///
/// For `list[A[list[int]]]`, the list and its specialization remain part of the key, but
/// the growing alias `A` uses its definition's identity. Keeping only the outer definition
/// misses recursion through containers; dropping every argument conflates finite types.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct StructuralTypeIdentity<'db> {
    #[returns(ref)]
    kind: StructureKind<'db>,
    #[returns(deref)]
    children: Box<[TypeIdentity<'db>]>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for StructuralTypeIdentity<'_> {}

#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub enum StructureKind<'db> {
    Union(RecursivelyDefined),
    Intersection {
        positive: usize,
    },
    Nominal {
        origin: StaticClassLiteral<'db>,
        inherits_from_explicit_any: bool,
    },
    GenericAlias(StaticClassLiteral<'db>),
    SubclassOf,
    Protocol(StaticClassLiteral<'db>),
    TypedDict(StaticClassLiteral<'db>),
    Alias {
        origin: TypeAliasType<'db>,
        materialization: Option<MaterializationKind>,
    },
    Specialization {
        context: GenericContext<'db>,
        materialization: Option<MaterializationKind>,
        has_tuple: bool,
    },
    FixedTuple,
    VariableTuple {
        prefix: usize,
        unpacked_typevar: bool,
    },
    TypeForm,
    SlotDescriptor,
    Callable {
        kind: CallableTypeKind,
        deprecated: Option<OverloadLiteral<'db>>,
        projected: bool,
    },
    Signature(SignatureIdentityShape<'db>),
}

pub(super) fn type_identity<'db>(db: &'db dyn Db, ty: Type<'db>) -> TypeIdentity<'db> {
    ty.recursive_identity(db)
        .or_else(|| {
            IdentityBuilder::default()
                .of_type(db, ty)
                .map(TypeIdentity::Structural)
        })
        .unwrap_or(TypeIdentity::Other(ty))
}

/// Visit shared stored subtrees once while constructing an identity.
#[derive(Default)]
struct IdentityBuilder<'db> {
    identities: FxHashMap<Type<'db>, TypeIdentity<'db>>,
}

impl<'db> IdentityBuilder<'db> {
    fn type_identity(&mut self, db: &'db dyn Db, ty: Type<'db>) -> TypeIdentity<'db> {
        if let Some(identity) = self.identities.get(&ty) {
            return *identity;
        }
        let identity = ty
            .recursive_identity(db)
            .or_else(|| self.of_type(db, ty).map(TypeIdentity::Structural))
            .unwrap_or(TypeIdentity::Other(ty));
        self.identities.insert(ty, identity);
        identity
    }

    fn of_type(&mut self, db: &'db dyn Db, ty: Type<'db>) -> Option<StructuralTypeIdentity<'db>> {
        match ty {
            Type::Union(union) => self.compose(
                db,
                StructureKind::Union(union.recursively_defined(db)),
                union.elements(db).iter().copied(),
            ),
            Type::Intersection(intersection) => self.compose(
                db,
                StructureKind::Intersection {
                    positive: intersection.positive(db).len(),
                },
                intersection
                    .positive(db)
                    .iter()
                    .copied()
                    .chain(intersection.negative(db).iter().copied()),
            ),
            Type::NominalInstance(instance) => {
                if let Some(class) = instance.stored_class(db) {
                    self.compose_class(db, class, |origin| StructureKind::Nominal {
                        origin,
                        inherits_from_explicit_any: instance.inherits_from_explicit_any(),
                    })
                } else {
                    self.compose_tuple(db, instance.own_tuple_spec(db)?.as_ref())
                }
            }
            Type::GenericAlias(alias) => {
                self.compose_class(db, ClassType::Generic(alias), StructureKind::GenericAlias)
            }
            Type::TypeAlias(alias) => Some(StructuralTypeIdentity::new(
                db,
                StructureKind::Alias {
                    origin: alias.unspecialized(db),
                    materialization: alias.materialization_kind(db),
                },
                Box::<[_]>::from([TypeIdentity::Structural(
                    self.compose_specialization(db, alias.specialization(db)?)?,
                )]),
            )),
            Type::SubclassOf(subclass) => {
                self.compose(db, StructureKind::SubclassOf, [subclass.into()])
            }
            Type::ProtocolInstance(protocol) => {
                self.compose_class(db, *protocol.class_origin(db)?, StructureKind::Protocol)
            }
            Type::TypedDict(typed_dict) => {
                self.compose_class(db, typed_dict.defining_class()?, StructureKind::TypedDict)
            }
            Type::TypeForm(type_form) => {
                self.compose(db, StructureKind::TypeForm, [type_form.type_argument(db)])
            }
            Type::SlotDescriptor(descriptor) => self.compose(
                db,
                StructureKind::SlotDescriptor,
                [descriptor.value_type(db)],
            ),
            Type::Callable(callable) => self.compose_callable(db, callable),
            _ => None,
        }
    }

    /// Retain an exact key unless a child actually abstracts a recursive reference.
    fn compose(
        &mut self,
        db: &'db dyn Db,
        kind: StructureKind<'db>,
        types: impl IntoIterator<Item = Type<'db>>,
    ) -> Option<StructuralTypeIdentity<'db>> {
        let (children, changed) = self.children(db, types);
        changed.then(|| StructuralTypeIdentity::new(db, kind, children))
    }

    fn children(
        &mut self,
        db: &'db dyn Db,
        types: impl IntoIterator<Item = Type<'db>>,
    ) -> (Box<[TypeIdentity<'db>]>, bool) {
        let mut changed = false;
        let children: Box<[_]> = types
            .into_iter()
            .map(|ty| {
                let identity = self.type_identity(db, ty);
                changed |= identity != TypeIdentity::Other(ty);
                identity
            })
            .collect();
        (children, changed)
    }

    fn compose_callable(
        &mut self,
        db: &'db dyn Db,
        callable: CallableType<'db>,
    ) -> Option<StructuralTypeIdentity<'db>> {
        let kind = StructureKind::Callable {
            kind: callable.kind(db),
            deprecated: callable.deprecated(db),
            projected: matches!(callable.signature_source(db), SignatureSource::Projected(_)),
        };
        match callable.signature_source(db) {
            SignatureSource::Lambda(_) => None,
            SignatureSource::Projected(projection) => self.compose(db, kind, [projection.source()]),
            SignatureSource::Explicit(signatures) => {
                let mut changed = false;
                let signatures: Vec<_> = signatures
                    .iter()
                    .map(|signature| {
                        let (shape, types) = signature.recursion_identity_parts();
                        let (children, signature_changed) = self.children(db, types);
                        changed |= signature_changed;
                        (shape, children)
                    })
                    .collect();
                changed.then(|| {
                    StructuralTypeIdentity::new(
                        db,
                        kind,
                        signatures
                            .into_iter()
                            .map(|(shape, children)| {
                                TypeIdentity::Structural(StructuralTypeIdentity::new(
                                    db,
                                    StructureKind::Signature(shape),
                                    children,
                                ))
                            })
                            .collect::<Box<[_]>>(),
                    )
                })
            }
        }
    }

    fn compose_specialization(
        &mut self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Option<StructuralTypeIdentity<'db>> {
        let tuple = specialization.tuple_inner(db);
        self.compose(
            db,
            StructureKind::Specialization {
                context: specialization.generic_context(db),
                materialization: specialization.materialization_kind(db),
                has_tuple: tuple.is_some(),
            },
            specialization
                .types(db)
                .iter()
                .copied()
                .chain(tuple.map(Type::tuple)),
        )
    }

    fn compose_class(
        &mut self,
        db: &'db dyn Db,
        class: ClassType<'db>,
        kind: impl FnOnce(StaticClassLiteral<'db>) -> StructureKind<'db>,
    ) -> Option<StructuralTypeIdentity<'db>> {
        let ClassType::Generic(alias) = class else {
            return None;
        };
        Some(StructuralTypeIdentity::new(
            db,
            kind(alias.origin(db)),
            Box::<[_]>::from([TypeIdentity::Structural(
                self.compose_specialization(db, alias.specialization(db))?,
            )]),
        ))
    }

    fn compose_tuple(
        &mut self,
        db: &'db dyn Db,
        tuple: &TupleSpec<'db>,
    ) -> Option<StructuralTypeIdentity<'db>> {
        match tuple {
            Tuple::Fixed(tuple) => {
                self.compose(db, StructureKind::FixedTuple, tuple.iter_all_elements())
            }
            Tuple::Variable(tuple) => {
                let (variable, is_typevar) = match tuple.variable() {
                    VariableSegment::Homogeneous(ty) => (ty, false),
                    VariableSegment::TypeVarTuple(typevar) => (Type::TypeVar(typevar), true),
                };
                self.compose(
                    db,
                    StructureKind::VariableTuple {
                        prefix: tuple.prefix_elements().len(),
                        unpacked_typevar: is_typevar,
                    },
                    tuple
                        .iter_prefix_elements()
                        .chain([variable])
                        .chain(tuple.iter_suffix_elements()),
                )
            }
        }
    }
}
