#![allow(dead_code)]
use crate::ast_ids::NodeKey;
use crate::db::{QueryResult, SemanticDb, SemanticJar};
use crate::files::FileId;
use crate::module::{Module, ModuleName};
use crate::symbols::{
    resolve_global_symbol, symbol_table, GlobalSymbolId, ScopeId, ScopeKind, SymbolId,
};
use crate::{FxDashMap, FxIndexSet, Name};
use ruff_index::{newtype_index, IndexVec};
use rustc_hash::FxHashMap;

pub(crate) mod infer;

pub(crate) use infer::{infer_definition_type, infer_symbol_public_type};

/// unique ID for a type
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    /// the dynamic or gradual type: a statically-unknown set of values
    Any,
    /// the empty set of values
    Never,
    /// unknown type (no annotation)
    /// equivalent to Any, or to object in strict mode
    Unknown,
    /// name is not bound to any value
    Unbound,
    /// a specific function object
    Function(FunctionTypeId),
    /// a specific module object
    Module(ModuleTypeId),
    /// a specific class object
    Class(ClassTypeId),
    /// the set of Python objects with the given class in their __class__'s method resolution order
    Instance(ClassTypeId),
    Union(UnionTypeId),
    Intersection(IntersectionTypeId),
    IntLiteral(i64),
    // TODO protocols, callable types, overloads, generics, type vars
}

impl Type {
    fn display<'a>(&'a self, store: &'a TypeStore) -> DisplayType<'a> {
        DisplayType { ty: self, store }
    }

    pub const fn is_unbound(&self) -> bool {
        matches!(self, Type::Unbound)
    }

    pub const fn is_unknown(&self) -> bool {
        matches!(self, Type::Unknown)
    }

    pub fn get_member(&self, db: &dyn SemanticDb, name: &Name) -> QueryResult<Option<Type>> {
        match self {
            Type::Any => todo!("attribute lookup on Any type"),
            Type::Never => todo!("attribute lookup on Never type"),
            Type::Unknown => todo!("attribute lookup on Unknown type"),
            Type::Unbound => todo!("attribute lookup on Unbound type"),
            Type::Function(_) => todo!("attribute lookup on Function type"),
            Type::Module(module_id) => module_id.get_member(db, name),
            Type::Class(class_id) => class_id.get_class_member(db, name),
            Type::Instance(_) => {
                // TODO MRO? get_own_instance_member, get_instance_member
                todo!("attribute lookup on Instance type")
            }
            Type::Union(union_id) => {
                let jar: &SemanticJar = db.jar()?;
                let _todo_union_ref = jar.type_store.get_union(*union_id);
                // TODO perform the get_member on each type in the union
                // TODO return the union of those results
                // TODO if any of those results is `None` then include Unknown in the result union
                todo!("attribute lookup on Union type")
            }
            Type::Intersection(_) => {
                // TODO perform the get_member on each type in the intersection
                // TODO return the intersection of those results
                todo!("attribute lookup on Intersection type")
            }
            Type::IntLiteral(_) => {
                // TODO raise error
                Ok(Some(Type::Unknown))
            }
        }
    }
}

impl From<FunctionTypeId> for Type {
    fn from(id: FunctionTypeId) -> Self {
        Type::Function(id)
    }
}

impl From<UnionTypeId> for Type {
    fn from(id: UnionTypeId) -> Self {
        Type::Union(id)
    }
}

impl From<IntersectionTypeId> for Type {
    fn from(id: IntersectionTypeId) -> Self {
        Type::Intersection(id)
    }
}

// TODO: currently calling `get_function` et al and holding on to the `FunctionTypeRef` will lock a
// shard of this dashmap, for as long as you hold the reference. This may be a problem. We could
// switch to having all the arenas hold Arc, or we could see if we can split up ModuleTypeStore,
// and/or give it inner mutability and finer-grained internal locking.
#[derive(Debug, Default)]
pub struct TypeStore {
    modules: FxDashMap<FileId, ModuleTypeStore>,
}

impl TypeStore {
    pub fn remove_module(&mut self, file_id: FileId) {
        self.modules.remove(&file_id);
    }

    pub fn cache_symbol_public_type(&self, symbol: GlobalSymbolId, ty: Type) {
        self.add_or_get_module(symbol.file_id)
            .symbol_types
            .insert(symbol.symbol_id, ty);
    }

    pub fn cache_node_type(&self, file_id: FileId, node_key: NodeKey, ty: Type) {
        self.add_or_get_module(file_id)
            .node_types
            .insert(node_key, ty);
    }

    pub fn get_cached_symbol_public_type(&self, symbol: GlobalSymbolId) -> Option<Type> {
        self.try_get_module(symbol.file_id)?
            .symbol_types
            .get(&symbol.symbol_id)
            .copied()
    }

    pub fn get_cached_node_type(&self, file_id: FileId, node_key: &NodeKey) -> Option<Type> {
        self.try_get_module(file_id)?
            .node_types
            .get(node_key)
            .copied()
    }

    fn add_or_get_module(&self, file_id: FileId) -> ModuleStoreRefMut {
        self.modules
            .entry(file_id)
            .or_insert_with(|| ModuleTypeStore::new(file_id))
    }

    fn get_module(&self, file_id: FileId) -> ModuleStoreRef {
        self.try_get_module(file_id).expect("module should exist")
    }

    fn try_get_module(&self, file_id: FileId) -> Option<ModuleStoreRef> {
        self.modules.get(&file_id)
    }

    fn add_function(
        &self,
        file_id: FileId,
        name: &str,
        symbol_id: SymbolId,
        scope_id: ScopeId,
        decorators: Vec<Type>,
    ) -> FunctionTypeId {
        self.add_or_get_module(file_id)
            .add_function(name, symbol_id, scope_id, decorators)
    }

    fn add_class(
        &self,
        file_id: FileId,
        name: &str,
        scope_id: ScopeId,
        bases: Vec<Type>,
    ) -> ClassTypeId {
        self.add_or_get_module(file_id)
            .add_class(name, scope_id, bases)
    }

    fn add_union(&self, file_id: FileId, elems: &[Type]) -> UnionTypeId {
        self.add_or_get_module(file_id).add_union(elems)
    }

    fn add_intersection(
        &self,
        file_id: FileId,
        positive: &[Type],
        negative: &[Type],
    ) -> IntersectionTypeId {
        self.add_or_get_module(file_id)
            .add_intersection(positive, negative)
    }

    fn get_function(&self, id: FunctionTypeId) -> FunctionTypeRef {
        FunctionTypeRef {
            module_store: self.get_module(id.file_id),
            function_id: id.func_id,
        }
    }

    fn get_class(&self, id: ClassTypeId) -> ClassTypeRef {
        ClassTypeRef {
            module_store: self.get_module(id.file_id),
            class_id: id.class_id,
        }
    }

    fn get_union(&self, id: UnionTypeId) -> UnionTypeRef {
        UnionTypeRef {
            module_store: self.get_module(id.file_id),
            union_id: id.union_id,
        }
    }

    fn get_intersection(&self, id: IntersectionTypeId) -> IntersectionTypeRef {
        IntersectionTypeRef {
            module_store: self.get_module(id.file_id),
            intersection_id: id.intersection_id,
        }
    }
}

type ModuleStoreRef<'a> = dashmap::mapref::one::Ref<
    'a,
    FileId,
    ModuleTypeStore,
    std::hash::BuildHasherDefault<rustc_hash::FxHasher>,
>;

type ModuleStoreRefMut<'a> = dashmap::mapref::one::RefMut<
    'a,
    FileId,
    ModuleTypeStore,
    std::hash::BuildHasherDefault<rustc_hash::FxHasher>,
>;

#[derive(Debug)]
pub(crate) struct FunctionTypeRef<'a> {
    module_store: ModuleStoreRef<'a>,
    function_id: ModuleFunctionTypeId,
}

impl<'a> std::ops::Deref for FunctionTypeRef<'a> {
    type Target = FunctionType;

    fn deref(&self) -> &Self::Target {
        self.module_store.get_function(self.function_id)
    }
}

#[derive(Debug)]
pub(crate) struct ClassTypeRef<'a> {
    module_store: ModuleStoreRef<'a>,
    class_id: ModuleClassTypeId,
}

impl<'a> std::ops::Deref for ClassTypeRef<'a> {
    type Target = ClassType;

    fn deref(&self) -> &Self::Target {
        self.module_store.get_class(self.class_id)
    }
}

#[derive(Debug)]
pub(crate) struct UnionTypeRef<'a> {
    module_store: ModuleStoreRef<'a>,
    union_id: ModuleUnionTypeId,
}

impl<'a> std::ops::Deref for UnionTypeRef<'a> {
    type Target = UnionType;

    fn deref(&self) -> &Self::Target {
        self.module_store.get_union(self.union_id)
    }
}

#[derive(Debug)]
pub(crate) struct IntersectionTypeRef<'a> {
    module_store: ModuleStoreRef<'a>,
    intersection_id: ModuleIntersectionTypeId,
}

impl<'a> std::ops::Deref for IntersectionTypeRef<'a> {
    type Target = IntersectionType;

    fn deref(&self) -> &Self::Target {
        self.module_store.get_intersection(self.intersection_id)
    }
}

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
pub struct FunctionTypeId {
    file_id: FileId,
    func_id: ModuleFunctionTypeId,
}

impl FunctionTypeId {
    fn function(self, db: &dyn SemanticDb) -> QueryResult<FunctionTypeRef> {
        let jar: &SemanticJar = db.jar()?;
        Ok(jar.type_store.get_function(self))
    }

    pub(crate) fn name(self, db: &dyn SemanticDb) -> QueryResult<Name> {
        Ok(self.function(db)?.name().into())
    }

    pub(crate) fn global_symbol(self, db: &dyn SemanticDb) -> QueryResult<GlobalSymbolId> {
        Ok(GlobalSymbolId {
            file_id: self.file(),
            symbol_id: self.symbol(db)?,
        })
    }

    pub(crate) fn file(self) -> FileId {
        self.file_id
    }

    pub(crate) fn symbol(self, db: &dyn SemanticDb) -> QueryResult<SymbolId> {
        let FunctionType { symbol_id, .. } = *self.function(db)?;
        Ok(symbol_id)
    }

    pub(crate) fn get_containing_class(
        self,
        db: &dyn SemanticDb,
    ) -> QueryResult<Option<ClassTypeId>> {
        let table = symbol_table(db, self.file_id)?;
        let FunctionType { symbol_id, .. } = *self.function(db)?;
        let scope_id = symbol_id.symbol(&table).scope_id();
        let scope = scope_id.scope(&table);
        if !matches!(scope.kind(), ScopeKind::Class) {
            return Ok(None);
        };
        let Some(def) = scope.definition() else {
            return Ok(None);
        };
        let Some(symbol_id) = scope.defining_symbol() else {
            return Ok(None);
        };
        let Type::Class(class) = infer_definition_type(
            db,
            GlobalSymbolId {
                file_id: self.file_id,
                symbol_id,
            },
            def,
        )?
        else {
            return Ok(None);
        };
        Ok(Some(class))
    }

    pub(crate) fn has_decorator(
        self,
        db: &dyn SemanticDb,
        decorator_symbol: GlobalSymbolId,
    ) -> QueryResult<bool> {
        for deco_ty in self.function(db)?.decorators() {
            let Type::Function(deco_func) = deco_ty else {
                continue;
            };
            if deco_func.global_symbol(db)? == decorator_symbol {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
pub struct ModuleTypeId {
    module: Module,
    file_id: FileId,
}

impl ModuleTypeId {
    fn module(self, db: &dyn SemanticDb) -> QueryResult<ModuleStoreRef> {
        let jar: &SemanticJar = db.jar()?;
        Ok(jar.type_store.add_or_get_module(self.file_id).downgrade())
    }

    pub(crate) fn name(self, db: &dyn SemanticDb) -> QueryResult<ModuleName> {
        self.module.name(db)
    }

    fn get_member(self, db: &dyn SemanticDb, name: &Name) -> QueryResult<Option<Type>> {
        if let Some(symbol_id) = resolve_global_symbol(db, self.module, name)? {
            Ok(Some(infer_symbol_public_type(db, symbol_id)?))
        } else {
            Ok(None)
        }
    }
}

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
pub struct ClassTypeId {
    file_id: FileId,
    class_id: ModuleClassTypeId,
}

impl ClassTypeId {
    fn class(self, db: &dyn SemanticDb) -> QueryResult<ClassTypeRef> {
        let jar: &SemanticJar = db.jar()?;
        Ok(jar.type_store.get_class(self))
    }

    pub(crate) fn name(self, db: &dyn SemanticDb) -> QueryResult<Name> {
        Ok(self.class(db)?.name().into())
    }

    pub(crate) fn get_super_class_member(
        self,
        db: &dyn SemanticDb,
        name: &Name,
    ) -> QueryResult<Option<Type>> {
        // TODO we should linearize the MRO instead of doing this recursively
        let class = self.class(db)?;
        for base in class.bases() {
            if let Type::Class(base) = base {
                if let Some(own_member) = base.get_own_class_member(db, name)? {
                    return Ok(Some(own_member));
                }
                if let Some(base_member) = base.get_super_class_member(db, name)? {
                    return Ok(Some(base_member));
                }
            }
        }
        Ok(None)
    }

    fn get_own_class_member(self, db: &dyn SemanticDb, name: &Name) -> QueryResult<Option<Type>> {
        // TODO: this should distinguish instance-only members (e.g. `x: int`) and not return them
        let ClassType { scope_id, .. } = *self.class(db)?;
        let table = symbol_table(db, self.file_id)?;
        if let Some(symbol_id) = table.symbol_id_by_name(scope_id, name) {
            Ok(Some(infer_symbol_public_type(
                db,
                GlobalSymbolId {
                    file_id: self.file_id,
                    symbol_id,
                },
            )?))
        } else {
            Ok(None)
        }
    }

    /// Get own class member or fall back to super-class member.
    fn get_class_member(self, db: &dyn SemanticDb, name: &Name) -> QueryResult<Option<Type>> {
        self.get_own_class_member(db, name)
            .or_else(|_| self.get_super_class_member(db, name))
    }

    // TODO: get_own_instance_member, get_instance_member
}

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
pub struct UnionTypeId {
    file_id: FileId,
    union_id: ModuleUnionTypeId,
}

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
pub struct IntersectionTypeId {
    file_id: FileId,
    intersection_id: ModuleIntersectionTypeId,
}

#[newtype_index]
struct ModuleFunctionTypeId;

#[newtype_index]
struct ModuleClassTypeId;

#[newtype_index]
struct ModuleUnionTypeId;

#[newtype_index]
struct ModuleIntersectionTypeId;

#[derive(Debug)]
struct ModuleTypeStore {
    file_id: FileId,
    /// arena of all function types defined in this module
    functions: IndexVec<ModuleFunctionTypeId, FunctionType>,
    /// arena of all class types defined in this module
    classes: IndexVec<ModuleClassTypeId, ClassType>,
    /// arenda of all union types created in this module
    unions: IndexVec<ModuleUnionTypeId, UnionType>,
    /// arena of all intersection types created in this module
    intersections: IndexVec<ModuleIntersectionTypeId, IntersectionType>,
    /// cached public types of symbols in this module
    symbol_types: FxHashMap<SymbolId, Type>,
    /// cached types of AST nodes in this module
    node_types: FxHashMap<NodeKey, Type>,
}

impl ModuleTypeStore {
    fn new(file_id: FileId) -> Self {
        Self {
            file_id,
            functions: IndexVec::default(),
            classes: IndexVec::default(),
            unions: IndexVec::default(),
            intersections: IndexVec::default(),
            symbol_types: FxHashMap::default(),
            node_types: FxHashMap::default(),
        }
    }

    fn add_function(
        &mut self,
        name: &str,
        symbol_id: SymbolId,
        scope_id: ScopeId,
        decorators: Vec<Type>,
    ) -> FunctionTypeId {
        let func_id = self.functions.push(FunctionType {
            name: Name::new(name),
            symbol_id,
            scope_id,
            decorators,
        });
        FunctionTypeId {
            file_id: self.file_id,
            func_id,
        }
    }

    fn add_class(&mut self, name: &str, scope_id: ScopeId, bases: Vec<Type>) -> ClassTypeId {
        let class_id = self.classes.push(ClassType {
            name: Name::new(name),
            scope_id,
            // TODO: if no bases are given, that should imply [object]
            bases,
        });
        ClassTypeId {
            file_id: self.file_id,
            class_id,
        }
    }

    fn add_union(&mut self, elems: &[Type]) -> UnionTypeId {
        let union_id = self.unions.push(UnionType {
            elements: elems.iter().copied().collect(),
        });
        UnionTypeId {
            file_id: self.file_id,
            union_id,
        }
    }

    fn add_intersection(&mut self, positive: &[Type], negative: &[Type]) -> IntersectionTypeId {
        let intersection_id = self.intersections.push(IntersectionType {
            positive: positive.iter().copied().collect(),
            negative: negative.iter().copied().collect(),
        });
        IntersectionTypeId {
            file_id: self.file_id,
            intersection_id,
        }
    }

    fn get_function(&self, func_id: ModuleFunctionTypeId) -> &FunctionType {
        &self.functions[func_id]
    }

    fn get_class(&self, class_id: ModuleClassTypeId) -> &ClassType {
        &self.classes[class_id]
    }

    fn get_union(&self, union_id: ModuleUnionTypeId) -> &UnionType {
        &self.unions[union_id]
    }

    fn get_intersection(&self, intersection_id: ModuleIntersectionTypeId) -> &IntersectionType {
        &self.intersections[intersection_id]
    }
}

#[derive(Copy, Clone, Debug)]
struct DisplayType<'a> {
    ty: &'a Type,
    store: &'a TypeStore,
}

impl std::fmt::Display for DisplayType<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.ty {
            Type::Any => f.write_str("Any"),
            Type::Never => f.write_str("Never"),
            Type::Unknown => f.write_str("Unknown"),
            Type::Unbound => f.write_str("Unbound"),
            Type::Module(module_id) => {
                // NOTE: something like this?: "<module 'module-name' from 'path-from-fileid'>"
                todo!("{module_id:?}")
            }
            // TODO functions and classes should display using a fully qualified name
            Type::Class(class_id) => {
                f.write_str("Literal[")?;
                f.write_str(self.store.get_class(*class_id).name())?;
                f.write_str("]")
            }
            Type::Instance(class_id) => f.write_str(self.store.get_class(*class_id).name()),
            Type::Function(func_id) => f.write_str(self.store.get_function(*func_id).name()),
            Type::Union(union_id) => self
                .store
                .get_module(union_id.file_id)
                .get_union(union_id.union_id)
                .display(f, self.store),
            Type::Intersection(int_id) => self
                .store
                .get_module(int_id.file_id)
                .get_intersection(int_id.intersection_id)
                .display(f, self.store),
            Type::IntLiteral(n) => write!(f, "Literal[{n}]"),
        }
    }
}

#[derive(Debug)]
pub(crate) struct ClassType {
    /// Name of the class at definition
    name: Name,
    /// `ScopeId` of the class body
    scope_id: ScopeId,
    /// Types of all class bases
    bases: Vec<Type>,
}

impl ClassType {
    fn name(&self) -> &str {
        self.name.as_str()
    }

    fn bases(&self) -> &[Type] {
        self.bases.as_slice()
    }
}

#[derive(Debug)]
pub(crate) struct FunctionType {
    /// name of the function at definition
    name: Name,
    /// symbol which this function is a definition of
    symbol_id: SymbolId,
    /// scope of this function's body
    scope_id: ScopeId,
    /// types of all decorators on this function
    decorators: Vec<Type>,
}

impl FunctionType {
    fn name(&self) -> &str {
        self.name.as_str()
    }

    fn scope_id(&self) -> ScopeId {
        self.scope_id
    }

    pub(crate) fn decorators(&self) -> &[Type] {
        self.decorators.as_slice()
    }
}

#[derive(Debug)]
pub(crate) struct UnionType {
    // the union type includes values in any of these types
    elements: FxIndexSet<Type>,
}

impl UnionType {
    fn display(&self, f: &mut std::fmt::Formatter<'_>, store: &TypeStore) -> std::fmt::Result {
        f.write_str("(")?;
        let mut first = true;
        for ty in &self.elements {
            if !first {
                f.write_str(" | ")?;
            };
            first = false;
            write!(f, "{}", ty.display(store))?;
        }
        f.write_str(")")
    }
}

// Negation types aren't expressible in annotations, and are most likely to arise from type
// narrowing along with intersections (e.g. `if not isinstance(...)`), so we represent them
// directly in intersections rather than as a separate type. This sacrifices some efficiency in the
// case where a Not appears outside an intersection (unclear when that could even happen, but we'd
// have to represent it as a single-element intersection if it did) in exchange for better
// efficiency in the within-intersection case.
#[derive(Debug)]
pub(crate) struct IntersectionType {
    // the intersection type includes only values in all of these types
    positive: FxIndexSet<Type>,
    // the intersection type does not include any value in any of these types
    negative: FxIndexSet<Type>,
}

impl IntersectionType {
    fn display(&self, f: &mut std::fmt::Formatter<'_>, store: &TypeStore) -> std::fmt::Result {
        f.write_str("(")?;
        let mut first = true;
        for (neg, ty) in self
            .positive
            .iter()
            .map(|ty| (false, ty))
            .chain(self.negative.iter().map(|ty| (true, ty)))
        {
            if !first {
                f.write_str(" & ")?;
            };
            first = false;
            if neg {
                f.write_str("~")?;
            };
            write!(f, "{}", ty.display(store))?;
        }
        f.write_str(")")
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::files::Files;
    use crate::symbols::{SymbolFlags, SymbolTable};
    use crate::types::{Type, TypeStore};
    use crate::FxIndexSet;

    #[test]
    fn add_class() {
        let store = TypeStore::default();
        let files = Files::default();
        let file_id = files.intern(Path::new("/foo"));
        let id = store.add_class(file_id, "C", SymbolTable::root_scope_id(), Vec::new());
        assert_eq!(store.get_class(id).name(), "C");
        let inst = Type::Instance(id);
        assert_eq!(format!("{}", inst.display(&store)), "C");
    }

    #[test]
    fn add_function() {
        let store = TypeStore::default();
        let files = Files::default();
        let file_id = files.intern(Path::new("/foo"));
        let mut table = SymbolTable::new();
        let func_symbol = table.add_or_update_symbol(
            SymbolTable::root_scope_id(),
            "func",
            SymbolFlags::IS_DEFINED,
        );

        let id = store.add_function(
            file_id,
            "func",
            func_symbol,
            SymbolTable::root_scope_id(),
            vec![Type::Unknown],
        );
        assert_eq!(store.get_function(id).name(), "func");
        assert_eq!(store.get_function(id).decorators(), vec![Type::Unknown]);
        let func = Type::Function(id);
        assert_eq!(format!("{}", func.display(&store)), "func");
    }

    #[test]
    fn add_union() {
        let store = TypeStore::default();
        let files = Files::default();
        let file_id = files.intern(Path::new("/foo"));
        let c1 = store.add_class(file_id, "C1", SymbolTable::root_scope_id(), Vec::new());
        let c2 = store.add_class(file_id, "C2", SymbolTable::root_scope_id(), Vec::new());
        let elems = vec![Type::Instance(c1), Type::Instance(c2)];
        let id = store.add_union(file_id, &elems);
        assert_eq!(
            store.get_union(id).elements,
            elems.into_iter().collect::<FxIndexSet<_>>()
        );
        let union = Type::Union(id);
        assert_eq!(format!("{}", union.display(&store)), "(C1 | C2)");
    }

    #[test]
    fn add_intersection() {
        let store = TypeStore::default();
        let files = Files::default();
        let file_id = files.intern(Path::new("/foo"));
        let c1 = store.add_class(file_id, "C1", SymbolTable::root_scope_id(), Vec::new());
        let c2 = store.add_class(file_id, "C2", SymbolTable::root_scope_id(), Vec::new());
        let c3 = store.add_class(file_id, "C3", SymbolTable::root_scope_id(), Vec::new());
        let pos = vec![Type::Instance(c1), Type::Instance(c2)];
        let neg = vec![Type::Instance(c3)];
        let id = store.add_intersection(file_id, &pos, &neg);
        assert_eq!(
            store.get_intersection(id).positive,
            pos.into_iter().collect::<FxIndexSet<_>>()
        );
        assert_eq!(
            store.get_intersection(id).negative,
            neg.into_iter().collect::<FxIndexSet<_>>()
        );
        let intersection = Type::Intersection(id);
        assert_eq!(
            format!("{}", intersection.display(&store)),
            "(C1 & C2 & ~C3)"
        );
    }
}
