//! Key observations
//!
//! The HIR avoids allocations to large extends by:
//! * Using an arena per node type
//! * using ids and id ranges to reference items.
//!
//! Using separate arena per node type has the advantage that the IDs are relatively stable, because
//! they only change when a node of the same kind has been added or removed. (What's unclear is if that matters or if
//! it still triggers a re-compute because the AST-id in the node has changed).
//!
//! The HIR does not store all details. It mainly stores the *public* interface. There's a reference
//! back to the AST node to get more details.
//!
//!

use std::ops::{Index, Range};

use crate::ast_ids::{HasAstId, TypedAstId};
use ruff_index::{newtype_index, IndexVec};
use ruff_python_ast::{
    Stmt, StmtAnnAssign, StmtAssign, StmtClassDef, StmtFunctionDef, StmtTypeAlias, TypeParam,
    TypeParamParamSpec, TypeParamTypeVar, TypeParamTypeVarTuple,
};

use crate::files::FileId;
use crate::Name;

#[derive(Eq, PartialEq, Hash, Debug)]
pub struct HirAstId<N: HasAstId> {
    file_id: FileId,
    node_id: TypedAstId<N>,
}

impl<N: HasAstId> Copy for HirAstId<N> {}
impl<N: HasAstId> Clone for HirAstId<N> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<N: HasAstId> HirAstId<N> {
    pub fn upcast<M: HasAstId>(self) -> HirAstId<M>
    where
        N: Into<M>,
    {
        HirAstId {
            file_id: self.file_id,
            node_id: self.node_id.upcast(),
        }
    }
}

#[newtype_index]
pub struct FunctionId;

#[derive(Debug, Clone)]
pub struct Function {
    ast_id: HirAstId<StmtFunctionDef>,
    name: Name,
    parameters: Range<ParameterId>,
    // TODO: type_parameters, return expression, decorators
}

#[newtype_index]
pub struct ParameterId;

#[derive(Debug, Clone)]
pub struct Parameter {
    kind: ParameterKind,
    name: Name,
    default: Option<()>, // TODO use expression HIR
    ast_id: HirAstId<StmtFunctionDef>,
}

// TODO or should `Parameter` be an enum?
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum ParameterKind {
    PositionalOnly,
    Arguments,
    Vararg,
    KeywordOnly,
    Kwarg,
}

#[newtype_index]
pub struct ClassId;

#[derive(Debug, Clone)]
pub struct Class {
    name: Name,
    ast_id: HirAstId<StmtClassDef>,
    // TODO type parameters, inheritance, decorators, members
}

#[newtype_index]
pub struct AssignmentId;

// This can have more than one name...
// but that means we can't implement `name()` on `ModuleItem`.

#[derive(Debug, Clone)]
pub struct Assignment {
    // TODO: Handle multiple names  / targets
    name: Name,
    ast_id: HirAstId<StmtAssign>,
}

#[derive(Debug, Clone)]
pub struct AnnotatedAssignment {
    name: Name,
    ast_id: HirAstId<StmtAnnAssign>,
}

#[newtype_index]
pub struct AnnotatedAssignmentId;

#[newtype_index]
pub struct TypeAliasId;

#[derive(Debug, Clone)]
pub struct TypeAlias {
    name: Name,
    ast_id: HirAstId<StmtTypeAlias>,
    parameters: Range<TypeParameterId>,
}

#[newtype_index]
pub struct TypeParameterId;

#[derive(Debug, Clone)]
pub enum TypeParameter {
    TypeVar(TypeParameterTypeVar),
    ParamSpec(TypeParameterParamSpec),
    TypeVarTuple(TypeParameterTypeVarTuple),
}

impl TypeParameter {
    pub fn ast_id(&self) -> HirAstId<TypeParam> {
        match self {
            TypeParameter::TypeVar(type_var) => type_var.ast_id.upcast(),
            TypeParameter::ParamSpec(param_spec) => param_spec.ast_id.upcast(),
            TypeParameter::TypeVarTuple(type_var_tuple) => type_var_tuple.ast_id.upcast(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TypeParameterTypeVar {
    name: Name,
    ast_id: HirAstId<TypeParamTypeVar>,
}

#[derive(Debug, Clone)]
pub struct TypeParameterParamSpec {
    name: Name,
    ast_id: HirAstId<TypeParamParamSpec>,
}

#[derive(Debug, Clone)]
pub struct TypeParameterTypeVarTuple {
    name: Name,
    ast_id: HirAstId<TypeParamTypeVarTuple>,
}
// TODO We probably need to track more but I'm not sure. It kind of depends on how these nodes are used
//   downstream. For example, do we need to track augmented assignments? We probably should because they can change the
//   public interface by re-assigning. How about expression statements? Or do we even have to track all possible top-level statements?
//   The advantage of the current approach (by storing functions separate) is that function definitions
//   have very stable ids. A function id only changes if a function is added or removed, but it remains
//   unaffected if e.g. a class is added or removed.
//
// TODO how to handle imports?
pub enum DefinitionId {
    Function(FunctionId),
    Class(ClassId),
    Assignment(AssignmentId),
    AnnotatedAssignment(AnnotatedAssignmentId),
}

pub enum ModuleItem {
    Function(Function),
    Class(Class),
    Assignment(Assignment),
    AnnotatedAssignment(AnnotatedAssignment),
}

impl ModuleItem {
    pub fn ast_id(&self) -> HirAstId<Stmt> {
        match self {
            ModuleItem::Function(function) => function.ast_id.upcast(),
            ModuleItem::Class(class) => class.ast_id.upcast(),
            ModuleItem::Assignment(assignment) => assignment.ast_id.upcast(),
            ModuleItem::AnnotatedAssignment(annotation) => annotation.ast_id.upcast(),
        }
    }

    pub fn name(&self) -> Option<&Name> {
        match self {
            ModuleItem::Function(function) => Some(&function.name),
            ModuleItem::Class(class) => Some(&class.name),
            ModuleItem::Assignment(assignment) => Some(&assignment.name),
            ModuleItem::AnnotatedAssignment(annotation) => Some(&annotation.name),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Module {
    functions: IndexVec<FunctionId, Function>,
    classes: IndexVec<ClassId, Class>,
    assignments: IndexVec<AssignmentId, Assignment>,
    annotated_assignments: IndexVec<AnnotatedAssignmentId, AnnotatedAssignment>,
    type_aliases: IndexVec<TypeAliasId, TypeAlias>,
}

impl Index<FunctionId> for Module {
    type Output = Function;

    fn index(&self, index: FunctionId) -> &Self::Output {
        &self.functions[index]
    }
}

impl Index<ClassId> for Module {
    type Output = Class;

    fn index(&self, index: ClassId) -> &Self::Output {
        &self.classes[index]
    }
}

impl Index<AssignmentId> for Module {
    type Output = Assignment;

    fn index(&self, index: AssignmentId) -> &Self::Output {
        &self.assignments[index]
    }
}

impl Index<AnnotatedAssignmentId> for Module {
    type Output = AnnotatedAssignment;

    fn index(&self, index: AnnotatedAssignmentId) -> &Self::Output {
        &self.annotated_assignments[index]
    }
}

impl Index<TypeAliasId> for Module {
    type Output = TypeAlias;

    fn index(&self, index: TypeAliasId) -> &Self::Output {
        &self.type_aliases[index]
    }
}
