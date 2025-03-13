// This is a generated file. Don't modify it by hand!
// Run `crates/ruff_python_ast/generate.py` to re-generate the file.

use crate::name::Name;

/// See also [mod](https://docs.python.org/3/library/ast.html#ast.mod)
#[derive(Clone, Debug, PartialEq)]
pub enum Mod {
    Module(crate::ModModule),
    Expression(crate::ModExpression),
}

impl From<crate::ModModule> for Mod {
    fn from(node: crate::ModModule) -> Self {
        Self::Module(node)
    }
}

impl From<crate::ModExpression> for Mod {
    fn from(node: crate::ModExpression) -> Self {
        Self::Expression(node)
    }
}

impl ruff_text_size::Ranged for Mod {
    fn range(&self) -> ruff_text_size::TextRange {
        match self {
            Self::Module(node) => node.range(),
            Self::Expression(node) => node.range(),
        }
    }
}

#[allow(dead_code, clippy::match_wildcard_for_single_variants)]
impl Mod {
    #[inline]
    pub const fn is_module(&self) -> bool {
        matches!(self, Self::Module(_))
    }

    #[inline]
    pub fn module(self) -> Option<crate::ModModule> {
        match self {
            Self::Module(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_module(self) -> crate::ModModule {
        match self {
            Self::Module(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_module_mut(&mut self) -> Option<&mut crate::ModModule> {
        match self {
            Self::Module(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_module(&self) -> Option<&crate::ModModule> {
        match self {
            Self::Module(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_expression(&self) -> bool {
        matches!(self, Self::Expression(_))
    }

    #[inline]
    pub fn expression(self) -> Option<crate::ModExpression> {
        match self {
            Self::Expression(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_expression(self) -> crate::ModExpression {
        match self {
            Self::Expression(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_expression_mut(&mut self) -> Option<&mut crate::ModExpression> {
        match self {
            Self::Expression(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_expression(&self) -> Option<&crate::ModExpression> {
        match self {
            Self::Expression(val) => Some(val),
            _ => None,
        }
    }
}

/// See also [stmt](https://docs.python.org/3/library/ast.html#ast.stmt)
#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    FunctionDef(crate::StmtFunctionDef),
    ClassDef(crate::StmtClassDef),
    Return(crate::StmtReturn),
    Delete(crate::StmtDelete),
    TypeAlias(crate::StmtTypeAlias),
    Assign(crate::StmtAssign),
    AugAssign(crate::StmtAugAssign),
    AnnAssign(crate::StmtAnnAssign),
    For(crate::StmtFor),
    While(crate::StmtWhile),
    If(crate::StmtIf),
    With(crate::StmtWith),
    Match(crate::StmtMatch),
    Raise(crate::StmtRaise),
    Try(crate::StmtTry),
    Assert(crate::StmtAssert),
    Import(crate::StmtImport),
    ImportFrom(crate::StmtImportFrom),
    Global(crate::StmtGlobal),
    Nonlocal(crate::StmtNonlocal),
    Expr(crate::StmtExpr),
    Pass(crate::StmtPass),
    Break(crate::StmtBreak),
    Continue(crate::StmtContinue),
    IpyEscapeCommand(crate::StmtIpyEscapeCommand),
}

impl From<crate::StmtFunctionDef> for Stmt {
    fn from(node: crate::StmtFunctionDef) -> Self {
        Self::FunctionDef(node)
    }
}

impl From<crate::StmtClassDef> for Stmt {
    fn from(node: crate::StmtClassDef) -> Self {
        Self::ClassDef(node)
    }
}

impl From<crate::StmtReturn> for Stmt {
    fn from(node: crate::StmtReturn) -> Self {
        Self::Return(node)
    }
}

impl From<crate::StmtDelete> for Stmt {
    fn from(node: crate::StmtDelete) -> Self {
        Self::Delete(node)
    }
}

impl From<crate::StmtTypeAlias> for Stmt {
    fn from(node: crate::StmtTypeAlias) -> Self {
        Self::TypeAlias(node)
    }
}

impl From<crate::StmtAssign> for Stmt {
    fn from(node: crate::StmtAssign) -> Self {
        Self::Assign(node)
    }
}

impl From<crate::StmtAugAssign> for Stmt {
    fn from(node: crate::StmtAugAssign) -> Self {
        Self::AugAssign(node)
    }
}

impl From<crate::StmtAnnAssign> for Stmt {
    fn from(node: crate::StmtAnnAssign) -> Self {
        Self::AnnAssign(node)
    }
}

impl From<crate::StmtFor> for Stmt {
    fn from(node: crate::StmtFor) -> Self {
        Self::For(node)
    }
}

impl From<crate::StmtWhile> for Stmt {
    fn from(node: crate::StmtWhile) -> Self {
        Self::While(node)
    }
}

impl From<crate::StmtIf> for Stmt {
    fn from(node: crate::StmtIf) -> Self {
        Self::If(node)
    }
}

impl From<crate::StmtWith> for Stmt {
    fn from(node: crate::StmtWith) -> Self {
        Self::With(node)
    }
}

impl From<crate::StmtMatch> for Stmt {
    fn from(node: crate::StmtMatch) -> Self {
        Self::Match(node)
    }
}

impl From<crate::StmtRaise> for Stmt {
    fn from(node: crate::StmtRaise) -> Self {
        Self::Raise(node)
    }
}

impl From<crate::StmtTry> for Stmt {
    fn from(node: crate::StmtTry) -> Self {
        Self::Try(node)
    }
}

impl From<crate::StmtAssert> for Stmt {
    fn from(node: crate::StmtAssert) -> Self {
        Self::Assert(node)
    }
}

impl From<crate::StmtImport> for Stmt {
    fn from(node: crate::StmtImport) -> Self {
        Self::Import(node)
    }
}

impl From<crate::StmtImportFrom> for Stmt {
    fn from(node: crate::StmtImportFrom) -> Self {
        Self::ImportFrom(node)
    }
}

impl From<crate::StmtGlobal> for Stmt {
    fn from(node: crate::StmtGlobal) -> Self {
        Self::Global(node)
    }
}

impl From<crate::StmtNonlocal> for Stmt {
    fn from(node: crate::StmtNonlocal) -> Self {
        Self::Nonlocal(node)
    }
}

impl From<crate::StmtExpr> for Stmt {
    fn from(node: crate::StmtExpr) -> Self {
        Self::Expr(node)
    }
}

impl From<crate::StmtPass> for Stmt {
    fn from(node: crate::StmtPass) -> Self {
        Self::Pass(node)
    }
}

impl From<crate::StmtBreak> for Stmt {
    fn from(node: crate::StmtBreak) -> Self {
        Self::Break(node)
    }
}

impl From<crate::StmtContinue> for Stmt {
    fn from(node: crate::StmtContinue) -> Self {
        Self::Continue(node)
    }
}

impl From<crate::StmtIpyEscapeCommand> for Stmt {
    fn from(node: crate::StmtIpyEscapeCommand) -> Self {
        Self::IpyEscapeCommand(node)
    }
}

impl ruff_text_size::Ranged for Stmt {
    fn range(&self) -> ruff_text_size::TextRange {
        match self {
            Self::FunctionDef(node) => node.range(),
            Self::ClassDef(node) => node.range(),
            Self::Return(node) => node.range(),
            Self::Delete(node) => node.range(),
            Self::TypeAlias(node) => node.range(),
            Self::Assign(node) => node.range(),
            Self::AugAssign(node) => node.range(),
            Self::AnnAssign(node) => node.range(),
            Self::For(node) => node.range(),
            Self::While(node) => node.range(),
            Self::If(node) => node.range(),
            Self::With(node) => node.range(),
            Self::Match(node) => node.range(),
            Self::Raise(node) => node.range(),
            Self::Try(node) => node.range(),
            Self::Assert(node) => node.range(),
            Self::Import(node) => node.range(),
            Self::ImportFrom(node) => node.range(),
            Self::Global(node) => node.range(),
            Self::Nonlocal(node) => node.range(),
            Self::Expr(node) => node.range(),
            Self::Pass(node) => node.range(),
            Self::Break(node) => node.range(),
            Self::Continue(node) => node.range(),
            Self::IpyEscapeCommand(node) => node.range(),
        }
    }
}

#[allow(dead_code, clippy::match_wildcard_for_single_variants)]
impl Stmt {
    #[inline]
    pub const fn is_function_def_stmt(&self) -> bool {
        matches!(self, Self::FunctionDef(_))
    }

    #[inline]
    pub fn function_def_stmt(self) -> Option<crate::StmtFunctionDef> {
        match self {
            Self::FunctionDef(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_function_def_stmt(self) -> crate::StmtFunctionDef {
        match self {
            Self::FunctionDef(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_function_def_stmt_mut(&mut self) -> Option<&mut crate::StmtFunctionDef> {
        match self {
            Self::FunctionDef(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_function_def_stmt(&self) -> Option<&crate::StmtFunctionDef> {
        match self {
            Self::FunctionDef(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_class_def_stmt(&self) -> bool {
        matches!(self, Self::ClassDef(_))
    }

    #[inline]
    pub fn class_def_stmt(self) -> Option<crate::StmtClassDef> {
        match self {
            Self::ClassDef(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_class_def_stmt(self) -> crate::StmtClassDef {
        match self {
            Self::ClassDef(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_class_def_stmt_mut(&mut self) -> Option<&mut crate::StmtClassDef> {
        match self {
            Self::ClassDef(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_class_def_stmt(&self) -> Option<&crate::StmtClassDef> {
        match self {
            Self::ClassDef(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_return_stmt(&self) -> bool {
        matches!(self, Self::Return(_))
    }

    #[inline]
    pub fn return_stmt(self) -> Option<crate::StmtReturn> {
        match self {
            Self::Return(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_return_stmt(self) -> crate::StmtReturn {
        match self {
            Self::Return(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_return_stmt_mut(&mut self) -> Option<&mut crate::StmtReturn> {
        match self {
            Self::Return(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_return_stmt(&self) -> Option<&crate::StmtReturn> {
        match self {
            Self::Return(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_delete_stmt(&self) -> bool {
        matches!(self, Self::Delete(_))
    }

    #[inline]
    pub fn delete_stmt(self) -> Option<crate::StmtDelete> {
        match self {
            Self::Delete(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_delete_stmt(self) -> crate::StmtDelete {
        match self {
            Self::Delete(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_delete_stmt_mut(&mut self) -> Option<&mut crate::StmtDelete> {
        match self {
            Self::Delete(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_delete_stmt(&self) -> Option<&crate::StmtDelete> {
        match self {
            Self::Delete(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_type_alias_stmt(&self) -> bool {
        matches!(self, Self::TypeAlias(_))
    }

    #[inline]
    pub fn type_alias_stmt(self) -> Option<crate::StmtTypeAlias> {
        match self {
            Self::TypeAlias(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_type_alias_stmt(self) -> crate::StmtTypeAlias {
        match self {
            Self::TypeAlias(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_type_alias_stmt_mut(&mut self) -> Option<&mut crate::StmtTypeAlias> {
        match self {
            Self::TypeAlias(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_type_alias_stmt(&self) -> Option<&crate::StmtTypeAlias> {
        match self {
            Self::TypeAlias(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_assign_stmt(&self) -> bool {
        matches!(self, Self::Assign(_))
    }

    #[inline]
    pub fn assign_stmt(self) -> Option<crate::StmtAssign> {
        match self {
            Self::Assign(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_assign_stmt(self) -> crate::StmtAssign {
        match self {
            Self::Assign(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_assign_stmt_mut(&mut self) -> Option<&mut crate::StmtAssign> {
        match self {
            Self::Assign(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_assign_stmt(&self) -> Option<&crate::StmtAssign> {
        match self {
            Self::Assign(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_aug_assign_stmt(&self) -> bool {
        matches!(self, Self::AugAssign(_))
    }

    #[inline]
    pub fn aug_assign_stmt(self) -> Option<crate::StmtAugAssign> {
        match self {
            Self::AugAssign(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_aug_assign_stmt(self) -> crate::StmtAugAssign {
        match self {
            Self::AugAssign(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_aug_assign_stmt_mut(&mut self) -> Option<&mut crate::StmtAugAssign> {
        match self {
            Self::AugAssign(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_aug_assign_stmt(&self) -> Option<&crate::StmtAugAssign> {
        match self {
            Self::AugAssign(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_ann_assign_stmt(&self) -> bool {
        matches!(self, Self::AnnAssign(_))
    }

    #[inline]
    pub fn ann_assign_stmt(self) -> Option<crate::StmtAnnAssign> {
        match self {
            Self::AnnAssign(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_ann_assign_stmt(self) -> crate::StmtAnnAssign {
        match self {
            Self::AnnAssign(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_ann_assign_stmt_mut(&mut self) -> Option<&mut crate::StmtAnnAssign> {
        match self {
            Self::AnnAssign(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_ann_assign_stmt(&self) -> Option<&crate::StmtAnnAssign> {
        match self {
            Self::AnnAssign(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_for_stmt(&self) -> bool {
        matches!(self, Self::For(_))
    }

    #[inline]
    pub fn for_stmt(self) -> Option<crate::StmtFor> {
        match self {
            Self::For(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_for_stmt(self) -> crate::StmtFor {
        match self {
            Self::For(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_for_stmt_mut(&mut self) -> Option<&mut crate::StmtFor> {
        match self {
            Self::For(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_for_stmt(&self) -> Option<&crate::StmtFor> {
        match self {
            Self::For(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_while_stmt(&self) -> bool {
        matches!(self, Self::While(_))
    }

    #[inline]
    pub fn while_stmt(self) -> Option<crate::StmtWhile> {
        match self {
            Self::While(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_while_stmt(self) -> crate::StmtWhile {
        match self {
            Self::While(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_while_stmt_mut(&mut self) -> Option<&mut crate::StmtWhile> {
        match self {
            Self::While(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_while_stmt(&self) -> Option<&crate::StmtWhile> {
        match self {
            Self::While(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_if_stmt(&self) -> bool {
        matches!(self, Self::If(_))
    }

    #[inline]
    pub fn if_stmt(self) -> Option<crate::StmtIf> {
        match self {
            Self::If(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_if_stmt(self) -> crate::StmtIf {
        match self {
            Self::If(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_if_stmt_mut(&mut self) -> Option<&mut crate::StmtIf> {
        match self {
            Self::If(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_if_stmt(&self) -> Option<&crate::StmtIf> {
        match self {
            Self::If(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_with_stmt(&self) -> bool {
        matches!(self, Self::With(_))
    }

    #[inline]
    pub fn with_stmt(self) -> Option<crate::StmtWith> {
        match self {
            Self::With(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_with_stmt(self) -> crate::StmtWith {
        match self {
            Self::With(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_with_stmt_mut(&mut self) -> Option<&mut crate::StmtWith> {
        match self {
            Self::With(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_with_stmt(&self) -> Option<&crate::StmtWith> {
        match self {
            Self::With(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_match_stmt(&self) -> bool {
        matches!(self, Self::Match(_))
    }

    #[inline]
    pub fn match_stmt(self) -> Option<crate::StmtMatch> {
        match self {
            Self::Match(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_match_stmt(self) -> crate::StmtMatch {
        match self {
            Self::Match(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_match_stmt_mut(&mut self) -> Option<&mut crate::StmtMatch> {
        match self {
            Self::Match(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_match_stmt(&self) -> Option<&crate::StmtMatch> {
        match self {
            Self::Match(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_raise_stmt(&self) -> bool {
        matches!(self, Self::Raise(_))
    }

    #[inline]
    pub fn raise_stmt(self) -> Option<crate::StmtRaise> {
        match self {
            Self::Raise(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_raise_stmt(self) -> crate::StmtRaise {
        match self {
            Self::Raise(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_raise_stmt_mut(&mut self) -> Option<&mut crate::StmtRaise> {
        match self {
            Self::Raise(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_raise_stmt(&self) -> Option<&crate::StmtRaise> {
        match self {
            Self::Raise(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_try_stmt(&self) -> bool {
        matches!(self, Self::Try(_))
    }

    #[inline]
    pub fn try_stmt(self) -> Option<crate::StmtTry> {
        match self {
            Self::Try(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_try_stmt(self) -> crate::StmtTry {
        match self {
            Self::Try(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_try_stmt_mut(&mut self) -> Option<&mut crate::StmtTry> {
        match self {
            Self::Try(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_try_stmt(&self) -> Option<&crate::StmtTry> {
        match self {
            Self::Try(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_assert_stmt(&self) -> bool {
        matches!(self, Self::Assert(_))
    }

    #[inline]
    pub fn assert_stmt(self) -> Option<crate::StmtAssert> {
        match self {
            Self::Assert(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_assert_stmt(self) -> crate::StmtAssert {
        match self {
            Self::Assert(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_assert_stmt_mut(&mut self) -> Option<&mut crate::StmtAssert> {
        match self {
            Self::Assert(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_assert_stmt(&self) -> Option<&crate::StmtAssert> {
        match self {
            Self::Assert(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_import_stmt(&self) -> bool {
        matches!(self, Self::Import(_))
    }

    #[inline]
    pub fn import_stmt(self) -> Option<crate::StmtImport> {
        match self {
            Self::Import(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_import_stmt(self) -> crate::StmtImport {
        match self {
            Self::Import(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_import_stmt_mut(&mut self) -> Option<&mut crate::StmtImport> {
        match self {
            Self::Import(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_import_stmt(&self) -> Option<&crate::StmtImport> {
        match self {
            Self::Import(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_import_from_stmt(&self) -> bool {
        matches!(self, Self::ImportFrom(_))
    }

    #[inline]
    pub fn import_from_stmt(self) -> Option<crate::StmtImportFrom> {
        match self {
            Self::ImportFrom(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_import_from_stmt(self) -> crate::StmtImportFrom {
        match self {
            Self::ImportFrom(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_import_from_stmt_mut(&mut self) -> Option<&mut crate::StmtImportFrom> {
        match self {
            Self::ImportFrom(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_import_from_stmt(&self) -> Option<&crate::StmtImportFrom> {
        match self {
            Self::ImportFrom(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_global_stmt(&self) -> bool {
        matches!(self, Self::Global(_))
    }

    #[inline]
    pub fn global_stmt(self) -> Option<crate::StmtGlobal> {
        match self {
            Self::Global(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_global_stmt(self) -> crate::StmtGlobal {
        match self {
            Self::Global(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_global_stmt_mut(&mut self) -> Option<&mut crate::StmtGlobal> {
        match self {
            Self::Global(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_global_stmt(&self) -> Option<&crate::StmtGlobal> {
        match self {
            Self::Global(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_nonlocal_stmt(&self) -> bool {
        matches!(self, Self::Nonlocal(_))
    }

    #[inline]
    pub fn nonlocal_stmt(self) -> Option<crate::StmtNonlocal> {
        match self {
            Self::Nonlocal(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_nonlocal_stmt(self) -> crate::StmtNonlocal {
        match self {
            Self::Nonlocal(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_nonlocal_stmt_mut(&mut self) -> Option<&mut crate::StmtNonlocal> {
        match self {
            Self::Nonlocal(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_nonlocal_stmt(&self) -> Option<&crate::StmtNonlocal> {
        match self {
            Self::Nonlocal(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_expr_stmt(&self) -> bool {
        matches!(self, Self::Expr(_))
    }

    #[inline]
    pub fn expr_stmt(self) -> Option<crate::StmtExpr> {
        match self {
            Self::Expr(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_expr_stmt(self) -> crate::StmtExpr {
        match self {
            Self::Expr(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_expr_stmt_mut(&mut self) -> Option<&mut crate::StmtExpr> {
        match self {
            Self::Expr(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_expr_stmt(&self) -> Option<&crate::StmtExpr> {
        match self {
            Self::Expr(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_pass_stmt(&self) -> bool {
        matches!(self, Self::Pass(_))
    }

    #[inline]
    pub fn pass_stmt(self) -> Option<crate::StmtPass> {
        match self {
            Self::Pass(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_pass_stmt(self) -> crate::StmtPass {
        match self {
            Self::Pass(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_pass_stmt_mut(&mut self) -> Option<&mut crate::StmtPass> {
        match self {
            Self::Pass(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_pass_stmt(&self) -> Option<&crate::StmtPass> {
        match self {
            Self::Pass(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_break_stmt(&self) -> bool {
        matches!(self, Self::Break(_))
    }

    #[inline]
    pub fn break_stmt(self) -> Option<crate::StmtBreak> {
        match self {
            Self::Break(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_break_stmt(self) -> crate::StmtBreak {
        match self {
            Self::Break(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_break_stmt_mut(&mut self) -> Option<&mut crate::StmtBreak> {
        match self {
            Self::Break(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_break_stmt(&self) -> Option<&crate::StmtBreak> {
        match self {
            Self::Break(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_continue_stmt(&self) -> bool {
        matches!(self, Self::Continue(_))
    }

    #[inline]
    pub fn continue_stmt(self) -> Option<crate::StmtContinue> {
        match self {
            Self::Continue(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_continue_stmt(self) -> crate::StmtContinue {
        match self {
            Self::Continue(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_continue_stmt_mut(&mut self) -> Option<&mut crate::StmtContinue> {
        match self {
            Self::Continue(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_continue_stmt(&self) -> Option<&crate::StmtContinue> {
        match self {
            Self::Continue(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_ipy_escape_command_stmt(&self) -> bool {
        matches!(self, Self::IpyEscapeCommand(_))
    }

    #[inline]
    pub fn ipy_escape_command_stmt(self) -> Option<crate::StmtIpyEscapeCommand> {
        match self {
            Self::IpyEscapeCommand(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_ipy_escape_command_stmt(self) -> crate::StmtIpyEscapeCommand {
        match self {
            Self::IpyEscapeCommand(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_ipy_escape_command_stmt_mut(&mut self) -> Option<&mut crate::StmtIpyEscapeCommand> {
        match self {
            Self::IpyEscapeCommand(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_ipy_escape_command_stmt(&self) -> Option<&crate::StmtIpyEscapeCommand> {
        match self {
            Self::IpyEscapeCommand(val) => Some(val),
            _ => None,
        }
    }
}

/// See also [expr](https://docs.python.org/3/library/ast.html#ast.expr)
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    BoolOp(crate::ExprBoolOp),
    Named(crate::ExprNamed),
    BinOp(crate::ExprBinOp),
    UnaryOp(crate::ExprUnaryOp),
    Lambda(crate::ExprLambda),
    If(crate::ExprIf),
    Dict(crate::ExprDict),
    Set(crate::ExprSet),
    ListComp(crate::ExprListComp),
    SetComp(crate::ExprSetComp),
    DictComp(crate::ExprDictComp),
    Generator(crate::ExprGenerator),
    Await(crate::ExprAwait),
    Yield(crate::ExprYield),
    YieldFrom(crate::ExprYieldFrom),
    Compare(crate::ExprCompare),
    Call(crate::ExprCall),
    FString(crate::ExprFString),
    StringLiteral(crate::ExprStringLiteral),
    BytesLiteral(crate::ExprBytesLiteral),
    NumberLiteral(crate::ExprNumberLiteral),
    BooleanLiteral(crate::ExprBooleanLiteral),
    NoneLiteral(crate::ExprNoneLiteral),
    EllipsisLiteral(crate::ExprEllipsisLiteral),
    Attribute(crate::ExprAttribute),
    Subscript(crate::ExprSubscript),
    Starred(crate::ExprStarred),
    Name(crate::ExprName),
    List(crate::ExprList),
    Tuple(crate::ExprTuple),
    Slice(crate::ExprSlice),
    IpyEscapeCommand(crate::ExprIpyEscapeCommand),
}

impl From<crate::ExprBoolOp> for Expr {
    fn from(node: crate::ExprBoolOp) -> Self {
        Self::BoolOp(node)
    }
}

impl From<crate::ExprNamed> for Expr {
    fn from(node: crate::ExprNamed) -> Self {
        Self::Named(node)
    }
}

impl From<crate::ExprBinOp> for Expr {
    fn from(node: crate::ExprBinOp) -> Self {
        Self::BinOp(node)
    }
}

impl From<crate::ExprUnaryOp> for Expr {
    fn from(node: crate::ExprUnaryOp) -> Self {
        Self::UnaryOp(node)
    }
}

impl From<crate::ExprLambda> for Expr {
    fn from(node: crate::ExprLambda) -> Self {
        Self::Lambda(node)
    }
}

impl From<crate::ExprIf> for Expr {
    fn from(node: crate::ExprIf) -> Self {
        Self::If(node)
    }
}

impl From<crate::ExprDict> for Expr {
    fn from(node: crate::ExprDict) -> Self {
        Self::Dict(node)
    }
}

impl From<crate::ExprSet> for Expr {
    fn from(node: crate::ExprSet) -> Self {
        Self::Set(node)
    }
}

impl From<crate::ExprListComp> for Expr {
    fn from(node: crate::ExprListComp) -> Self {
        Self::ListComp(node)
    }
}

impl From<crate::ExprSetComp> for Expr {
    fn from(node: crate::ExprSetComp) -> Self {
        Self::SetComp(node)
    }
}

impl From<crate::ExprDictComp> for Expr {
    fn from(node: crate::ExprDictComp) -> Self {
        Self::DictComp(node)
    }
}

impl From<crate::ExprGenerator> for Expr {
    fn from(node: crate::ExprGenerator) -> Self {
        Self::Generator(node)
    }
}

impl From<crate::ExprAwait> for Expr {
    fn from(node: crate::ExprAwait) -> Self {
        Self::Await(node)
    }
}

impl From<crate::ExprYield> for Expr {
    fn from(node: crate::ExprYield) -> Self {
        Self::Yield(node)
    }
}

impl From<crate::ExprYieldFrom> for Expr {
    fn from(node: crate::ExprYieldFrom) -> Self {
        Self::YieldFrom(node)
    }
}

impl From<crate::ExprCompare> for Expr {
    fn from(node: crate::ExprCompare) -> Self {
        Self::Compare(node)
    }
}

impl From<crate::ExprCall> for Expr {
    fn from(node: crate::ExprCall) -> Self {
        Self::Call(node)
    }
}

impl From<crate::ExprFString> for Expr {
    fn from(node: crate::ExprFString) -> Self {
        Self::FString(node)
    }
}

impl From<crate::ExprStringLiteral> for Expr {
    fn from(node: crate::ExprStringLiteral) -> Self {
        Self::StringLiteral(node)
    }
}

impl From<crate::ExprBytesLiteral> for Expr {
    fn from(node: crate::ExprBytesLiteral) -> Self {
        Self::BytesLiteral(node)
    }
}

impl From<crate::ExprNumberLiteral> for Expr {
    fn from(node: crate::ExprNumberLiteral) -> Self {
        Self::NumberLiteral(node)
    }
}

impl From<crate::ExprBooleanLiteral> for Expr {
    fn from(node: crate::ExprBooleanLiteral) -> Self {
        Self::BooleanLiteral(node)
    }
}

impl From<crate::ExprNoneLiteral> for Expr {
    fn from(node: crate::ExprNoneLiteral) -> Self {
        Self::NoneLiteral(node)
    }
}

impl From<crate::ExprEllipsisLiteral> for Expr {
    fn from(node: crate::ExprEllipsisLiteral) -> Self {
        Self::EllipsisLiteral(node)
    }
}

impl From<crate::ExprAttribute> for Expr {
    fn from(node: crate::ExprAttribute) -> Self {
        Self::Attribute(node)
    }
}

impl From<crate::ExprSubscript> for Expr {
    fn from(node: crate::ExprSubscript) -> Self {
        Self::Subscript(node)
    }
}

impl From<crate::ExprStarred> for Expr {
    fn from(node: crate::ExprStarred) -> Self {
        Self::Starred(node)
    }
}

impl From<crate::ExprName> for Expr {
    fn from(node: crate::ExprName) -> Self {
        Self::Name(node)
    }
}

impl From<crate::ExprList> for Expr {
    fn from(node: crate::ExprList) -> Self {
        Self::List(node)
    }
}

impl From<crate::ExprTuple> for Expr {
    fn from(node: crate::ExprTuple) -> Self {
        Self::Tuple(node)
    }
}

impl From<crate::ExprSlice> for Expr {
    fn from(node: crate::ExprSlice) -> Self {
        Self::Slice(node)
    }
}

impl From<crate::ExprIpyEscapeCommand> for Expr {
    fn from(node: crate::ExprIpyEscapeCommand) -> Self {
        Self::IpyEscapeCommand(node)
    }
}

impl ruff_text_size::Ranged for Expr {
    fn range(&self) -> ruff_text_size::TextRange {
        match self {
            Self::BoolOp(node) => node.range(),
            Self::Named(node) => node.range(),
            Self::BinOp(node) => node.range(),
            Self::UnaryOp(node) => node.range(),
            Self::Lambda(node) => node.range(),
            Self::If(node) => node.range(),
            Self::Dict(node) => node.range(),
            Self::Set(node) => node.range(),
            Self::ListComp(node) => node.range(),
            Self::SetComp(node) => node.range(),
            Self::DictComp(node) => node.range(),
            Self::Generator(node) => node.range(),
            Self::Await(node) => node.range(),
            Self::Yield(node) => node.range(),
            Self::YieldFrom(node) => node.range(),
            Self::Compare(node) => node.range(),
            Self::Call(node) => node.range(),
            Self::FString(node) => node.range(),
            Self::StringLiteral(node) => node.range(),
            Self::BytesLiteral(node) => node.range(),
            Self::NumberLiteral(node) => node.range(),
            Self::BooleanLiteral(node) => node.range(),
            Self::NoneLiteral(node) => node.range(),
            Self::EllipsisLiteral(node) => node.range(),
            Self::Attribute(node) => node.range(),
            Self::Subscript(node) => node.range(),
            Self::Starred(node) => node.range(),
            Self::Name(node) => node.range(),
            Self::List(node) => node.range(),
            Self::Tuple(node) => node.range(),
            Self::Slice(node) => node.range(),
            Self::IpyEscapeCommand(node) => node.range(),
        }
    }
}

#[allow(dead_code, clippy::match_wildcard_for_single_variants)]
impl Expr {
    #[inline]
    pub const fn is_bool_op_expr(&self) -> bool {
        matches!(self, Self::BoolOp(_))
    }

    #[inline]
    pub fn bool_op_expr(self) -> Option<crate::ExprBoolOp> {
        match self {
            Self::BoolOp(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_bool_op_expr(self) -> crate::ExprBoolOp {
        match self {
            Self::BoolOp(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_bool_op_expr_mut(&mut self) -> Option<&mut crate::ExprBoolOp> {
        match self {
            Self::BoolOp(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_bool_op_expr(&self) -> Option<&crate::ExprBoolOp> {
        match self {
            Self::BoolOp(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_named_expr(&self) -> bool {
        matches!(self, Self::Named(_))
    }

    #[inline]
    pub fn named_expr(self) -> Option<crate::ExprNamed> {
        match self {
            Self::Named(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_named_expr(self) -> crate::ExprNamed {
        match self {
            Self::Named(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_named_expr_mut(&mut self) -> Option<&mut crate::ExprNamed> {
        match self {
            Self::Named(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_named_expr(&self) -> Option<&crate::ExprNamed> {
        match self {
            Self::Named(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_bin_op_expr(&self) -> bool {
        matches!(self, Self::BinOp(_))
    }

    #[inline]
    pub fn bin_op_expr(self) -> Option<crate::ExprBinOp> {
        match self {
            Self::BinOp(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_bin_op_expr(self) -> crate::ExprBinOp {
        match self {
            Self::BinOp(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_bin_op_expr_mut(&mut self) -> Option<&mut crate::ExprBinOp> {
        match self {
            Self::BinOp(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_bin_op_expr(&self) -> Option<&crate::ExprBinOp> {
        match self {
            Self::BinOp(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_unary_op_expr(&self) -> bool {
        matches!(self, Self::UnaryOp(_))
    }

    #[inline]
    pub fn unary_op_expr(self) -> Option<crate::ExprUnaryOp> {
        match self {
            Self::UnaryOp(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_unary_op_expr(self) -> crate::ExprUnaryOp {
        match self {
            Self::UnaryOp(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_unary_op_expr_mut(&mut self) -> Option<&mut crate::ExprUnaryOp> {
        match self {
            Self::UnaryOp(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_unary_op_expr(&self) -> Option<&crate::ExprUnaryOp> {
        match self {
            Self::UnaryOp(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_lambda_expr(&self) -> bool {
        matches!(self, Self::Lambda(_))
    }

    #[inline]
    pub fn lambda_expr(self) -> Option<crate::ExprLambda> {
        match self {
            Self::Lambda(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_lambda_expr(self) -> crate::ExprLambda {
        match self {
            Self::Lambda(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_lambda_expr_mut(&mut self) -> Option<&mut crate::ExprLambda> {
        match self {
            Self::Lambda(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_lambda_expr(&self) -> Option<&crate::ExprLambda> {
        match self {
            Self::Lambda(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_if_expr(&self) -> bool {
        matches!(self, Self::If(_))
    }

    #[inline]
    pub fn if_expr(self) -> Option<crate::ExprIf> {
        match self {
            Self::If(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_if_expr(self) -> crate::ExprIf {
        match self {
            Self::If(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_if_expr_mut(&mut self) -> Option<&mut crate::ExprIf> {
        match self {
            Self::If(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_if_expr(&self) -> Option<&crate::ExprIf> {
        match self {
            Self::If(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_dict_expr(&self) -> bool {
        matches!(self, Self::Dict(_))
    }

    #[inline]
    pub fn dict_expr(self) -> Option<crate::ExprDict> {
        match self {
            Self::Dict(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_dict_expr(self) -> crate::ExprDict {
        match self {
            Self::Dict(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_dict_expr_mut(&mut self) -> Option<&mut crate::ExprDict> {
        match self {
            Self::Dict(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_dict_expr(&self) -> Option<&crate::ExprDict> {
        match self {
            Self::Dict(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_set_expr(&self) -> bool {
        matches!(self, Self::Set(_))
    }

    #[inline]
    pub fn set_expr(self) -> Option<crate::ExprSet> {
        match self {
            Self::Set(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_set_expr(self) -> crate::ExprSet {
        match self {
            Self::Set(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_set_expr_mut(&mut self) -> Option<&mut crate::ExprSet> {
        match self {
            Self::Set(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_set_expr(&self) -> Option<&crate::ExprSet> {
        match self {
            Self::Set(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_list_comp_expr(&self) -> bool {
        matches!(self, Self::ListComp(_))
    }

    #[inline]
    pub fn list_comp_expr(self) -> Option<crate::ExprListComp> {
        match self {
            Self::ListComp(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_list_comp_expr(self) -> crate::ExprListComp {
        match self {
            Self::ListComp(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_list_comp_expr_mut(&mut self) -> Option<&mut crate::ExprListComp> {
        match self {
            Self::ListComp(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_list_comp_expr(&self) -> Option<&crate::ExprListComp> {
        match self {
            Self::ListComp(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_set_comp_expr(&self) -> bool {
        matches!(self, Self::SetComp(_))
    }

    #[inline]
    pub fn set_comp_expr(self) -> Option<crate::ExprSetComp> {
        match self {
            Self::SetComp(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_set_comp_expr(self) -> crate::ExprSetComp {
        match self {
            Self::SetComp(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_set_comp_expr_mut(&mut self) -> Option<&mut crate::ExprSetComp> {
        match self {
            Self::SetComp(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_set_comp_expr(&self) -> Option<&crate::ExprSetComp> {
        match self {
            Self::SetComp(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_dict_comp_expr(&self) -> bool {
        matches!(self, Self::DictComp(_))
    }

    #[inline]
    pub fn dict_comp_expr(self) -> Option<crate::ExprDictComp> {
        match self {
            Self::DictComp(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_dict_comp_expr(self) -> crate::ExprDictComp {
        match self {
            Self::DictComp(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_dict_comp_expr_mut(&mut self) -> Option<&mut crate::ExprDictComp> {
        match self {
            Self::DictComp(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_dict_comp_expr(&self) -> Option<&crate::ExprDictComp> {
        match self {
            Self::DictComp(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_generator_expr(&self) -> bool {
        matches!(self, Self::Generator(_))
    }

    #[inline]
    pub fn generator_expr(self) -> Option<crate::ExprGenerator> {
        match self {
            Self::Generator(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_generator_expr(self) -> crate::ExprGenerator {
        match self {
            Self::Generator(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_generator_expr_mut(&mut self) -> Option<&mut crate::ExprGenerator> {
        match self {
            Self::Generator(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_generator_expr(&self) -> Option<&crate::ExprGenerator> {
        match self {
            Self::Generator(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_await_expr(&self) -> bool {
        matches!(self, Self::Await(_))
    }

    #[inline]
    pub fn await_expr(self) -> Option<crate::ExprAwait> {
        match self {
            Self::Await(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_await_expr(self) -> crate::ExprAwait {
        match self {
            Self::Await(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_await_expr_mut(&mut self) -> Option<&mut crate::ExprAwait> {
        match self {
            Self::Await(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_await_expr(&self) -> Option<&crate::ExprAwait> {
        match self {
            Self::Await(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_yield_expr(&self) -> bool {
        matches!(self, Self::Yield(_))
    }

    #[inline]
    pub fn yield_expr(self) -> Option<crate::ExprYield> {
        match self {
            Self::Yield(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_yield_expr(self) -> crate::ExprYield {
        match self {
            Self::Yield(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_yield_expr_mut(&mut self) -> Option<&mut crate::ExprYield> {
        match self {
            Self::Yield(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_yield_expr(&self) -> Option<&crate::ExprYield> {
        match self {
            Self::Yield(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_yield_from_expr(&self) -> bool {
        matches!(self, Self::YieldFrom(_))
    }

    #[inline]
    pub fn yield_from_expr(self) -> Option<crate::ExprYieldFrom> {
        match self {
            Self::YieldFrom(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_yield_from_expr(self) -> crate::ExprYieldFrom {
        match self {
            Self::YieldFrom(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_yield_from_expr_mut(&mut self) -> Option<&mut crate::ExprYieldFrom> {
        match self {
            Self::YieldFrom(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_yield_from_expr(&self) -> Option<&crate::ExprYieldFrom> {
        match self {
            Self::YieldFrom(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_compare_expr(&self) -> bool {
        matches!(self, Self::Compare(_))
    }

    #[inline]
    pub fn compare_expr(self) -> Option<crate::ExprCompare> {
        match self {
            Self::Compare(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_compare_expr(self) -> crate::ExprCompare {
        match self {
            Self::Compare(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_compare_expr_mut(&mut self) -> Option<&mut crate::ExprCompare> {
        match self {
            Self::Compare(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_compare_expr(&self) -> Option<&crate::ExprCompare> {
        match self {
            Self::Compare(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_call_expr(&self) -> bool {
        matches!(self, Self::Call(_))
    }

    #[inline]
    pub fn call_expr(self) -> Option<crate::ExprCall> {
        match self {
            Self::Call(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_call_expr(self) -> crate::ExprCall {
        match self {
            Self::Call(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_call_expr_mut(&mut self) -> Option<&mut crate::ExprCall> {
        match self {
            Self::Call(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_call_expr(&self) -> Option<&crate::ExprCall> {
        match self {
            Self::Call(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_f_string_expr(&self) -> bool {
        matches!(self, Self::FString(_))
    }

    #[inline]
    pub fn f_string_expr(self) -> Option<crate::ExprFString> {
        match self {
            Self::FString(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_f_string_expr(self) -> crate::ExprFString {
        match self {
            Self::FString(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_f_string_expr_mut(&mut self) -> Option<&mut crate::ExprFString> {
        match self {
            Self::FString(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_f_string_expr(&self) -> Option<&crate::ExprFString> {
        match self {
            Self::FString(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_string_literal_expr(&self) -> bool {
        matches!(self, Self::StringLiteral(_))
    }

    #[inline]
    pub fn string_literal_expr(self) -> Option<crate::ExprStringLiteral> {
        match self {
            Self::StringLiteral(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_string_literal_expr(self) -> crate::ExprStringLiteral {
        match self {
            Self::StringLiteral(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_string_literal_expr_mut(&mut self) -> Option<&mut crate::ExprStringLiteral> {
        match self {
            Self::StringLiteral(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_string_literal_expr(&self) -> Option<&crate::ExprStringLiteral> {
        match self {
            Self::StringLiteral(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_bytes_literal_expr(&self) -> bool {
        matches!(self, Self::BytesLiteral(_))
    }

    #[inline]
    pub fn bytes_literal_expr(self) -> Option<crate::ExprBytesLiteral> {
        match self {
            Self::BytesLiteral(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_bytes_literal_expr(self) -> crate::ExprBytesLiteral {
        match self {
            Self::BytesLiteral(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_bytes_literal_expr_mut(&mut self) -> Option<&mut crate::ExprBytesLiteral> {
        match self {
            Self::BytesLiteral(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_bytes_literal_expr(&self) -> Option<&crate::ExprBytesLiteral> {
        match self {
            Self::BytesLiteral(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_number_literal_expr(&self) -> bool {
        matches!(self, Self::NumberLiteral(_))
    }

    #[inline]
    pub fn number_literal_expr(self) -> Option<crate::ExprNumberLiteral> {
        match self {
            Self::NumberLiteral(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_number_literal_expr(self) -> crate::ExprNumberLiteral {
        match self {
            Self::NumberLiteral(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_number_literal_expr_mut(&mut self) -> Option<&mut crate::ExprNumberLiteral> {
        match self {
            Self::NumberLiteral(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_number_literal_expr(&self) -> Option<&crate::ExprNumberLiteral> {
        match self {
            Self::NumberLiteral(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_boolean_literal_expr(&self) -> bool {
        matches!(self, Self::BooleanLiteral(_))
    }

    #[inline]
    pub fn boolean_literal_expr(self) -> Option<crate::ExprBooleanLiteral> {
        match self {
            Self::BooleanLiteral(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_boolean_literal_expr(self) -> crate::ExprBooleanLiteral {
        match self {
            Self::BooleanLiteral(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_boolean_literal_expr_mut(&mut self) -> Option<&mut crate::ExprBooleanLiteral> {
        match self {
            Self::BooleanLiteral(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_boolean_literal_expr(&self) -> Option<&crate::ExprBooleanLiteral> {
        match self {
            Self::BooleanLiteral(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_none_literal_expr(&self) -> bool {
        matches!(self, Self::NoneLiteral(_))
    }

    #[inline]
    pub fn none_literal_expr(self) -> Option<crate::ExprNoneLiteral> {
        match self {
            Self::NoneLiteral(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_none_literal_expr(self) -> crate::ExprNoneLiteral {
        match self {
            Self::NoneLiteral(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_none_literal_expr_mut(&mut self) -> Option<&mut crate::ExprNoneLiteral> {
        match self {
            Self::NoneLiteral(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_none_literal_expr(&self) -> Option<&crate::ExprNoneLiteral> {
        match self {
            Self::NoneLiteral(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_ellipsis_literal_expr(&self) -> bool {
        matches!(self, Self::EllipsisLiteral(_))
    }

    #[inline]
    pub fn ellipsis_literal_expr(self) -> Option<crate::ExprEllipsisLiteral> {
        match self {
            Self::EllipsisLiteral(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_ellipsis_literal_expr(self) -> crate::ExprEllipsisLiteral {
        match self {
            Self::EllipsisLiteral(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_ellipsis_literal_expr_mut(&mut self) -> Option<&mut crate::ExprEllipsisLiteral> {
        match self {
            Self::EllipsisLiteral(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_ellipsis_literal_expr(&self) -> Option<&crate::ExprEllipsisLiteral> {
        match self {
            Self::EllipsisLiteral(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_attribute_expr(&self) -> bool {
        matches!(self, Self::Attribute(_))
    }

    #[inline]
    pub fn attribute_expr(self) -> Option<crate::ExprAttribute> {
        match self {
            Self::Attribute(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_attribute_expr(self) -> crate::ExprAttribute {
        match self {
            Self::Attribute(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_attribute_expr_mut(&mut self) -> Option<&mut crate::ExprAttribute> {
        match self {
            Self::Attribute(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_attribute_expr(&self) -> Option<&crate::ExprAttribute> {
        match self {
            Self::Attribute(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_subscript_expr(&self) -> bool {
        matches!(self, Self::Subscript(_))
    }

    #[inline]
    pub fn subscript_expr(self) -> Option<crate::ExprSubscript> {
        match self {
            Self::Subscript(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_subscript_expr(self) -> crate::ExprSubscript {
        match self {
            Self::Subscript(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_subscript_expr_mut(&mut self) -> Option<&mut crate::ExprSubscript> {
        match self {
            Self::Subscript(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_subscript_expr(&self) -> Option<&crate::ExprSubscript> {
        match self {
            Self::Subscript(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_starred_expr(&self) -> bool {
        matches!(self, Self::Starred(_))
    }

    #[inline]
    pub fn starred_expr(self) -> Option<crate::ExprStarred> {
        match self {
            Self::Starred(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_starred_expr(self) -> crate::ExprStarred {
        match self {
            Self::Starred(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_starred_expr_mut(&mut self) -> Option<&mut crate::ExprStarred> {
        match self {
            Self::Starred(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_starred_expr(&self) -> Option<&crate::ExprStarred> {
        match self {
            Self::Starred(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_name_expr(&self) -> bool {
        matches!(self, Self::Name(_))
    }

    #[inline]
    pub fn name_expr(self) -> Option<crate::ExprName> {
        match self {
            Self::Name(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_name_expr(self) -> crate::ExprName {
        match self {
            Self::Name(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_name_expr_mut(&mut self) -> Option<&mut crate::ExprName> {
        match self {
            Self::Name(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_name_expr(&self) -> Option<&crate::ExprName> {
        match self {
            Self::Name(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_list_expr(&self) -> bool {
        matches!(self, Self::List(_))
    }

    #[inline]
    pub fn list_expr(self) -> Option<crate::ExprList> {
        match self {
            Self::List(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_list_expr(self) -> crate::ExprList {
        match self {
            Self::List(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_list_expr_mut(&mut self) -> Option<&mut crate::ExprList> {
        match self {
            Self::List(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_list_expr(&self) -> Option<&crate::ExprList> {
        match self {
            Self::List(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_tuple_expr(&self) -> bool {
        matches!(self, Self::Tuple(_))
    }

    #[inline]
    pub fn tuple_expr(self) -> Option<crate::ExprTuple> {
        match self {
            Self::Tuple(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_tuple_expr(self) -> crate::ExprTuple {
        match self {
            Self::Tuple(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_tuple_expr_mut(&mut self) -> Option<&mut crate::ExprTuple> {
        match self {
            Self::Tuple(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_tuple_expr(&self) -> Option<&crate::ExprTuple> {
        match self {
            Self::Tuple(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_slice_expr(&self) -> bool {
        matches!(self, Self::Slice(_))
    }

    #[inline]
    pub fn slice_expr(self) -> Option<crate::ExprSlice> {
        match self {
            Self::Slice(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_slice_expr(self) -> crate::ExprSlice {
        match self {
            Self::Slice(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_slice_expr_mut(&mut self) -> Option<&mut crate::ExprSlice> {
        match self {
            Self::Slice(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_slice_expr(&self) -> Option<&crate::ExprSlice> {
        match self {
            Self::Slice(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_ipy_escape_command_expr(&self) -> bool {
        matches!(self, Self::IpyEscapeCommand(_))
    }

    #[inline]
    pub fn ipy_escape_command_expr(self) -> Option<crate::ExprIpyEscapeCommand> {
        match self {
            Self::IpyEscapeCommand(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_ipy_escape_command_expr(self) -> crate::ExprIpyEscapeCommand {
        match self {
            Self::IpyEscapeCommand(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_ipy_escape_command_expr_mut(&mut self) -> Option<&mut crate::ExprIpyEscapeCommand> {
        match self {
            Self::IpyEscapeCommand(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_ipy_escape_command_expr(&self) -> Option<&crate::ExprIpyEscapeCommand> {
        match self {
            Self::IpyEscapeCommand(val) => Some(val),
            _ => None,
        }
    }
}

/// See also [excepthandler](https://docs.python.org/3/library/ast.html#ast.excepthandler)
#[derive(Clone, Debug, PartialEq)]
pub enum ExceptHandler {
    ExceptHandler(crate::ExceptHandlerExceptHandler),
}

impl From<crate::ExceptHandlerExceptHandler> for ExceptHandler {
    fn from(node: crate::ExceptHandlerExceptHandler) -> Self {
        Self::ExceptHandler(node)
    }
}

impl ruff_text_size::Ranged for ExceptHandler {
    fn range(&self) -> ruff_text_size::TextRange {
        match self {
            Self::ExceptHandler(node) => node.range(),
        }
    }
}

#[allow(dead_code, clippy::match_wildcard_for_single_variants)]
impl ExceptHandler {
    #[inline]
    pub const fn is_except_handler(&self) -> bool {
        matches!(self, Self::ExceptHandler(_))
    }

    #[inline]
    pub fn except_handler(self) -> Option<crate::ExceptHandlerExceptHandler> {
        match self {
            Self::ExceptHandler(val) => Some(val),
        }
    }

    #[inline]
    pub fn expect_except_handler(self) -> crate::ExceptHandlerExceptHandler {
        match self {
            Self::ExceptHandler(val) => val,
        }
    }

    #[inline]
    pub fn as_except_handler_mut(&mut self) -> Option<&mut crate::ExceptHandlerExceptHandler> {
        match self {
            Self::ExceptHandler(val) => Some(val),
        }
    }

    #[inline]
    pub fn as_except_handler(&self) -> Option<&crate::ExceptHandlerExceptHandler> {
        match self {
            Self::ExceptHandler(val) => Some(val),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum FStringElement {
    Expression(crate::FStringExpressionElement),
    Literal(crate::FStringLiteralElement),
}

impl From<crate::FStringExpressionElement> for FStringElement {
    fn from(node: crate::FStringExpressionElement) -> Self {
        Self::Expression(node)
    }
}

impl From<crate::FStringLiteralElement> for FStringElement {
    fn from(node: crate::FStringLiteralElement) -> Self {
        Self::Literal(node)
    }
}

impl ruff_text_size::Ranged for FStringElement {
    fn range(&self) -> ruff_text_size::TextRange {
        match self {
            Self::Expression(node) => node.range(),
            Self::Literal(node) => node.range(),
        }
    }
}

#[allow(dead_code, clippy::match_wildcard_for_single_variants)]
impl FStringElement {
    #[inline]
    pub const fn is_expression(&self) -> bool {
        matches!(self, Self::Expression(_))
    }

    #[inline]
    pub fn expression(self) -> Option<crate::FStringExpressionElement> {
        match self {
            Self::Expression(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_expression(self) -> crate::FStringExpressionElement {
        match self {
            Self::Expression(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_expression_mut(&mut self) -> Option<&mut crate::FStringExpressionElement> {
        match self {
            Self::Expression(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_expression(&self) -> Option<&crate::FStringExpressionElement> {
        match self {
            Self::Expression(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_literal(&self) -> bool {
        matches!(self, Self::Literal(_))
    }

    #[inline]
    pub fn literal(self) -> Option<crate::FStringLiteralElement> {
        match self {
            Self::Literal(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_literal(self) -> crate::FStringLiteralElement {
        match self {
            Self::Literal(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_literal_mut(&mut self) -> Option<&mut crate::FStringLiteralElement> {
        match self {
            Self::Literal(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_literal(&self) -> Option<&crate::FStringLiteralElement> {
        match self {
            Self::Literal(val) => Some(val),
            _ => None,
        }
    }
}

/// See also [pattern](https://docs.python.org/3/library/ast.html#ast.pattern)
#[derive(Clone, Debug, PartialEq)]
pub enum Pattern {
    MatchValue(crate::PatternMatchValue),
    MatchSingleton(crate::PatternMatchSingleton),
    MatchSequence(crate::PatternMatchSequence),
    MatchMapping(crate::PatternMatchMapping),
    MatchClass(crate::PatternMatchClass),
    MatchStar(crate::PatternMatchStar),
    MatchAs(crate::PatternMatchAs),
    MatchOr(crate::PatternMatchOr),
}

impl From<crate::PatternMatchValue> for Pattern {
    fn from(node: crate::PatternMatchValue) -> Self {
        Self::MatchValue(node)
    }
}

impl From<crate::PatternMatchSingleton> for Pattern {
    fn from(node: crate::PatternMatchSingleton) -> Self {
        Self::MatchSingleton(node)
    }
}

impl From<crate::PatternMatchSequence> for Pattern {
    fn from(node: crate::PatternMatchSequence) -> Self {
        Self::MatchSequence(node)
    }
}

impl From<crate::PatternMatchMapping> for Pattern {
    fn from(node: crate::PatternMatchMapping) -> Self {
        Self::MatchMapping(node)
    }
}

impl From<crate::PatternMatchClass> for Pattern {
    fn from(node: crate::PatternMatchClass) -> Self {
        Self::MatchClass(node)
    }
}

impl From<crate::PatternMatchStar> for Pattern {
    fn from(node: crate::PatternMatchStar) -> Self {
        Self::MatchStar(node)
    }
}

impl From<crate::PatternMatchAs> for Pattern {
    fn from(node: crate::PatternMatchAs) -> Self {
        Self::MatchAs(node)
    }
}

impl From<crate::PatternMatchOr> for Pattern {
    fn from(node: crate::PatternMatchOr) -> Self {
        Self::MatchOr(node)
    }
}

impl ruff_text_size::Ranged for Pattern {
    fn range(&self) -> ruff_text_size::TextRange {
        match self {
            Self::MatchValue(node) => node.range(),
            Self::MatchSingleton(node) => node.range(),
            Self::MatchSequence(node) => node.range(),
            Self::MatchMapping(node) => node.range(),
            Self::MatchClass(node) => node.range(),
            Self::MatchStar(node) => node.range(),
            Self::MatchAs(node) => node.range(),
            Self::MatchOr(node) => node.range(),
        }
    }
}

#[allow(dead_code, clippy::match_wildcard_for_single_variants)]
impl Pattern {
    #[inline]
    pub const fn is_match_value(&self) -> bool {
        matches!(self, Self::MatchValue(_))
    }

    #[inline]
    pub fn match_value(self) -> Option<crate::PatternMatchValue> {
        match self {
            Self::MatchValue(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_match_value(self) -> crate::PatternMatchValue {
        match self {
            Self::MatchValue(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_match_value_mut(&mut self) -> Option<&mut crate::PatternMatchValue> {
        match self {
            Self::MatchValue(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_match_value(&self) -> Option<&crate::PatternMatchValue> {
        match self {
            Self::MatchValue(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_match_singleton(&self) -> bool {
        matches!(self, Self::MatchSingleton(_))
    }

    #[inline]
    pub fn match_singleton(self) -> Option<crate::PatternMatchSingleton> {
        match self {
            Self::MatchSingleton(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_match_singleton(self) -> crate::PatternMatchSingleton {
        match self {
            Self::MatchSingleton(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_match_singleton_mut(&mut self) -> Option<&mut crate::PatternMatchSingleton> {
        match self {
            Self::MatchSingleton(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_match_singleton(&self) -> Option<&crate::PatternMatchSingleton> {
        match self {
            Self::MatchSingleton(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_match_sequence(&self) -> bool {
        matches!(self, Self::MatchSequence(_))
    }

    #[inline]
    pub fn match_sequence(self) -> Option<crate::PatternMatchSequence> {
        match self {
            Self::MatchSequence(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_match_sequence(self) -> crate::PatternMatchSequence {
        match self {
            Self::MatchSequence(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_match_sequence_mut(&mut self) -> Option<&mut crate::PatternMatchSequence> {
        match self {
            Self::MatchSequence(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_match_sequence(&self) -> Option<&crate::PatternMatchSequence> {
        match self {
            Self::MatchSequence(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_match_mapping(&self) -> bool {
        matches!(self, Self::MatchMapping(_))
    }

    #[inline]
    pub fn match_mapping(self) -> Option<crate::PatternMatchMapping> {
        match self {
            Self::MatchMapping(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_match_mapping(self) -> crate::PatternMatchMapping {
        match self {
            Self::MatchMapping(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_match_mapping_mut(&mut self) -> Option<&mut crate::PatternMatchMapping> {
        match self {
            Self::MatchMapping(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_match_mapping(&self) -> Option<&crate::PatternMatchMapping> {
        match self {
            Self::MatchMapping(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_match_class(&self) -> bool {
        matches!(self, Self::MatchClass(_))
    }

    #[inline]
    pub fn match_class(self) -> Option<crate::PatternMatchClass> {
        match self {
            Self::MatchClass(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_match_class(self) -> crate::PatternMatchClass {
        match self {
            Self::MatchClass(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_match_class_mut(&mut self) -> Option<&mut crate::PatternMatchClass> {
        match self {
            Self::MatchClass(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_match_class(&self) -> Option<&crate::PatternMatchClass> {
        match self {
            Self::MatchClass(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_match_star(&self) -> bool {
        matches!(self, Self::MatchStar(_))
    }

    #[inline]
    pub fn match_star(self) -> Option<crate::PatternMatchStar> {
        match self {
            Self::MatchStar(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_match_star(self) -> crate::PatternMatchStar {
        match self {
            Self::MatchStar(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_match_star_mut(&mut self) -> Option<&mut crate::PatternMatchStar> {
        match self {
            Self::MatchStar(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_match_star(&self) -> Option<&crate::PatternMatchStar> {
        match self {
            Self::MatchStar(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_match_as(&self) -> bool {
        matches!(self, Self::MatchAs(_))
    }

    #[inline]
    pub fn match_as(self) -> Option<crate::PatternMatchAs> {
        match self {
            Self::MatchAs(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_match_as(self) -> crate::PatternMatchAs {
        match self {
            Self::MatchAs(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_match_as_mut(&mut self) -> Option<&mut crate::PatternMatchAs> {
        match self {
            Self::MatchAs(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_match_as(&self) -> Option<&crate::PatternMatchAs> {
        match self {
            Self::MatchAs(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_match_or(&self) -> bool {
        matches!(self, Self::MatchOr(_))
    }

    #[inline]
    pub fn match_or(self) -> Option<crate::PatternMatchOr> {
        match self {
            Self::MatchOr(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_match_or(self) -> crate::PatternMatchOr {
        match self {
            Self::MatchOr(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_match_or_mut(&mut self) -> Option<&mut crate::PatternMatchOr> {
        match self {
            Self::MatchOr(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_match_or(&self) -> Option<&crate::PatternMatchOr> {
        match self {
            Self::MatchOr(val) => Some(val),
            _ => None,
        }
    }
}

/// See also [type_param](https://docs.python.org/3/library/ast.html#ast.type_param)
#[derive(Clone, Debug, PartialEq)]
pub enum TypeParam {
    TypeVar(crate::TypeParamTypeVar),
    TypeVarTuple(crate::TypeParamTypeVarTuple),
    ParamSpec(crate::TypeParamParamSpec),
}

impl From<crate::TypeParamTypeVar> for TypeParam {
    fn from(node: crate::TypeParamTypeVar) -> Self {
        Self::TypeVar(node)
    }
}

impl From<crate::TypeParamTypeVarTuple> for TypeParam {
    fn from(node: crate::TypeParamTypeVarTuple) -> Self {
        Self::TypeVarTuple(node)
    }
}

impl From<crate::TypeParamParamSpec> for TypeParam {
    fn from(node: crate::TypeParamParamSpec) -> Self {
        Self::ParamSpec(node)
    }
}

impl ruff_text_size::Ranged for TypeParam {
    fn range(&self) -> ruff_text_size::TextRange {
        match self {
            Self::TypeVar(node) => node.range(),
            Self::TypeVarTuple(node) => node.range(),
            Self::ParamSpec(node) => node.range(),
        }
    }
}

#[allow(dead_code, clippy::match_wildcard_for_single_variants)]
impl TypeParam {
    #[inline]
    pub const fn is_type_var(&self) -> bool {
        matches!(self, Self::TypeVar(_))
    }

    #[inline]
    pub fn type_var(self) -> Option<crate::TypeParamTypeVar> {
        match self {
            Self::TypeVar(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_type_var(self) -> crate::TypeParamTypeVar {
        match self {
            Self::TypeVar(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_type_var_mut(&mut self) -> Option<&mut crate::TypeParamTypeVar> {
        match self {
            Self::TypeVar(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_type_var(&self) -> Option<&crate::TypeParamTypeVar> {
        match self {
            Self::TypeVar(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_type_var_tuple(&self) -> bool {
        matches!(self, Self::TypeVarTuple(_))
    }

    #[inline]
    pub fn type_var_tuple(self) -> Option<crate::TypeParamTypeVarTuple> {
        match self {
            Self::TypeVarTuple(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_type_var_tuple(self) -> crate::TypeParamTypeVarTuple {
        match self {
            Self::TypeVarTuple(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_type_var_tuple_mut(&mut self) -> Option<&mut crate::TypeParamTypeVarTuple> {
        match self {
            Self::TypeVarTuple(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_type_var_tuple(&self) -> Option<&crate::TypeParamTypeVarTuple> {
        match self {
            Self::TypeVarTuple(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_param_spec(&self) -> bool {
        matches!(self, Self::ParamSpec(_))
    }

    #[inline]
    pub fn param_spec(self) -> Option<crate::TypeParamParamSpec> {
        match self {
            Self::ParamSpec(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_param_spec(self) -> crate::TypeParamParamSpec {
        match self {
            Self::ParamSpec(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_param_spec_mut(&mut self) -> Option<&mut crate::TypeParamParamSpec> {
        match self {
            Self::ParamSpec(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_param_spec(&self) -> Option<&crate::TypeParamParamSpec> {
        match self {
            Self::ParamSpec(val) => Some(val),
            _ => None,
        }
    }
}

impl ruff_text_size::Ranged for crate::ModModule {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ModExpression {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtFunctionDef {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtClassDef {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtReturn {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtDelete {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtTypeAlias {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtAssign {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtAugAssign {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtAnnAssign {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtFor {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtWhile {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtIf {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtWith {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtMatch {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtRaise {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtTry {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtAssert {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtImport {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtImportFrom {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtGlobal {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtNonlocal {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtExpr {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtPass {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtBreak {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtContinue {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtIpyEscapeCommand {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprBoolOp {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprNamed {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprBinOp {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprUnaryOp {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprLambda {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprIf {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprDict {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprSet {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprListComp {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprSetComp {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprDictComp {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprGenerator {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprAwait {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprYield {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprYieldFrom {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprCompare {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprCall {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprFString {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprStringLiteral {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprBytesLiteral {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprNumberLiteral {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprBooleanLiteral {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprNoneLiteral {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprEllipsisLiteral {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprAttribute {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprSubscript {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprStarred {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprName {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprList {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprTuple {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprSlice {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprIpyEscapeCommand {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExceptHandlerExceptHandler {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::FStringExpressionElement {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::FStringLiteralElement {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::PatternMatchValue {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::PatternMatchSingleton {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::PatternMatchSequence {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::PatternMatchMapping {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::PatternMatchClass {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::PatternMatchStar {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::PatternMatchAs {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::PatternMatchOr {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::TypeParamTypeVar {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::TypeParamTypeVarTuple {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::TypeParamParamSpec {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::FStringFormatSpec {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::PatternArguments {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::PatternKeyword {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::Comprehension {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::Arguments {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::Parameters {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::Parameter {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ParameterWithDefault {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::Keyword {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::Alias {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::WithItem {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::MatchCase {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::Decorator {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ElifElseClause {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::TypeParams {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::FString {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StringLiteral {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::BytesLiteral {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::Identifier {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl Mod {
    #[allow(unused)]
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: crate::visitor::source_order::SourceOrderVisitor<'a> + ?Sized,
    {
        match self {
            Mod::Module(node) => node.visit_source_order(visitor),
            Mod::Expression(node) => node.visit_source_order(visitor),
        }
    }
}

impl Stmt {
    #[allow(unused)]
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: crate::visitor::source_order::SourceOrderVisitor<'a> + ?Sized,
    {
        match self {
            Stmt::FunctionDef(node) => node.visit_source_order(visitor),
            Stmt::ClassDef(node) => node.visit_source_order(visitor),
            Stmt::Return(node) => node.visit_source_order(visitor),
            Stmt::Delete(node) => node.visit_source_order(visitor),
            Stmt::TypeAlias(node) => node.visit_source_order(visitor),
            Stmt::Assign(node) => node.visit_source_order(visitor),
            Stmt::AugAssign(node) => node.visit_source_order(visitor),
            Stmt::AnnAssign(node) => node.visit_source_order(visitor),
            Stmt::For(node) => node.visit_source_order(visitor),
            Stmt::While(node) => node.visit_source_order(visitor),
            Stmt::If(node) => node.visit_source_order(visitor),
            Stmt::With(node) => node.visit_source_order(visitor),
            Stmt::Match(node) => node.visit_source_order(visitor),
            Stmt::Raise(node) => node.visit_source_order(visitor),
            Stmt::Try(node) => node.visit_source_order(visitor),
            Stmt::Assert(node) => node.visit_source_order(visitor),
            Stmt::Import(node) => node.visit_source_order(visitor),
            Stmt::ImportFrom(node) => node.visit_source_order(visitor),
            Stmt::Global(node) => node.visit_source_order(visitor),
            Stmt::Nonlocal(node) => node.visit_source_order(visitor),
            Stmt::Expr(node) => node.visit_source_order(visitor),
            Stmt::Pass(node) => node.visit_source_order(visitor),
            Stmt::Break(node) => node.visit_source_order(visitor),
            Stmt::Continue(node) => node.visit_source_order(visitor),
            Stmt::IpyEscapeCommand(node) => node.visit_source_order(visitor),
        }
    }
}

impl Expr {
    #[allow(unused)]
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: crate::visitor::source_order::SourceOrderVisitor<'a> + ?Sized,
    {
        match self {
            Expr::BoolOp(node) => node.visit_source_order(visitor),
            Expr::Named(node) => node.visit_source_order(visitor),
            Expr::BinOp(node) => node.visit_source_order(visitor),
            Expr::UnaryOp(node) => node.visit_source_order(visitor),
            Expr::Lambda(node) => node.visit_source_order(visitor),
            Expr::If(node) => node.visit_source_order(visitor),
            Expr::Dict(node) => node.visit_source_order(visitor),
            Expr::Set(node) => node.visit_source_order(visitor),
            Expr::ListComp(node) => node.visit_source_order(visitor),
            Expr::SetComp(node) => node.visit_source_order(visitor),
            Expr::DictComp(node) => node.visit_source_order(visitor),
            Expr::Generator(node) => node.visit_source_order(visitor),
            Expr::Await(node) => node.visit_source_order(visitor),
            Expr::Yield(node) => node.visit_source_order(visitor),
            Expr::YieldFrom(node) => node.visit_source_order(visitor),
            Expr::Compare(node) => node.visit_source_order(visitor),
            Expr::Call(node) => node.visit_source_order(visitor),
            Expr::FString(node) => node.visit_source_order(visitor),
            Expr::StringLiteral(node) => node.visit_source_order(visitor),
            Expr::BytesLiteral(node) => node.visit_source_order(visitor),
            Expr::NumberLiteral(node) => node.visit_source_order(visitor),
            Expr::BooleanLiteral(node) => node.visit_source_order(visitor),
            Expr::NoneLiteral(node) => node.visit_source_order(visitor),
            Expr::EllipsisLiteral(node) => node.visit_source_order(visitor),
            Expr::Attribute(node) => node.visit_source_order(visitor),
            Expr::Subscript(node) => node.visit_source_order(visitor),
            Expr::Starred(node) => node.visit_source_order(visitor),
            Expr::Name(node) => node.visit_source_order(visitor),
            Expr::List(node) => node.visit_source_order(visitor),
            Expr::Tuple(node) => node.visit_source_order(visitor),
            Expr::Slice(node) => node.visit_source_order(visitor),
            Expr::IpyEscapeCommand(node) => node.visit_source_order(visitor),
        }
    }
}

impl ExceptHandler {
    #[allow(unused)]
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: crate::visitor::source_order::SourceOrderVisitor<'a> + ?Sized,
    {
        match self {
            ExceptHandler::ExceptHandler(node) => node.visit_source_order(visitor),
        }
    }
}

impl FStringElement {
    #[allow(unused)]
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: crate::visitor::source_order::SourceOrderVisitor<'a> + ?Sized,
    {
        match self {
            FStringElement::Expression(node) => node.visit_source_order(visitor),
            FStringElement::Literal(node) => node.visit_source_order(visitor),
        }
    }
}

impl Pattern {
    #[allow(unused)]
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: crate::visitor::source_order::SourceOrderVisitor<'a> + ?Sized,
    {
        match self {
            Pattern::MatchValue(node) => node.visit_source_order(visitor),
            Pattern::MatchSingleton(node) => node.visit_source_order(visitor),
            Pattern::MatchSequence(node) => node.visit_source_order(visitor),
            Pattern::MatchMapping(node) => node.visit_source_order(visitor),
            Pattern::MatchClass(node) => node.visit_source_order(visitor),
            Pattern::MatchStar(node) => node.visit_source_order(visitor),
            Pattern::MatchAs(node) => node.visit_source_order(visitor),
            Pattern::MatchOr(node) => node.visit_source_order(visitor),
        }
    }
}

impl TypeParam {
    #[allow(unused)]
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: crate::visitor::source_order::SourceOrderVisitor<'a> + ?Sized,
    {
        match self {
            TypeParam::TypeVar(node) => node.visit_source_order(visitor),
            TypeParam::TypeVarTuple(node) => node.visit_source_order(visitor),
            TypeParam::ParamSpec(node) => node.visit_source_order(visitor),
        }
    }
}

/// See also [mod](https://docs.python.org/3/library/ast.html#ast.mod)
#[derive(Clone, Copy, Debug, PartialEq, is_macro::Is)]
pub enum ModRef<'a> {
    Module(&'a crate::ModModule),
    Expression(&'a crate::ModExpression),
}

impl<'a> From<&'a Mod> for ModRef<'a> {
    fn from(node: &'a Mod) -> Self {
        match node {
            Mod::Module(node) => ModRef::Module(node),
            Mod::Expression(node) => ModRef::Expression(node),
        }
    }
}

impl<'a> From<&'a crate::ModModule> for ModRef<'a> {
    fn from(node: &'a crate::ModModule) -> Self {
        Self::Module(node)
    }
}

impl<'a> From<&'a crate::ModExpression> for ModRef<'a> {
    fn from(node: &'a crate::ModExpression) -> Self {
        Self::Expression(node)
    }
}

impl ruff_text_size::Ranged for ModRef<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        match self {
            Self::Module(node) => node.range(),
            Self::Expression(node) => node.range(),
        }
    }
}

/// See also [stmt](https://docs.python.org/3/library/ast.html#ast.stmt)
#[derive(Clone, Copy, Debug, PartialEq, is_macro::Is)]
pub enum StmtRef<'a> {
    #[is(name = "function_def_stmt")]
    FunctionDef(&'a crate::StmtFunctionDef),
    #[is(name = "class_def_stmt")]
    ClassDef(&'a crate::StmtClassDef),
    #[is(name = "return_stmt")]
    Return(&'a crate::StmtReturn),
    #[is(name = "delete_stmt")]
    Delete(&'a crate::StmtDelete),
    #[is(name = "type_alias_stmt")]
    TypeAlias(&'a crate::StmtTypeAlias),
    #[is(name = "assign_stmt")]
    Assign(&'a crate::StmtAssign),
    #[is(name = "aug_assign_stmt")]
    AugAssign(&'a crate::StmtAugAssign),
    #[is(name = "ann_assign_stmt")]
    AnnAssign(&'a crate::StmtAnnAssign),
    #[is(name = "for_stmt")]
    For(&'a crate::StmtFor),
    #[is(name = "while_stmt")]
    While(&'a crate::StmtWhile),
    #[is(name = "if_stmt")]
    If(&'a crate::StmtIf),
    #[is(name = "with_stmt")]
    With(&'a crate::StmtWith),
    #[is(name = "match_stmt")]
    Match(&'a crate::StmtMatch),
    #[is(name = "raise_stmt")]
    Raise(&'a crate::StmtRaise),
    #[is(name = "try_stmt")]
    Try(&'a crate::StmtTry),
    #[is(name = "assert_stmt")]
    Assert(&'a crate::StmtAssert),
    #[is(name = "import_stmt")]
    Import(&'a crate::StmtImport),
    #[is(name = "import_from_stmt")]
    ImportFrom(&'a crate::StmtImportFrom),
    #[is(name = "global_stmt")]
    Global(&'a crate::StmtGlobal),
    #[is(name = "nonlocal_stmt")]
    Nonlocal(&'a crate::StmtNonlocal),
    #[is(name = "expr_stmt")]
    Expr(&'a crate::StmtExpr),
    #[is(name = "pass_stmt")]
    Pass(&'a crate::StmtPass),
    #[is(name = "break_stmt")]
    Break(&'a crate::StmtBreak),
    #[is(name = "continue_stmt")]
    Continue(&'a crate::StmtContinue),
    #[is(name = "ipy_escape_command_stmt")]
    IpyEscapeCommand(&'a crate::StmtIpyEscapeCommand),
}

impl<'a> From<&'a Stmt> for StmtRef<'a> {
    fn from(node: &'a Stmt) -> Self {
        match node {
            Stmt::FunctionDef(node) => StmtRef::FunctionDef(node),
            Stmt::ClassDef(node) => StmtRef::ClassDef(node),
            Stmt::Return(node) => StmtRef::Return(node),
            Stmt::Delete(node) => StmtRef::Delete(node),
            Stmt::TypeAlias(node) => StmtRef::TypeAlias(node),
            Stmt::Assign(node) => StmtRef::Assign(node),
            Stmt::AugAssign(node) => StmtRef::AugAssign(node),
            Stmt::AnnAssign(node) => StmtRef::AnnAssign(node),
            Stmt::For(node) => StmtRef::For(node),
            Stmt::While(node) => StmtRef::While(node),
            Stmt::If(node) => StmtRef::If(node),
            Stmt::With(node) => StmtRef::With(node),
            Stmt::Match(node) => StmtRef::Match(node),
            Stmt::Raise(node) => StmtRef::Raise(node),
            Stmt::Try(node) => StmtRef::Try(node),
            Stmt::Assert(node) => StmtRef::Assert(node),
            Stmt::Import(node) => StmtRef::Import(node),
            Stmt::ImportFrom(node) => StmtRef::ImportFrom(node),
            Stmt::Global(node) => StmtRef::Global(node),
            Stmt::Nonlocal(node) => StmtRef::Nonlocal(node),
            Stmt::Expr(node) => StmtRef::Expr(node),
            Stmt::Pass(node) => StmtRef::Pass(node),
            Stmt::Break(node) => StmtRef::Break(node),
            Stmt::Continue(node) => StmtRef::Continue(node),
            Stmt::IpyEscapeCommand(node) => StmtRef::IpyEscapeCommand(node),
        }
    }
}

impl<'a> From<&'a crate::StmtFunctionDef> for StmtRef<'a> {
    fn from(node: &'a crate::StmtFunctionDef) -> Self {
        Self::FunctionDef(node)
    }
}

impl<'a> From<&'a crate::StmtClassDef> for StmtRef<'a> {
    fn from(node: &'a crate::StmtClassDef) -> Self {
        Self::ClassDef(node)
    }
}

impl<'a> From<&'a crate::StmtReturn> for StmtRef<'a> {
    fn from(node: &'a crate::StmtReturn) -> Self {
        Self::Return(node)
    }
}

impl<'a> From<&'a crate::StmtDelete> for StmtRef<'a> {
    fn from(node: &'a crate::StmtDelete) -> Self {
        Self::Delete(node)
    }
}

impl<'a> From<&'a crate::StmtTypeAlias> for StmtRef<'a> {
    fn from(node: &'a crate::StmtTypeAlias) -> Self {
        Self::TypeAlias(node)
    }
}

impl<'a> From<&'a crate::StmtAssign> for StmtRef<'a> {
    fn from(node: &'a crate::StmtAssign) -> Self {
        Self::Assign(node)
    }
}

impl<'a> From<&'a crate::StmtAugAssign> for StmtRef<'a> {
    fn from(node: &'a crate::StmtAugAssign) -> Self {
        Self::AugAssign(node)
    }
}

impl<'a> From<&'a crate::StmtAnnAssign> for StmtRef<'a> {
    fn from(node: &'a crate::StmtAnnAssign) -> Self {
        Self::AnnAssign(node)
    }
}

impl<'a> From<&'a crate::StmtFor> for StmtRef<'a> {
    fn from(node: &'a crate::StmtFor) -> Self {
        Self::For(node)
    }
}

impl<'a> From<&'a crate::StmtWhile> for StmtRef<'a> {
    fn from(node: &'a crate::StmtWhile) -> Self {
        Self::While(node)
    }
}

impl<'a> From<&'a crate::StmtIf> for StmtRef<'a> {
    fn from(node: &'a crate::StmtIf) -> Self {
        Self::If(node)
    }
}

impl<'a> From<&'a crate::StmtWith> for StmtRef<'a> {
    fn from(node: &'a crate::StmtWith) -> Self {
        Self::With(node)
    }
}

impl<'a> From<&'a crate::StmtMatch> for StmtRef<'a> {
    fn from(node: &'a crate::StmtMatch) -> Self {
        Self::Match(node)
    }
}

impl<'a> From<&'a crate::StmtRaise> for StmtRef<'a> {
    fn from(node: &'a crate::StmtRaise) -> Self {
        Self::Raise(node)
    }
}

impl<'a> From<&'a crate::StmtTry> for StmtRef<'a> {
    fn from(node: &'a crate::StmtTry) -> Self {
        Self::Try(node)
    }
}

impl<'a> From<&'a crate::StmtAssert> for StmtRef<'a> {
    fn from(node: &'a crate::StmtAssert) -> Self {
        Self::Assert(node)
    }
}

impl<'a> From<&'a crate::StmtImport> for StmtRef<'a> {
    fn from(node: &'a crate::StmtImport) -> Self {
        Self::Import(node)
    }
}

impl<'a> From<&'a crate::StmtImportFrom> for StmtRef<'a> {
    fn from(node: &'a crate::StmtImportFrom) -> Self {
        Self::ImportFrom(node)
    }
}

impl<'a> From<&'a crate::StmtGlobal> for StmtRef<'a> {
    fn from(node: &'a crate::StmtGlobal) -> Self {
        Self::Global(node)
    }
}

impl<'a> From<&'a crate::StmtNonlocal> for StmtRef<'a> {
    fn from(node: &'a crate::StmtNonlocal) -> Self {
        Self::Nonlocal(node)
    }
}

impl<'a> From<&'a crate::StmtExpr> for StmtRef<'a> {
    fn from(node: &'a crate::StmtExpr) -> Self {
        Self::Expr(node)
    }
}

impl<'a> From<&'a crate::StmtPass> for StmtRef<'a> {
    fn from(node: &'a crate::StmtPass) -> Self {
        Self::Pass(node)
    }
}

impl<'a> From<&'a crate::StmtBreak> for StmtRef<'a> {
    fn from(node: &'a crate::StmtBreak) -> Self {
        Self::Break(node)
    }
}

impl<'a> From<&'a crate::StmtContinue> for StmtRef<'a> {
    fn from(node: &'a crate::StmtContinue) -> Self {
        Self::Continue(node)
    }
}

impl<'a> From<&'a crate::StmtIpyEscapeCommand> for StmtRef<'a> {
    fn from(node: &'a crate::StmtIpyEscapeCommand) -> Self {
        Self::IpyEscapeCommand(node)
    }
}

impl ruff_text_size::Ranged for StmtRef<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        match self {
            Self::FunctionDef(node) => node.range(),
            Self::ClassDef(node) => node.range(),
            Self::Return(node) => node.range(),
            Self::Delete(node) => node.range(),
            Self::TypeAlias(node) => node.range(),
            Self::Assign(node) => node.range(),
            Self::AugAssign(node) => node.range(),
            Self::AnnAssign(node) => node.range(),
            Self::For(node) => node.range(),
            Self::While(node) => node.range(),
            Self::If(node) => node.range(),
            Self::With(node) => node.range(),
            Self::Match(node) => node.range(),
            Self::Raise(node) => node.range(),
            Self::Try(node) => node.range(),
            Self::Assert(node) => node.range(),
            Self::Import(node) => node.range(),
            Self::ImportFrom(node) => node.range(),
            Self::Global(node) => node.range(),
            Self::Nonlocal(node) => node.range(),
            Self::Expr(node) => node.range(),
            Self::Pass(node) => node.range(),
            Self::Break(node) => node.range(),
            Self::Continue(node) => node.range(),
            Self::IpyEscapeCommand(node) => node.range(),
        }
    }
}

/// See also [expr](https://docs.python.org/3/library/ast.html#ast.expr)
#[derive(Clone, Copy, Debug, PartialEq, is_macro::Is)]
pub enum ExprRef<'a> {
    #[is(name = "bool_op_expr")]
    BoolOp(&'a crate::ExprBoolOp),
    #[is(name = "named_expr")]
    Named(&'a crate::ExprNamed),
    #[is(name = "bin_op_expr")]
    BinOp(&'a crate::ExprBinOp),
    #[is(name = "unary_op_expr")]
    UnaryOp(&'a crate::ExprUnaryOp),
    #[is(name = "lambda_expr")]
    Lambda(&'a crate::ExprLambda),
    #[is(name = "if_expr")]
    If(&'a crate::ExprIf),
    #[is(name = "dict_expr")]
    Dict(&'a crate::ExprDict),
    #[is(name = "set_expr")]
    Set(&'a crate::ExprSet),
    #[is(name = "list_comp_expr")]
    ListComp(&'a crate::ExprListComp),
    #[is(name = "set_comp_expr")]
    SetComp(&'a crate::ExprSetComp),
    #[is(name = "dict_comp_expr")]
    DictComp(&'a crate::ExprDictComp),
    #[is(name = "generator_expr")]
    Generator(&'a crate::ExprGenerator),
    #[is(name = "await_expr")]
    Await(&'a crate::ExprAwait),
    #[is(name = "yield_expr")]
    Yield(&'a crate::ExprYield),
    #[is(name = "yield_from_expr")]
    YieldFrom(&'a crate::ExprYieldFrom),
    #[is(name = "compare_expr")]
    Compare(&'a crate::ExprCompare),
    #[is(name = "call_expr")]
    Call(&'a crate::ExprCall),
    #[is(name = "f_string_expr")]
    FString(&'a crate::ExprFString),
    #[is(name = "string_literal_expr")]
    StringLiteral(&'a crate::ExprStringLiteral),
    #[is(name = "bytes_literal_expr")]
    BytesLiteral(&'a crate::ExprBytesLiteral),
    #[is(name = "number_literal_expr")]
    NumberLiteral(&'a crate::ExprNumberLiteral),
    #[is(name = "boolean_literal_expr")]
    BooleanLiteral(&'a crate::ExprBooleanLiteral),
    #[is(name = "none_literal_expr")]
    NoneLiteral(&'a crate::ExprNoneLiteral),
    #[is(name = "ellipsis_literal_expr")]
    EllipsisLiteral(&'a crate::ExprEllipsisLiteral),
    #[is(name = "attribute_expr")]
    Attribute(&'a crate::ExprAttribute),
    #[is(name = "subscript_expr")]
    Subscript(&'a crate::ExprSubscript),
    #[is(name = "starred_expr")]
    Starred(&'a crate::ExprStarred),
    #[is(name = "name_expr")]
    Name(&'a crate::ExprName),
    #[is(name = "list_expr")]
    List(&'a crate::ExprList),
    #[is(name = "tuple_expr")]
    Tuple(&'a crate::ExprTuple),
    #[is(name = "slice_expr")]
    Slice(&'a crate::ExprSlice),
    #[is(name = "ipy_escape_command_expr")]
    IpyEscapeCommand(&'a crate::ExprIpyEscapeCommand),
}

impl<'a> From<&'a Expr> for ExprRef<'a> {
    fn from(node: &'a Expr) -> Self {
        match node {
            Expr::BoolOp(node) => ExprRef::BoolOp(node),
            Expr::Named(node) => ExprRef::Named(node),
            Expr::BinOp(node) => ExprRef::BinOp(node),
            Expr::UnaryOp(node) => ExprRef::UnaryOp(node),
            Expr::Lambda(node) => ExprRef::Lambda(node),
            Expr::If(node) => ExprRef::If(node),
            Expr::Dict(node) => ExprRef::Dict(node),
            Expr::Set(node) => ExprRef::Set(node),
            Expr::ListComp(node) => ExprRef::ListComp(node),
            Expr::SetComp(node) => ExprRef::SetComp(node),
            Expr::DictComp(node) => ExprRef::DictComp(node),
            Expr::Generator(node) => ExprRef::Generator(node),
            Expr::Await(node) => ExprRef::Await(node),
            Expr::Yield(node) => ExprRef::Yield(node),
            Expr::YieldFrom(node) => ExprRef::YieldFrom(node),
            Expr::Compare(node) => ExprRef::Compare(node),
            Expr::Call(node) => ExprRef::Call(node),
            Expr::FString(node) => ExprRef::FString(node),
            Expr::StringLiteral(node) => ExprRef::StringLiteral(node),
            Expr::BytesLiteral(node) => ExprRef::BytesLiteral(node),
            Expr::NumberLiteral(node) => ExprRef::NumberLiteral(node),
            Expr::BooleanLiteral(node) => ExprRef::BooleanLiteral(node),
            Expr::NoneLiteral(node) => ExprRef::NoneLiteral(node),
            Expr::EllipsisLiteral(node) => ExprRef::EllipsisLiteral(node),
            Expr::Attribute(node) => ExprRef::Attribute(node),
            Expr::Subscript(node) => ExprRef::Subscript(node),
            Expr::Starred(node) => ExprRef::Starred(node),
            Expr::Name(node) => ExprRef::Name(node),
            Expr::List(node) => ExprRef::List(node),
            Expr::Tuple(node) => ExprRef::Tuple(node),
            Expr::Slice(node) => ExprRef::Slice(node),
            Expr::IpyEscapeCommand(node) => ExprRef::IpyEscapeCommand(node),
        }
    }
}

impl<'a> From<&'a crate::ExprBoolOp> for ExprRef<'a> {
    fn from(node: &'a crate::ExprBoolOp) -> Self {
        Self::BoolOp(node)
    }
}

impl<'a> From<&'a crate::ExprNamed> for ExprRef<'a> {
    fn from(node: &'a crate::ExprNamed) -> Self {
        Self::Named(node)
    }
}

impl<'a> From<&'a crate::ExprBinOp> for ExprRef<'a> {
    fn from(node: &'a crate::ExprBinOp) -> Self {
        Self::BinOp(node)
    }
}

impl<'a> From<&'a crate::ExprUnaryOp> for ExprRef<'a> {
    fn from(node: &'a crate::ExprUnaryOp) -> Self {
        Self::UnaryOp(node)
    }
}

impl<'a> From<&'a crate::ExprLambda> for ExprRef<'a> {
    fn from(node: &'a crate::ExprLambda) -> Self {
        Self::Lambda(node)
    }
}

impl<'a> From<&'a crate::ExprIf> for ExprRef<'a> {
    fn from(node: &'a crate::ExprIf) -> Self {
        Self::If(node)
    }
}

impl<'a> From<&'a crate::ExprDict> for ExprRef<'a> {
    fn from(node: &'a crate::ExprDict) -> Self {
        Self::Dict(node)
    }
}

impl<'a> From<&'a crate::ExprSet> for ExprRef<'a> {
    fn from(node: &'a crate::ExprSet) -> Self {
        Self::Set(node)
    }
}

impl<'a> From<&'a crate::ExprListComp> for ExprRef<'a> {
    fn from(node: &'a crate::ExprListComp) -> Self {
        Self::ListComp(node)
    }
}

impl<'a> From<&'a crate::ExprSetComp> for ExprRef<'a> {
    fn from(node: &'a crate::ExprSetComp) -> Self {
        Self::SetComp(node)
    }
}

impl<'a> From<&'a crate::ExprDictComp> for ExprRef<'a> {
    fn from(node: &'a crate::ExprDictComp) -> Self {
        Self::DictComp(node)
    }
}

impl<'a> From<&'a crate::ExprGenerator> for ExprRef<'a> {
    fn from(node: &'a crate::ExprGenerator) -> Self {
        Self::Generator(node)
    }
}

impl<'a> From<&'a crate::ExprAwait> for ExprRef<'a> {
    fn from(node: &'a crate::ExprAwait) -> Self {
        Self::Await(node)
    }
}

impl<'a> From<&'a crate::ExprYield> for ExprRef<'a> {
    fn from(node: &'a crate::ExprYield) -> Self {
        Self::Yield(node)
    }
}

impl<'a> From<&'a crate::ExprYieldFrom> for ExprRef<'a> {
    fn from(node: &'a crate::ExprYieldFrom) -> Self {
        Self::YieldFrom(node)
    }
}

impl<'a> From<&'a crate::ExprCompare> for ExprRef<'a> {
    fn from(node: &'a crate::ExprCompare) -> Self {
        Self::Compare(node)
    }
}

impl<'a> From<&'a crate::ExprCall> for ExprRef<'a> {
    fn from(node: &'a crate::ExprCall) -> Self {
        Self::Call(node)
    }
}

impl<'a> From<&'a crate::ExprFString> for ExprRef<'a> {
    fn from(node: &'a crate::ExprFString) -> Self {
        Self::FString(node)
    }
}

impl<'a> From<&'a crate::ExprStringLiteral> for ExprRef<'a> {
    fn from(node: &'a crate::ExprStringLiteral) -> Self {
        Self::StringLiteral(node)
    }
}

impl<'a> From<&'a crate::ExprBytesLiteral> for ExprRef<'a> {
    fn from(node: &'a crate::ExprBytesLiteral) -> Self {
        Self::BytesLiteral(node)
    }
}

impl<'a> From<&'a crate::ExprNumberLiteral> for ExprRef<'a> {
    fn from(node: &'a crate::ExprNumberLiteral) -> Self {
        Self::NumberLiteral(node)
    }
}

impl<'a> From<&'a crate::ExprBooleanLiteral> for ExprRef<'a> {
    fn from(node: &'a crate::ExprBooleanLiteral) -> Self {
        Self::BooleanLiteral(node)
    }
}

impl<'a> From<&'a crate::ExprNoneLiteral> for ExprRef<'a> {
    fn from(node: &'a crate::ExprNoneLiteral) -> Self {
        Self::NoneLiteral(node)
    }
}

impl<'a> From<&'a crate::ExprEllipsisLiteral> for ExprRef<'a> {
    fn from(node: &'a crate::ExprEllipsisLiteral) -> Self {
        Self::EllipsisLiteral(node)
    }
}

impl<'a> From<&'a crate::ExprAttribute> for ExprRef<'a> {
    fn from(node: &'a crate::ExprAttribute) -> Self {
        Self::Attribute(node)
    }
}

impl<'a> From<&'a crate::ExprSubscript> for ExprRef<'a> {
    fn from(node: &'a crate::ExprSubscript) -> Self {
        Self::Subscript(node)
    }
}

impl<'a> From<&'a crate::ExprStarred> for ExprRef<'a> {
    fn from(node: &'a crate::ExprStarred) -> Self {
        Self::Starred(node)
    }
}

impl<'a> From<&'a crate::ExprName> for ExprRef<'a> {
    fn from(node: &'a crate::ExprName) -> Self {
        Self::Name(node)
    }
}

impl<'a> From<&'a crate::ExprList> for ExprRef<'a> {
    fn from(node: &'a crate::ExprList) -> Self {
        Self::List(node)
    }
}

impl<'a> From<&'a crate::ExprTuple> for ExprRef<'a> {
    fn from(node: &'a crate::ExprTuple) -> Self {
        Self::Tuple(node)
    }
}

impl<'a> From<&'a crate::ExprSlice> for ExprRef<'a> {
    fn from(node: &'a crate::ExprSlice) -> Self {
        Self::Slice(node)
    }
}

impl<'a> From<&'a crate::ExprIpyEscapeCommand> for ExprRef<'a> {
    fn from(node: &'a crate::ExprIpyEscapeCommand) -> Self {
        Self::IpyEscapeCommand(node)
    }
}

impl ruff_text_size::Ranged for ExprRef<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        match self {
            Self::BoolOp(node) => node.range(),
            Self::Named(node) => node.range(),
            Self::BinOp(node) => node.range(),
            Self::UnaryOp(node) => node.range(),
            Self::Lambda(node) => node.range(),
            Self::If(node) => node.range(),
            Self::Dict(node) => node.range(),
            Self::Set(node) => node.range(),
            Self::ListComp(node) => node.range(),
            Self::SetComp(node) => node.range(),
            Self::DictComp(node) => node.range(),
            Self::Generator(node) => node.range(),
            Self::Await(node) => node.range(),
            Self::Yield(node) => node.range(),
            Self::YieldFrom(node) => node.range(),
            Self::Compare(node) => node.range(),
            Self::Call(node) => node.range(),
            Self::FString(node) => node.range(),
            Self::StringLiteral(node) => node.range(),
            Self::BytesLiteral(node) => node.range(),
            Self::NumberLiteral(node) => node.range(),
            Self::BooleanLiteral(node) => node.range(),
            Self::NoneLiteral(node) => node.range(),
            Self::EllipsisLiteral(node) => node.range(),
            Self::Attribute(node) => node.range(),
            Self::Subscript(node) => node.range(),
            Self::Starred(node) => node.range(),
            Self::Name(node) => node.range(),
            Self::List(node) => node.range(),
            Self::Tuple(node) => node.range(),
            Self::Slice(node) => node.range(),
            Self::IpyEscapeCommand(node) => node.range(),
        }
    }
}

/// See also [excepthandler](https://docs.python.org/3/library/ast.html#ast.excepthandler)
#[derive(Clone, Copy, Debug, PartialEq, is_macro::Is)]
pub enum ExceptHandlerRef<'a> {
    ExceptHandler(&'a crate::ExceptHandlerExceptHandler),
}

impl<'a> From<&'a ExceptHandler> for ExceptHandlerRef<'a> {
    fn from(node: &'a ExceptHandler) -> Self {
        match node {
            ExceptHandler::ExceptHandler(node) => ExceptHandlerRef::ExceptHandler(node),
        }
    }
}

impl<'a> From<&'a crate::ExceptHandlerExceptHandler> for ExceptHandlerRef<'a> {
    fn from(node: &'a crate::ExceptHandlerExceptHandler) -> Self {
        Self::ExceptHandler(node)
    }
}

impl ruff_text_size::Ranged for ExceptHandlerRef<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        match self {
            Self::ExceptHandler(node) => node.range(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, is_macro::Is)]
pub enum FStringElementRef<'a> {
    Expression(&'a crate::FStringExpressionElement),
    Literal(&'a crate::FStringLiteralElement),
}

impl<'a> From<&'a FStringElement> for FStringElementRef<'a> {
    fn from(node: &'a FStringElement) -> Self {
        match node {
            FStringElement::Expression(node) => FStringElementRef::Expression(node),
            FStringElement::Literal(node) => FStringElementRef::Literal(node),
        }
    }
}

impl<'a> From<&'a crate::FStringExpressionElement> for FStringElementRef<'a> {
    fn from(node: &'a crate::FStringExpressionElement) -> Self {
        Self::Expression(node)
    }
}

impl<'a> From<&'a crate::FStringLiteralElement> for FStringElementRef<'a> {
    fn from(node: &'a crate::FStringLiteralElement) -> Self {
        Self::Literal(node)
    }
}

impl ruff_text_size::Ranged for FStringElementRef<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        match self {
            Self::Expression(node) => node.range(),
            Self::Literal(node) => node.range(),
        }
    }
}

/// See also [pattern](https://docs.python.org/3/library/ast.html#ast.pattern)
#[derive(Clone, Copy, Debug, PartialEq, is_macro::Is)]
pub enum PatternRef<'a> {
    MatchValue(&'a crate::PatternMatchValue),
    MatchSingleton(&'a crate::PatternMatchSingleton),
    MatchSequence(&'a crate::PatternMatchSequence),
    MatchMapping(&'a crate::PatternMatchMapping),
    MatchClass(&'a crate::PatternMatchClass),
    MatchStar(&'a crate::PatternMatchStar),
    MatchAs(&'a crate::PatternMatchAs),
    MatchOr(&'a crate::PatternMatchOr),
}

impl<'a> From<&'a Pattern> for PatternRef<'a> {
    fn from(node: &'a Pattern) -> Self {
        match node {
            Pattern::MatchValue(node) => PatternRef::MatchValue(node),
            Pattern::MatchSingleton(node) => PatternRef::MatchSingleton(node),
            Pattern::MatchSequence(node) => PatternRef::MatchSequence(node),
            Pattern::MatchMapping(node) => PatternRef::MatchMapping(node),
            Pattern::MatchClass(node) => PatternRef::MatchClass(node),
            Pattern::MatchStar(node) => PatternRef::MatchStar(node),
            Pattern::MatchAs(node) => PatternRef::MatchAs(node),
            Pattern::MatchOr(node) => PatternRef::MatchOr(node),
        }
    }
}

impl<'a> From<&'a crate::PatternMatchValue> for PatternRef<'a> {
    fn from(node: &'a crate::PatternMatchValue) -> Self {
        Self::MatchValue(node)
    }
}

impl<'a> From<&'a crate::PatternMatchSingleton> for PatternRef<'a> {
    fn from(node: &'a crate::PatternMatchSingleton) -> Self {
        Self::MatchSingleton(node)
    }
}

impl<'a> From<&'a crate::PatternMatchSequence> for PatternRef<'a> {
    fn from(node: &'a crate::PatternMatchSequence) -> Self {
        Self::MatchSequence(node)
    }
}

impl<'a> From<&'a crate::PatternMatchMapping> for PatternRef<'a> {
    fn from(node: &'a crate::PatternMatchMapping) -> Self {
        Self::MatchMapping(node)
    }
}

impl<'a> From<&'a crate::PatternMatchClass> for PatternRef<'a> {
    fn from(node: &'a crate::PatternMatchClass) -> Self {
        Self::MatchClass(node)
    }
}

impl<'a> From<&'a crate::PatternMatchStar> for PatternRef<'a> {
    fn from(node: &'a crate::PatternMatchStar) -> Self {
        Self::MatchStar(node)
    }
}

impl<'a> From<&'a crate::PatternMatchAs> for PatternRef<'a> {
    fn from(node: &'a crate::PatternMatchAs) -> Self {
        Self::MatchAs(node)
    }
}

impl<'a> From<&'a crate::PatternMatchOr> for PatternRef<'a> {
    fn from(node: &'a crate::PatternMatchOr) -> Self {
        Self::MatchOr(node)
    }
}

impl ruff_text_size::Ranged for PatternRef<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        match self {
            Self::MatchValue(node) => node.range(),
            Self::MatchSingleton(node) => node.range(),
            Self::MatchSequence(node) => node.range(),
            Self::MatchMapping(node) => node.range(),
            Self::MatchClass(node) => node.range(),
            Self::MatchStar(node) => node.range(),
            Self::MatchAs(node) => node.range(),
            Self::MatchOr(node) => node.range(),
        }
    }
}

/// See also [type_param](https://docs.python.org/3/library/ast.html#ast.type_param)
#[derive(Clone, Copy, Debug, PartialEq, is_macro::Is)]
pub enum TypeParamRef<'a> {
    TypeVar(&'a crate::TypeParamTypeVar),
    TypeVarTuple(&'a crate::TypeParamTypeVarTuple),
    ParamSpec(&'a crate::TypeParamParamSpec),
}

impl<'a> From<&'a TypeParam> for TypeParamRef<'a> {
    fn from(node: &'a TypeParam) -> Self {
        match node {
            TypeParam::TypeVar(node) => TypeParamRef::TypeVar(node),
            TypeParam::TypeVarTuple(node) => TypeParamRef::TypeVarTuple(node),
            TypeParam::ParamSpec(node) => TypeParamRef::ParamSpec(node),
        }
    }
}

impl<'a> From<&'a crate::TypeParamTypeVar> for TypeParamRef<'a> {
    fn from(node: &'a crate::TypeParamTypeVar) -> Self {
        Self::TypeVar(node)
    }
}

impl<'a> From<&'a crate::TypeParamTypeVarTuple> for TypeParamRef<'a> {
    fn from(node: &'a crate::TypeParamTypeVarTuple) -> Self {
        Self::TypeVarTuple(node)
    }
}

impl<'a> From<&'a crate::TypeParamParamSpec> for TypeParamRef<'a> {
    fn from(node: &'a crate::TypeParamParamSpec) -> Self {
        Self::ParamSpec(node)
    }
}

impl ruff_text_size::Ranged for TypeParamRef<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        match self {
            Self::TypeVar(node) => node.range(),
            Self::TypeVarTuple(node) => node.range(),
            Self::ParamSpec(node) => node.range(),
        }
    }
}

#[derive(Copy, Clone, Debug, is_macro::Is, PartialEq)]
pub enum AnyNodeRef<'a> {
    ModModule(&'a crate::ModModule),
    ModExpression(&'a crate::ModExpression),
    StmtFunctionDef(&'a crate::StmtFunctionDef),
    StmtClassDef(&'a crate::StmtClassDef),
    StmtReturn(&'a crate::StmtReturn),
    StmtDelete(&'a crate::StmtDelete),
    StmtTypeAlias(&'a crate::StmtTypeAlias),
    StmtAssign(&'a crate::StmtAssign),
    StmtAugAssign(&'a crate::StmtAugAssign),
    StmtAnnAssign(&'a crate::StmtAnnAssign),
    StmtFor(&'a crate::StmtFor),
    StmtWhile(&'a crate::StmtWhile),
    StmtIf(&'a crate::StmtIf),
    StmtWith(&'a crate::StmtWith),
    StmtMatch(&'a crate::StmtMatch),
    StmtRaise(&'a crate::StmtRaise),
    StmtTry(&'a crate::StmtTry),
    StmtAssert(&'a crate::StmtAssert),
    StmtImport(&'a crate::StmtImport),
    StmtImportFrom(&'a crate::StmtImportFrom),
    StmtGlobal(&'a crate::StmtGlobal),
    StmtNonlocal(&'a crate::StmtNonlocal),
    StmtExpr(&'a crate::StmtExpr),
    StmtPass(&'a crate::StmtPass),
    StmtBreak(&'a crate::StmtBreak),
    StmtContinue(&'a crate::StmtContinue),
    StmtIpyEscapeCommand(&'a crate::StmtIpyEscapeCommand),
    ExprBoolOp(&'a crate::ExprBoolOp),
    ExprNamed(&'a crate::ExprNamed),
    ExprBinOp(&'a crate::ExprBinOp),
    ExprUnaryOp(&'a crate::ExprUnaryOp),
    ExprLambda(&'a crate::ExprLambda),
    ExprIf(&'a crate::ExprIf),
    ExprDict(&'a crate::ExprDict),
    ExprSet(&'a crate::ExprSet),
    ExprListComp(&'a crate::ExprListComp),
    ExprSetComp(&'a crate::ExprSetComp),
    ExprDictComp(&'a crate::ExprDictComp),
    ExprGenerator(&'a crate::ExprGenerator),
    ExprAwait(&'a crate::ExprAwait),
    ExprYield(&'a crate::ExprYield),
    ExprYieldFrom(&'a crate::ExprYieldFrom),
    ExprCompare(&'a crate::ExprCompare),
    ExprCall(&'a crate::ExprCall),
    ExprFString(&'a crate::ExprFString),
    ExprStringLiteral(&'a crate::ExprStringLiteral),
    ExprBytesLiteral(&'a crate::ExprBytesLiteral),
    ExprNumberLiteral(&'a crate::ExprNumberLiteral),
    ExprBooleanLiteral(&'a crate::ExprBooleanLiteral),
    ExprNoneLiteral(&'a crate::ExprNoneLiteral),
    ExprEllipsisLiteral(&'a crate::ExprEllipsisLiteral),
    ExprAttribute(&'a crate::ExprAttribute),
    ExprSubscript(&'a crate::ExprSubscript),
    ExprStarred(&'a crate::ExprStarred),
    ExprName(&'a crate::ExprName),
    ExprList(&'a crate::ExprList),
    ExprTuple(&'a crate::ExprTuple),
    ExprSlice(&'a crate::ExprSlice),
    ExprIpyEscapeCommand(&'a crate::ExprIpyEscapeCommand),
    ExceptHandlerExceptHandler(&'a crate::ExceptHandlerExceptHandler),
    FStringExpressionElement(&'a crate::FStringExpressionElement),
    FStringLiteralElement(&'a crate::FStringLiteralElement),
    PatternMatchValue(&'a crate::PatternMatchValue),
    PatternMatchSingleton(&'a crate::PatternMatchSingleton),
    PatternMatchSequence(&'a crate::PatternMatchSequence),
    PatternMatchMapping(&'a crate::PatternMatchMapping),
    PatternMatchClass(&'a crate::PatternMatchClass),
    PatternMatchStar(&'a crate::PatternMatchStar),
    PatternMatchAs(&'a crate::PatternMatchAs),
    PatternMatchOr(&'a crate::PatternMatchOr),
    TypeParamTypeVar(&'a crate::TypeParamTypeVar),
    TypeParamTypeVarTuple(&'a crate::TypeParamTypeVarTuple),
    TypeParamParamSpec(&'a crate::TypeParamParamSpec),
    FStringFormatSpec(&'a crate::FStringFormatSpec),
    PatternArguments(&'a crate::PatternArguments),
    PatternKeyword(&'a crate::PatternKeyword),
    Comprehension(&'a crate::Comprehension),
    Arguments(&'a crate::Arguments),
    Parameters(&'a crate::Parameters),
    Parameter(&'a crate::Parameter),
    ParameterWithDefault(&'a crate::ParameterWithDefault),
    Keyword(&'a crate::Keyword),
    Alias(&'a crate::Alias),
    WithItem(&'a crate::WithItem),
    MatchCase(&'a crate::MatchCase),
    Decorator(&'a crate::Decorator),
    ElifElseClause(&'a crate::ElifElseClause),
    TypeParams(&'a crate::TypeParams),
    FString(&'a crate::FString),
    StringLiteral(&'a crate::StringLiteral),
    BytesLiteral(&'a crate::BytesLiteral),
    Identifier(&'a crate::Identifier),
}

impl<'a> From<&'a Mod> for AnyNodeRef<'a> {
    fn from(node: &'a Mod) -> AnyNodeRef<'a> {
        match node {
            Mod::Module(node) => AnyNodeRef::ModModule(node),
            Mod::Expression(node) => AnyNodeRef::ModExpression(node),
        }
    }
}

impl<'a> From<ModRef<'a>> for AnyNodeRef<'a> {
    fn from(node: ModRef<'a>) -> AnyNodeRef<'a> {
        match node {
            ModRef::Module(node) => AnyNodeRef::ModModule(node),
            ModRef::Expression(node) => AnyNodeRef::ModExpression(node),
        }
    }
}

impl<'a> From<&'a Stmt> for AnyNodeRef<'a> {
    fn from(node: &'a Stmt) -> AnyNodeRef<'a> {
        match node {
            Stmt::FunctionDef(node) => AnyNodeRef::StmtFunctionDef(node),
            Stmt::ClassDef(node) => AnyNodeRef::StmtClassDef(node),
            Stmt::Return(node) => AnyNodeRef::StmtReturn(node),
            Stmt::Delete(node) => AnyNodeRef::StmtDelete(node),
            Stmt::TypeAlias(node) => AnyNodeRef::StmtTypeAlias(node),
            Stmt::Assign(node) => AnyNodeRef::StmtAssign(node),
            Stmt::AugAssign(node) => AnyNodeRef::StmtAugAssign(node),
            Stmt::AnnAssign(node) => AnyNodeRef::StmtAnnAssign(node),
            Stmt::For(node) => AnyNodeRef::StmtFor(node),
            Stmt::While(node) => AnyNodeRef::StmtWhile(node),
            Stmt::If(node) => AnyNodeRef::StmtIf(node),
            Stmt::With(node) => AnyNodeRef::StmtWith(node),
            Stmt::Match(node) => AnyNodeRef::StmtMatch(node),
            Stmt::Raise(node) => AnyNodeRef::StmtRaise(node),
            Stmt::Try(node) => AnyNodeRef::StmtTry(node),
            Stmt::Assert(node) => AnyNodeRef::StmtAssert(node),
            Stmt::Import(node) => AnyNodeRef::StmtImport(node),
            Stmt::ImportFrom(node) => AnyNodeRef::StmtImportFrom(node),
            Stmt::Global(node) => AnyNodeRef::StmtGlobal(node),
            Stmt::Nonlocal(node) => AnyNodeRef::StmtNonlocal(node),
            Stmt::Expr(node) => AnyNodeRef::StmtExpr(node),
            Stmt::Pass(node) => AnyNodeRef::StmtPass(node),
            Stmt::Break(node) => AnyNodeRef::StmtBreak(node),
            Stmt::Continue(node) => AnyNodeRef::StmtContinue(node),
            Stmt::IpyEscapeCommand(node) => AnyNodeRef::StmtIpyEscapeCommand(node),
        }
    }
}

impl<'a> From<StmtRef<'a>> for AnyNodeRef<'a> {
    fn from(node: StmtRef<'a>) -> AnyNodeRef<'a> {
        match node {
            StmtRef::FunctionDef(node) => AnyNodeRef::StmtFunctionDef(node),
            StmtRef::ClassDef(node) => AnyNodeRef::StmtClassDef(node),
            StmtRef::Return(node) => AnyNodeRef::StmtReturn(node),
            StmtRef::Delete(node) => AnyNodeRef::StmtDelete(node),
            StmtRef::TypeAlias(node) => AnyNodeRef::StmtTypeAlias(node),
            StmtRef::Assign(node) => AnyNodeRef::StmtAssign(node),
            StmtRef::AugAssign(node) => AnyNodeRef::StmtAugAssign(node),
            StmtRef::AnnAssign(node) => AnyNodeRef::StmtAnnAssign(node),
            StmtRef::For(node) => AnyNodeRef::StmtFor(node),
            StmtRef::While(node) => AnyNodeRef::StmtWhile(node),
            StmtRef::If(node) => AnyNodeRef::StmtIf(node),
            StmtRef::With(node) => AnyNodeRef::StmtWith(node),
            StmtRef::Match(node) => AnyNodeRef::StmtMatch(node),
            StmtRef::Raise(node) => AnyNodeRef::StmtRaise(node),
            StmtRef::Try(node) => AnyNodeRef::StmtTry(node),
            StmtRef::Assert(node) => AnyNodeRef::StmtAssert(node),
            StmtRef::Import(node) => AnyNodeRef::StmtImport(node),
            StmtRef::ImportFrom(node) => AnyNodeRef::StmtImportFrom(node),
            StmtRef::Global(node) => AnyNodeRef::StmtGlobal(node),
            StmtRef::Nonlocal(node) => AnyNodeRef::StmtNonlocal(node),
            StmtRef::Expr(node) => AnyNodeRef::StmtExpr(node),
            StmtRef::Pass(node) => AnyNodeRef::StmtPass(node),
            StmtRef::Break(node) => AnyNodeRef::StmtBreak(node),
            StmtRef::Continue(node) => AnyNodeRef::StmtContinue(node),
            StmtRef::IpyEscapeCommand(node) => AnyNodeRef::StmtIpyEscapeCommand(node),
        }
    }
}

impl<'a> From<&'a Expr> for AnyNodeRef<'a> {
    fn from(node: &'a Expr) -> AnyNodeRef<'a> {
        match node {
            Expr::BoolOp(node) => AnyNodeRef::ExprBoolOp(node),
            Expr::Named(node) => AnyNodeRef::ExprNamed(node),
            Expr::BinOp(node) => AnyNodeRef::ExprBinOp(node),
            Expr::UnaryOp(node) => AnyNodeRef::ExprUnaryOp(node),
            Expr::Lambda(node) => AnyNodeRef::ExprLambda(node),
            Expr::If(node) => AnyNodeRef::ExprIf(node),
            Expr::Dict(node) => AnyNodeRef::ExprDict(node),
            Expr::Set(node) => AnyNodeRef::ExprSet(node),
            Expr::ListComp(node) => AnyNodeRef::ExprListComp(node),
            Expr::SetComp(node) => AnyNodeRef::ExprSetComp(node),
            Expr::DictComp(node) => AnyNodeRef::ExprDictComp(node),
            Expr::Generator(node) => AnyNodeRef::ExprGenerator(node),
            Expr::Await(node) => AnyNodeRef::ExprAwait(node),
            Expr::Yield(node) => AnyNodeRef::ExprYield(node),
            Expr::YieldFrom(node) => AnyNodeRef::ExprYieldFrom(node),
            Expr::Compare(node) => AnyNodeRef::ExprCompare(node),
            Expr::Call(node) => AnyNodeRef::ExprCall(node),
            Expr::FString(node) => AnyNodeRef::ExprFString(node),
            Expr::StringLiteral(node) => AnyNodeRef::ExprStringLiteral(node),
            Expr::BytesLiteral(node) => AnyNodeRef::ExprBytesLiteral(node),
            Expr::NumberLiteral(node) => AnyNodeRef::ExprNumberLiteral(node),
            Expr::BooleanLiteral(node) => AnyNodeRef::ExprBooleanLiteral(node),
            Expr::NoneLiteral(node) => AnyNodeRef::ExprNoneLiteral(node),
            Expr::EllipsisLiteral(node) => AnyNodeRef::ExprEllipsisLiteral(node),
            Expr::Attribute(node) => AnyNodeRef::ExprAttribute(node),
            Expr::Subscript(node) => AnyNodeRef::ExprSubscript(node),
            Expr::Starred(node) => AnyNodeRef::ExprStarred(node),
            Expr::Name(node) => AnyNodeRef::ExprName(node),
            Expr::List(node) => AnyNodeRef::ExprList(node),
            Expr::Tuple(node) => AnyNodeRef::ExprTuple(node),
            Expr::Slice(node) => AnyNodeRef::ExprSlice(node),
            Expr::IpyEscapeCommand(node) => AnyNodeRef::ExprIpyEscapeCommand(node),
        }
    }
}

impl<'a> From<ExprRef<'a>> for AnyNodeRef<'a> {
    fn from(node: ExprRef<'a>) -> AnyNodeRef<'a> {
        match node {
            ExprRef::BoolOp(node) => AnyNodeRef::ExprBoolOp(node),
            ExprRef::Named(node) => AnyNodeRef::ExprNamed(node),
            ExprRef::BinOp(node) => AnyNodeRef::ExprBinOp(node),
            ExprRef::UnaryOp(node) => AnyNodeRef::ExprUnaryOp(node),
            ExprRef::Lambda(node) => AnyNodeRef::ExprLambda(node),
            ExprRef::If(node) => AnyNodeRef::ExprIf(node),
            ExprRef::Dict(node) => AnyNodeRef::ExprDict(node),
            ExprRef::Set(node) => AnyNodeRef::ExprSet(node),
            ExprRef::ListComp(node) => AnyNodeRef::ExprListComp(node),
            ExprRef::SetComp(node) => AnyNodeRef::ExprSetComp(node),
            ExprRef::DictComp(node) => AnyNodeRef::ExprDictComp(node),
            ExprRef::Generator(node) => AnyNodeRef::ExprGenerator(node),
            ExprRef::Await(node) => AnyNodeRef::ExprAwait(node),
            ExprRef::Yield(node) => AnyNodeRef::ExprYield(node),
            ExprRef::YieldFrom(node) => AnyNodeRef::ExprYieldFrom(node),
            ExprRef::Compare(node) => AnyNodeRef::ExprCompare(node),
            ExprRef::Call(node) => AnyNodeRef::ExprCall(node),
            ExprRef::FString(node) => AnyNodeRef::ExprFString(node),
            ExprRef::StringLiteral(node) => AnyNodeRef::ExprStringLiteral(node),
            ExprRef::BytesLiteral(node) => AnyNodeRef::ExprBytesLiteral(node),
            ExprRef::NumberLiteral(node) => AnyNodeRef::ExprNumberLiteral(node),
            ExprRef::BooleanLiteral(node) => AnyNodeRef::ExprBooleanLiteral(node),
            ExprRef::NoneLiteral(node) => AnyNodeRef::ExprNoneLiteral(node),
            ExprRef::EllipsisLiteral(node) => AnyNodeRef::ExprEllipsisLiteral(node),
            ExprRef::Attribute(node) => AnyNodeRef::ExprAttribute(node),
            ExprRef::Subscript(node) => AnyNodeRef::ExprSubscript(node),
            ExprRef::Starred(node) => AnyNodeRef::ExprStarred(node),
            ExprRef::Name(node) => AnyNodeRef::ExprName(node),
            ExprRef::List(node) => AnyNodeRef::ExprList(node),
            ExprRef::Tuple(node) => AnyNodeRef::ExprTuple(node),
            ExprRef::Slice(node) => AnyNodeRef::ExprSlice(node),
            ExprRef::IpyEscapeCommand(node) => AnyNodeRef::ExprIpyEscapeCommand(node),
        }
    }
}

impl<'a> From<&'a ExceptHandler> for AnyNodeRef<'a> {
    fn from(node: &'a ExceptHandler) -> AnyNodeRef<'a> {
        match node {
            ExceptHandler::ExceptHandler(node) => AnyNodeRef::ExceptHandlerExceptHandler(node),
        }
    }
}

impl<'a> From<ExceptHandlerRef<'a>> for AnyNodeRef<'a> {
    fn from(node: ExceptHandlerRef<'a>) -> AnyNodeRef<'a> {
        match node {
            ExceptHandlerRef::ExceptHandler(node) => AnyNodeRef::ExceptHandlerExceptHandler(node),
        }
    }
}

impl<'a> From<&'a FStringElement> for AnyNodeRef<'a> {
    fn from(node: &'a FStringElement) -> AnyNodeRef<'a> {
        match node {
            FStringElement::Expression(node) => AnyNodeRef::FStringExpressionElement(node),
            FStringElement::Literal(node) => AnyNodeRef::FStringLiteralElement(node),
        }
    }
}

impl<'a> From<FStringElementRef<'a>> for AnyNodeRef<'a> {
    fn from(node: FStringElementRef<'a>) -> AnyNodeRef<'a> {
        match node {
            FStringElementRef::Expression(node) => AnyNodeRef::FStringExpressionElement(node),
            FStringElementRef::Literal(node) => AnyNodeRef::FStringLiteralElement(node),
        }
    }
}

impl<'a> From<&'a Pattern> for AnyNodeRef<'a> {
    fn from(node: &'a Pattern) -> AnyNodeRef<'a> {
        match node {
            Pattern::MatchValue(node) => AnyNodeRef::PatternMatchValue(node),
            Pattern::MatchSingleton(node) => AnyNodeRef::PatternMatchSingleton(node),
            Pattern::MatchSequence(node) => AnyNodeRef::PatternMatchSequence(node),
            Pattern::MatchMapping(node) => AnyNodeRef::PatternMatchMapping(node),
            Pattern::MatchClass(node) => AnyNodeRef::PatternMatchClass(node),
            Pattern::MatchStar(node) => AnyNodeRef::PatternMatchStar(node),
            Pattern::MatchAs(node) => AnyNodeRef::PatternMatchAs(node),
            Pattern::MatchOr(node) => AnyNodeRef::PatternMatchOr(node),
        }
    }
}

impl<'a> From<PatternRef<'a>> for AnyNodeRef<'a> {
    fn from(node: PatternRef<'a>) -> AnyNodeRef<'a> {
        match node {
            PatternRef::MatchValue(node) => AnyNodeRef::PatternMatchValue(node),
            PatternRef::MatchSingleton(node) => AnyNodeRef::PatternMatchSingleton(node),
            PatternRef::MatchSequence(node) => AnyNodeRef::PatternMatchSequence(node),
            PatternRef::MatchMapping(node) => AnyNodeRef::PatternMatchMapping(node),
            PatternRef::MatchClass(node) => AnyNodeRef::PatternMatchClass(node),
            PatternRef::MatchStar(node) => AnyNodeRef::PatternMatchStar(node),
            PatternRef::MatchAs(node) => AnyNodeRef::PatternMatchAs(node),
            PatternRef::MatchOr(node) => AnyNodeRef::PatternMatchOr(node),
        }
    }
}

impl<'a> From<&'a TypeParam> for AnyNodeRef<'a> {
    fn from(node: &'a TypeParam) -> AnyNodeRef<'a> {
        match node {
            TypeParam::TypeVar(node) => AnyNodeRef::TypeParamTypeVar(node),
            TypeParam::TypeVarTuple(node) => AnyNodeRef::TypeParamTypeVarTuple(node),
            TypeParam::ParamSpec(node) => AnyNodeRef::TypeParamParamSpec(node),
        }
    }
}

impl<'a> From<TypeParamRef<'a>> for AnyNodeRef<'a> {
    fn from(node: TypeParamRef<'a>) -> AnyNodeRef<'a> {
        match node {
            TypeParamRef::TypeVar(node) => AnyNodeRef::TypeParamTypeVar(node),
            TypeParamRef::TypeVarTuple(node) => AnyNodeRef::TypeParamTypeVarTuple(node),
            TypeParamRef::ParamSpec(node) => AnyNodeRef::TypeParamParamSpec(node),
        }
    }
}

impl<'a> From<&'a crate::ModModule> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ModModule) -> AnyNodeRef<'a> {
        AnyNodeRef::ModModule(node)
    }
}

impl<'a> From<&'a crate::ModExpression> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ModExpression) -> AnyNodeRef<'a> {
        AnyNodeRef::ModExpression(node)
    }
}

impl<'a> From<&'a crate::StmtFunctionDef> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtFunctionDef) -> AnyNodeRef<'a> {
        AnyNodeRef::StmtFunctionDef(node)
    }
}

impl<'a> From<&'a crate::StmtClassDef> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtClassDef) -> AnyNodeRef<'a> {
        AnyNodeRef::StmtClassDef(node)
    }
}

impl<'a> From<&'a crate::StmtReturn> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtReturn) -> AnyNodeRef<'a> {
        AnyNodeRef::StmtReturn(node)
    }
}

impl<'a> From<&'a crate::StmtDelete> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtDelete) -> AnyNodeRef<'a> {
        AnyNodeRef::StmtDelete(node)
    }
}

impl<'a> From<&'a crate::StmtTypeAlias> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtTypeAlias) -> AnyNodeRef<'a> {
        AnyNodeRef::StmtTypeAlias(node)
    }
}

impl<'a> From<&'a crate::StmtAssign> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtAssign) -> AnyNodeRef<'a> {
        AnyNodeRef::StmtAssign(node)
    }
}

impl<'a> From<&'a crate::StmtAugAssign> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtAugAssign) -> AnyNodeRef<'a> {
        AnyNodeRef::StmtAugAssign(node)
    }
}

impl<'a> From<&'a crate::StmtAnnAssign> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtAnnAssign) -> AnyNodeRef<'a> {
        AnyNodeRef::StmtAnnAssign(node)
    }
}

impl<'a> From<&'a crate::StmtFor> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtFor) -> AnyNodeRef<'a> {
        AnyNodeRef::StmtFor(node)
    }
}

impl<'a> From<&'a crate::StmtWhile> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtWhile) -> AnyNodeRef<'a> {
        AnyNodeRef::StmtWhile(node)
    }
}

impl<'a> From<&'a crate::StmtIf> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtIf) -> AnyNodeRef<'a> {
        AnyNodeRef::StmtIf(node)
    }
}

impl<'a> From<&'a crate::StmtWith> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtWith) -> AnyNodeRef<'a> {
        AnyNodeRef::StmtWith(node)
    }
}

impl<'a> From<&'a crate::StmtMatch> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtMatch) -> AnyNodeRef<'a> {
        AnyNodeRef::StmtMatch(node)
    }
}

impl<'a> From<&'a crate::StmtRaise> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtRaise) -> AnyNodeRef<'a> {
        AnyNodeRef::StmtRaise(node)
    }
}

impl<'a> From<&'a crate::StmtTry> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtTry) -> AnyNodeRef<'a> {
        AnyNodeRef::StmtTry(node)
    }
}

impl<'a> From<&'a crate::StmtAssert> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtAssert) -> AnyNodeRef<'a> {
        AnyNodeRef::StmtAssert(node)
    }
}

impl<'a> From<&'a crate::StmtImport> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtImport) -> AnyNodeRef<'a> {
        AnyNodeRef::StmtImport(node)
    }
}

impl<'a> From<&'a crate::StmtImportFrom> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtImportFrom) -> AnyNodeRef<'a> {
        AnyNodeRef::StmtImportFrom(node)
    }
}

impl<'a> From<&'a crate::StmtGlobal> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtGlobal) -> AnyNodeRef<'a> {
        AnyNodeRef::StmtGlobal(node)
    }
}

impl<'a> From<&'a crate::StmtNonlocal> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtNonlocal) -> AnyNodeRef<'a> {
        AnyNodeRef::StmtNonlocal(node)
    }
}

impl<'a> From<&'a crate::StmtExpr> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtExpr) -> AnyNodeRef<'a> {
        AnyNodeRef::StmtExpr(node)
    }
}

impl<'a> From<&'a crate::StmtPass> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtPass) -> AnyNodeRef<'a> {
        AnyNodeRef::StmtPass(node)
    }
}

impl<'a> From<&'a crate::StmtBreak> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtBreak) -> AnyNodeRef<'a> {
        AnyNodeRef::StmtBreak(node)
    }
}

impl<'a> From<&'a crate::StmtContinue> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtContinue) -> AnyNodeRef<'a> {
        AnyNodeRef::StmtContinue(node)
    }
}

impl<'a> From<&'a crate::StmtIpyEscapeCommand> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtIpyEscapeCommand) -> AnyNodeRef<'a> {
        AnyNodeRef::StmtIpyEscapeCommand(node)
    }
}

impl<'a> From<&'a crate::ExprBoolOp> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprBoolOp) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprBoolOp(node)
    }
}

impl<'a> From<&'a crate::ExprNamed> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprNamed) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprNamed(node)
    }
}

impl<'a> From<&'a crate::ExprBinOp> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprBinOp) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprBinOp(node)
    }
}

impl<'a> From<&'a crate::ExprUnaryOp> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprUnaryOp) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprUnaryOp(node)
    }
}

impl<'a> From<&'a crate::ExprLambda> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprLambda) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprLambda(node)
    }
}

impl<'a> From<&'a crate::ExprIf> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprIf) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprIf(node)
    }
}

impl<'a> From<&'a crate::ExprDict> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprDict) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprDict(node)
    }
}

impl<'a> From<&'a crate::ExprSet> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprSet) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprSet(node)
    }
}

impl<'a> From<&'a crate::ExprListComp> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprListComp) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprListComp(node)
    }
}

impl<'a> From<&'a crate::ExprSetComp> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprSetComp) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprSetComp(node)
    }
}

impl<'a> From<&'a crate::ExprDictComp> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprDictComp) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprDictComp(node)
    }
}

impl<'a> From<&'a crate::ExprGenerator> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprGenerator) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprGenerator(node)
    }
}

impl<'a> From<&'a crate::ExprAwait> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprAwait) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprAwait(node)
    }
}

impl<'a> From<&'a crate::ExprYield> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprYield) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprYield(node)
    }
}

impl<'a> From<&'a crate::ExprYieldFrom> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprYieldFrom) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprYieldFrom(node)
    }
}

impl<'a> From<&'a crate::ExprCompare> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprCompare) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprCompare(node)
    }
}

impl<'a> From<&'a crate::ExprCall> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprCall) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprCall(node)
    }
}

impl<'a> From<&'a crate::ExprFString> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprFString) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprFString(node)
    }
}

impl<'a> From<&'a crate::ExprStringLiteral> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprStringLiteral) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprStringLiteral(node)
    }
}

impl<'a> From<&'a crate::ExprBytesLiteral> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprBytesLiteral) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprBytesLiteral(node)
    }
}

impl<'a> From<&'a crate::ExprNumberLiteral> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprNumberLiteral) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprNumberLiteral(node)
    }
}

impl<'a> From<&'a crate::ExprBooleanLiteral> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprBooleanLiteral) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprBooleanLiteral(node)
    }
}

impl<'a> From<&'a crate::ExprNoneLiteral> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprNoneLiteral) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprNoneLiteral(node)
    }
}

impl<'a> From<&'a crate::ExprEllipsisLiteral> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprEllipsisLiteral) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprEllipsisLiteral(node)
    }
}

impl<'a> From<&'a crate::ExprAttribute> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprAttribute) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprAttribute(node)
    }
}

impl<'a> From<&'a crate::ExprSubscript> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprSubscript) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprSubscript(node)
    }
}

impl<'a> From<&'a crate::ExprStarred> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprStarred) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprStarred(node)
    }
}

impl<'a> From<&'a crate::ExprName> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprName) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprName(node)
    }
}

impl<'a> From<&'a crate::ExprList> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprList) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprList(node)
    }
}

impl<'a> From<&'a crate::ExprTuple> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprTuple) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprTuple(node)
    }
}

impl<'a> From<&'a crate::ExprSlice> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprSlice) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprSlice(node)
    }
}

impl<'a> From<&'a crate::ExprIpyEscapeCommand> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprIpyEscapeCommand) -> AnyNodeRef<'a> {
        AnyNodeRef::ExprIpyEscapeCommand(node)
    }
}

impl<'a> From<&'a crate::ExceptHandlerExceptHandler> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExceptHandlerExceptHandler) -> AnyNodeRef<'a> {
        AnyNodeRef::ExceptHandlerExceptHandler(node)
    }
}

impl<'a> From<&'a crate::FStringExpressionElement> for AnyNodeRef<'a> {
    fn from(node: &'a crate::FStringExpressionElement) -> AnyNodeRef<'a> {
        AnyNodeRef::FStringExpressionElement(node)
    }
}

impl<'a> From<&'a crate::FStringLiteralElement> for AnyNodeRef<'a> {
    fn from(node: &'a crate::FStringLiteralElement) -> AnyNodeRef<'a> {
        AnyNodeRef::FStringLiteralElement(node)
    }
}

impl<'a> From<&'a crate::PatternMatchValue> for AnyNodeRef<'a> {
    fn from(node: &'a crate::PatternMatchValue) -> AnyNodeRef<'a> {
        AnyNodeRef::PatternMatchValue(node)
    }
}

impl<'a> From<&'a crate::PatternMatchSingleton> for AnyNodeRef<'a> {
    fn from(node: &'a crate::PatternMatchSingleton) -> AnyNodeRef<'a> {
        AnyNodeRef::PatternMatchSingleton(node)
    }
}

impl<'a> From<&'a crate::PatternMatchSequence> for AnyNodeRef<'a> {
    fn from(node: &'a crate::PatternMatchSequence) -> AnyNodeRef<'a> {
        AnyNodeRef::PatternMatchSequence(node)
    }
}

impl<'a> From<&'a crate::PatternMatchMapping> for AnyNodeRef<'a> {
    fn from(node: &'a crate::PatternMatchMapping) -> AnyNodeRef<'a> {
        AnyNodeRef::PatternMatchMapping(node)
    }
}

impl<'a> From<&'a crate::PatternMatchClass> for AnyNodeRef<'a> {
    fn from(node: &'a crate::PatternMatchClass) -> AnyNodeRef<'a> {
        AnyNodeRef::PatternMatchClass(node)
    }
}

impl<'a> From<&'a crate::PatternMatchStar> for AnyNodeRef<'a> {
    fn from(node: &'a crate::PatternMatchStar) -> AnyNodeRef<'a> {
        AnyNodeRef::PatternMatchStar(node)
    }
}

impl<'a> From<&'a crate::PatternMatchAs> for AnyNodeRef<'a> {
    fn from(node: &'a crate::PatternMatchAs) -> AnyNodeRef<'a> {
        AnyNodeRef::PatternMatchAs(node)
    }
}

impl<'a> From<&'a crate::PatternMatchOr> for AnyNodeRef<'a> {
    fn from(node: &'a crate::PatternMatchOr) -> AnyNodeRef<'a> {
        AnyNodeRef::PatternMatchOr(node)
    }
}

impl<'a> From<&'a crate::TypeParamTypeVar> for AnyNodeRef<'a> {
    fn from(node: &'a crate::TypeParamTypeVar) -> AnyNodeRef<'a> {
        AnyNodeRef::TypeParamTypeVar(node)
    }
}

impl<'a> From<&'a crate::TypeParamTypeVarTuple> for AnyNodeRef<'a> {
    fn from(node: &'a crate::TypeParamTypeVarTuple) -> AnyNodeRef<'a> {
        AnyNodeRef::TypeParamTypeVarTuple(node)
    }
}

impl<'a> From<&'a crate::TypeParamParamSpec> for AnyNodeRef<'a> {
    fn from(node: &'a crate::TypeParamParamSpec) -> AnyNodeRef<'a> {
        AnyNodeRef::TypeParamParamSpec(node)
    }
}

impl<'a> From<&'a crate::FStringFormatSpec> for AnyNodeRef<'a> {
    fn from(node: &'a crate::FStringFormatSpec) -> AnyNodeRef<'a> {
        AnyNodeRef::FStringFormatSpec(node)
    }
}

impl<'a> From<&'a crate::PatternArguments> for AnyNodeRef<'a> {
    fn from(node: &'a crate::PatternArguments) -> AnyNodeRef<'a> {
        AnyNodeRef::PatternArguments(node)
    }
}

impl<'a> From<&'a crate::PatternKeyword> for AnyNodeRef<'a> {
    fn from(node: &'a crate::PatternKeyword) -> AnyNodeRef<'a> {
        AnyNodeRef::PatternKeyword(node)
    }
}

impl<'a> From<&'a crate::Comprehension> for AnyNodeRef<'a> {
    fn from(node: &'a crate::Comprehension) -> AnyNodeRef<'a> {
        AnyNodeRef::Comprehension(node)
    }
}

impl<'a> From<&'a crate::Arguments> for AnyNodeRef<'a> {
    fn from(node: &'a crate::Arguments) -> AnyNodeRef<'a> {
        AnyNodeRef::Arguments(node)
    }
}

impl<'a> From<&'a crate::Parameters> for AnyNodeRef<'a> {
    fn from(node: &'a crate::Parameters) -> AnyNodeRef<'a> {
        AnyNodeRef::Parameters(node)
    }
}

impl<'a> From<&'a crate::Parameter> for AnyNodeRef<'a> {
    fn from(node: &'a crate::Parameter) -> AnyNodeRef<'a> {
        AnyNodeRef::Parameter(node)
    }
}

impl<'a> From<&'a crate::ParameterWithDefault> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ParameterWithDefault) -> AnyNodeRef<'a> {
        AnyNodeRef::ParameterWithDefault(node)
    }
}

impl<'a> From<&'a crate::Keyword> for AnyNodeRef<'a> {
    fn from(node: &'a crate::Keyword) -> AnyNodeRef<'a> {
        AnyNodeRef::Keyword(node)
    }
}

impl<'a> From<&'a crate::Alias> for AnyNodeRef<'a> {
    fn from(node: &'a crate::Alias) -> AnyNodeRef<'a> {
        AnyNodeRef::Alias(node)
    }
}

impl<'a> From<&'a crate::WithItem> for AnyNodeRef<'a> {
    fn from(node: &'a crate::WithItem) -> AnyNodeRef<'a> {
        AnyNodeRef::WithItem(node)
    }
}

impl<'a> From<&'a crate::MatchCase> for AnyNodeRef<'a> {
    fn from(node: &'a crate::MatchCase) -> AnyNodeRef<'a> {
        AnyNodeRef::MatchCase(node)
    }
}

impl<'a> From<&'a crate::Decorator> for AnyNodeRef<'a> {
    fn from(node: &'a crate::Decorator) -> AnyNodeRef<'a> {
        AnyNodeRef::Decorator(node)
    }
}

impl<'a> From<&'a crate::ElifElseClause> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ElifElseClause) -> AnyNodeRef<'a> {
        AnyNodeRef::ElifElseClause(node)
    }
}

impl<'a> From<&'a crate::TypeParams> for AnyNodeRef<'a> {
    fn from(node: &'a crate::TypeParams) -> AnyNodeRef<'a> {
        AnyNodeRef::TypeParams(node)
    }
}

impl<'a> From<&'a crate::FString> for AnyNodeRef<'a> {
    fn from(node: &'a crate::FString) -> AnyNodeRef<'a> {
        AnyNodeRef::FString(node)
    }
}

impl<'a> From<&'a crate::StringLiteral> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StringLiteral) -> AnyNodeRef<'a> {
        AnyNodeRef::StringLiteral(node)
    }
}

impl<'a> From<&'a crate::BytesLiteral> for AnyNodeRef<'a> {
    fn from(node: &'a crate::BytesLiteral) -> AnyNodeRef<'a> {
        AnyNodeRef::BytesLiteral(node)
    }
}

impl<'a> From<&'a crate::Identifier> for AnyNodeRef<'a> {
    fn from(node: &'a crate::Identifier) -> AnyNodeRef<'a> {
        AnyNodeRef::Identifier(node)
    }
}

impl ruff_text_size::Ranged for AnyNodeRef<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        match self {
            AnyNodeRef::ModModule(node) => node.range(),
            AnyNodeRef::ModExpression(node) => node.range(),
            AnyNodeRef::StmtFunctionDef(node) => node.range(),
            AnyNodeRef::StmtClassDef(node) => node.range(),
            AnyNodeRef::StmtReturn(node) => node.range(),
            AnyNodeRef::StmtDelete(node) => node.range(),
            AnyNodeRef::StmtTypeAlias(node) => node.range(),
            AnyNodeRef::StmtAssign(node) => node.range(),
            AnyNodeRef::StmtAugAssign(node) => node.range(),
            AnyNodeRef::StmtAnnAssign(node) => node.range(),
            AnyNodeRef::StmtFor(node) => node.range(),
            AnyNodeRef::StmtWhile(node) => node.range(),
            AnyNodeRef::StmtIf(node) => node.range(),
            AnyNodeRef::StmtWith(node) => node.range(),
            AnyNodeRef::StmtMatch(node) => node.range(),
            AnyNodeRef::StmtRaise(node) => node.range(),
            AnyNodeRef::StmtTry(node) => node.range(),
            AnyNodeRef::StmtAssert(node) => node.range(),
            AnyNodeRef::StmtImport(node) => node.range(),
            AnyNodeRef::StmtImportFrom(node) => node.range(),
            AnyNodeRef::StmtGlobal(node) => node.range(),
            AnyNodeRef::StmtNonlocal(node) => node.range(),
            AnyNodeRef::StmtExpr(node) => node.range(),
            AnyNodeRef::StmtPass(node) => node.range(),
            AnyNodeRef::StmtBreak(node) => node.range(),
            AnyNodeRef::StmtContinue(node) => node.range(),
            AnyNodeRef::StmtIpyEscapeCommand(node) => node.range(),
            AnyNodeRef::ExprBoolOp(node) => node.range(),
            AnyNodeRef::ExprNamed(node) => node.range(),
            AnyNodeRef::ExprBinOp(node) => node.range(),
            AnyNodeRef::ExprUnaryOp(node) => node.range(),
            AnyNodeRef::ExprLambda(node) => node.range(),
            AnyNodeRef::ExprIf(node) => node.range(),
            AnyNodeRef::ExprDict(node) => node.range(),
            AnyNodeRef::ExprSet(node) => node.range(),
            AnyNodeRef::ExprListComp(node) => node.range(),
            AnyNodeRef::ExprSetComp(node) => node.range(),
            AnyNodeRef::ExprDictComp(node) => node.range(),
            AnyNodeRef::ExprGenerator(node) => node.range(),
            AnyNodeRef::ExprAwait(node) => node.range(),
            AnyNodeRef::ExprYield(node) => node.range(),
            AnyNodeRef::ExprYieldFrom(node) => node.range(),
            AnyNodeRef::ExprCompare(node) => node.range(),
            AnyNodeRef::ExprCall(node) => node.range(),
            AnyNodeRef::ExprFString(node) => node.range(),
            AnyNodeRef::ExprStringLiteral(node) => node.range(),
            AnyNodeRef::ExprBytesLiteral(node) => node.range(),
            AnyNodeRef::ExprNumberLiteral(node) => node.range(),
            AnyNodeRef::ExprBooleanLiteral(node) => node.range(),
            AnyNodeRef::ExprNoneLiteral(node) => node.range(),
            AnyNodeRef::ExprEllipsisLiteral(node) => node.range(),
            AnyNodeRef::ExprAttribute(node) => node.range(),
            AnyNodeRef::ExprSubscript(node) => node.range(),
            AnyNodeRef::ExprStarred(node) => node.range(),
            AnyNodeRef::ExprName(node) => node.range(),
            AnyNodeRef::ExprList(node) => node.range(),
            AnyNodeRef::ExprTuple(node) => node.range(),
            AnyNodeRef::ExprSlice(node) => node.range(),
            AnyNodeRef::ExprIpyEscapeCommand(node) => node.range(),
            AnyNodeRef::ExceptHandlerExceptHandler(node) => node.range(),
            AnyNodeRef::FStringExpressionElement(node) => node.range(),
            AnyNodeRef::FStringLiteralElement(node) => node.range(),
            AnyNodeRef::PatternMatchValue(node) => node.range(),
            AnyNodeRef::PatternMatchSingleton(node) => node.range(),
            AnyNodeRef::PatternMatchSequence(node) => node.range(),
            AnyNodeRef::PatternMatchMapping(node) => node.range(),
            AnyNodeRef::PatternMatchClass(node) => node.range(),
            AnyNodeRef::PatternMatchStar(node) => node.range(),
            AnyNodeRef::PatternMatchAs(node) => node.range(),
            AnyNodeRef::PatternMatchOr(node) => node.range(),
            AnyNodeRef::TypeParamTypeVar(node) => node.range(),
            AnyNodeRef::TypeParamTypeVarTuple(node) => node.range(),
            AnyNodeRef::TypeParamParamSpec(node) => node.range(),
            AnyNodeRef::FStringFormatSpec(node) => node.range(),
            AnyNodeRef::PatternArguments(node) => node.range(),
            AnyNodeRef::PatternKeyword(node) => node.range(),
            AnyNodeRef::Comprehension(node) => node.range(),
            AnyNodeRef::Arguments(node) => node.range(),
            AnyNodeRef::Parameters(node) => node.range(),
            AnyNodeRef::Parameter(node) => node.range(),
            AnyNodeRef::ParameterWithDefault(node) => node.range(),
            AnyNodeRef::Keyword(node) => node.range(),
            AnyNodeRef::Alias(node) => node.range(),
            AnyNodeRef::WithItem(node) => node.range(),
            AnyNodeRef::MatchCase(node) => node.range(),
            AnyNodeRef::Decorator(node) => node.range(),
            AnyNodeRef::ElifElseClause(node) => node.range(),
            AnyNodeRef::TypeParams(node) => node.range(),
            AnyNodeRef::FString(node) => node.range(),
            AnyNodeRef::StringLiteral(node) => node.range(),
            AnyNodeRef::BytesLiteral(node) => node.range(),
            AnyNodeRef::Identifier(node) => node.range(),
        }
    }
}

impl AnyNodeRef<'_> {
    pub fn as_ptr(&self) -> std::ptr::NonNull<()> {
        match self {
            AnyNodeRef::ModModule(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ModExpression(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::StmtFunctionDef(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::StmtClassDef(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::StmtReturn(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::StmtDelete(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::StmtTypeAlias(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::StmtAssign(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::StmtAugAssign(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::StmtAnnAssign(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::StmtFor(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::StmtWhile(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::StmtIf(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::StmtWith(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::StmtMatch(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::StmtRaise(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::StmtTry(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::StmtAssert(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::StmtImport(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::StmtImportFrom(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::StmtGlobal(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::StmtNonlocal(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::StmtExpr(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::StmtPass(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::StmtBreak(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::StmtContinue(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::StmtIpyEscapeCommand(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprBoolOp(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprNamed(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprBinOp(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprUnaryOp(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprLambda(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprIf(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprDict(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprSet(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprListComp(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprSetComp(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprDictComp(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprGenerator(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprAwait(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprYield(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprYieldFrom(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprCompare(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprCall(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprFString(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprStringLiteral(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprBytesLiteral(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprNumberLiteral(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprBooleanLiteral(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprNoneLiteral(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprEllipsisLiteral(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprAttribute(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprSubscript(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprStarred(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprName(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprList(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprTuple(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprSlice(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExprIpyEscapeCommand(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ExceptHandlerExceptHandler(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::FStringExpressionElement(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::FStringLiteralElement(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::PatternMatchValue(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::PatternMatchSingleton(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::PatternMatchSequence(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::PatternMatchMapping(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::PatternMatchClass(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::PatternMatchStar(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::PatternMatchAs(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::PatternMatchOr(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::TypeParamTypeVar(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::TypeParamTypeVarTuple(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::TypeParamParamSpec(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::FStringFormatSpec(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::PatternArguments(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::PatternKeyword(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::Comprehension(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::Arguments(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::Parameters(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::Parameter(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ParameterWithDefault(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::Keyword(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::Alias(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::WithItem(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::MatchCase(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::Decorator(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::ElifElseClause(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::TypeParams(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::FString(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::StringLiteral(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::BytesLiteral(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::Identifier(node) => std::ptr::NonNull::from(*node).cast(),
        }
    }
}

impl<'a> AnyNodeRef<'a> {
    pub fn visit_preorder<'b, V>(self, visitor: &mut V)
    where
        V: crate::visitor::source_order::SourceOrderVisitor<'b> + ?Sized,
        'a: 'b,
    {
        match self {
            AnyNodeRef::ModModule(node) => node.visit_source_order(visitor),
            AnyNodeRef::ModExpression(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtFunctionDef(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtClassDef(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtReturn(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtDelete(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtTypeAlias(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtAssign(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtAugAssign(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtAnnAssign(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtFor(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtWhile(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtIf(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtWith(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtMatch(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtRaise(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtTry(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtAssert(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtImport(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtImportFrom(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtGlobal(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtNonlocal(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtExpr(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtPass(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtBreak(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtContinue(node) => node.visit_source_order(visitor),
            AnyNodeRef::StmtIpyEscapeCommand(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprBoolOp(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprNamed(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprBinOp(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprUnaryOp(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprLambda(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprIf(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprDict(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprSet(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprListComp(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprSetComp(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprDictComp(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprGenerator(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprAwait(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprYield(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprYieldFrom(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprCompare(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprCall(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprFString(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprStringLiteral(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprBytesLiteral(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprNumberLiteral(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprBooleanLiteral(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprNoneLiteral(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprEllipsisLiteral(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprAttribute(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprSubscript(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprStarred(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprName(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprList(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprTuple(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprSlice(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExprIpyEscapeCommand(node) => node.visit_source_order(visitor),
            AnyNodeRef::ExceptHandlerExceptHandler(node) => node.visit_source_order(visitor),
            AnyNodeRef::FStringExpressionElement(node) => node.visit_source_order(visitor),
            AnyNodeRef::FStringLiteralElement(node) => node.visit_source_order(visitor),
            AnyNodeRef::PatternMatchValue(node) => node.visit_source_order(visitor),
            AnyNodeRef::PatternMatchSingleton(node) => node.visit_source_order(visitor),
            AnyNodeRef::PatternMatchSequence(node) => node.visit_source_order(visitor),
            AnyNodeRef::PatternMatchMapping(node) => node.visit_source_order(visitor),
            AnyNodeRef::PatternMatchClass(node) => node.visit_source_order(visitor),
            AnyNodeRef::PatternMatchStar(node) => node.visit_source_order(visitor),
            AnyNodeRef::PatternMatchAs(node) => node.visit_source_order(visitor),
            AnyNodeRef::PatternMatchOr(node) => node.visit_source_order(visitor),
            AnyNodeRef::TypeParamTypeVar(node) => node.visit_source_order(visitor),
            AnyNodeRef::TypeParamTypeVarTuple(node) => node.visit_source_order(visitor),
            AnyNodeRef::TypeParamParamSpec(node) => node.visit_source_order(visitor),
            AnyNodeRef::FStringFormatSpec(node) => node.visit_source_order(visitor),
            AnyNodeRef::PatternArguments(node) => node.visit_source_order(visitor),
            AnyNodeRef::PatternKeyword(node) => node.visit_source_order(visitor),
            AnyNodeRef::Comprehension(node) => node.visit_source_order(visitor),
            AnyNodeRef::Arguments(node) => node.visit_source_order(visitor),
            AnyNodeRef::Parameters(node) => node.visit_source_order(visitor),
            AnyNodeRef::Parameter(node) => node.visit_source_order(visitor),
            AnyNodeRef::ParameterWithDefault(node) => node.visit_source_order(visitor),
            AnyNodeRef::Keyword(node) => node.visit_source_order(visitor),
            AnyNodeRef::Alias(node) => node.visit_source_order(visitor),
            AnyNodeRef::WithItem(node) => node.visit_source_order(visitor),
            AnyNodeRef::MatchCase(node) => node.visit_source_order(visitor),
            AnyNodeRef::Decorator(node) => node.visit_source_order(visitor),
            AnyNodeRef::ElifElseClause(node) => node.visit_source_order(visitor),
            AnyNodeRef::TypeParams(node) => node.visit_source_order(visitor),
            AnyNodeRef::FString(node) => node.visit_source_order(visitor),
            AnyNodeRef::StringLiteral(node) => node.visit_source_order(visitor),
            AnyNodeRef::BytesLiteral(node) => node.visit_source_order(visitor),
            AnyNodeRef::Identifier(node) => node.visit_source_order(visitor),
        }
    }
}

impl AnyNodeRef<'_> {
    pub const fn is_module(self) -> bool {
        matches!(
            self,
            AnyNodeRef::ModModule(_) | AnyNodeRef::ModExpression(_)
        )
    }
}

impl AnyNodeRef<'_> {
    pub const fn is_statement(self) -> bool {
        matches!(
            self,
            AnyNodeRef::StmtFunctionDef(_)
                | AnyNodeRef::StmtClassDef(_)
                | AnyNodeRef::StmtReturn(_)
                | AnyNodeRef::StmtDelete(_)
                | AnyNodeRef::StmtTypeAlias(_)
                | AnyNodeRef::StmtAssign(_)
                | AnyNodeRef::StmtAugAssign(_)
                | AnyNodeRef::StmtAnnAssign(_)
                | AnyNodeRef::StmtFor(_)
                | AnyNodeRef::StmtWhile(_)
                | AnyNodeRef::StmtIf(_)
                | AnyNodeRef::StmtWith(_)
                | AnyNodeRef::StmtMatch(_)
                | AnyNodeRef::StmtRaise(_)
                | AnyNodeRef::StmtTry(_)
                | AnyNodeRef::StmtAssert(_)
                | AnyNodeRef::StmtImport(_)
                | AnyNodeRef::StmtImportFrom(_)
                | AnyNodeRef::StmtGlobal(_)
                | AnyNodeRef::StmtNonlocal(_)
                | AnyNodeRef::StmtExpr(_)
                | AnyNodeRef::StmtPass(_)
                | AnyNodeRef::StmtBreak(_)
                | AnyNodeRef::StmtContinue(_)
                | AnyNodeRef::StmtIpyEscapeCommand(_)
        )
    }
}

impl AnyNodeRef<'_> {
    pub const fn is_expression(self) -> bool {
        matches!(
            self,
            AnyNodeRef::ExprBoolOp(_)
                | AnyNodeRef::ExprNamed(_)
                | AnyNodeRef::ExprBinOp(_)
                | AnyNodeRef::ExprUnaryOp(_)
                | AnyNodeRef::ExprLambda(_)
                | AnyNodeRef::ExprIf(_)
                | AnyNodeRef::ExprDict(_)
                | AnyNodeRef::ExprSet(_)
                | AnyNodeRef::ExprListComp(_)
                | AnyNodeRef::ExprSetComp(_)
                | AnyNodeRef::ExprDictComp(_)
                | AnyNodeRef::ExprGenerator(_)
                | AnyNodeRef::ExprAwait(_)
                | AnyNodeRef::ExprYield(_)
                | AnyNodeRef::ExprYieldFrom(_)
                | AnyNodeRef::ExprCompare(_)
                | AnyNodeRef::ExprCall(_)
                | AnyNodeRef::ExprFString(_)
                | AnyNodeRef::ExprStringLiteral(_)
                | AnyNodeRef::ExprBytesLiteral(_)
                | AnyNodeRef::ExprNumberLiteral(_)
                | AnyNodeRef::ExprBooleanLiteral(_)
                | AnyNodeRef::ExprNoneLiteral(_)
                | AnyNodeRef::ExprEllipsisLiteral(_)
                | AnyNodeRef::ExprAttribute(_)
                | AnyNodeRef::ExprSubscript(_)
                | AnyNodeRef::ExprStarred(_)
                | AnyNodeRef::ExprName(_)
                | AnyNodeRef::ExprList(_)
                | AnyNodeRef::ExprTuple(_)
                | AnyNodeRef::ExprSlice(_)
                | AnyNodeRef::ExprIpyEscapeCommand(_)
        )
    }
}

impl AnyNodeRef<'_> {
    pub const fn is_except_handler(self) -> bool {
        matches!(self, AnyNodeRef::ExceptHandlerExceptHandler(_))
    }
}

impl AnyNodeRef<'_> {
    pub const fn is_f_string_element(self) -> bool {
        matches!(
            self,
            AnyNodeRef::FStringExpressionElement(_) | AnyNodeRef::FStringLiteralElement(_)
        )
    }
}

impl AnyNodeRef<'_> {
    pub const fn is_pattern(self) -> bool {
        matches!(
            self,
            AnyNodeRef::PatternMatchValue(_)
                | AnyNodeRef::PatternMatchSingleton(_)
                | AnyNodeRef::PatternMatchSequence(_)
                | AnyNodeRef::PatternMatchMapping(_)
                | AnyNodeRef::PatternMatchClass(_)
                | AnyNodeRef::PatternMatchStar(_)
                | AnyNodeRef::PatternMatchAs(_)
                | AnyNodeRef::PatternMatchOr(_)
        )
    }
}

impl AnyNodeRef<'_> {
    pub const fn is_type_param(self) -> bool {
        matches!(
            self,
            AnyNodeRef::TypeParamTypeVar(_)
                | AnyNodeRef::TypeParamTypeVarTuple(_)
                | AnyNodeRef::TypeParamParamSpec(_)
        )
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum NodeKind {
    ModModule,
    ModExpression,
    StmtFunctionDef,
    StmtClassDef,
    StmtReturn,
    StmtDelete,
    StmtTypeAlias,
    StmtAssign,
    StmtAugAssign,
    StmtAnnAssign,
    StmtFor,
    StmtWhile,
    StmtIf,
    StmtWith,
    StmtMatch,
    StmtRaise,
    StmtTry,
    StmtAssert,
    StmtImport,
    StmtImportFrom,
    StmtGlobal,
    StmtNonlocal,
    StmtExpr,
    StmtPass,
    StmtBreak,
    StmtContinue,
    StmtIpyEscapeCommand,
    ExprBoolOp,
    ExprNamed,
    ExprBinOp,
    ExprUnaryOp,
    ExprLambda,
    ExprIf,
    ExprDict,
    ExprSet,
    ExprListComp,
    ExprSetComp,
    ExprDictComp,
    ExprGenerator,
    ExprAwait,
    ExprYield,
    ExprYieldFrom,
    ExprCompare,
    ExprCall,
    ExprFString,
    ExprStringLiteral,
    ExprBytesLiteral,
    ExprNumberLiteral,
    ExprBooleanLiteral,
    ExprNoneLiteral,
    ExprEllipsisLiteral,
    ExprAttribute,
    ExprSubscript,
    ExprStarred,
    ExprName,
    ExprList,
    ExprTuple,
    ExprSlice,
    ExprIpyEscapeCommand,
    ExceptHandlerExceptHandler,
    FStringExpressionElement,
    FStringLiteralElement,
    PatternMatchValue,
    PatternMatchSingleton,
    PatternMatchSequence,
    PatternMatchMapping,
    PatternMatchClass,
    PatternMatchStar,
    PatternMatchAs,
    PatternMatchOr,
    TypeParamTypeVar,
    TypeParamTypeVarTuple,
    TypeParamParamSpec,
    FStringFormatSpec,
    PatternArguments,
    PatternKeyword,
    Comprehension,
    Arguments,
    Parameters,
    Parameter,
    ParameterWithDefault,
    Keyword,
    Alias,
    WithItem,
    MatchCase,
    Decorator,
    ElifElseClause,
    TypeParams,
    FString,
    StringLiteral,
    BytesLiteral,
    Identifier,
}

impl AnyNodeRef<'_> {
    pub const fn kind(self) -> NodeKind {
        match self {
            AnyNodeRef::ModModule(_) => NodeKind::ModModule,
            AnyNodeRef::ModExpression(_) => NodeKind::ModExpression,
            AnyNodeRef::StmtFunctionDef(_) => NodeKind::StmtFunctionDef,
            AnyNodeRef::StmtClassDef(_) => NodeKind::StmtClassDef,
            AnyNodeRef::StmtReturn(_) => NodeKind::StmtReturn,
            AnyNodeRef::StmtDelete(_) => NodeKind::StmtDelete,
            AnyNodeRef::StmtTypeAlias(_) => NodeKind::StmtTypeAlias,
            AnyNodeRef::StmtAssign(_) => NodeKind::StmtAssign,
            AnyNodeRef::StmtAugAssign(_) => NodeKind::StmtAugAssign,
            AnyNodeRef::StmtAnnAssign(_) => NodeKind::StmtAnnAssign,
            AnyNodeRef::StmtFor(_) => NodeKind::StmtFor,
            AnyNodeRef::StmtWhile(_) => NodeKind::StmtWhile,
            AnyNodeRef::StmtIf(_) => NodeKind::StmtIf,
            AnyNodeRef::StmtWith(_) => NodeKind::StmtWith,
            AnyNodeRef::StmtMatch(_) => NodeKind::StmtMatch,
            AnyNodeRef::StmtRaise(_) => NodeKind::StmtRaise,
            AnyNodeRef::StmtTry(_) => NodeKind::StmtTry,
            AnyNodeRef::StmtAssert(_) => NodeKind::StmtAssert,
            AnyNodeRef::StmtImport(_) => NodeKind::StmtImport,
            AnyNodeRef::StmtImportFrom(_) => NodeKind::StmtImportFrom,
            AnyNodeRef::StmtGlobal(_) => NodeKind::StmtGlobal,
            AnyNodeRef::StmtNonlocal(_) => NodeKind::StmtNonlocal,
            AnyNodeRef::StmtExpr(_) => NodeKind::StmtExpr,
            AnyNodeRef::StmtPass(_) => NodeKind::StmtPass,
            AnyNodeRef::StmtBreak(_) => NodeKind::StmtBreak,
            AnyNodeRef::StmtContinue(_) => NodeKind::StmtContinue,
            AnyNodeRef::StmtIpyEscapeCommand(_) => NodeKind::StmtIpyEscapeCommand,
            AnyNodeRef::ExprBoolOp(_) => NodeKind::ExprBoolOp,
            AnyNodeRef::ExprNamed(_) => NodeKind::ExprNamed,
            AnyNodeRef::ExprBinOp(_) => NodeKind::ExprBinOp,
            AnyNodeRef::ExprUnaryOp(_) => NodeKind::ExprUnaryOp,
            AnyNodeRef::ExprLambda(_) => NodeKind::ExprLambda,
            AnyNodeRef::ExprIf(_) => NodeKind::ExprIf,
            AnyNodeRef::ExprDict(_) => NodeKind::ExprDict,
            AnyNodeRef::ExprSet(_) => NodeKind::ExprSet,
            AnyNodeRef::ExprListComp(_) => NodeKind::ExprListComp,
            AnyNodeRef::ExprSetComp(_) => NodeKind::ExprSetComp,
            AnyNodeRef::ExprDictComp(_) => NodeKind::ExprDictComp,
            AnyNodeRef::ExprGenerator(_) => NodeKind::ExprGenerator,
            AnyNodeRef::ExprAwait(_) => NodeKind::ExprAwait,
            AnyNodeRef::ExprYield(_) => NodeKind::ExprYield,
            AnyNodeRef::ExprYieldFrom(_) => NodeKind::ExprYieldFrom,
            AnyNodeRef::ExprCompare(_) => NodeKind::ExprCompare,
            AnyNodeRef::ExprCall(_) => NodeKind::ExprCall,
            AnyNodeRef::ExprFString(_) => NodeKind::ExprFString,
            AnyNodeRef::ExprStringLiteral(_) => NodeKind::ExprStringLiteral,
            AnyNodeRef::ExprBytesLiteral(_) => NodeKind::ExprBytesLiteral,
            AnyNodeRef::ExprNumberLiteral(_) => NodeKind::ExprNumberLiteral,
            AnyNodeRef::ExprBooleanLiteral(_) => NodeKind::ExprBooleanLiteral,
            AnyNodeRef::ExprNoneLiteral(_) => NodeKind::ExprNoneLiteral,
            AnyNodeRef::ExprEllipsisLiteral(_) => NodeKind::ExprEllipsisLiteral,
            AnyNodeRef::ExprAttribute(_) => NodeKind::ExprAttribute,
            AnyNodeRef::ExprSubscript(_) => NodeKind::ExprSubscript,
            AnyNodeRef::ExprStarred(_) => NodeKind::ExprStarred,
            AnyNodeRef::ExprName(_) => NodeKind::ExprName,
            AnyNodeRef::ExprList(_) => NodeKind::ExprList,
            AnyNodeRef::ExprTuple(_) => NodeKind::ExprTuple,
            AnyNodeRef::ExprSlice(_) => NodeKind::ExprSlice,
            AnyNodeRef::ExprIpyEscapeCommand(_) => NodeKind::ExprIpyEscapeCommand,
            AnyNodeRef::ExceptHandlerExceptHandler(_) => NodeKind::ExceptHandlerExceptHandler,
            AnyNodeRef::FStringExpressionElement(_) => NodeKind::FStringExpressionElement,
            AnyNodeRef::FStringLiteralElement(_) => NodeKind::FStringLiteralElement,
            AnyNodeRef::PatternMatchValue(_) => NodeKind::PatternMatchValue,
            AnyNodeRef::PatternMatchSingleton(_) => NodeKind::PatternMatchSingleton,
            AnyNodeRef::PatternMatchSequence(_) => NodeKind::PatternMatchSequence,
            AnyNodeRef::PatternMatchMapping(_) => NodeKind::PatternMatchMapping,
            AnyNodeRef::PatternMatchClass(_) => NodeKind::PatternMatchClass,
            AnyNodeRef::PatternMatchStar(_) => NodeKind::PatternMatchStar,
            AnyNodeRef::PatternMatchAs(_) => NodeKind::PatternMatchAs,
            AnyNodeRef::PatternMatchOr(_) => NodeKind::PatternMatchOr,
            AnyNodeRef::TypeParamTypeVar(_) => NodeKind::TypeParamTypeVar,
            AnyNodeRef::TypeParamTypeVarTuple(_) => NodeKind::TypeParamTypeVarTuple,
            AnyNodeRef::TypeParamParamSpec(_) => NodeKind::TypeParamParamSpec,
            AnyNodeRef::FStringFormatSpec(_) => NodeKind::FStringFormatSpec,
            AnyNodeRef::PatternArguments(_) => NodeKind::PatternArguments,
            AnyNodeRef::PatternKeyword(_) => NodeKind::PatternKeyword,
            AnyNodeRef::Comprehension(_) => NodeKind::Comprehension,
            AnyNodeRef::Arguments(_) => NodeKind::Arguments,
            AnyNodeRef::Parameters(_) => NodeKind::Parameters,
            AnyNodeRef::Parameter(_) => NodeKind::Parameter,
            AnyNodeRef::ParameterWithDefault(_) => NodeKind::ParameterWithDefault,
            AnyNodeRef::Keyword(_) => NodeKind::Keyword,
            AnyNodeRef::Alias(_) => NodeKind::Alias,
            AnyNodeRef::WithItem(_) => NodeKind::WithItem,
            AnyNodeRef::MatchCase(_) => NodeKind::MatchCase,
            AnyNodeRef::Decorator(_) => NodeKind::Decorator,
            AnyNodeRef::ElifElseClause(_) => NodeKind::ElifElseClause,
            AnyNodeRef::TypeParams(_) => NodeKind::TypeParams,
            AnyNodeRef::FString(_) => NodeKind::FString,
            AnyNodeRef::StringLiteral(_) => NodeKind::StringLiteral,
            AnyNodeRef::BytesLiteral(_) => NodeKind::BytesLiteral,
            AnyNodeRef::Identifier(_) => NodeKind::Identifier,
        }
    }
}

/// See also [FunctionDef](https://docs.python.org/3/library/ast.html#ast.FunctionDef)
/// and [AsyncFunctionDef](https://docs.python.org/3/library/ast.html#ast.AsyncFunctionDef).
///
/// This type differs from the original Python AST, as it collapses the synchronous and asynchronous variants into a single type.
#[derive(Clone, Debug, PartialEq)]
pub struct StmtFunctionDef {
    pub range: ruff_text_size::TextRange,
    pub is_async: bool,
    pub decorator_list: Vec<crate::Decorator>,
    pub name: crate::Identifier,
    pub type_params: Option<Box<crate::TypeParams>>,
    pub parameters: Box<crate::Parameters>,
    pub returns: Option<Box<Expr>>,
    pub body: Vec<Stmt>,
}

/// See also [ClassDef](https://docs.python.org/3/library/ast.html#ast.ClassDef)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtClassDef {
    pub range: ruff_text_size::TextRange,
    pub decorator_list: Vec<crate::Decorator>,
    pub name: crate::Identifier,
    pub type_params: Option<Box<crate::TypeParams>>,
    pub arguments: Option<Box<crate::Arguments>>,
    pub body: Vec<Stmt>,
}

/// See also [Return](https://docs.python.org/3/library/ast.html#ast.Return)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtReturn {
    pub range: ruff_text_size::TextRange,
    pub value: Option<Box<Expr>>,
}

/// See also [Delete](https://docs.python.org/3/library/ast.html#ast.Delete)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtDelete {
    pub range: ruff_text_size::TextRange,
    pub targets: Vec<Expr>,
}

/// See also [TypeAlias](https://docs.python.org/3/library/ast.html#ast.TypeAlias)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtTypeAlias {
    pub range: ruff_text_size::TextRange,
    pub name: Box<Expr>,
    pub type_params: Option<crate::TypeParams>,
    pub value: Box<Expr>,
}

/// See also [Assign](https://docs.python.org/3/library/ast.html#ast.Assign)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtAssign {
    pub range: ruff_text_size::TextRange,
    pub targets: Vec<Expr>,
    pub value: Box<Expr>,
}

/// See also [AugAssign](https://docs.python.org/3/library/ast.html#ast.AugAssign)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtAugAssign {
    pub range: ruff_text_size::TextRange,
    pub target: Box<Expr>,
    pub op: crate::Operator,
    pub value: Box<Expr>,
}

/// See also [AnnAssign](https://docs.python.org/3/library/ast.html#ast.AnnAssign)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtAnnAssign {
    pub range: ruff_text_size::TextRange,
    pub target: Box<Expr>,
    pub annotation: Box<Expr>,
    pub value: Option<Box<Expr>>,
    pub simple: bool,
}

/// See also [For](https://docs.python.org/3/library/ast.html#ast.For)
/// and [AsyncFor](https://docs.python.org/3/library/ast.html#ast.AsyncFor).
///
/// This type differs from the original Python AST, as it collapses the synchronous and asynchronous variants into a single type.
#[derive(Clone, Debug, PartialEq)]
pub struct StmtFor {
    pub range: ruff_text_size::TextRange,
    pub is_async: bool,
    pub target: Box<Expr>,
    pub iter: Box<Expr>,
    pub body: Vec<Stmt>,
    pub orelse: Vec<Stmt>,
}

/// See also [While](https://docs.python.org/3/library/ast.html#ast.While)
/// and [AsyncWhile](https://docs.python.org/3/library/ast.html#ast.AsyncWhile).
#[derive(Clone, Debug, PartialEq)]
pub struct StmtWhile {
    pub range: ruff_text_size::TextRange,
    pub test: Box<Expr>,
    pub body: Vec<Stmt>,
    pub orelse: Vec<Stmt>,
}

/// See also [If](https://docs.python.org/3/library/ast.html#ast.If)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtIf {
    pub range: ruff_text_size::TextRange,
    pub test: Box<Expr>,
    pub body: Vec<Stmt>,
    pub elif_else_clauses: Vec<crate::ElifElseClause>,
}

/// See also [With](https://docs.python.org/3/library/ast.html#ast.With)
/// and [AsyncWith](https://docs.python.org/3/library/ast.html#ast.AsyncWith).
///
/// This type differs from the original Python AST, as it collapses the synchronous and asynchronous variants into a single type.
#[derive(Clone, Debug, PartialEq)]
pub struct StmtWith {
    pub range: ruff_text_size::TextRange,
    pub is_async: bool,
    pub items: Vec<crate::WithItem>,
    pub body: Vec<Stmt>,
}

/// See also [Match](https://docs.python.org/3/library/ast.html#ast.Match)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtMatch {
    pub range: ruff_text_size::TextRange,
    pub subject: Box<Expr>,
    pub cases: Vec<crate::MatchCase>,
}

/// See also [Raise](https://docs.python.org/3/library/ast.html#ast.Raise)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtRaise {
    pub range: ruff_text_size::TextRange,
    pub exc: Option<Box<Expr>>,
    pub cause: Option<Box<Expr>>,
}

/// See also [Try](https://docs.python.org/3/library/ast.html#ast.Try)
/// and [TryStar](https://docs.python.org/3/library/ast.html#ast.TryStar)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtTry {
    pub range: ruff_text_size::TextRange,
    pub body: Vec<Stmt>,
    pub handlers: Vec<ExceptHandler>,
    pub orelse: Vec<Stmt>,
    pub finalbody: Vec<Stmt>,
    pub is_star: bool,
}

/// See also [Assert](https://docs.python.org/3/library/ast.html#ast.Assert)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtAssert {
    pub range: ruff_text_size::TextRange,
    pub test: Box<Expr>,
    pub msg: Option<Box<Expr>>,
}

/// See also [Import](https://docs.python.org/3/library/ast.html#ast.Import)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtImport {
    pub range: ruff_text_size::TextRange,
    pub names: Vec<crate::Alias>,
}

/// See also [ImportFrom](https://docs.python.org/3/library/ast.html#ast.ImportFrom)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtImportFrom {
    pub range: ruff_text_size::TextRange,
    pub module: Option<crate::Identifier>,
    pub names: Vec<crate::Alias>,
    pub level: u32,
}

/// See also [Global](https://docs.python.org/3/library/ast.html#ast.Global)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtGlobal {
    pub range: ruff_text_size::TextRange,
    pub names: Vec<crate::Identifier>,
}

/// See also [Nonlocal](https://docs.python.org/3/library/ast.html#ast.Nonlocal)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtNonlocal {
    pub range: ruff_text_size::TextRange,
    pub names: Vec<crate::Identifier>,
}

/// See also [Expr](https://docs.python.org/3/library/ast.html#ast.Expr)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtExpr {
    pub range: ruff_text_size::TextRange,
    pub value: Box<Expr>,
}

/// See also [Pass](https://docs.python.org/3/library/ast.html#ast.Pass)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtPass {
    pub range: ruff_text_size::TextRange,
}

/// See also [Break](https://docs.python.org/3/library/ast.html#ast.Break)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtBreak {
    pub range: ruff_text_size::TextRange,
}

/// See also [Continue](https://docs.python.org/3/library/ast.html#ast.Continue)
#[derive(Clone, Debug, PartialEq)]
pub struct StmtContinue {
    pub range: ruff_text_size::TextRange,
}

/// An AST node used to represent a IPython escape command at the statement level.
///
/// For example,
/// ```python
/// %matplotlib inline
/// ```
///
/// ## Terminology
///
/// Escape commands are special IPython syntax which starts with a token to identify
/// the escape kind followed by the command value itself. [Escape kind] are the kind
/// of escape commands that are recognized by the token: `%`, `%%`, `!`, `!!`,
/// `?`, `??`, `/`, `;`, and `,`.
///
/// Help command (or Dynamic Object Introspection as it's called) are the escape commands
/// of the kind `?` and `??`. For example, `?str.replace`. Help end command are a subset
/// of Help command where the token can be at the end of the line i.e., after the value.
/// For example, `str.replace?`.
///
/// Here's where things get tricky. I'll divide the help end command into two types for
/// better understanding:
/// 1. Strict version: The token is _only_ at the end of the line. For example,
///    `str.replace?` or `str.replace??`.
/// 2. Combined version: Along with the `?` or `??` token, which are at the end of the
///    line, there are other escape kind tokens that are present at the start as well.
///    For example, `%matplotlib?` or `%%timeit?`.
///
/// Priority comes into picture for the "Combined version" mentioned above. How do
/// we determine the escape kind if there are tokens on both side of the value, i.e., which
/// token to choose? The Help end command always takes priority over any other token which
/// means that if there is `?`/`??` at the end then that is used to determine the kind.
/// For example, in `%matplotlib?` the escape kind is determined using the `?` token
/// instead of `%` token.
///
/// ## Syntax
///
/// `<IpyEscapeKind><Command value>`
///
/// The simplest form is an escape kind token followed by the command value. For example,
/// `%matplotlib inline`, `/foo`, `!pwd`, etc.
///
/// `<Command value><IpyEscapeKind ("?" or "??")>`
///
/// The help end escape command would be the reverse of the above syntax. Here, the
/// escape kind token can only be either `?` or `??` and it is at the end of the line.
/// For example, `str.replace?`, `math.pi??`, etc.
///
/// `<IpyEscapeKind><Command value><EscapeKind ("?" or "??")>`
///
/// The final syntax is the combined version of the above two. For example, `%matplotlib?`,
/// `%%timeit??`, etc.
///
/// [Escape kind]: crate::IpyEscapeKind
///
#[derive(Clone, Debug, PartialEq)]
pub struct StmtIpyEscapeCommand {
    pub range: ruff_text_size::TextRange,
    pub kind: crate::IpyEscapeKind,
    pub value: Box<str>,
}

/// See also [BoolOp](https://docs.python.org/3/library/ast.html#ast.BoolOp)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprBoolOp {
    pub range: ruff_text_size::TextRange,
    pub op: crate::BoolOp,
    pub values: Vec<Expr>,
}

/// See also [NamedExpr](https://docs.python.org/3/library/ast.html#ast.NamedExpr)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprNamed {
    pub range: ruff_text_size::TextRange,
    pub target: Box<Expr>,
    pub value: Box<Expr>,
}

/// See also [BinOp](https://docs.python.org/3/library/ast.html#ast.BinOp)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprBinOp {
    pub range: ruff_text_size::TextRange,
    pub left: Box<Expr>,
    pub op: crate::Operator,
    pub right: Box<Expr>,
}

/// See also [UnaryOp](https://docs.python.org/3/library/ast.html#ast.UnaryOp)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprUnaryOp {
    pub range: ruff_text_size::TextRange,
    pub op: crate::UnaryOp,
    pub operand: Box<Expr>,
}

/// See also [Lambda](https://docs.python.org/3/library/ast.html#ast.Lambda)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprLambda {
    pub range: ruff_text_size::TextRange,
    pub parameters: Option<Box<crate::Parameters>>,
    pub body: Box<Expr>,
}

/// See also [IfExp](https://docs.python.org/3/library/ast.html#ast.IfExp)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprIf {
    pub range: ruff_text_size::TextRange,
    pub test: Box<Expr>,
    pub body: Box<Expr>,
    pub orelse: Box<Expr>,
}

/// See also [Dict](https://docs.python.org/3/library/ast.html#ast.Dict)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprDict {
    pub range: ruff_text_size::TextRange,
    pub items: Vec<crate::DictItem>,
}

/// See also [Set](https://docs.python.org/3/library/ast.html#ast.Set)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprSet {
    pub range: ruff_text_size::TextRange,
    pub elts: Vec<Expr>,
}

/// See also [ListComp](https://docs.python.org/3/library/ast.html#ast.ListComp)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprListComp {
    pub range: ruff_text_size::TextRange,
    pub elt: Box<Expr>,
    pub generators: Vec<crate::Comprehension>,
}

/// See also [SetComp](https://docs.python.org/3/library/ast.html#ast.SetComp)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprSetComp {
    pub range: ruff_text_size::TextRange,
    pub elt: Box<Expr>,
    pub generators: Vec<crate::Comprehension>,
}

/// See also [DictComp](https://docs.python.org/3/library/ast.html#ast.DictComp)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprDictComp {
    pub range: ruff_text_size::TextRange,
    pub key: Box<Expr>,
    pub value: Box<Expr>,
    pub generators: Vec<crate::Comprehension>,
}

/// See also [GeneratorExp](https://docs.python.org/3/library/ast.html#ast.GeneratorExp)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprGenerator {
    pub range: ruff_text_size::TextRange,
    pub elt: Box<Expr>,
    pub generators: Vec<crate::Comprehension>,
    pub parenthesized: bool,
}

/// See also [Await](https://docs.python.org/3/library/ast.html#ast.Await)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprAwait {
    pub range: ruff_text_size::TextRange,
    pub value: Box<Expr>,
}

/// See also [Yield](https://docs.python.org/3/library/ast.html#ast.Yield)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprYield {
    pub range: ruff_text_size::TextRange,
    pub value: Option<Box<Expr>>,
}

/// See also [YieldFrom](https://docs.python.org/3/library/ast.html#ast.YieldFrom)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprYieldFrom {
    pub range: ruff_text_size::TextRange,
    pub value: Box<Expr>,
}

/// See also [Compare](https://docs.python.org/3/library/ast.html#ast.Compare)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprCompare {
    pub range: ruff_text_size::TextRange,
    pub left: Box<Expr>,
    pub ops: Box<[crate::CmpOp]>,
    pub comparators: Box<[Expr]>,
}

/// See also [Call](https://docs.python.org/3/library/ast.html#ast.Call)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprCall {
    pub range: ruff_text_size::TextRange,
    pub func: Box<Expr>,
    pub arguments: crate::Arguments,
}

/// An AST node that represents either a single-part f-string literal
/// or an implicitly concatenated f-string literal.
///
/// This type differs from the original Python AST `JoinedStr` in that it
/// doesn't join the implicitly concatenated parts into a single string. Instead,
/// it keeps them separate and provide various methods to access the parts.
///
/// See also [JoinedStr](https://docs.python.org/3/library/ast.html#ast.JoinedStr)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprFString {
    pub range: ruff_text_size::TextRange,
    pub value: crate::FStringValue,
}

/// An AST node that represents either a single-part string literal
/// or an implicitly concatenated string literal.
#[derive(Clone, Debug, PartialEq)]
pub struct ExprStringLiteral {
    pub range: ruff_text_size::TextRange,
    pub value: crate::StringLiteralValue,
}

/// An AST node that represents either a single-part bytestring literal
/// or an implicitly concatenated bytestring literal.
#[derive(Clone, Debug, PartialEq)]
pub struct ExprBytesLiteral {
    pub range: ruff_text_size::TextRange,
    pub value: crate::BytesLiteralValue,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExprNumberLiteral {
    pub range: ruff_text_size::TextRange,
    pub value: crate::Number,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct ExprBooleanLiteral {
    pub range: ruff_text_size::TextRange,
    pub value: bool,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct ExprNoneLiteral {
    pub range: ruff_text_size::TextRange,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct ExprEllipsisLiteral {
    pub range: ruff_text_size::TextRange,
}

/// See also [Attribute](https://docs.python.org/3/library/ast.html#ast.Attribute)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprAttribute {
    pub range: ruff_text_size::TextRange,
    pub value: Box<Expr>,
    pub attr: crate::Identifier,
    pub ctx: crate::ExprContext,
}

/// See also [Subscript](https://docs.python.org/3/library/ast.html#ast.Subscript)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprSubscript {
    pub range: ruff_text_size::TextRange,
    pub value: Box<Expr>,
    pub slice: Box<Expr>,
    pub ctx: crate::ExprContext,
}

/// See also [Starred](https://docs.python.org/3/library/ast.html#ast.Starred)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprStarred {
    pub range: ruff_text_size::TextRange,
    pub value: Box<Expr>,
    pub ctx: crate::ExprContext,
}

/// See also [Name](https://docs.python.org/3/library/ast.html#ast.Name)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprName {
    pub range: ruff_text_size::TextRange,
    pub id: Name,
    pub ctx: crate::ExprContext,
}

/// See also [List](https://docs.python.org/3/library/ast.html#ast.List)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprList {
    pub range: ruff_text_size::TextRange,
    pub elts: Vec<Expr>,
    pub ctx: crate::ExprContext,
}

/// See also [Tuple](https://docs.python.org/3/library/ast.html#ast.Tuple)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprTuple {
    pub range: ruff_text_size::TextRange,
    pub elts: Vec<Expr>,
    pub ctx: crate::ExprContext,
    pub parenthesized: bool,
}

/// See also [Slice](https://docs.python.org/3/library/ast.html#ast.Slice)
#[derive(Clone, Debug, PartialEq)]
pub struct ExprSlice {
    pub range: ruff_text_size::TextRange,
    pub lower: Option<Box<Expr>>,
    pub upper: Option<Box<Expr>>,
    pub step: Option<Box<Expr>>,
}

/// An AST node used to represent a IPython escape command at the expression level.
///
/// For example,
/// ```python
/// dir = !pwd
/// ```
///
/// Here, the escape kind can only be `!` or `%` otherwise it is a syntax error.
///
/// For more information related to terminology and syntax of escape commands,
/// see [`StmtIpyEscapeCommand`].
#[derive(Clone, Debug, PartialEq)]
pub struct ExprIpyEscapeCommand {
    pub range: ruff_text_size::TextRange,
    pub kind: crate::IpyEscapeKind,
    pub value: Box<str>,
}
