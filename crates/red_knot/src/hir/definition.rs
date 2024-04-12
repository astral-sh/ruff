use std::ops::{Index, Range};

use ruff_index::{newtype_index, IndexVec};
use ruff_python_ast::visitor::preorder;
use ruff_python_ast::visitor::preorder::PreorderVisitor;
use ruff_python_ast::{
    Decorator, ExceptHandler, ExceptHandlerExceptHandler, Expr, MatchCase, ModModule, Stmt,
    StmtAnnAssign, StmtAssign, StmtClassDef, StmtFunctionDef, StmtGlobal, StmtImport,
    StmtImportFrom, StmtNonlocal, StmtTypeAlias, TypeParam, TypeParamParamSpec, TypeParamTypeVar,
    TypeParamTypeVarTuple, WithItem,
};

use crate::ast_ids::{AstIds, HasAstId};
use crate::files::FileId;
use crate::hir::HirAstId;
use crate::Name;

#[newtype_index]
pub struct FunctionId;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Function {
    ast_id: HirAstId<StmtFunctionDef>,
    name: Name,
    parameters: Range<ParameterId>,
    type_parameters: Range<TypeParameterId>, // TODO: type_parameters, return expression, decorators
}

#[newtype_index]
pub struct ParameterId;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Parameter {
    kind: ParameterKind,
    name: Name,
    default: Option<()>, // TODO use expression HIR
    ast_id: HirAstId<ruff_python_ast::Parameter>,
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

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Class {
    name: Name,
    ast_id: HirAstId<StmtClassDef>,
    // TODO type parameters, inheritance, decorators, members
}

#[newtype_index]
pub struct AssignmentId;

// This can have more than one name...
// but that means we can't implement `name()` on `ModuleItem`.

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Assignment {
    // TODO: Handle multiple names  / targets
    name: Name,
    ast_id: HirAstId<StmtAssign>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AnnotatedAssignment {
    name: Name,
    ast_id: HirAstId<StmtAnnAssign>,
}

#[newtype_index]
pub struct AnnotatedAssignmentId;

#[newtype_index]
pub struct TypeAliasId;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TypeAlias {
    name: Name,
    ast_id: HirAstId<StmtTypeAlias>,
    parameters: Range<TypeParameterId>,
}

#[newtype_index]
pub struct TypeParameterId;

#[derive(Debug, Clone, Eq, PartialEq)]
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

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TypeParameterTypeVar {
    name: Name,
    ast_id: HirAstId<TypeParamTypeVar>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TypeParameterParamSpec {
    name: Name,
    ast_id: HirAstId<TypeParamParamSpec>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TypeParameterTypeVarTuple {
    name: Name,
    ast_id: HirAstId<TypeParamTypeVarTuple>,
}

#[newtype_index]
pub struct GlobalId;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Global {
    ast_id: HirAstId<StmtGlobal>,
}

#[newtype_index]
pub struct NonLocalId;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NonLocal {
    ast_id: HirAstId<StmtNonlocal>,
}

pub enum DefinitionId {
    Function(FunctionId),
    Parameter(ParameterId),
    Class(ClassId),
    Assignment(AssignmentId),
    AnnotatedAssignment(AnnotatedAssignmentId),
    Global(GlobalId),
    NonLocal(NonLocalId),
    TypeParameter(TypeParameterId),
    TypeAlias(TypeAlias),
}

pub enum DefinitionItem {
    Function(Function),
    Parameter(Parameter),
    Class(Class),
    Assignment(Assignment),
    AnnotatedAssignment(AnnotatedAssignment),
    Global(Global),
    NonLocal(NonLocal),
    TypeParameter(TypeParameter),
    TypeAlias(TypeAlias),
}

// The closest is rust-analyzers item-tree. It only represents "Items" which make the public interface of a module
// (it excludes any other statement or expressions). rust-analyzer uses it as the main input to the name resolution
// algorithm
// > It is the input to the name resolution algorithm, as well as to the queries defined in `adt.rs`,
// > `data.rs`, and most things in `attr.rs`.
//
// > One important purpose of this layer is to provide an "invalidation barrier" for incremental
// > computations: when typing inside an item body, the `ItemTree` of the modified file is typically
// > unaffected, so we don't have to recompute name resolution results or item data (see `data.rs`).
//
// I haven't fully figured this out but I think that this composes the "public" interface of a module?
// But maybe that's too optimistic.
//
//
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct Definitions {
    functions: IndexVec<FunctionId, Function>,
    parameters: IndexVec<ParameterId, Parameter>,
    classes: IndexVec<ClassId, Class>,
    assignments: IndexVec<AssignmentId, Assignment>,
    annotated_assignments: IndexVec<AnnotatedAssignmentId, AnnotatedAssignment>,
    type_aliases: IndexVec<TypeAliasId, TypeAlias>,
    type_parameters: IndexVec<TypeParameterId, TypeParameter>,
    globals: IndexVec<GlobalId, Global>,
    non_locals: IndexVec<NonLocalId, NonLocal>,
}

impl Definitions {
    pub fn from_module(module: &ModModule, ast_ids: &AstIds, file_id: FileId) -> Self {
        let mut visitor = DefinitionsVisitor {
            definitions: Definitions::default(),
            ast_ids,
            file_id,
        };

        visitor.visit_body(&module.body);

        visitor.definitions
    }
}

impl Index<FunctionId> for Definitions {
    type Output = Function;

    fn index(&self, index: FunctionId) -> &Self::Output {
        &self.functions[index]
    }
}

impl Index<ParameterId> for Definitions {
    type Output = Parameter;

    fn index(&self, index: ParameterId) -> &Self::Output {
        &self.parameters[index]
    }
}

impl Index<ClassId> for Definitions {
    type Output = Class;

    fn index(&self, index: ClassId) -> &Self::Output {
        &self.classes[index]
    }
}

impl Index<AssignmentId> for Definitions {
    type Output = Assignment;

    fn index(&self, index: AssignmentId) -> &Self::Output {
        &self.assignments[index]
    }
}

impl Index<AnnotatedAssignmentId> for Definitions {
    type Output = AnnotatedAssignment;

    fn index(&self, index: AnnotatedAssignmentId) -> &Self::Output {
        &self.annotated_assignments[index]
    }
}

impl Index<TypeAliasId> for Definitions {
    type Output = TypeAlias;

    fn index(&self, index: TypeAliasId) -> &Self::Output {
        &self.type_aliases[index]
    }
}

impl Index<GlobalId> for Definitions {
    type Output = Global;

    fn index(&self, index: GlobalId) -> &Self::Output {
        &self.globals[index]
    }
}

impl Index<NonLocalId> for Definitions {
    type Output = NonLocal;

    fn index(&self, index: NonLocalId) -> &Self::Output {
        &self.non_locals[index]
    }
}

impl Index<TypeParameterId> for Definitions {
    type Output = TypeParameter;

    fn index(&self, index: TypeParameterId) -> &Self::Output {
        &self.type_parameters[index]
    }
}

struct DefinitionsVisitor<'a> {
    definitions: Definitions,
    ast_ids: &'a AstIds,
    file_id: FileId,
}

impl DefinitionsVisitor<'_> {
    fn ast_id<N: HasAstId>(&self, node: &N) -> HirAstId<N> {
        HirAstId {
            file_id: self.file_id,
            node_id: self.ast_ids.ast_id(node),
        }
    }

    fn lower_function_def(&mut self, function: &StmtFunctionDef) -> FunctionId {
        let name = Name::new(&function.name);

        let first_type_parameter_id = self.definitions.type_parameters.next_index();
        let mut last_type_parameter_id = first_type_parameter_id;

        if let Some(type_params) = &function.type_params {
            for parameter in &type_params.type_params {
                let id = self.lower_type_parameter(parameter);
                last_type_parameter_id = id;
            }
        }

        let parameters = self.lower_parameters(&function.parameters);

        self.definitions.functions.push(Function {
            name,
            ast_id: self.ast_id(function),
            parameters,
            type_parameters: first_type_parameter_id..last_type_parameter_id,
        })
    }

    fn lower_parameters(&mut self, parameters: &ruff_python_ast::Parameters) -> Range<ParameterId> {
        let first_parameter_id = self.definitions.parameters.next_index();
        let mut last_parameter_id = first_parameter_id;

        for parameter in &parameters.posonlyargs {
            last_parameter_id = self.definitions.parameters.push(Parameter {
                kind: ParameterKind::PositionalOnly,
                name: Name::new(&parameter.parameter.name),
                default: None,
                ast_id: self.ast_id(&parameter.parameter),
            });
        }

        if let Some(vararg) = &parameters.vararg {
            last_parameter_id = self.definitions.parameters.push(Parameter {
                kind: ParameterKind::Vararg,
                name: Name::new(&vararg.name),
                default: None,
                ast_id: self.ast_id(vararg),
            });
        }

        for parameter in &parameters.kwonlyargs {
            last_parameter_id = self.definitions.parameters.push(Parameter {
                kind: ParameterKind::KeywordOnly,
                name: Name::new(&parameter.parameter.name),
                default: None,
                ast_id: self.ast_id(&parameter.parameter),
            });
        }

        if let Some(kwarg) = &parameters.kwarg {
            last_parameter_id = self.definitions.parameters.push(Parameter {
                kind: ParameterKind::KeywordOnly,
                name: Name::new(&kwarg.name),
                default: None,
                ast_id: self.ast_id(kwarg),
            });
        }

        first_parameter_id..last_parameter_id
    }

    fn lower_class_def(&mut self, class: &StmtClassDef) -> ClassId {
        let name = Name::new(&class.name);

        self.definitions.classes.push(Class {
            name,
            ast_id: self.ast_id(class),
        })
    }

    fn lower_assignment(&mut self, assignment: &StmtAssign) {
        // FIXME handle multiple names
        if let Some(Expr::Name(name)) = assignment.targets.first() {
            self.definitions.assignments.push(Assignment {
                name: Name::new(&name.id),
                ast_id: self.ast_id(assignment),
            });
        }
    }

    fn lower_annotated_assignment(&mut self, annotated_assignment: &StmtAnnAssign) {
        if let Expr::Name(name) = &*annotated_assignment.target {
            self.definitions
                .annotated_assignments
                .push(AnnotatedAssignment {
                    name: Name::new(&name.id),
                    ast_id: self.ast_id(annotated_assignment),
                });
        }
    }

    fn lower_type_alias(&mut self, type_alias: &StmtTypeAlias) {
        if let Expr::Name(name) = &*type_alias.name {
            let name = Name::new(&name.id);

            let lower_parameters_id = self.definitions.type_parameters.next_index();
            let mut last_parameter_id = lower_parameters_id;

            if let Some(type_params) = &type_alias.type_params {
                for type_parameter in &type_params.type_params {
                    let id = self.lower_type_parameter(type_parameter);
                    last_parameter_id = id;
                }
            }

            self.definitions.type_aliases.push(TypeAlias {
                name,
                ast_id: self.ast_id(type_alias),
                parameters: lower_parameters_id..last_parameter_id,
            });
        }
    }

    fn lower_type_parameter(&mut self, type_parameter: &TypeParam) -> TypeParameterId {
        match type_parameter {
            TypeParam::TypeVar(type_var) => {
                let id = self
                    .definitions
                    .type_parameters
                    .push(TypeParameter::TypeVar(TypeParameterTypeVar {
                        name: Name::new(&type_var.name),
                        ast_id: self.ast_id(type_var),
                    }));
                id
            }
            TypeParam::ParamSpec(param_spec) => {
                let id = self
                    .definitions
                    .type_parameters
                    .push(TypeParameter::ParamSpec(TypeParameterParamSpec {
                        name: Name::new(&param_spec.name),
                        ast_id: self.ast_id(param_spec),
                    }));
                id
            }
            TypeParam::TypeVarTuple(type_var_tuple) => {
                let id = self
                    .definitions
                    .type_parameters
                    .push(TypeParameter::TypeVarTuple(TypeParameterTypeVarTuple {
                        name: Name::new(&type_var_tuple.name),
                        ast_id: self.ast_id(type_var_tuple),
                    }));
                id
            }
        }
    }

    fn lower_import(&mut self, import: &StmtImport) {
        // TODO
    }

    fn lower_import_from(&mut self, import_from: &StmtImportFrom) {
        // TODO
    }

    fn lower_global(&mut self, global: &StmtGlobal) -> GlobalId {
        self.definitions.globals.push(Global {
            ast_id: self.ast_id(global),
        })
    }

    fn lower_non_local(&mut self, non_local: &StmtNonlocal) -> NonLocalId {
        self.definitions.non_locals.push(NonLocal {
            ast_id: self.ast_id(non_local),
        })
    }

    fn lower_except_handler(&mut self, except_handler: &ExceptHandlerExceptHandler) {
        // TODO
    }

    fn lower_with_item(&mut self, with_item: &WithItem) {
        // TODO
    }

    fn lower_match_case(&mut self, match_case: &MatchCase) {
        // TODO
    }
}

impl PreorderVisitor<'_> for DefinitionsVisitor<'_> {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            // Definition statements
            Stmt::FunctionDef(definition) => {
                self.lower_function_def(definition);
                self.visit_body(&definition.body);
            }
            Stmt::ClassDef(definition) => {
                self.lower_class_def(definition);
                self.visit_body(&definition.body);
            }
            Stmt::Assign(assignment) => {
                self.lower_assignment(assignment);
            }
            Stmt::AnnAssign(annotated_assignment) => {
                self.lower_annotated_assignment(annotated_assignment);
            }
            Stmt::TypeAlias(type_alias) => {
                self.lower_type_alias(type_alias);
            }

            Stmt::Import(import) => self.lower_import(import),
            Stmt::ImportFrom(import_from) => self.lower_import_from(import_from),
            Stmt::Global(global) => {
                self.lower_global(global);
            }
            Stmt::Nonlocal(non_local) => {
                self.lower_non_local(non_local);
            }

            // Visit the compound statement bodies because they can contain other definitions.
            Stmt::For(_)
            | Stmt::While(_)
            | Stmt::If(_)
            | Stmt::With(_)
            | Stmt::Match(_)
            | Stmt::Try(_) => {
                preorder::walk_stmt(self, stmt);
            }

            // Skip over simple statements because they can't contain any other definitions.
            Stmt::Return(_)
            | Stmt::Delete(_)
            | Stmt::AugAssign(_)
            | Stmt::Raise(_)
            | Stmt::Assert(_)
            | Stmt::Expr(_)
            | Stmt::Pass(_)
            | Stmt::Break(_)
            | Stmt::Continue(_)
            | Stmt::IpyEscapeCommand(_) => {
                // No op
            }
        }
    }

    fn visit_expr(&mut self, _: &'_ Expr) {}

    fn visit_decorator(&mut self, _decorator: &'_ Decorator) {}

    fn visit_except_handler(&mut self, except_handler: &'_ ExceptHandler) {
        match except_handler {
            ExceptHandler::ExceptHandler(except_handler) => {
                self.lower_except_handler(except_handler)
            }
        }
    }

    fn visit_with_item(&mut self, with_item: &'_ WithItem) {
        self.lower_with_item(&with_item);
    }

    fn visit_match_case(&mut self, match_case: &'_ MatchCase) {
        self.lower_match_case(&match_case);
        self.visit_body(&match_case.body);
    }
}
