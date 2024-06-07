use rustc_hash::FxHashMap;

use ruff_index::{newtype_index, IndexVec};
use ruff_python_ast::{
    AstNode, StmtAnnAssign, StmtAssign, StmtClassDef, StmtFunctionDef, StmtImport, StmtImportFrom,
};

use crate::ast_ids::NodeKey;
use crate::module::ModuleName;
use crate::salsa_db::semantic::ast_ids::{
    ast_ids, AnnotatedAssignmentId, AssignmentId, AstIdNode, ClassId, ExpressionId, FunctionId,
};
use crate::salsa_db::semantic::definition::{Definition, ImportDefinition, ImportFromDefinition};
use crate::salsa_db::semantic::flow_graph::ReachableDefinition;
use crate::salsa_db::semantic::module::resolve_module_name;
use crate::salsa_db::semantic::symbol_table::{symbol_table, NodeWithScopeId, ScopeId, SymbolId};
use crate::salsa_db::semantic::types::infer::infer_types;
use crate::salsa_db::semantic::{
    global_symbol_type, global_symbol_type_by_name, Db, GlobalId, GlobalSymbolId, Jar,
};
use crate::salsa_db::source::File;
use crate::{FxIndexSet, Name};

pub mod infer;

#[salsa::tracked(jar=Jar, return_ref)]
pub(crate) fn typing_scopes(db: &dyn Db, file: File) -> TypingScopes {
    let ast_ids = ast_ids(db, file);

    let functions = ast_ids
        .functions()
        .map(|(id, _)| (id, FunctionTypingScope::new(db, file, id)))
        .collect();
    let classes = ast_ids
        .classes()
        .map(|(id, _)| (id, ClassTypingScope::new(db, file, id)))
        .collect();

    TypingScopes { functions, classes }
}

#[salsa::tracked(jar=Jar)]
pub struct FunctionTypingScope {
    file: File,
    #[id]
    id: FunctionId,
}

impl FunctionTypingScope {
    pub fn node(self, db: &dyn Db) -> &StmtFunctionDef {
        StmtFunctionDef::lookup(db, self.file(db), self.id(db))
    }
}

#[salsa::tracked(jar=Jar)]
pub struct ClassTypingScope {
    file: File,
    #[id]
    id: ClassId,
}

impl ClassTypingScope {
    pub fn node(self, db: &dyn Db) -> &StmtClassDef {
        StmtClassDef::lookup(db, self.file(db), self.id(db))
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct TypingScopes {
    functions: FxHashMap<FunctionId, FunctionTypingScope>,
    classes: FxHashMap<ClassId, ClassTypingScope>,
}

impl std::ops::Index<FunctionId> for TypingScopes {
    type Output = FunctionTypingScope;

    fn index(&self, index: FunctionId) -> &Self::Output {
        &self.functions[&index]
    }
}

impl std::ops::Index<ClassId> for TypingScopes {
    type Output = ClassTypingScope;

    fn index(&self, index: ClassId) -> &Self::Output {
        &self.classes[&index]
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum TypingScope {
    Function(FunctionTypingScope),
    Class(ClassTypingScope),
    Module(File),
}

impl TypingScope {
    pub fn for_symbol(db: &dyn Db, symbol: GlobalSymbolId) -> TypingScope {
        let symbols = symbol_table(db, symbol.file());
        let scope = symbols.scope_id_of_symbol(symbol.local());

        Self::for_symbol_scope(db, GlobalId::new(symbol.file(), scope))
    }

    pub fn for_expression(db: &dyn Db, expression_id: GlobalId<ExpressionId>) -> TypingScope {
        let symbols = symbol_table(db, expression_id.file());

        let expression_scope = symbols.scope_id_of_expression(expression_id.local());
        TypingScope::for_symbol_scope(db, GlobalId::new(expression_id.file(), expression_scope))
    }

    fn for_symbol_scope(db: &dyn Db, scope_id: GlobalId<ScopeId>) -> TypingScope {
        let typing_scopes = typing_scopes(db, scope_id.file());
        let symbols = symbol_table(db, scope_id.file());
        let scope = symbols.scope(scope_id.local());

        let mut ancestors = std::iter::once((scope_id.local(), scope))
            .chain(symbols.parent_scopes(scope_id.local()));

        let typing_scope = ancestors.find_map(|(_, scope)| match scope.definition()? {
            Definition::Function(function) => Some(typing_scopes[*function].into()),
            Definition::Class(class) => Some(typing_scopes[*class].into()),
            _ => None,
        });
        typing_scope.unwrap_or(TypingScope::Module(scope_id.file()))
    }

    fn file(self, db: &dyn Db) -> File {
        match self {
            TypingScope::Function(function) => function.file(db),
            TypingScope::Class(class) => class.file(db),
            TypingScope::Module(file) => file,
        }
    }
}

impl From<FunctionTypingScope> for TypingScope {
    fn from(value: FunctionTypingScope) -> Self {
        TypingScope::Function(value)
    }
}

impl From<ClassTypingScope> for TypingScope {
    fn from(value: ClassTypingScope) -> Self {
        TypingScope::Class(value)
    }
}

impl From<File> for TypingScope {
    fn from(value: File) -> Self {
        TypingScope::Module(value)
    }
}

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
    Function(GlobalTypeId<FunctionTypeId>),
    // TODO should this be `ResolvedModule`? It would require that `ResolvedModule` is interned.
    /// a specific module object
    Module(GlobalTypeId<ModuleTypeId>),
    /// a specific class object
    Class(GlobalTypeId<ClassTypeId>),
    /// the set of Python objects with the given class in their __class__'s method resolution order
    Instance(GlobalTypeId<ClassTypeId>),
    Union(GlobalTypeId<UnionTypeId>),
    Intersection(GlobalTypeId<IntersectionTypeId>),
    IntLiteral(i64),
    // TODO protocols, callable types, overloads, generics, type vars
}

impl Type {
    fn display<'a>(self, context: &'a TypingContext<'a>) -> DisplayType<'a> {
        DisplayType { ty: self, context }
    }

    pub const fn is_unbound(&self) -> bool {
        matches!(self, Type::Unbound)
    }

    pub const fn is_unknown(&self) -> bool {
        matches!(self, Type::Unknown)
    }

    // FIXME: The main issue here is that `.member()` can call into `infer_module` or `infer_class`
    //  which may be running right now. This is a problem because it can lead to cycles
    pub fn member(&self, context: &TypingContext, name: &Name) -> Option<Type> {
        match self {
            Type::Any => todo!("attribute lookup on Any type"),
            Type::Never => todo!("attribute lookup on Never type"),
            Type::Unknown => todo!("attribute lookup on Unknown type"),
            Type::Unbound => todo!("attribute lookup on Unbound type"),
            Type::Function(_) => todo!("attribute lookup on Function type"),
            Type::Module(module_id) => module_id.ty(context).member(context, name),
            Type::Class(class_id) => class_id.ty(context).member(context, name),
            Type::Instance(_) => {
                // TODO MRO? get_own_instance_member, get_instance_member
                todo!("attribute lookup on Instance type")
            }
            Type::Union(union_id) => {
                let _union = union_id.ty(context);
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
                Some(Type::Unknown)
            }
        }
    }
}

#[derive(Copy, Clone)]
struct DisplayType<'a> {
    ty: Type,
    context: &'a TypingContext<'a>,
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
                f.write_str(class_id.ty(self.context).name())?;
                f.write_str("]")
            }
            Type::Instance(class_id) => f.write_str(class_id.ty(self.context).name()),
            Type::Function(func_id) => f.write_str(func_id.ty(self.context).name()),
            Type::Union(union_id) => union_id.ty(self.context).display(f, self.context),
            Type::Intersection(int_id) => int_id.ty(self.context).display(f, self.context),
            Type::IntLiteral(n) => write!(f, "Literal[{n}]"),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct GlobalTypeId<T>
where
    T: LocalTypeId,
{
    scope: TypingScope,
    local_id: T,
}

impl<T: LocalTypeId> GlobalTypeId<T> {
    pub fn new(scope: TypingScope, local_id: T) -> Self {
        Self { scope, local_id }
    }

    fn scope(&self) -> TypingScope {
        self.scope
    }

    fn local_id(&self) -> T {
        self.local_id
    }

    fn ty<'a>(&self, context: &TypingContext<'a>) -> &'a T::Ty {
        let types = context.types(self.scope());

        self.local_id.ty(&types)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ModuleTypeId;

impl LocalTypeId for ModuleTypeId {
    type Ty = ModuleType;

    fn ty<'a>(&self, types: &'a TypeInference) -> &'a Self::Ty {
        types.module.as_ref().unwrap()
    }

    fn intern(ty: Self::Ty, types: &mut TypeInference) -> Self {
        assert_eq!(types.module.as_ref(), None);
        types.module = Some(ty);

        ModuleTypeId
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct ModuleType {
    file: File,
}

impl ModuleType {
    fn new(file: File) -> Self {
        Self { file }
    }

    fn member(&self, context: &TypingContext, name: &Name) -> Option<Type> {
        let symbol_table = symbol_table(context.db, self.scope().file(context.db));
        let symbol_id = symbol_table.root_symbol_id_by_name(name)?;

        Some(global_symbol_type(
            context.db,
            GlobalSymbolId::new(self.file, symbol_id),
        ))
    }

    fn scope(&self) -> TypingScope {
        self.file.into()
    }
}

#[newtype_index]
pub struct FunctionTypeId;

impl LocalTypeId for FunctionTypeId {
    type Ty = FunctionType;

    fn ty<'a>(&self, types: &'a TypeInference) -> &'a Self::Ty {
        &types.functions[*self]
    }

    fn intern(ty: Self::Ty, types: &mut TypeInference) -> Self {
        types.functions.push(ty)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct FunctionType {
    /// name of the function at definition
    name: Name,
    /// types of all decorators on this function
    decorators: Vec<Type>,

    typing_scope: FunctionTypingScope,
}

impl FunctionType {
    fn name(&self) -> &str {
        self.name.as_str()
    }

    pub(crate) fn decorators(&self) -> &[Type] {
        self.decorators.as_slice()
    }
}

#[newtype_index]
pub struct ClassTypeId;

impl LocalTypeId for ClassTypeId {
    type Ty = ClassType;

    fn ty<'a>(&self, types: &'a TypeInference) -> &'a Self::Ty {
        &types.classes[*self]
    }

    fn intern(ty: Self::Ty, types: &mut TypeInference) -> Self {
        types.classes.push(ty)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct ClassType {
    /// Name of the class at definition
    name: Name,

    /// Types of all class bases
    bases: Vec<Type>,

    typing_scope: ClassTypingScope,
}

impl ClassType {
    fn name(&self) -> &str {
        self.name.as_str()
    }

    fn bases(&self) -> &[Type] {
        self.bases.as_slice()
    }

    pub fn member(&self, context: &TypingContext, name: &Name) -> Option<Type> {
        let file = self.typing_scope.file(context.db());
        let symbol_table = symbol_table(context.db(), file);
        let scope_id = symbol_table
            .scope_id_for_node(NodeWithScopeId::Class(self.typing_scope.id(context.db())));

        let symbol_id = symbol_table.symbol_id_by_name(scope_id, name)?;

        Some(global_symbol_type(
            context.db,
            GlobalSymbolId::new(file, symbol_id),
        ))
    }
}

#[newtype_index]
pub struct UnionTypeId;

impl LocalTypeId for UnionTypeId {
    type Ty = UnionType;

    fn ty<'a>(&self, types: &'a TypeInference) -> &'a Self::Ty {
        &types.unions[*self]
    }

    fn intern(ty: Self::Ty, types: &mut TypeInference) -> Self {
        types.unions.push(ty)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct UnionType {
    // the union type includes values in any of these types
    elements: FxIndexSet<Type>,
}

impl UnionType {
    fn display(
        &self,
        f: &mut std::fmt::Formatter<'_>,
        context: &TypingContext,
    ) -> std::fmt::Result {
        f.write_str("(")?;
        let mut first = true;
        for ty in &self.elements {
            if !first {
                f.write_str(" | ")?;
            };
            first = false;
            write!(f, "{}", ty.display(context))?;
        }
        f.write_str(")")
    }
}

#[newtype_index]
pub struct IntersectionTypeId;

impl LocalTypeId for IntersectionTypeId {
    type Ty = IntersectionType;

    fn ty<'a>(&self, types: &'a TypeInference) -> &'a Self::Ty {
        &types.intersections[*self]
    }

    fn intern(ty: Self::Ty, types: &mut TypeInference) -> Self {
        types.intersections.push(ty)
    }
}

// Negation types aren't expressible in annotations, and are most likely to arise from type
// narrowing along with intersections (e.g. `if not isinstance(...)`), so we represent them
// directly in intersections rather than as a separate type. This sacrifices some efficiency in the
// case where a Not appears outside an intersection (unclear when that could even happen, but we'd
// have to represent it as a single-element intersection if it did) in exchange for better
// efficiency in the within-intersection case.
#[derive(Debug, Eq, PartialEq)]
pub struct IntersectionType {
    // the intersection type includes only values in all of these types
    positive: FxIndexSet<Type>,
    // the intersection type does not include any value in any of these types
    negative: FxIndexSet<Type>,
}

impl IntersectionType {
    fn display(
        &self,
        f: &mut std::fmt::Formatter<'_>,
        context: &TypingContext,
    ) -> std::fmt::Result {
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
            write!(f, "{}", ty.display(context))?;
        }
        f.write_str(")")
    }
}

pub trait LocalTypeId: Copy {
    type Ty;

    /// Resolves the type of the data from `types_store`
    ///
    /// ## Panics
    /// May panic if `self` is from another scope than `types_store` or returns an invalid type.
    fn ty<'a>(&self, types: &'a TypeInference) -> &'a Self::Ty;

    fn intern(ty: Self::Ty, types: &mut TypeInference) -> Self;
}

pub struct TypingContext<'a> {
    db: &'a dyn Db,
    local: Option<(TypingScope, &'a TypeInference)>,
}

impl<'a> TypingContext<'a> {
    pub fn local(db: &'a dyn Db, local_scope: TypingScope, types: &'a TypeInference) -> Self {
        Self {
            db,
            local: Some((local_scope, types)),
        }
    }

    pub fn global(db: &'a dyn Db) -> Self {
        Self { db, local: None }
    }

    pub fn db(&self) -> &'a dyn Db {
        self.db
    }

    pub fn types(&self, scope: TypingScope) -> &'a TypeInference {
        if let Some((local_scope, types)) = self.local {
            if local_scope == scope {
                return types;
            }
        }

        infer_types(self.db, scope)
    }

    fn infer_definitions(
        &self,
        mut definitions: impl Iterator<Item = ReachableDefinition>,
        symbol_scope: GlobalId<ScopeId>,
    ) -> DefinitionType {
        let Some(first) = definitions.next() else {
            return DefinitionType::Type(Type::Unknown);
        };

        let typing_scope = TypingScope::for_symbol_scope(self.db(), symbol_scope);
        let types = self.types(typing_scope);

        if let Some(second) = definitions.next() {
            let elements: FxIndexSet<_> = [first, second]
                .into_iter()
                .chain(definitions)
                .map(|definition| self.infer_definition(definition, types))
                .collect();

            // The fact that the interner is local to a body means that we can't reuse the same union type
            // across different call sites. But that's something we aren't doing yet anyway. Our interner doesn't
            // deduplicate union types that are identical.
            DefinitionType::Union {
                ty: UnionType { elements },
                scope: typing_scope,
            }
        } else {
            DefinitionType::Type(self.infer_definition(first, types))
        }
    }

    /// Infers the type of a location definition.
    fn infer_definition(
        &self,
        definition: ReachableDefinition,
        definition_types: &TypeInference,
    ) -> Type {
        let ReachableDefinition::Definition(definition) = definition else {
            return Type::Unbound;
        };

        definition_types
            .local_definitions
            .get(&definition)
            .copied()
            .unwrap_or(Type::Unbound)
    }
}

#[derive(Debug, Eq, PartialEq, Default)]
pub struct TypeInference {
    module: Option<ModuleType>,
    functions: IndexVec<FunctionTypeId, FunctionType>,
    classes: IndexVec<ClassTypeId, ClassType>,
    unions: IndexVec<UnionTypeId, UnionType>,
    intersections: IndexVec<IntersectionTypeId, IntersectionType>,

    local_definitions: FxHashMap<Definition, Type>,
    public_symbol_types: FxHashMap<SymbolId, Type>,
    expression_types: FxHashMap<ExpressionId, Type>,
}

impl TypeInference {
    fn shrink_to_fit(&mut self) {
        self.functions.shrink_to_fit();
        self.classes.shrink_to_fit();
        self.unions.shrink_to_fit();
        self.intersections.shrink_to_fit();

        self.public_symbol_types.shrink_to_fit();
        self.expression_types.shrink_to_fit();
        self.local_definitions.shrink_to_fit();
    }

    pub fn symbol_ty(&self, id: SymbolId) -> Type {
        self.public_symbol_types
            .get(&id)
            .copied()
            .unwrap_or(Type::Unknown)
    }

    pub fn expression_ty(&self, id: ExpressionId) -> Type {
        self.expression_types
            .get(&id)
            .copied()
            .unwrap_or(Type::Unknown)
    }
}

enum DefinitionType {
    Union { ty: UnionType, scope: TypingScope },
    Type(Type),
}

impl DefinitionType {
    fn into_type(self, types: &mut TypeInference) -> Type {
        match self {
            DefinitionType::Union { ty, scope } => {
                let union_id = UnionTypeId::intern(ty, types);
                Type::Union(GlobalTypeId::new(scope, union_id))
            }
            DefinitionType::Type(ty) => ty,
        }
    }
}
