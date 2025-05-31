use crate::Db;
use crate::semantic_index::symbol::ScopeId;
use crate::semantic_index::{
    attribute_scopes, global_scope, semantic_index, symbol_table, use_def_map,
};
use crate::symbol::{imported_symbol, symbol_from_bindings, symbol_from_declarations};
use crate::types::{ClassBase, ClassLiteral, KnownClass, Type};
use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;

struct AllMembers {
    members: FxHashSet<Name>,
}

impl AllMembers {
    fn of<'db>(db: &'db dyn Db, ty: Type<'db>) -> Self {
        let mut all_members = Self {
            members: FxHashSet::default(),
        };
        all_members.extend_with_type(db, ty);
        all_members
    }

    fn extend_with_type<'db>(&mut self, db: &'db dyn Db, ty: Type<'db>) {
        match ty {
            Type::Union(union) => self.members.extend(
                union
                    .elements(db)
                    .iter()
                    .map(|ty| AllMembers::of(db, *ty).members)
                    .reduce(|acc, members| acc.intersection(&members).cloned().collect())
                    .unwrap_or_default(),
            ),

            Type::Intersection(intersection) => self.members.extend(
                intersection
                    .positive(db)
                    .iter()
                    .map(|ty| AllMembers::of(db, *ty).members)
                    .reduce(|acc, members| acc.union(&members).cloned().collect())
                    .unwrap_or_default(),
            ),

            Type::NominalInstance(instance) => {
                let (class_literal, _specialization) = instance.class.class_literal(db);
                self.extend_with_class_members(db, class_literal);
                self.extend_with_instance_members(db, class_literal);
            }

            Type::ClassLiteral(class_literal) => {
                self.extend_with_class_members(db, class_literal);

                if let Type::ClassLiteral(meta_class_literal) = ty.to_meta_type(db) {
                    self.extend_with_class_members(db, meta_class_literal);
                }
            }

            Type::GenericAlias(generic_alias) => {
                let class_literal = generic_alias.origin(db);
                self.extend_with_class_members(db, class_literal);
            }

            Type::SubclassOf(subclass_of_type) => {
                if let Some(class_literal) = subclass_of_type.subclass_of().into_class() {
                    self.extend_with_class_members(db, class_literal.class_literal(db).0);
                }
            }

            Type::Dynamic(_) | Type::Never | Type::AlwaysTruthy | Type::AlwaysFalsy => {}

            Type::IntLiteral(_)
            | Type::BooleanLiteral(_)
            | Type::StringLiteral(_)
            | Type::BytesLiteral(_)
            | Type::LiteralString
            | Type::Tuple(_)
            | Type::PropertyInstance(_)
            | Type::FunctionLiteral(_)
            | Type::BoundMethod(_)
            | Type::MethodWrapper(_)
            | Type::WrapperDescriptor(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::Callable(_)
            | Type::ProtocolInstance(_)
            | Type::SpecialForm(_)
            | Type::KnownInstance(_)
            | Type::TypeVar(_)
            | Type::BoundSuper(_) => {
                if let Type::ClassLiteral(class_literal) = ty.to_meta_type(db) {
                    self.extend_with_class_members(db, class_literal);
                }
            }

            Type::ModuleLiteral(literal) => {
                self.extend_with_type(db, KnownClass::ModuleType.to_instance(db));

                let Some(file) = literal.module(db).file() else {
                    return;
                };

                let module_scope = global_scope(db, file);
                let use_def_map = use_def_map(db, module_scope);
                let symbol_table = symbol_table(db, module_scope);

                for (symbol_id, _) in use_def_map.all_public_declarations() {
                    let symbol_name = symbol_table.symbol(symbol_id).name();
                    if !imported_symbol(db, file, symbol_name, None)
                        .symbol
                        .is_unbound()
                    {
                        self.members
                            .insert(symbol_table.symbol(symbol_id).name().clone());
                    }
                }
            }
        }
    }

    fn extend_with_declarations_and_bindings(&mut self, db: &dyn Db, scope_id: ScopeId) {
        let use_def_map = use_def_map(db, scope_id);
        let symbol_table = symbol_table(db, scope_id);

        for (symbol_id, declarations) in use_def_map.all_public_declarations() {
            if symbol_from_declarations(db, declarations)
                .is_ok_and(|result| !result.symbol.is_unbound())
            {
                self.members
                    .insert(symbol_table.symbol(symbol_id).name().clone());
            }
        }

        for (symbol_id, bindings) in use_def_map.all_public_bindings() {
            if !symbol_from_bindings(db, bindings).is_unbound() {
                self.members
                    .insert(symbol_table.symbol(symbol_id).name().clone());
            }
        }
    }

    fn extend_with_class_members<'db>(
        &mut self,
        db: &'db dyn Db,
        class_literal: ClassLiteral<'db>,
    ) {
        for parent in class_literal
            .iter_mro(db, None)
            .filter_map(ClassBase::into_class)
            .map(|class| class.class_literal(db).0)
        {
            let parent_scope = parent.body_scope(db);
            self.extend_with_declarations_and_bindings(db, parent_scope);
        }
    }

    fn extend_with_instance_members<'db>(
        &mut self,
        db: &'db dyn Db,
        class_literal: ClassLiteral<'db>,
    ) {
        for parent in class_literal
            .iter_mro(db, None)
            .filter_map(ClassBase::into_class)
            .map(|class| class.class_literal(db).0)
        {
            let class_body_scope = parent.body_scope(db);
            let file = class_body_scope.file(db);
            let index = semantic_index(db, file);
            for function_scope_id in attribute_scopes(db, class_body_scope) {
                let attribute_table = index.instance_attribute_table(function_scope_id);
                for symbol in attribute_table.symbols() {
                    self.members.insert(symbol.name().clone());
                }
            }
        }
    }
}

/// List all members of a given type: anything that would be valid when accessed
/// as an attribute on an object of the given type.
pub(crate) fn all_members<'db>(db: &'db dyn Db, ty: Type<'db>) -> FxHashSet<Name> {
    AllMembers::of(db, ty).members
}
