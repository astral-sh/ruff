#![allow(dead_code)]
use crate::ast_ids::NodeKey;
use crate::files::FileId;
use crate::symbols::SymbolId;
use crate::{FxDashMap, FxIndexSet, Name};
use ruff_index::{newtype_index, IndexVec};
use rustc_hash::FxHashMap;

pub(crate) mod infer;

pub(crate) use infer::infer_symbol_type;

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
    /// a specific class object
    Class(ClassTypeId),
    /// the set of Python objects with the given class in their __class__'s method resolution order
    Instance(ClassTypeId),
    Union(UnionTypeId),
    Intersection(IntersectionTypeId),
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

    pub fn cache_symbol_type(&self, file_id: FileId, symbol_id: SymbolId, ty: Type) {
        self.add_or_get_module(file_id)
            .symbol_types
            .insert(symbol_id, ty);
    }

    pub fn cache_node_type(&self, file_id: FileId, node_key: NodeKey, ty: Type) {
        self.add_or_get_module(file_id)
            .node_types
            .insert(node_key, ty);
    }

    pub fn get_cached_symbol_type(&self, file_id: FileId, symbol_id: SymbolId) -> Option<Type> {
        self.try_get_module(file_id)?
            .symbol_types
            .get(&symbol_id)
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

    fn add_function(&self, file_id: FileId, name: &str) -> FunctionTypeId {
        self.add_or_get_module(file_id).add_function(name)
    }

    fn add_class(&self, file_id: FileId, name: &str, bases: Vec<Type>) -> ClassTypeId {
        self.add_or_get_module(file_id).add_class(name, bases)
    }

    fn add_union(&mut self, file_id: FileId, elems: &[Type]) -> UnionTypeId {
        self.add_or_get_module(file_id).add_union(elems)
    }

    fn add_intersection(
        &mut self,
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

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
pub struct ClassTypeId {
    file_id: FileId,
    class_id: ModuleClassTypeId,
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
    /// cached types of symbols in this module
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

    fn add_function(&mut self, name: &str) -> FunctionTypeId {
        let func_id = self.functions.push(FunctionType {
            name: Name::new(name),
        });
        FunctionTypeId {
            file_id: self.file_id,
            func_id,
        }
    }

    fn add_class(&mut self, name: &str, bases: Vec<Type>) -> ClassTypeId {
        let class_id = self.classes.push(ClassType {
            name: Name::new(name),
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
        }
    }
}

#[derive(Debug)]
pub(crate) struct ClassType {
    name: Name,
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
    name: Name,
}

impl FunctionType {
    fn name(&self) -> &str {
        self.name.as_str()
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
// efficiency in the not-within-intersection case.
#[derive(Debug)]
pub(crate) struct IntersectionType {
    // the intersection type includes only values in all of these types
    positive: FxIndexSet<Type>,
    // negated elements of the intersection, e.g.
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
    use crate::files::Files;
    use crate::types::{Type, TypeStore};
    use crate::FxIndexSet;
    use std::path::Path;

    #[test]
    fn add_class() {
        let store = TypeStore::default();
        let files = Files::default();
        let file_id = files.intern(Path::new("/foo"));
        let id = store.add_class(file_id, "C", Vec::new());
        assert_eq!(store.get_class(id).name(), "C");
        let inst = Type::Instance(id);
        assert_eq!(format!("{}", inst.display(&store)), "C");
    }

    #[test]
    fn add_function() {
        let store = TypeStore::default();
        let files = Files::default();
        let file_id = files.intern(Path::new("/foo"));
        let id = store.add_function(file_id, "func");
        assert_eq!(store.get_function(id).name(), "func");
        let func = Type::Function(id);
        assert_eq!(format!("{}", func.display(&store)), "func");
    }

    #[test]
    fn add_union() {
        let mut store = TypeStore::default();
        let files = Files::default();
        let file_id = files.intern(Path::new("/foo"));
        let c1 = store.add_class(file_id, "C1", Vec::new());
        let c2 = store.add_class(file_id, "C2", Vec::new());
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
        let mut store = TypeStore::default();
        let files = Files::default();
        let file_id = files.intern(Path::new("/foo"));
        let c1 = store.add_class(file_id, "C1", Vec::new());
        let c2 = store.add_class(file_id, "C2", Vec::new());
        let c3 = store.add_class(file_id, "C3", Vec::new());
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
