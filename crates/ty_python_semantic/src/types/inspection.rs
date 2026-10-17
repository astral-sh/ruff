//! Metadata queries that consume expanded types inside the semantic operation.

use crate::types::recursive::TypeQueryKey;
use crate::types::recursive::TypeQueryVisit;
use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;

use super::enums::EnumClassLiteral;
use super::{CycleKind, Type, TypeOperations, TypeQuery, TypeQueryContext};
use crate::Db;

/// A broad category for presenting a type in an editor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypeCategory {
    Function,
    Method,
    Module,
    Class,
    Instance,
    EnumLiteral,
    Value,
    Protocol,
    TypeVariable,
}

impl<'db> Type<'db> {
    /// Classify the first recognizable alternative after expanding aliases.
    /// Unions and positive intersection elements retain their existing order.
    pub fn category(self, db: &'db dyn Db, env: &TypeOperations<'db>) -> Option<TypeCategory> {
        TypeQueryContext::with_root(env, FindTypeCategory, |query| query.visit(db, self))
    }

    /// Collect string literals from aliases, unions, and positive intersection elements.
    /// Other alternatives contribute no candidates.
    pub(crate) fn string_literal_candidates(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
    ) -> Vec<String> {
        TypeQueryContext::with_root(env, StringLiteralCandidates, |query| query.visit(db, self))
    }

    /// Return one enum class and the canonical names of its literal members.
    /// Every alternative must be an enum literal from that same class.
    pub(crate) fn enum_literal_members(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
    ) -> Option<(EnumClassLiteral<'db>, FxHashSet<Name>)> {
        TypeQueryContext::with_root(env, EnumLiteralSubjectNames, |query| query.visit(db, self))
    }
}

struct FindTypeCategory;

impl<'db> TypeQuery<'db> for FindTypeCategory {
    fn recursion_key(&self) -> TypeQueryKey<'db> {
        TypeQueryKey::FindTypeCategory
    }

    type Output = Option<TypeCategory>;

    fn on_cycle(
        &self,
        _traversal: &TypeQueryVisit<'_, '_, 'db, Self>,
        _ty: Type<'db>,
        _kind: CycleKind,
        _state: (),
    ) -> Self::Output {
        None
    }

    fn visit(
        &self,
        db: &'db dyn Db,
        traversal: &TypeQueryVisit<'_, '_, 'db, Self>,
        ty: Type<'db>,
        _state: (),
    ) -> Self::Output {
        Some(match ty {
            Type::FunctionLiteral(_)
            | Type::DataclassDecorator(_)
            | Type::WrapperDescriptor(_)
            | Type::DataclassTransformer(_)
            | Type::Callable(_) => TypeCategory::Function,
            Type::BoundMethod(_) | Type::KnownBoundMethod(_) => TypeCategory::Method,
            Type::ModuleLiteral(_) => TypeCategory::Module,
            Type::ClassLiteral(_) | Type::GenericAlias(_) | Type::SubclassOf(_) => {
                TypeCategory::Class
            }
            Type::NominalInstance(_)
            | Type::PropertyInstance(_)
            | Type::SlotDescriptor(_)
            | Type::BoundSuper(_)
            | Type::TypedDict(_)
            | Type::NewTypeInstance(_)
            | Type::EnumComplement(_) => TypeCategory::Instance,
            Type::LiteralValue(literal) if literal.is_enum() => TypeCategory::EnumLiteral,
            Type::LiteralValue(_) | Type::TypeIs(_) | Type::TypeGuard(_) | Type::TypeForm(_) => {
                TypeCategory::Value
            }
            Type::ProtocolInstance(_) => TypeCategory::Protocol,
            Type::TypeVar(_) => TypeCategory::TypeVariable,
            Type::Union(union) => union
                .elements(db)
                .iter()
                .find_map(|&ty| traversal.visit(db, ty))?,
            Type::Intersection(intersection) => intersection
                .iter_positive(db)
                .find_map(|ty| traversal.visit(db, ty))?,
            Type::Dynamic(_)
            | Type::Divergent(_)
            | Type::Never
            | Type::SpecialForm(_)
            | Type::KnownInstance(_)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::TypeAlias(_)
            | Type::Recursive(_) => return None,
            Type::RecursiveVar(_) => {
                unreachable!("semantic operation on an unbound recursive variable")
            }
        })
    }
}

struct StringLiteralCandidates;

impl<'db> TypeQuery<'db> for StringLiteralCandidates {
    fn recursion_key(&self) -> TypeQueryKey<'db> {
        TypeQueryKey::StringLiteralCandidates
    }

    type Output = Vec<String>;

    fn on_cycle(
        &self,
        _traversal: &TypeQueryVisit<'_, '_, 'db, Self>,
        _ty: Type<'db>,
        _kind: CycleKind,
        _state: (),
    ) -> Self::Output {
        Vec::new()
    }

    fn visit(
        &self,
        db: &'db dyn Db,
        traversal: &TypeQueryVisit<'_, '_, 'db, Self>,
        ty: Type<'db>,
        _state: (),
    ) -> Self::Output {
        match ty {
            Type::LiteralValue(literal) => literal
                .as_string()
                .map(|string_literal| vec![string_literal.value(db).to_string()])
                .unwrap_or_default(),
            Type::Union(union) => union
                .elements(db)
                .iter()
                .flat_map(|element| traversal.visit(db, *element))
                .collect(),
            Type::Intersection(intersection) => intersection
                .positive(db)
                .iter()
                .flat_map(|element| traversal.visit(db, *element))
                .collect(),
            _ => Vec::new(),
        }
    }
}

struct EnumLiteralSubjectNames;

impl<'db> TypeQuery<'db> for EnumLiteralSubjectNames {
    fn recursion_key(&self) -> TypeQueryKey<'db> {
        TypeQueryKey::EnumLiteralSubjectNames
    }

    type Output = Option<(EnumClassLiteral<'db>, FxHashSet<Name>)>;

    fn on_cycle(
        &self,
        _context: &TypeQueryVisit<'_, '_, 'db, Self>,
        _ty: Type<'db>,
        _kind: CycleKind,
        (): (),
    ) -> Self::Output {
        None
    }

    fn visit(
        &self,
        db: &'db dyn Db,
        context: &TypeQueryVisit<'_, '_, 'db, Self>,
        ty: Type<'db>,
        (): (),
    ) -> Self::Output {
        if let Type::Union(union) = ty {
            let mut elements = union.elements(db).iter();
            let (class, mut names) = context.visit(db, *elements.next()?)?;
            for element in elements {
                let (other_class, other_names) = context.visit(db, *element)?;
                if class != other_class {
                    return None;
                }
                names.extend(other_names);
            }
            return Some((class, names));
        }
        let literal = ty.as_enum_literal()?;
        let class = literal.enum_class_literal(db);
        let name = class.resolve_member(db, literal.name(db))?;
        Some((class, [name.clone()].into_iter().collect()))
    }
}
