// This is a generated file. Don't modify it by hand!
// Run `crates/ruff_python_ast/generate.py` to re-generate the file.

use crate::name::AstName;
use crate::visitor::source_order::SourceOrderVisitor;
use ruff_allocator::{Box as ArenaBox, Slice as ArenaSlice};

/// See also [mod](https://docs.python.org/3/library/ast.html#ast.mod)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub enum Mod<'ast> {
    Module(crate::ModModule<'ast>),
    Expression(crate::ModExpression<'ast>),
}

impl<'ast> From<crate::ModModule<'ast>> for Mod<'ast> {
    fn from(node: crate::ModModule<'ast>) -> Self {
        Self::Module(node)
    }
}

impl<'ast> From<crate::ModExpression<'ast>> for Mod<'ast> {
    fn from(node: crate::ModExpression<'ast>) -> Self {
        Self::Expression(node)
    }
}

impl ruff_text_size::Ranged for Mod<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        match self {
            Self::Module(node) => node.range(),
            Self::Expression(node) => node.range(),
        }
    }
}

impl crate::HasNodeIndex for Mod<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        match self {
            Self::Module(node) => node.node_index(),
            Self::Expression(node) => node.node_index(),
        }
    }
}

#[allow(dead_code, clippy::match_wildcard_for_single_variants)]
impl<'ast> Mod<'ast> {
    #[inline]
    pub const fn is_module(&self) -> bool {
        matches!(self, Self::Module(_))
    }

    #[inline]
    pub fn module(self) -> Option<crate::ModModule<'ast>> {
        match self {
            Self::Module(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_module(self) -> crate::ModModule<'ast> {
        match self {
            Self::Module(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_module_mut(&mut self) -> Option<&mut crate::ModModule<'ast>> {
        match self {
            Self::Module(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_module(&self) -> Option<&crate::ModModule<'ast>> {
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
    pub fn expression(self) -> Option<crate::ModExpression<'ast>> {
        match self {
            Self::Expression(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_expression(self) -> crate::ModExpression<'ast> {
        match self {
            Self::Expression(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_expression_mut(&mut self) -> Option<&mut crate::ModExpression<'ast>> {
        match self {
            Self::Expression(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_expression(&self) -> Option<&crate::ModExpression<'ast>> {
        match self {
            Self::Expression(val) => Some(val),
            _ => None,
        }
    }
}

/// See also [stmt](https://docs.python.org/3/library/ast.html#ast.stmt)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub enum Stmt<'ast> {
    FunctionDef(crate::StmtFunctionDef<'ast>),
    ClassDef(crate::StmtClassDef<'ast>),
    Return(crate::StmtReturn<'ast>),
    Delete(crate::StmtDelete<'ast>),
    TypeAlias(crate::StmtTypeAlias<'ast>),
    Assign(crate::StmtAssign<'ast>),
    AugAssign(crate::StmtAugAssign<'ast>),
    AnnAssign(crate::StmtAnnAssign<'ast>),
    For(crate::StmtFor<'ast>),
    While(crate::StmtWhile<'ast>),
    If(crate::StmtIf<'ast>),
    With(crate::StmtWith<'ast>),
    Match(crate::StmtMatch<'ast>),
    Raise(crate::StmtRaise<'ast>),
    Try(crate::StmtTry<'ast>),
    Assert(crate::StmtAssert<'ast>),
    Import(crate::StmtImport<'ast>),
    ImportFrom(crate::StmtImportFrom<'ast>),
    Global(crate::StmtGlobal<'ast>),
    Nonlocal(crate::StmtNonlocal<'ast>),
    Expr(crate::StmtExpr<'ast>),
    Pass(crate::StmtPass),
    Break(crate::StmtBreak),
    Continue(crate::StmtContinue),
    IpyEscapeCommand(crate::StmtIpyEscapeCommand<'ast>),
}

impl<'ast> From<crate::StmtFunctionDef<'ast>> for Stmt<'ast> {
    fn from(node: crate::StmtFunctionDef<'ast>) -> Self {
        Self::FunctionDef(node)
    }
}

impl<'ast> From<crate::StmtClassDef<'ast>> for Stmt<'ast> {
    fn from(node: crate::StmtClassDef<'ast>) -> Self {
        Self::ClassDef(node)
    }
}

impl<'ast> From<crate::StmtReturn<'ast>> for Stmt<'ast> {
    fn from(node: crate::StmtReturn<'ast>) -> Self {
        Self::Return(node)
    }
}

impl<'ast> From<crate::StmtDelete<'ast>> for Stmt<'ast> {
    fn from(node: crate::StmtDelete<'ast>) -> Self {
        Self::Delete(node)
    }
}

impl<'ast> From<crate::StmtTypeAlias<'ast>> for Stmt<'ast> {
    fn from(node: crate::StmtTypeAlias<'ast>) -> Self {
        Self::TypeAlias(node)
    }
}

impl<'ast> From<crate::StmtAssign<'ast>> for Stmt<'ast> {
    fn from(node: crate::StmtAssign<'ast>) -> Self {
        Self::Assign(node)
    }
}

impl<'ast> From<crate::StmtAugAssign<'ast>> for Stmt<'ast> {
    fn from(node: crate::StmtAugAssign<'ast>) -> Self {
        Self::AugAssign(node)
    }
}

impl<'ast> From<crate::StmtAnnAssign<'ast>> for Stmt<'ast> {
    fn from(node: crate::StmtAnnAssign<'ast>) -> Self {
        Self::AnnAssign(node)
    }
}

impl<'ast> From<crate::StmtFor<'ast>> for Stmt<'ast> {
    fn from(node: crate::StmtFor<'ast>) -> Self {
        Self::For(node)
    }
}

impl<'ast> From<crate::StmtWhile<'ast>> for Stmt<'ast> {
    fn from(node: crate::StmtWhile<'ast>) -> Self {
        Self::While(node)
    }
}

impl<'ast> From<crate::StmtIf<'ast>> for Stmt<'ast> {
    fn from(node: crate::StmtIf<'ast>) -> Self {
        Self::If(node)
    }
}

impl<'ast> From<crate::StmtWith<'ast>> for Stmt<'ast> {
    fn from(node: crate::StmtWith<'ast>) -> Self {
        Self::With(node)
    }
}

impl<'ast> From<crate::StmtMatch<'ast>> for Stmt<'ast> {
    fn from(node: crate::StmtMatch<'ast>) -> Self {
        Self::Match(node)
    }
}

impl<'ast> From<crate::StmtRaise<'ast>> for Stmt<'ast> {
    fn from(node: crate::StmtRaise<'ast>) -> Self {
        Self::Raise(node)
    }
}

impl<'ast> From<crate::StmtTry<'ast>> for Stmt<'ast> {
    fn from(node: crate::StmtTry<'ast>) -> Self {
        Self::Try(node)
    }
}

impl<'ast> From<crate::StmtAssert<'ast>> for Stmt<'ast> {
    fn from(node: crate::StmtAssert<'ast>) -> Self {
        Self::Assert(node)
    }
}

impl<'ast> From<crate::StmtImport<'ast>> for Stmt<'ast> {
    fn from(node: crate::StmtImport<'ast>) -> Self {
        Self::Import(node)
    }
}

impl<'ast> From<crate::StmtImportFrom<'ast>> for Stmt<'ast> {
    fn from(node: crate::StmtImportFrom<'ast>) -> Self {
        Self::ImportFrom(node)
    }
}

impl<'ast> From<crate::StmtGlobal<'ast>> for Stmt<'ast> {
    fn from(node: crate::StmtGlobal<'ast>) -> Self {
        Self::Global(node)
    }
}

impl<'ast> From<crate::StmtNonlocal<'ast>> for Stmt<'ast> {
    fn from(node: crate::StmtNonlocal<'ast>) -> Self {
        Self::Nonlocal(node)
    }
}

impl<'ast> From<crate::StmtExpr<'ast>> for Stmt<'ast> {
    fn from(node: crate::StmtExpr<'ast>) -> Self {
        Self::Expr(node)
    }
}

impl From<crate::StmtPass> for Stmt<'_> {
    fn from(node: crate::StmtPass) -> Self {
        Self::Pass(node)
    }
}

impl From<crate::StmtBreak> for Stmt<'_> {
    fn from(node: crate::StmtBreak) -> Self {
        Self::Break(node)
    }
}

impl From<crate::StmtContinue> for Stmt<'_> {
    fn from(node: crate::StmtContinue) -> Self {
        Self::Continue(node)
    }
}

impl<'ast> From<crate::StmtIpyEscapeCommand<'ast>> for Stmt<'ast> {
    fn from(node: crate::StmtIpyEscapeCommand<'ast>) -> Self {
        Self::IpyEscapeCommand(node)
    }
}

impl ruff_text_size::Ranged for Stmt<'_> {
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

impl crate::HasNodeIndex for Stmt<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        match self {
            Self::FunctionDef(node) => node.node_index(),
            Self::ClassDef(node) => node.node_index(),
            Self::Return(node) => node.node_index(),
            Self::Delete(node) => node.node_index(),
            Self::TypeAlias(node) => node.node_index(),
            Self::Assign(node) => node.node_index(),
            Self::AugAssign(node) => node.node_index(),
            Self::AnnAssign(node) => node.node_index(),
            Self::For(node) => node.node_index(),
            Self::While(node) => node.node_index(),
            Self::If(node) => node.node_index(),
            Self::With(node) => node.node_index(),
            Self::Match(node) => node.node_index(),
            Self::Raise(node) => node.node_index(),
            Self::Try(node) => node.node_index(),
            Self::Assert(node) => node.node_index(),
            Self::Import(node) => node.node_index(),
            Self::ImportFrom(node) => node.node_index(),
            Self::Global(node) => node.node_index(),
            Self::Nonlocal(node) => node.node_index(),
            Self::Expr(node) => node.node_index(),
            Self::Pass(node) => node.node_index(),
            Self::Break(node) => node.node_index(),
            Self::Continue(node) => node.node_index(),
            Self::IpyEscapeCommand(node) => node.node_index(),
        }
    }
}

#[allow(dead_code, clippy::match_wildcard_for_single_variants)]
impl<'ast> Stmt<'ast> {
    #[inline]
    pub const fn is_function_def_stmt(&self) -> bool {
        matches!(self, Self::FunctionDef(_))
    }

    #[inline]
    pub fn function_def_stmt(self) -> Option<crate::StmtFunctionDef<'ast>> {
        match self {
            Self::FunctionDef(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_function_def_stmt(self) -> crate::StmtFunctionDef<'ast> {
        match self {
            Self::FunctionDef(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_function_def_stmt_mut(&mut self) -> Option<&mut crate::StmtFunctionDef<'ast>> {
        match self {
            Self::FunctionDef(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_function_def_stmt(&self) -> Option<&crate::StmtFunctionDef<'ast>> {
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
    pub fn class_def_stmt(self) -> Option<crate::StmtClassDef<'ast>> {
        match self {
            Self::ClassDef(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_class_def_stmt(self) -> crate::StmtClassDef<'ast> {
        match self {
            Self::ClassDef(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_class_def_stmt_mut(&mut self) -> Option<&mut crate::StmtClassDef<'ast>> {
        match self {
            Self::ClassDef(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_class_def_stmt(&self) -> Option<&crate::StmtClassDef<'ast>> {
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
    pub fn return_stmt(self) -> Option<crate::StmtReturn<'ast>> {
        match self {
            Self::Return(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_return_stmt(self) -> crate::StmtReturn<'ast> {
        match self {
            Self::Return(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_return_stmt_mut(&mut self) -> Option<&mut crate::StmtReturn<'ast>> {
        match self {
            Self::Return(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_return_stmt(&self) -> Option<&crate::StmtReturn<'ast>> {
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
    pub fn delete_stmt(self) -> Option<crate::StmtDelete<'ast>> {
        match self {
            Self::Delete(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_delete_stmt(self) -> crate::StmtDelete<'ast> {
        match self {
            Self::Delete(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_delete_stmt_mut(&mut self) -> Option<&mut crate::StmtDelete<'ast>> {
        match self {
            Self::Delete(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_delete_stmt(&self) -> Option<&crate::StmtDelete<'ast>> {
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
    pub fn type_alias_stmt(self) -> Option<crate::StmtTypeAlias<'ast>> {
        match self {
            Self::TypeAlias(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_type_alias_stmt(self) -> crate::StmtTypeAlias<'ast> {
        match self {
            Self::TypeAlias(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_type_alias_stmt_mut(&mut self) -> Option<&mut crate::StmtTypeAlias<'ast>> {
        match self {
            Self::TypeAlias(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_type_alias_stmt(&self) -> Option<&crate::StmtTypeAlias<'ast>> {
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
    pub fn assign_stmt(self) -> Option<crate::StmtAssign<'ast>> {
        match self {
            Self::Assign(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_assign_stmt(self) -> crate::StmtAssign<'ast> {
        match self {
            Self::Assign(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_assign_stmt_mut(&mut self) -> Option<&mut crate::StmtAssign<'ast>> {
        match self {
            Self::Assign(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_assign_stmt(&self) -> Option<&crate::StmtAssign<'ast>> {
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
    pub fn aug_assign_stmt(self) -> Option<crate::StmtAugAssign<'ast>> {
        match self {
            Self::AugAssign(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_aug_assign_stmt(self) -> crate::StmtAugAssign<'ast> {
        match self {
            Self::AugAssign(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_aug_assign_stmt_mut(&mut self) -> Option<&mut crate::StmtAugAssign<'ast>> {
        match self {
            Self::AugAssign(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_aug_assign_stmt(&self) -> Option<&crate::StmtAugAssign<'ast>> {
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
    pub fn ann_assign_stmt(self) -> Option<crate::StmtAnnAssign<'ast>> {
        match self {
            Self::AnnAssign(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_ann_assign_stmt(self) -> crate::StmtAnnAssign<'ast> {
        match self {
            Self::AnnAssign(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_ann_assign_stmt_mut(&mut self) -> Option<&mut crate::StmtAnnAssign<'ast>> {
        match self {
            Self::AnnAssign(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_ann_assign_stmt(&self) -> Option<&crate::StmtAnnAssign<'ast>> {
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
    pub fn for_stmt(self) -> Option<crate::StmtFor<'ast>> {
        match self {
            Self::For(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_for_stmt(self) -> crate::StmtFor<'ast> {
        match self {
            Self::For(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_for_stmt_mut(&mut self) -> Option<&mut crate::StmtFor<'ast>> {
        match self {
            Self::For(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_for_stmt(&self) -> Option<&crate::StmtFor<'ast>> {
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
    pub fn while_stmt(self) -> Option<crate::StmtWhile<'ast>> {
        match self {
            Self::While(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_while_stmt(self) -> crate::StmtWhile<'ast> {
        match self {
            Self::While(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_while_stmt_mut(&mut self) -> Option<&mut crate::StmtWhile<'ast>> {
        match self {
            Self::While(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_while_stmt(&self) -> Option<&crate::StmtWhile<'ast>> {
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
    pub fn if_stmt(self) -> Option<crate::StmtIf<'ast>> {
        match self {
            Self::If(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_if_stmt(self) -> crate::StmtIf<'ast> {
        match self {
            Self::If(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_if_stmt_mut(&mut self) -> Option<&mut crate::StmtIf<'ast>> {
        match self {
            Self::If(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_if_stmt(&self) -> Option<&crate::StmtIf<'ast>> {
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
    pub fn with_stmt(self) -> Option<crate::StmtWith<'ast>> {
        match self {
            Self::With(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_with_stmt(self) -> crate::StmtWith<'ast> {
        match self {
            Self::With(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_with_stmt_mut(&mut self) -> Option<&mut crate::StmtWith<'ast>> {
        match self {
            Self::With(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_with_stmt(&self) -> Option<&crate::StmtWith<'ast>> {
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
    pub fn match_stmt(self) -> Option<crate::StmtMatch<'ast>> {
        match self {
            Self::Match(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_match_stmt(self) -> crate::StmtMatch<'ast> {
        match self {
            Self::Match(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_match_stmt_mut(&mut self) -> Option<&mut crate::StmtMatch<'ast>> {
        match self {
            Self::Match(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_match_stmt(&self) -> Option<&crate::StmtMatch<'ast>> {
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
    pub fn raise_stmt(self) -> Option<crate::StmtRaise<'ast>> {
        match self {
            Self::Raise(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_raise_stmt(self) -> crate::StmtRaise<'ast> {
        match self {
            Self::Raise(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_raise_stmt_mut(&mut self) -> Option<&mut crate::StmtRaise<'ast>> {
        match self {
            Self::Raise(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_raise_stmt(&self) -> Option<&crate::StmtRaise<'ast>> {
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
    pub fn try_stmt(self) -> Option<crate::StmtTry<'ast>> {
        match self {
            Self::Try(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_try_stmt(self) -> crate::StmtTry<'ast> {
        match self {
            Self::Try(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_try_stmt_mut(&mut self) -> Option<&mut crate::StmtTry<'ast>> {
        match self {
            Self::Try(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_try_stmt(&self) -> Option<&crate::StmtTry<'ast>> {
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
    pub fn assert_stmt(self) -> Option<crate::StmtAssert<'ast>> {
        match self {
            Self::Assert(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_assert_stmt(self) -> crate::StmtAssert<'ast> {
        match self {
            Self::Assert(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_assert_stmt_mut(&mut self) -> Option<&mut crate::StmtAssert<'ast>> {
        match self {
            Self::Assert(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_assert_stmt(&self) -> Option<&crate::StmtAssert<'ast>> {
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
    pub fn import_stmt(self) -> Option<crate::StmtImport<'ast>> {
        match self {
            Self::Import(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_import_stmt(self) -> crate::StmtImport<'ast> {
        match self {
            Self::Import(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_import_stmt_mut(&mut self) -> Option<&mut crate::StmtImport<'ast>> {
        match self {
            Self::Import(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_import_stmt(&self) -> Option<&crate::StmtImport<'ast>> {
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
    pub fn import_from_stmt(self) -> Option<crate::StmtImportFrom<'ast>> {
        match self {
            Self::ImportFrom(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_import_from_stmt(self) -> crate::StmtImportFrom<'ast> {
        match self {
            Self::ImportFrom(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_import_from_stmt_mut(&mut self) -> Option<&mut crate::StmtImportFrom<'ast>> {
        match self {
            Self::ImportFrom(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_import_from_stmt(&self) -> Option<&crate::StmtImportFrom<'ast>> {
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
    pub fn global_stmt(self) -> Option<crate::StmtGlobal<'ast>> {
        match self {
            Self::Global(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_global_stmt(self) -> crate::StmtGlobal<'ast> {
        match self {
            Self::Global(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_global_stmt_mut(&mut self) -> Option<&mut crate::StmtGlobal<'ast>> {
        match self {
            Self::Global(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_global_stmt(&self) -> Option<&crate::StmtGlobal<'ast>> {
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
    pub fn nonlocal_stmt(self) -> Option<crate::StmtNonlocal<'ast>> {
        match self {
            Self::Nonlocal(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_nonlocal_stmt(self) -> crate::StmtNonlocal<'ast> {
        match self {
            Self::Nonlocal(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_nonlocal_stmt_mut(&mut self) -> Option<&mut crate::StmtNonlocal<'ast>> {
        match self {
            Self::Nonlocal(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_nonlocal_stmt(&self) -> Option<&crate::StmtNonlocal<'ast>> {
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
    pub fn expr_stmt(self) -> Option<crate::StmtExpr<'ast>> {
        match self {
            Self::Expr(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_expr_stmt(self) -> crate::StmtExpr<'ast> {
        match self {
            Self::Expr(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_expr_stmt_mut(&mut self) -> Option<&mut crate::StmtExpr<'ast>> {
        match self {
            Self::Expr(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_expr_stmt(&self) -> Option<&crate::StmtExpr<'ast>> {
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
    pub fn ipy_escape_command_stmt(self) -> Option<crate::StmtIpyEscapeCommand<'ast>> {
        match self {
            Self::IpyEscapeCommand(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_ipy_escape_command_stmt(self) -> crate::StmtIpyEscapeCommand<'ast> {
        match self {
            Self::IpyEscapeCommand(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_ipy_escape_command_stmt_mut(
        &mut self,
    ) -> Option<&mut crate::StmtIpyEscapeCommand<'ast>> {
        match self {
            Self::IpyEscapeCommand(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_ipy_escape_command_stmt(&self) -> Option<&crate::StmtIpyEscapeCommand<'ast>> {
        match self {
            Self::IpyEscapeCommand(val) => Some(val),
            _ => None,
        }
    }
}

/// See also [expr](https://docs.python.org/3/library/ast.html#ast.expr)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub enum Expr<'ast> {
    BoolOp(crate::ExprBoolOp<'ast>),
    Named(crate::ExprNamed<'ast>),
    BinOp(crate::ExprBinOp<'ast>),
    UnaryOp(crate::ExprUnaryOp<'ast>),
    Lambda(crate::ExprLambda<'ast>),
    If(crate::ExprIf<'ast>),
    Dict(crate::ExprDict<'ast>),
    Set(crate::ExprSet<'ast>),
    ListComp(crate::ExprListComp<'ast>),
    SetComp(crate::ExprSetComp<'ast>),
    DictComp(crate::ExprDictComp<'ast>),
    Generator(crate::ExprGenerator<'ast>),
    Await(crate::ExprAwait<'ast>),
    Yield(crate::ExprYield<'ast>),
    YieldFrom(crate::ExprYieldFrom<'ast>),
    Compare(crate::ExprCompare<'ast>),
    Call(crate::ExprCall<'ast>),
    FString(crate::ExprFString<'ast>),
    TString(crate::ExprTString<'ast>),
    StringLiteral(crate::ExprStringLiteral<'ast>),
    BytesLiteral(crate::ExprBytesLiteral<'ast>),
    NumberLiteral(crate::ExprNumberLiteral<'ast>),
    BooleanLiteral(crate::ExprBooleanLiteral),
    NoneLiteral(crate::ExprNoneLiteral),
    EllipsisLiteral(crate::ExprEllipsisLiteral),
    Attribute(crate::ExprAttribute<'ast>),
    Subscript(crate::ExprSubscript<'ast>),
    Starred(crate::ExprStarred<'ast>),
    Name(crate::ExprName<'ast>),
    List(crate::ExprList<'ast>),
    Tuple(crate::ExprTuple<'ast>),
    Slice(crate::ExprSlice<'ast>),
    IpyEscapeCommand(crate::ExprIpyEscapeCommand<'ast>),
}

impl<'ast> From<crate::ExprBoolOp<'ast>> for Expr<'ast> {
    fn from(node: crate::ExprBoolOp<'ast>) -> Self {
        Self::BoolOp(node)
    }
}

impl<'ast> From<crate::ExprNamed<'ast>> for Expr<'ast> {
    fn from(node: crate::ExprNamed<'ast>) -> Self {
        Self::Named(node)
    }
}

impl<'ast> From<crate::ExprBinOp<'ast>> for Expr<'ast> {
    fn from(node: crate::ExprBinOp<'ast>) -> Self {
        Self::BinOp(node)
    }
}

impl<'ast> From<crate::ExprUnaryOp<'ast>> for Expr<'ast> {
    fn from(node: crate::ExprUnaryOp<'ast>) -> Self {
        Self::UnaryOp(node)
    }
}

impl<'ast> From<crate::ExprLambda<'ast>> for Expr<'ast> {
    fn from(node: crate::ExprLambda<'ast>) -> Self {
        Self::Lambda(node)
    }
}

impl<'ast> From<crate::ExprIf<'ast>> for Expr<'ast> {
    fn from(node: crate::ExprIf<'ast>) -> Self {
        Self::If(node)
    }
}

impl<'ast> From<crate::ExprDict<'ast>> for Expr<'ast> {
    fn from(node: crate::ExprDict<'ast>) -> Self {
        Self::Dict(node)
    }
}

impl<'ast> From<crate::ExprSet<'ast>> for Expr<'ast> {
    fn from(node: crate::ExprSet<'ast>) -> Self {
        Self::Set(node)
    }
}

impl<'ast> From<crate::ExprListComp<'ast>> for Expr<'ast> {
    fn from(node: crate::ExprListComp<'ast>) -> Self {
        Self::ListComp(node)
    }
}

impl<'ast> From<crate::ExprSetComp<'ast>> for Expr<'ast> {
    fn from(node: crate::ExprSetComp<'ast>) -> Self {
        Self::SetComp(node)
    }
}

impl<'ast> From<crate::ExprDictComp<'ast>> for Expr<'ast> {
    fn from(node: crate::ExprDictComp<'ast>) -> Self {
        Self::DictComp(node)
    }
}

impl<'ast> From<crate::ExprGenerator<'ast>> for Expr<'ast> {
    fn from(node: crate::ExprGenerator<'ast>) -> Self {
        Self::Generator(node)
    }
}

impl<'ast> From<crate::ExprAwait<'ast>> for Expr<'ast> {
    fn from(node: crate::ExprAwait<'ast>) -> Self {
        Self::Await(node)
    }
}

impl<'ast> From<crate::ExprYield<'ast>> for Expr<'ast> {
    fn from(node: crate::ExprYield<'ast>) -> Self {
        Self::Yield(node)
    }
}

impl<'ast> From<crate::ExprYieldFrom<'ast>> for Expr<'ast> {
    fn from(node: crate::ExprYieldFrom<'ast>) -> Self {
        Self::YieldFrom(node)
    }
}

impl<'ast> From<crate::ExprCompare<'ast>> for Expr<'ast> {
    fn from(node: crate::ExprCompare<'ast>) -> Self {
        Self::Compare(node)
    }
}

impl<'ast> From<crate::ExprCall<'ast>> for Expr<'ast> {
    fn from(node: crate::ExprCall<'ast>) -> Self {
        Self::Call(node)
    }
}

impl<'ast> From<crate::ExprFString<'ast>> for Expr<'ast> {
    fn from(node: crate::ExprFString<'ast>) -> Self {
        Self::FString(node)
    }
}

impl<'ast> From<crate::ExprTString<'ast>> for Expr<'ast> {
    fn from(node: crate::ExprTString<'ast>) -> Self {
        Self::TString(node)
    }
}

impl<'ast> From<crate::ExprStringLiteral<'ast>> for Expr<'ast> {
    fn from(node: crate::ExprStringLiteral<'ast>) -> Self {
        Self::StringLiteral(node)
    }
}

impl<'ast> From<crate::ExprBytesLiteral<'ast>> for Expr<'ast> {
    fn from(node: crate::ExprBytesLiteral<'ast>) -> Self {
        Self::BytesLiteral(node)
    }
}

impl<'ast> From<crate::ExprNumberLiteral<'ast>> for Expr<'ast> {
    fn from(node: crate::ExprNumberLiteral<'ast>) -> Self {
        Self::NumberLiteral(node)
    }
}

impl From<crate::ExprBooleanLiteral> for Expr<'_> {
    fn from(node: crate::ExprBooleanLiteral) -> Self {
        Self::BooleanLiteral(node)
    }
}

impl From<crate::ExprNoneLiteral> for Expr<'_> {
    fn from(node: crate::ExprNoneLiteral) -> Self {
        Self::NoneLiteral(node)
    }
}

impl From<crate::ExprEllipsisLiteral> for Expr<'_> {
    fn from(node: crate::ExprEllipsisLiteral) -> Self {
        Self::EllipsisLiteral(node)
    }
}

impl<'ast> From<crate::ExprAttribute<'ast>> for Expr<'ast> {
    fn from(node: crate::ExprAttribute<'ast>) -> Self {
        Self::Attribute(node)
    }
}

impl<'ast> From<crate::ExprSubscript<'ast>> for Expr<'ast> {
    fn from(node: crate::ExprSubscript<'ast>) -> Self {
        Self::Subscript(node)
    }
}

impl<'ast> From<crate::ExprStarred<'ast>> for Expr<'ast> {
    fn from(node: crate::ExprStarred<'ast>) -> Self {
        Self::Starred(node)
    }
}

impl<'ast> From<crate::ExprName<'ast>> for Expr<'ast> {
    fn from(node: crate::ExprName<'ast>) -> Self {
        Self::Name(node)
    }
}

impl<'ast> From<crate::ExprList<'ast>> for Expr<'ast> {
    fn from(node: crate::ExprList<'ast>) -> Self {
        Self::List(node)
    }
}

impl<'ast> From<crate::ExprTuple<'ast>> for Expr<'ast> {
    fn from(node: crate::ExprTuple<'ast>) -> Self {
        Self::Tuple(node)
    }
}

impl<'ast> From<crate::ExprSlice<'ast>> for Expr<'ast> {
    fn from(node: crate::ExprSlice<'ast>) -> Self {
        Self::Slice(node)
    }
}

impl<'ast> From<crate::ExprIpyEscapeCommand<'ast>> for Expr<'ast> {
    fn from(node: crate::ExprIpyEscapeCommand<'ast>) -> Self {
        Self::IpyEscapeCommand(node)
    }
}

impl ruff_text_size::Ranged for Expr<'_> {
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
            Self::TString(node) => node.range(),
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

impl crate::HasNodeIndex for Expr<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        match self {
            Self::BoolOp(node) => node.node_index(),
            Self::Named(node) => node.node_index(),
            Self::BinOp(node) => node.node_index(),
            Self::UnaryOp(node) => node.node_index(),
            Self::Lambda(node) => node.node_index(),
            Self::If(node) => node.node_index(),
            Self::Dict(node) => node.node_index(),
            Self::Set(node) => node.node_index(),
            Self::ListComp(node) => node.node_index(),
            Self::SetComp(node) => node.node_index(),
            Self::DictComp(node) => node.node_index(),
            Self::Generator(node) => node.node_index(),
            Self::Await(node) => node.node_index(),
            Self::Yield(node) => node.node_index(),
            Self::YieldFrom(node) => node.node_index(),
            Self::Compare(node) => node.node_index(),
            Self::Call(node) => node.node_index(),
            Self::FString(node) => node.node_index(),
            Self::TString(node) => node.node_index(),
            Self::StringLiteral(node) => node.node_index(),
            Self::BytesLiteral(node) => node.node_index(),
            Self::NumberLiteral(node) => node.node_index(),
            Self::BooleanLiteral(node) => node.node_index(),
            Self::NoneLiteral(node) => node.node_index(),
            Self::EllipsisLiteral(node) => node.node_index(),
            Self::Attribute(node) => node.node_index(),
            Self::Subscript(node) => node.node_index(),
            Self::Starred(node) => node.node_index(),
            Self::Name(node) => node.node_index(),
            Self::List(node) => node.node_index(),
            Self::Tuple(node) => node.node_index(),
            Self::Slice(node) => node.node_index(),
            Self::IpyEscapeCommand(node) => node.node_index(),
        }
    }
}

#[allow(dead_code, clippy::match_wildcard_for_single_variants)]
impl<'ast> Expr<'ast> {
    #[inline]
    pub const fn is_bool_op_expr(&self) -> bool {
        matches!(self, Self::BoolOp(_))
    }

    #[inline]
    pub fn bool_op_expr(self) -> Option<crate::ExprBoolOp<'ast>> {
        match self {
            Self::BoolOp(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_bool_op_expr(self) -> crate::ExprBoolOp<'ast> {
        match self {
            Self::BoolOp(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_bool_op_expr_mut(&mut self) -> Option<&mut crate::ExprBoolOp<'ast>> {
        match self {
            Self::BoolOp(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_bool_op_expr(&self) -> Option<&crate::ExprBoolOp<'ast>> {
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
    pub fn named_expr(self) -> Option<crate::ExprNamed<'ast>> {
        match self {
            Self::Named(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_named_expr(self) -> crate::ExprNamed<'ast> {
        match self {
            Self::Named(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_named_expr_mut(&mut self) -> Option<&mut crate::ExprNamed<'ast>> {
        match self {
            Self::Named(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_named_expr(&self) -> Option<&crate::ExprNamed<'ast>> {
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
    pub fn bin_op_expr(self) -> Option<crate::ExprBinOp<'ast>> {
        match self {
            Self::BinOp(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_bin_op_expr(self) -> crate::ExprBinOp<'ast> {
        match self {
            Self::BinOp(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_bin_op_expr_mut(&mut self) -> Option<&mut crate::ExprBinOp<'ast>> {
        match self {
            Self::BinOp(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_bin_op_expr(&self) -> Option<&crate::ExprBinOp<'ast>> {
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
    pub fn unary_op_expr(self) -> Option<crate::ExprUnaryOp<'ast>> {
        match self {
            Self::UnaryOp(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_unary_op_expr(self) -> crate::ExprUnaryOp<'ast> {
        match self {
            Self::UnaryOp(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_unary_op_expr_mut(&mut self) -> Option<&mut crate::ExprUnaryOp<'ast>> {
        match self {
            Self::UnaryOp(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_unary_op_expr(&self) -> Option<&crate::ExprUnaryOp<'ast>> {
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
    pub fn lambda_expr(self) -> Option<crate::ExprLambda<'ast>> {
        match self {
            Self::Lambda(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_lambda_expr(self) -> crate::ExprLambda<'ast> {
        match self {
            Self::Lambda(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_lambda_expr_mut(&mut self) -> Option<&mut crate::ExprLambda<'ast>> {
        match self {
            Self::Lambda(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_lambda_expr(&self) -> Option<&crate::ExprLambda<'ast>> {
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
    pub fn if_expr(self) -> Option<crate::ExprIf<'ast>> {
        match self {
            Self::If(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_if_expr(self) -> crate::ExprIf<'ast> {
        match self {
            Self::If(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_if_expr_mut(&mut self) -> Option<&mut crate::ExprIf<'ast>> {
        match self {
            Self::If(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_if_expr(&self) -> Option<&crate::ExprIf<'ast>> {
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
    pub fn dict_expr(self) -> Option<crate::ExprDict<'ast>> {
        match self {
            Self::Dict(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_dict_expr(self) -> crate::ExprDict<'ast> {
        match self {
            Self::Dict(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_dict_expr_mut(&mut self) -> Option<&mut crate::ExprDict<'ast>> {
        match self {
            Self::Dict(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_dict_expr(&self) -> Option<&crate::ExprDict<'ast>> {
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
    pub fn set_expr(self) -> Option<crate::ExprSet<'ast>> {
        match self {
            Self::Set(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_set_expr(self) -> crate::ExprSet<'ast> {
        match self {
            Self::Set(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_set_expr_mut(&mut self) -> Option<&mut crate::ExprSet<'ast>> {
        match self {
            Self::Set(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_set_expr(&self) -> Option<&crate::ExprSet<'ast>> {
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
    pub fn list_comp_expr(self) -> Option<crate::ExprListComp<'ast>> {
        match self {
            Self::ListComp(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_list_comp_expr(self) -> crate::ExprListComp<'ast> {
        match self {
            Self::ListComp(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_list_comp_expr_mut(&mut self) -> Option<&mut crate::ExprListComp<'ast>> {
        match self {
            Self::ListComp(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_list_comp_expr(&self) -> Option<&crate::ExprListComp<'ast>> {
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
    pub fn set_comp_expr(self) -> Option<crate::ExprSetComp<'ast>> {
        match self {
            Self::SetComp(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_set_comp_expr(self) -> crate::ExprSetComp<'ast> {
        match self {
            Self::SetComp(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_set_comp_expr_mut(&mut self) -> Option<&mut crate::ExprSetComp<'ast>> {
        match self {
            Self::SetComp(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_set_comp_expr(&self) -> Option<&crate::ExprSetComp<'ast>> {
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
    pub fn dict_comp_expr(self) -> Option<crate::ExprDictComp<'ast>> {
        match self {
            Self::DictComp(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_dict_comp_expr(self) -> crate::ExprDictComp<'ast> {
        match self {
            Self::DictComp(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_dict_comp_expr_mut(&mut self) -> Option<&mut crate::ExprDictComp<'ast>> {
        match self {
            Self::DictComp(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_dict_comp_expr(&self) -> Option<&crate::ExprDictComp<'ast>> {
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
    pub fn generator_expr(self) -> Option<crate::ExprGenerator<'ast>> {
        match self {
            Self::Generator(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_generator_expr(self) -> crate::ExprGenerator<'ast> {
        match self {
            Self::Generator(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_generator_expr_mut(&mut self) -> Option<&mut crate::ExprGenerator<'ast>> {
        match self {
            Self::Generator(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_generator_expr(&self) -> Option<&crate::ExprGenerator<'ast>> {
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
    pub fn await_expr(self) -> Option<crate::ExprAwait<'ast>> {
        match self {
            Self::Await(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_await_expr(self) -> crate::ExprAwait<'ast> {
        match self {
            Self::Await(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_await_expr_mut(&mut self) -> Option<&mut crate::ExprAwait<'ast>> {
        match self {
            Self::Await(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_await_expr(&self) -> Option<&crate::ExprAwait<'ast>> {
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
    pub fn yield_expr(self) -> Option<crate::ExprYield<'ast>> {
        match self {
            Self::Yield(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_yield_expr(self) -> crate::ExprYield<'ast> {
        match self {
            Self::Yield(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_yield_expr_mut(&mut self) -> Option<&mut crate::ExprYield<'ast>> {
        match self {
            Self::Yield(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_yield_expr(&self) -> Option<&crate::ExprYield<'ast>> {
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
    pub fn yield_from_expr(self) -> Option<crate::ExprYieldFrom<'ast>> {
        match self {
            Self::YieldFrom(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_yield_from_expr(self) -> crate::ExprYieldFrom<'ast> {
        match self {
            Self::YieldFrom(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_yield_from_expr_mut(&mut self) -> Option<&mut crate::ExprYieldFrom<'ast>> {
        match self {
            Self::YieldFrom(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_yield_from_expr(&self) -> Option<&crate::ExprYieldFrom<'ast>> {
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
    pub fn compare_expr(self) -> Option<crate::ExprCompare<'ast>> {
        match self {
            Self::Compare(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_compare_expr(self) -> crate::ExprCompare<'ast> {
        match self {
            Self::Compare(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_compare_expr_mut(&mut self) -> Option<&mut crate::ExprCompare<'ast>> {
        match self {
            Self::Compare(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_compare_expr(&self) -> Option<&crate::ExprCompare<'ast>> {
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
    pub fn call_expr(self) -> Option<crate::ExprCall<'ast>> {
        match self {
            Self::Call(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_call_expr(self) -> crate::ExprCall<'ast> {
        match self {
            Self::Call(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_call_expr_mut(&mut self) -> Option<&mut crate::ExprCall<'ast>> {
        match self {
            Self::Call(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_call_expr(&self) -> Option<&crate::ExprCall<'ast>> {
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
    pub fn f_string_expr(self) -> Option<crate::ExprFString<'ast>> {
        match self {
            Self::FString(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_f_string_expr(self) -> crate::ExprFString<'ast> {
        match self {
            Self::FString(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_f_string_expr_mut(&mut self) -> Option<&mut crate::ExprFString<'ast>> {
        match self {
            Self::FString(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_f_string_expr(&self) -> Option<&crate::ExprFString<'ast>> {
        match self {
            Self::FString(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_t_string_expr(&self) -> bool {
        matches!(self, Self::TString(_))
    }

    #[inline]
    pub fn t_string_expr(self) -> Option<crate::ExprTString<'ast>> {
        match self {
            Self::TString(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_t_string_expr(self) -> crate::ExprTString<'ast> {
        match self {
            Self::TString(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_t_string_expr_mut(&mut self) -> Option<&mut crate::ExprTString<'ast>> {
        match self {
            Self::TString(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_t_string_expr(&self) -> Option<&crate::ExprTString<'ast>> {
        match self {
            Self::TString(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_string_literal_expr(&self) -> bool {
        matches!(self, Self::StringLiteral(_))
    }

    #[inline]
    pub fn string_literal_expr(self) -> Option<crate::ExprStringLiteral<'ast>> {
        match self {
            Self::StringLiteral(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_string_literal_expr(self) -> crate::ExprStringLiteral<'ast> {
        match self {
            Self::StringLiteral(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_string_literal_expr_mut(&mut self) -> Option<&mut crate::ExprStringLiteral<'ast>> {
        match self {
            Self::StringLiteral(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_string_literal_expr(&self) -> Option<&crate::ExprStringLiteral<'ast>> {
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
    pub fn bytes_literal_expr(self) -> Option<crate::ExprBytesLiteral<'ast>> {
        match self {
            Self::BytesLiteral(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_bytes_literal_expr(self) -> crate::ExprBytesLiteral<'ast> {
        match self {
            Self::BytesLiteral(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_bytes_literal_expr_mut(&mut self) -> Option<&mut crate::ExprBytesLiteral<'ast>> {
        match self {
            Self::BytesLiteral(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_bytes_literal_expr(&self) -> Option<&crate::ExprBytesLiteral<'ast>> {
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
    pub fn number_literal_expr(self) -> Option<crate::ExprNumberLiteral<'ast>> {
        match self {
            Self::NumberLiteral(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_number_literal_expr(self) -> crate::ExprNumberLiteral<'ast> {
        match self {
            Self::NumberLiteral(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_number_literal_expr_mut(&mut self) -> Option<&mut crate::ExprNumberLiteral<'ast>> {
        match self {
            Self::NumberLiteral(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_number_literal_expr(&self) -> Option<&crate::ExprNumberLiteral<'ast>> {
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
    pub fn attribute_expr(self) -> Option<crate::ExprAttribute<'ast>> {
        match self {
            Self::Attribute(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_attribute_expr(self) -> crate::ExprAttribute<'ast> {
        match self {
            Self::Attribute(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_attribute_expr_mut(&mut self) -> Option<&mut crate::ExprAttribute<'ast>> {
        match self {
            Self::Attribute(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_attribute_expr(&self) -> Option<&crate::ExprAttribute<'ast>> {
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
    pub fn subscript_expr(self) -> Option<crate::ExprSubscript<'ast>> {
        match self {
            Self::Subscript(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_subscript_expr(self) -> crate::ExprSubscript<'ast> {
        match self {
            Self::Subscript(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_subscript_expr_mut(&mut self) -> Option<&mut crate::ExprSubscript<'ast>> {
        match self {
            Self::Subscript(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_subscript_expr(&self) -> Option<&crate::ExprSubscript<'ast>> {
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
    pub fn starred_expr(self) -> Option<crate::ExprStarred<'ast>> {
        match self {
            Self::Starred(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_starred_expr(self) -> crate::ExprStarred<'ast> {
        match self {
            Self::Starred(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_starred_expr_mut(&mut self) -> Option<&mut crate::ExprStarred<'ast>> {
        match self {
            Self::Starred(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_starred_expr(&self) -> Option<&crate::ExprStarred<'ast>> {
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
    pub fn name_expr(self) -> Option<crate::ExprName<'ast>> {
        match self {
            Self::Name(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_name_expr(self) -> crate::ExprName<'ast> {
        match self {
            Self::Name(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_name_expr_mut(&mut self) -> Option<&mut crate::ExprName<'ast>> {
        match self {
            Self::Name(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_name_expr(&self) -> Option<&crate::ExprName<'ast>> {
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
    pub fn list_expr(self) -> Option<crate::ExprList<'ast>> {
        match self {
            Self::List(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_list_expr(self) -> crate::ExprList<'ast> {
        match self {
            Self::List(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_list_expr_mut(&mut self) -> Option<&mut crate::ExprList<'ast>> {
        match self {
            Self::List(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_list_expr(&self) -> Option<&crate::ExprList<'ast>> {
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
    pub fn tuple_expr(self) -> Option<crate::ExprTuple<'ast>> {
        match self {
            Self::Tuple(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_tuple_expr(self) -> crate::ExprTuple<'ast> {
        match self {
            Self::Tuple(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_tuple_expr_mut(&mut self) -> Option<&mut crate::ExprTuple<'ast>> {
        match self {
            Self::Tuple(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_tuple_expr(&self) -> Option<&crate::ExprTuple<'ast>> {
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
    pub fn slice_expr(self) -> Option<crate::ExprSlice<'ast>> {
        match self {
            Self::Slice(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_slice_expr(self) -> crate::ExprSlice<'ast> {
        match self {
            Self::Slice(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_slice_expr_mut(&mut self) -> Option<&mut crate::ExprSlice<'ast>> {
        match self {
            Self::Slice(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_slice_expr(&self) -> Option<&crate::ExprSlice<'ast>> {
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
    pub fn ipy_escape_command_expr(self) -> Option<crate::ExprIpyEscapeCommand<'ast>> {
        match self {
            Self::IpyEscapeCommand(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_ipy_escape_command_expr(self) -> crate::ExprIpyEscapeCommand<'ast> {
        match self {
            Self::IpyEscapeCommand(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_ipy_escape_command_expr_mut(
        &mut self,
    ) -> Option<&mut crate::ExprIpyEscapeCommand<'ast>> {
        match self {
            Self::IpyEscapeCommand(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_ipy_escape_command_expr(&self) -> Option<&crate::ExprIpyEscapeCommand<'ast>> {
        match self {
            Self::IpyEscapeCommand(val) => Some(val),
            _ => None,
        }
    }
}

/// See also [excepthandler](https://docs.python.org/3/library/ast.html#ast.excepthandler)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub enum ExceptHandler<'ast> {
    ExceptHandler(crate::ExceptHandlerExceptHandler<'ast>),
}

impl<'ast> From<crate::ExceptHandlerExceptHandler<'ast>> for ExceptHandler<'ast> {
    fn from(node: crate::ExceptHandlerExceptHandler<'ast>) -> Self {
        Self::ExceptHandler(node)
    }
}

impl ruff_text_size::Ranged for ExceptHandler<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        match self {
            Self::ExceptHandler(node) => node.range(),
        }
    }
}

impl crate::HasNodeIndex for ExceptHandler<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        match self {
            Self::ExceptHandler(node) => node.node_index(),
        }
    }
}

#[allow(dead_code, clippy::match_wildcard_for_single_variants)]
impl<'ast> ExceptHandler<'ast> {
    #[inline]
    pub const fn is_except_handler(&self) -> bool {
        matches!(self, Self::ExceptHandler(_))
    }

    #[inline]
    pub fn except_handler(self) -> Option<crate::ExceptHandlerExceptHandler<'ast>> {
        match self {
            Self::ExceptHandler(val) => Some(val),
        }
    }

    #[inline]
    pub fn expect_except_handler(self) -> crate::ExceptHandlerExceptHandler<'ast> {
        match self {
            Self::ExceptHandler(val) => val,
        }
    }

    #[inline]
    pub fn as_except_handler_mut(
        &mut self,
    ) -> Option<&mut crate::ExceptHandlerExceptHandler<'ast>> {
        match self {
            Self::ExceptHandler(val) => Some(val),
        }
    }

    #[inline]
    pub fn as_except_handler(&self) -> Option<&crate::ExceptHandlerExceptHandler<'ast>> {
        match self {
            Self::ExceptHandler(val) => Some(val),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub enum InterpolatedStringElement<'ast> {
    Interpolation(crate::InterpolatedElement<'ast>),
    Literal(crate::InterpolatedStringLiteralElement<'ast>),
}

impl<'ast> From<crate::InterpolatedElement<'ast>> for InterpolatedStringElement<'ast> {
    fn from(node: crate::InterpolatedElement<'ast>) -> Self {
        Self::Interpolation(node)
    }
}

impl<'ast> From<crate::InterpolatedStringLiteralElement<'ast>> for InterpolatedStringElement<'ast> {
    fn from(node: crate::InterpolatedStringLiteralElement<'ast>) -> Self {
        Self::Literal(node)
    }
}

impl ruff_text_size::Ranged for InterpolatedStringElement<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        match self {
            Self::Interpolation(node) => node.range(),
            Self::Literal(node) => node.range(),
        }
    }
}

impl crate::HasNodeIndex for InterpolatedStringElement<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        match self {
            Self::Interpolation(node) => node.node_index(),
            Self::Literal(node) => node.node_index(),
        }
    }
}

#[allow(dead_code, clippy::match_wildcard_for_single_variants)]
impl<'ast> InterpolatedStringElement<'ast> {
    #[inline]
    pub const fn is_interpolation(&self) -> bool {
        matches!(self, Self::Interpolation(_))
    }

    #[inline]
    pub fn interpolation(self) -> Option<crate::InterpolatedElement<'ast>> {
        match self {
            Self::Interpolation(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_interpolation(self) -> crate::InterpolatedElement<'ast> {
        match self {
            Self::Interpolation(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_interpolation_mut(&mut self) -> Option<&mut crate::InterpolatedElement<'ast>> {
        match self {
            Self::Interpolation(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_interpolation(&self) -> Option<&crate::InterpolatedElement<'ast>> {
        match self {
            Self::Interpolation(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_literal(&self) -> bool {
        matches!(self, Self::Literal(_))
    }

    #[inline]
    pub fn literal(self) -> Option<crate::InterpolatedStringLiteralElement<'ast>> {
        match self {
            Self::Literal(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_literal(self) -> crate::InterpolatedStringLiteralElement<'ast> {
        match self {
            Self::Literal(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_literal_mut(&mut self) -> Option<&mut crate::InterpolatedStringLiteralElement<'ast>> {
        match self {
            Self::Literal(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_literal(&self) -> Option<&crate::InterpolatedStringLiteralElement<'ast>> {
        match self {
            Self::Literal(val) => Some(val),
            _ => None,
        }
    }
}

/// See also [pattern](https://docs.python.org/3/library/ast.html#ast.pattern)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub enum Pattern<'ast> {
    MatchValue(crate::PatternMatchValue<'ast>),
    MatchSingleton(crate::PatternMatchSingleton),
    MatchSequence(crate::PatternMatchSequence<'ast>),
    MatchMapping(crate::PatternMatchMapping<'ast>),
    MatchClass(crate::PatternMatchClass<'ast>),
    MatchStar(crate::PatternMatchStar<'ast>),
    MatchAs(crate::PatternMatchAs<'ast>),
    MatchOr(crate::PatternMatchOr<'ast>),
}

impl<'ast> From<crate::PatternMatchValue<'ast>> for Pattern<'ast> {
    fn from(node: crate::PatternMatchValue<'ast>) -> Self {
        Self::MatchValue(node)
    }
}

impl From<crate::PatternMatchSingleton> for Pattern<'_> {
    fn from(node: crate::PatternMatchSingleton) -> Self {
        Self::MatchSingleton(node)
    }
}

impl<'ast> From<crate::PatternMatchSequence<'ast>> for Pattern<'ast> {
    fn from(node: crate::PatternMatchSequence<'ast>) -> Self {
        Self::MatchSequence(node)
    }
}

impl<'ast> From<crate::PatternMatchMapping<'ast>> for Pattern<'ast> {
    fn from(node: crate::PatternMatchMapping<'ast>) -> Self {
        Self::MatchMapping(node)
    }
}

impl<'ast> From<crate::PatternMatchClass<'ast>> for Pattern<'ast> {
    fn from(node: crate::PatternMatchClass<'ast>) -> Self {
        Self::MatchClass(node)
    }
}

impl<'ast> From<crate::PatternMatchStar<'ast>> for Pattern<'ast> {
    fn from(node: crate::PatternMatchStar<'ast>) -> Self {
        Self::MatchStar(node)
    }
}

impl<'ast> From<crate::PatternMatchAs<'ast>> for Pattern<'ast> {
    fn from(node: crate::PatternMatchAs<'ast>) -> Self {
        Self::MatchAs(node)
    }
}

impl<'ast> From<crate::PatternMatchOr<'ast>> for Pattern<'ast> {
    fn from(node: crate::PatternMatchOr<'ast>) -> Self {
        Self::MatchOr(node)
    }
}

impl ruff_text_size::Ranged for Pattern<'_> {
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

impl crate::HasNodeIndex for Pattern<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        match self {
            Self::MatchValue(node) => node.node_index(),
            Self::MatchSingleton(node) => node.node_index(),
            Self::MatchSequence(node) => node.node_index(),
            Self::MatchMapping(node) => node.node_index(),
            Self::MatchClass(node) => node.node_index(),
            Self::MatchStar(node) => node.node_index(),
            Self::MatchAs(node) => node.node_index(),
            Self::MatchOr(node) => node.node_index(),
        }
    }
}

#[allow(dead_code, clippy::match_wildcard_for_single_variants)]
impl<'ast> Pattern<'ast> {
    #[inline]
    pub const fn is_match_value(&self) -> bool {
        matches!(self, Self::MatchValue(_))
    }

    #[inline]
    pub fn match_value(self) -> Option<crate::PatternMatchValue<'ast>> {
        match self {
            Self::MatchValue(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_match_value(self) -> crate::PatternMatchValue<'ast> {
        match self {
            Self::MatchValue(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_match_value_mut(&mut self) -> Option<&mut crate::PatternMatchValue<'ast>> {
        match self {
            Self::MatchValue(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_match_value(&self) -> Option<&crate::PatternMatchValue<'ast>> {
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
    pub fn match_sequence(self) -> Option<crate::PatternMatchSequence<'ast>> {
        match self {
            Self::MatchSequence(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_match_sequence(self) -> crate::PatternMatchSequence<'ast> {
        match self {
            Self::MatchSequence(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_match_sequence_mut(&mut self) -> Option<&mut crate::PatternMatchSequence<'ast>> {
        match self {
            Self::MatchSequence(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_match_sequence(&self) -> Option<&crate::PatternMatchSequence<'ast>> {
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
    pub fn match_mapping(self) -> Option<crate::PatternMatchMapping<'ast>> {
        match self {
            Self::MatchMapping(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_match_mapping(self) -> crate::PatternMatchMapping<'ast> {
        match self {
            Self::MatchMapping(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_match_mapping_mut(&mut self) -> Option<&mut crate::PatternMatchMapping<'ast>> {
        match self {
            Self::MatchMapping(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_match_mapping(&self) -> Option<&crate::PatternMatchMapping<'ast>> {
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
    pub fn match_class(self) -> Option<crate::PatternMatchClass<'ast>> {
        match self {
            Self::MatchClass(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_match_class(self) -> crate::PatternMatchClass<'ast> {
        match self {
            Self::MatchClass(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_match_class_mut(&mut self) -> Option<&mut crate::PatternMatchClass<'ast>> {
        match self {
            Self::MatchClass(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_match_class(&self) -> Option<&crate::PatternMatchClass<'ast>> {
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
    pub fn match_star(self) -> Option<crate::PatternMatchStar<'ast>> {
        match self {
            Self::MatchStar(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_match_star(self) -> crate::PatternMatchStar<'ast> {
        match self {
            Self::MatchStar(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_match_star_mut(&mut self) -> Option<&mut crate::PatternMatchStar<'ast>> {
        match self {
            Self::MatchStar(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_match_star(&self) -> Option<&crate::PatternMatchStar<'ast>> {
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
    pub fn match_as(self) -> Option<crate::PatternMatchAs<'ast>> {
        match self {
            Self::MatchAs(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_match_as(self) -> crate::PatternMatchAs<'ast> {
        match self {
            Self::MatchAs(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_match_as_mut(&mut self) -> Option<&mut crate::PatternMatchAs<'ast>> {
        match self {
            Self::MatchAs(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_match_as(&self) -> Option<&crate::PatternMatchAs<'ast>> {
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
    pub fn match_or(self) -> Option<crate::PatternMatchOr<'ast>> {
        match self {
            Self::MatchOr(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_match_or(self) -> crate::PatternMatchOr<'ast> {
        match self {
            Self::MatchOr(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_match_or_mut(&mut self) -> Option<&mut crate::PatternMatchOr<'ast>> {
        match self {
            Self::MatchOr(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_match_or(&self) -> Option<&crate::PatternMatchOr<'ast>> {
        match self {
            Self::MatchOr(val) => Some(val),
            _ => None,
        }
    }
}

/// See also [type_param](https://docs.python.org/3/library/ast.html#ast.type_param)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub enum TypeParam<'ast> {
    TypeVar(crate::TypeParamTypeVar<'ast>),
    TypeVarTuple(crate::TypeParamTypeVarTuple<'ast>),
    ParamSpec(crate::TypeParamParamSpec<'ast>),
}

impl<'ast> From<crate::TypeParamTypeVar<'ast>> for TypeParam<'ast> {
    fn from(node: crate::TypeParamTypeVar<'ast>) -> Self {
        Self::TypeVar(node)
    }
}

impl<'ast> From<crate::TypeParamTypeVarTuple<'ast>> for TypeParam<'ast> {
    fn from(node: crate::TypeParamTypeVarTuple<'ast>) -> Self {
        Self::TypeVarTuple(node)
    }
}

impl<'ast> From<crate::TypeParamParamSpec<'ast>> for TypeParam<'ast> {
    fn from(node: crate::TypeParamParamSpec<'ast>) -> Self {
        Self::ParamSpec(node)
    }
}

impl ruff_text_size::Ranged for TypeParam<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        match self {
            Self::TypeVar(node) => node.range(),
            Self::TypeVarTuple(node) => node.range(),
            Self::ParamSpec(node) => node.range(),
        }
    }
}

impl crate::HasNodeIndex for TypeParam<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        match self {
            Self::TypeVar(node) => node.node_index(),
            Self::TypeVarTuple(node) => node.node_index(),
            Self::ParamSpec(node) => node.node_index(),
        }
    }
}

#[allow(dead_code, clippy::match_wildcard_for_single_variants)]
impl<'ast> TypeParam<'ast> {
    #[inline]
    pub const fn is_type_var(&self) -> bool {
        matches!(self, Self::TypeVar(_))
    }

    #[inline]
    pub fn type_var(self) -> Option<crate::TypeParamTypeVar<'ast>> {
        match self {
            Self::TypeVar(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_type_var(self) -> crate::TypeParamTypeVar<'ast> {
        match self {
            Self::TypeVar(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_type_var_mut(&mut self) -> Option<&mut crate::TypeParamTypeVar<'ast>> {
        match self {
            Self::TypeVar(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_type_var(&self) -> Option<&crate::TypeParamTypeVar<'ast>> {
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
    pub fn type_var_tuple(self) -> Option<crate::TypeParamTypeVarTuple<'ast>> {
        match self {
            Self::TypeVarTuple(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_type_var_tuple(self) -> crate::TypeParamTypeVarTuple<'ast> {
        match self {
            Self::TypeVarTuple(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_type_var_tuple_mut(&mut self) -> Option<&mut crate::TypeParamTypeVarTuple<'ast>> {
        match self {
            Self::TypeVarTuple(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_type_var_tuple(&self) -> Option<&crate::TypeParamTypeVarTuple<'ast>> {
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
    pub fn param_spec(self) -> Option<crate::TypeParamParamSpec<'ast>> {
        match self {
            Self::ParamSpec(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn expect_param_spec(self) -> crate::TypeParamParamSpec<'ast> {
        match self {
            Self::ParamSpec(val) => val,
            _ => panic!("called expect on {self:?}"),
        }
    }

    #[inline]
    pub fn as_param_spec_mut(&mut self) -> Option<&mut crate::TypeParamParamSpec<'ast>> {
        match self {
            Self::ParamSpec(val) => Some(val),
            _ => None,
        }
    }

    #[inline]
    pub fn as_param_spec(&self) -> Option<&crate::TypeParamParamSpec<'ast>> {
        match self {
            Self::ParamSpec(val) => Some(val),
            _ => None,
        }
    }
}

impl ruff_text_size::Ranged for crate::ModModule<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ModExpression<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtFunctionDef<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtClassDef<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtReturn<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtDelete<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtTypeAlias<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtAssign<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtAugAssign<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtAnnAssign<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtFor<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtWhile<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtIf<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtWith<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtMatch<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtRaise<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtTry<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtAssert<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtImport<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtImportFrom<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtGlobal<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtNonlocal<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StmtExpr<'_> {
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

impl ruff_text_size::Ranged for crate::StmtIpyEscapeCommand<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprBoolOp<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprNamed<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprBinOp<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprUnaryOp<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprLambda<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprIf<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprDict<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprSet<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprListComp<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprSetComp<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprDictComp<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprGenerator<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprAwait<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprYield<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprYieldFrom<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprCompare<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprCall<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprFString<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprTString<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprStringLiteral<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprBytesLiteral<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprNumberLiteral<'_> {
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

impl ruff_text_size::Ranged for crate::ExprAttribute<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprSubscript<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprStarred<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprName<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprList<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprTuple<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprSlice<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExprIpyEscapeCommand<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ExceptHandlerExceptHandler<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::InterpolatedElement<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::InterpolatedStringLiteralElement<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::PatternMatchValue<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::PatternMatchSingleton {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::PatternMatchSequence<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::PatternMatchMapping<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::PatternMatchClass<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::PatternMatchStar<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::PatternMatchAs<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::PatternMatchOr<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::TypeParamTypeVar<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::TypeParamTypeVarTuple<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::TypeParamParamSpec<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::InterpolatedStringFormatSpec<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::PatternArguments<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::PatternKeyword<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::Comprehension<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::Arguments<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::Parameters<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::Parameter<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ParameterWithDefault<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::Keyword<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::Alias<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::WithItem<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::MatchCase<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::Decorator<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::ElifElseClause<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::TypeParams<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::FString<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::TString<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::StringLiteral<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::BytesLiteral<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl ruff_text_size::Ranged for crate::Identifier<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        self.range
    }
}

impl crate::HasNodeIndex for crate::ModModule<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ModExpression<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::StmtFunctionDef<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::StmtClassDef<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::StmtReturn<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::StmtDelete<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::StmtTypeAlias<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::StmtAssign<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::StmtAugAssign<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::StmtAnnAssign<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::StmtFor<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::StmtWhile<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::StmtIf<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::StmtWith<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::StmtMatch<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::StmtRaise<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::StmtTry<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::StmtAssert<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::StmtImport<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::StmtImportFrom<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::StmtGlobal<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::StmtNonlocal<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::StmtExpr<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::StmtPass {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::StmtBreak {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::StmtContinue {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::StmtIpyEscapeCommand<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprBoolOp<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprNamed<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprBinOp<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprUnaryOp<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprLambda<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprIf<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprDict<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprSet<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprListComp<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprSetComp<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprDictComp<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprGenerator<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprAwait<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprYield<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprYieldFrom<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprCompare<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprCall<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprFString<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprTString<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprStringLiteral<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprBytesLiteral<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprNumberLiteral<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprBooleanLiteral {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprNoneLiteral {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprEllipsisLiteral {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprAttribute<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprSubscript<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprStarred<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprName<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprList<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprTuple<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprSlice<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExprIpyEscapeCommand<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ExceptHandlerExceptHandler<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::InterpolatedElement<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::InterpolatedStringLiteralElement<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::PatternMatchValue<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::PatternMatchSingleton {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::PatternMatchSequence<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::PatternMatchMapping<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::PatternMatchClass<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::PatternMatchStar<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::PatternMatchAs<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::PatternMatchOr<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::TypeParamTypeVar<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::TypeParamTypeVarTuple<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::TypeParamParamSpec<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::InterpolatedStringFormatSpec<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::PatternArguments<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::PatternKeyword<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::Comprehension<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::Arguments<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::Parameters<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::Parameter<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ParameterWithDefault<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::Keyword<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::Alias<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::WithItem<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::MatchCase<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::Decorator<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::ElifElseClause<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::TypeParams<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::FString<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::TString<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::StringLiteral<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::BytesLiteral<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl crate::HasNodeIndex for crate::Identifier<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        &self.node_index
    }
}

impl<'ast> Mod<'ast> {
    #[allow(unused)]
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: crate::visitor::source_order::SourceOrderVisitor<'a> + ?Sized,
    {
        match self {
            Mod::Module(node) => node.visit_source_order(visitor),
            Mod::Expression(node) => node.visit_source_order(visitor),
        }
    }
}

impl<'ast> Stmt<'ast> {
    #[allow(unused)]
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
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

impl<'ast> Expr<'ast> {
    #[allow(unused)]
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
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
            Expr::TString(node) => node.visit_source_order(visitor),
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

impl<'ast> ExceptHandler<'ast> {
    #[allow(unused)]
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: crate::visitor::source_order::SourceOrderVisitor<'a> + ?Sized,
    {
        match self {
            ExceptHandler::ExceptHandler(node) => node.visit_source_order(visitor),
        }
    }
}

impl<'ast> InterpolatedStringElement<'ast> {
    #[allow(unused)]
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: crate::visitor::source_order::SourceOrderVisitor<'a> + ?Sized,
    {
        match self {
            InterpolatedStringElement::Interpolation(node) => node.visit_source_order(visitor),
            InterpolatedStringElement::Literal(node) => node.visit_source_order(visitor),
        }
    }
}

impl<'ast> Pattern<'ast> {
    #[allow(unused)]
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
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

impl<'ast> TypeParam<'ast> {
    #[allow(unused)]
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
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
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub enum ModRef<'a> {
    Module(&'a crate::ModModule<'a>),
    Expression(&'a crate::ModExpression<'a>),
}

impl<'a, 'ast: 'a> From<&'a Mod<'ast>> for ModRef<'a> {
    fn from(node: &'a Mod<'ast>) -> Self {
        match node {
            Mod::Module(node) => ModRef::Module(node),
            Mod::Expression(node) => ModRef::Expression(node),
        }
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ModModule<'ast>> for ModRef<'a> {
    fn from(node: &'a crate::ModModule<'ast>) -> Self {
        Self::Module(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ModExpression<'ast>> for ModRef<'a> {
    fn from(node: &'a crate::ModExpression<'ast>) -> Self {
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

impl crate::HasNodeIndex for ModRef<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        match self {
            Self::Module(node) => node.node_index(),
            Self::Expression(node) => node.node_index(),
        }
    }
}

/// See also [stmt](https://docs.python.org/3/library/ast.html#ast.stmt)
#[derive(Clone, Copy, Debug, PartialEq, is_macro::Is)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub enum StmtRef<'a> {
    #[is(name = "function_def_stmt")]
    FunctionDef(&'a crate::StmtFunctionDef<'a>),
    #[is(name = "class_def_stmt")]
    ClassDef(&'a crate::StmtClassDef<'a>),
    #[is(name = "return_stmt")]
    Return(&'a crate::StmtReturn<'a>),
    #[is(name = "delete_stmt")]
    Delete(&'a crate::StmtDelete<'a>),
    #[is(name = "type_alias_stmt")]
    TypeAlias(&'a crate::StmtTypeAlias<'a>),
    #[is(name = "assign_stmt")]
    Assign(&'a crate::StmtAssign<'a>),
    #[is(name = "aug_assign_stmt")]
    AugAssign(&'a crate::StmtAugAssign<'a>),
    #[is(name = "ann_assign_stmt")]
    AnnAssign(&'a crate::StmtAnnAssign<'a>),
    #[is(name = "for_stmt")]
    For(&'a crate::StmtFor<'a>),
    #[is(name = "while_stmt")]
    While(&'a crate::StmtWhile<'a>),
    #[is(name = "if_stmt")]
    If(&'a crate::StmtIf<'a>),
    #[is(name = "with_stmt")]
    With(&'a crate::StmtWith<'a>),
    #[is(name = "match_stmt")]
    Match(&'a crate::StmtMatch<'a>),
    #[is(name = "raise_stmt")]
    Raise(&'a crate::StmtRaise<'a>),
    #[is(name = "try_stmt")]
    Try(&'a crate::StmtTry<'a>),
    #[is(name = "assert_stmt")]
    Assert(&'a crate::StmtAssert<'a>),
    #[is(name = "import_stmt")]
    Import(&'a crate::StmtImport<'a>),
    #[is(name = "import_from_stmt")]
    ImportFrom(&'a crate::StmtImportFrom<'a>),
    #[is(name = "global_stmt")]
    Global(&'a crate::StmtGlobal<'a>),
    #[is(name = "nonlocal_stmt")]
    Nonlocal(&'a crate::StmtNonlocal<'a>),
    #[is(name = "expr_stmt")]
    Expr(&'a crate::StmtExpr<'a>),
    #[is(name = "pass_stmt")]
    Pass(&'a crate::StmtPass),
    #[is(name = "break_stmt")]
    Break(&'a crate::StmtBreak),
    #[is(name = "continue_stmt")]
    Continue(&'a crate::StmtContinue),
    #[is(name = "ipy_escape_command_stmt")]
    IpyEscapeCommand(&'a crate::StmtIpyEscapeCommand<'a>),
}

impl<'a, 'ast: 'a> From<&'a Stmt<'ast>> for StmtRef<'a> {
    fn from(node: &'a Stmt<'ast>) -> Self {
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

impl<'a, 'ast: 'a> From<&'a crate::StmtFunctionDef<'ast>> for StmtRef<'a> {
    fn from(node: &'a crate::StmtFunctionDef<'ast>) -> Self {
        Self::FunctionDef(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtClassDef<'ast>> for StmtRef<'a> {
    fn from(node: &'a crate::StmtClassDef<'ast>) -> Self {
        Self::ClassDef(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtReturn<'ast>> for StmtRef<'a> {
    fn from(node: &'a crate::StmtReturn<'ast>) -> Self {
        Self::Return(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtDelete<'ast>> for StmtRef<'a> {
    fn from(node: &'a crate::StmtDelete<'ast>) -> Self {
        Self::Delete(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtTypeAlias<'ast>> for StmtRef<'a> {
    fn from(node: &'a crate::StmtTypeAlias<'ast>) -> Self {
        Self::TypeAlias(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtAssign<'ast>> for StmtRef<'a> {
    fn from(node: &'a crate::StmtAssign<'ast>) -> Self {
        Self::Assign(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtAugAssign<'ast>> for StmtRef<'a> {
    fn from(node: &'a crate::StmtAugAssign<'ast>) -> Self {
        Self::AugAssign(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtAnnAssign<'ast>> for StmtRef<'a> {
    fn from(node: &'a crate::StmtAnnAssign<'ast>) -> Self {
        Self::AnnAssign(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtFor<'ast>> for StmtRef<'a> {
    fn from(node: &'a crate::StmtFor<'ast>) -> Self {
        Self::For(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtWhile<'ast>> for StmtRef<'a> {
    fn from(node: &'a crate::StmtWhile<'ast>) -> Self {
        Self::While(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtIf<'ast>> for StmtRef<'a> {
    fn from(node: &'a crate::StmtIf<'ast>) -> Self {
        Self::If(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtWith<'ast>> for StmtRef<'a> {
    fn from(node: &'a crate::StmtWith<'ast>) -> Self {
        Self::With(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtMatch<'ast>> for StmtRef<'a> {
    fn from(node: &'a crate::StmtMatch<'ast>) -> Self {
        Self::Match(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtRaise<'ast>> for StmtRef<'a> {
    fn from(node: &'a crate::StmtRaise<'ast>) -> Self {
        Self::Raise(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtTry<'ast>> for StmtRef<'a> {
    fn from(node: &'a crate::StmtTry<'ast>) -> Self {
        Self::Try(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtAssert<'ast>> for StmtRef<'a> {
    fn from(node: &'a crate::StmtAssert<'ast>) -> Self {
        Self::Assert(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtImport<'ast>> for StmtRef<'a> {
    fn from(node: &'a crate::StmtImport<'ast>) -> Self {
        Self::Import(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtImportFrom<'ast>> for StmtRef<'a> {
    fn from(node: &'a crate::StmtImportFrom<'ast>) -> Self {
        Self::ImportFrom(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtGlobal<'ast>> for StmtRef<'a> {
    fn from(node: &'a crate::StmtGlobal<'ast>) -> Self {
        Self::Global(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtNonlocal<'ast>> for StmtRef<'a> {
    fn from(node: &'a crate::StmtNonlocal<'ast>) -> Self {
        Self::Nonlocal(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtExpr<'ast>> for StmtRef<'a> {
    fn from(node: &'a crate::StmtExpr<'ast>) -> Self {
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

impl<'a, 'ast: 'a> From<&'a crate::StmtIpyEscapeCommand<'ast>> for StmtRef<'a> {
    fn from(node: &'a crate::StmtIpyEscapeCommand<'ast>) -> Self {
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

impl crate::HasNodeIndex for StmtRef<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        match self {
            Self::FunctionDef(node) => node.node_index(),
            Self::ClassDef(node) => node.node_index(),
            Self::Return(node) => node.node_index(),
            Self::Delete(node) => node.node_index(),
            Self::TypeAlias(node) => node.node_index(),
            Self::Assign(node) => node.node_index(),
            Self::AugAssign(node) => node.node_index(),
            Self::AnnAssign(node) => node.node_index(),
            Self::For(node) => node.node_index(),
            Self::While(node) => node.node_index(),
            Self::If(node) => node.node_index(),
            Self::With(node) => node.node_index(),
            Self::Match(node) => node.node_index(),
            Self::Raise(node) => node.node_index(),
            Self::Try(node) => node.node_index(),
            Self::Assert(node) => node.node_index(),
            Self::Import(node) => node.node_index(),
            Self::ImportFrom(node) => node.node_index(),
            Self::Global(node) => node.node_index(),
            Self::Nonlocal(node) => node.node_index(),
            Self::Expr(node) => node.node_index(),
            Self::Pass(node) => node.node_index(),
            Self::Break(node) => node.node_index(),
            Self::Continue(node) => node.node_index(),
            Self::IpyEscapeCommand(node) => node.node_index(),
        }
    }
}

/// See also [expr](https://docs.python.org/3/library/ast.html#ast.expr)
#[derive(Clone, Copy, Debug, PartialEq, is_macro::Is)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub enum ExprRef<'a> {
    #[is(name = "bool_op_expr")]
    BoolOp(&'a crate::ExprBoolOp<'a>),
    #[is(name = "named_expr")]
    Named(&'a crate::ExprNamed<'a>),
    #[is(name = "bin_op_expr")]
    BinOp(&'a crate::ExprBinOp<'a>),
    #[is(name = "unary_op_expr")]
    UnaryOp(&'a crate::ExprUnaryOp<'a>),
    #[is(name = "lambda_expr")]
    Lambda(&'a crate::ExprLambda<'a>),
    #[is(name = "if_expr")]
    If(&'a crate::ExprIf<'a>),
    #[is(name = "dict_expr")]
    Dict(&'a crate::ExprDict<'a>),
    #[is(name = "set_expr")]
    Set(&'a crate::ExprSet<'a>),
    #[is(name = "list_comp_expr")]
    ListComp(&'a crate::ExprListComp<'a>),
    #[is(name = "set_comp_expr")]
    SetComp(&'a crate::ExprSetComp<'a>),
    #[is(name = "dict_comp_expr")]
    DictComp(&'a crate::ExprDictComp<'a>),
    #[is(name = "generator_expr")]
    Generator(&'a crate::ExprGenerator<'a>),
    #[is(name = "await_expr")]
    Await(&'a crate::ExprAwait<'a>),
    #[is(name = "yield_expr")]
    Yield(&'a crate::ExprYield<'a>),
    #[is(name = "yield_from_expr")]
    YieldFrom(&'a crate::ExprYieldFrom<'a>),
    #[is(name = "compare_expr")]
    Compare(&'a crate::ExprCompare<'a>),
    #[is(name = "call_expr")]
    Call(&'a crate::ExprCall<'a>),
    #[is(name = "f_string_expr")]
    FString(&'a crate::ExprFString<'a>),
    #[is(name = "t_string_expr")]
    TString(&'a crate::ExprTString<'a>),
    #[is(name = "string_literal_expr")]
    StringLiteral(&'a crate::ExprStringLiteral<'a>),
    #[is(name = "bytes_literal_expr")]
    BytesLiteral(&'a crate::ExprBytesLiteral<'a>),
    #[is(name = "number_literal_expr")]
    NumberLiteral(&'a crate::ExprNumberLiteral<'a>),
    #[is(name = "boolean_literal_expr")]
    BooleanLiteral(&'a crate::ExprBooleanLiteral),
    #[is(name = "none_literal_expr")]
    NoneLiteral(&'a crate::ExprNoneLiteral),
    #[is(name = "ellipsis_literal_expr")]
    EllipsisLiteral(&'a crate::ExprEllipsisLiteral),
    #[is(name = "attribute_expr")]
    Attribute(&'a crate::ExprAttribute<'a>),
    #[is(name = "subscript_expr")]
    Subscript(&'a crate::ExprSubscript<'a>),
    #[is(name = "starred_expr")]
    Starred(&'a crate::ExprStarred<'a>),
    #[is(name = "name_expr")]
    Name(&'a crate::ExprName<'a>),
    #[is(name = "list_expr")]
    List(&'a crate::ExprList<'a>),
    #[is(name = "tuple_expr")]
    Tuple(&'a crate::ExprTuple<'a>),
    #[is(name = "slice_expr")]
    Slice(&'a crate::ExprSlice<'a>),
    #[is(name = "ipy_escape_command_expr")]
    IpyEscapeCommand(&'a crate::ExprIpyEscapeCommand<'a>),
}

impl<'a, 'ast: 'a> From<&'a Expr<'ast>> for ExprRef<'a> {
    fn from(node: &'a Expr<'ast>) -> Self {
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
            Expr::TString(node) => ExprRef::TString(node),
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

impl<'a, 'ast: 'a> From<&'a crate::ExprBoolOp<'ast>> for ExprRef<'a> {
    fn from(node: &'a crate::ExprBoolOp<'ast>) -> Self {
        Self::BoolOp(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprNamed<'ast>> for ExprRef<'a> {
    fn from(node: &'a crate::ExprNamed<'ast>) -> Self {
        Self::Named(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprBinOp<'ast>> for ExprRef<'a> {
    fn from(node: &'a crate::ExprBinOp<'ast>) -> Self {
        Self::BinOp(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprUnaryOp<'ast>> for ExprRef<'a> {
    fn from(node: &'a crate::ExprUnaryOp<'ast>) -> Self {
        Self::UnaryOp(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprLambda<'ast>> for ExprRef<'a> {
    fn from(node: &'a crate::ExprLambda<'ast>) -> Self {
        Self::Lambda(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprIf<'ast>> for ExprRef<'a> {
    fn from(node: &'a crate::ExprIf<'ast>) -> Self {
        Self::If(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprDict<'ast>> for ExprRef<'a> {
    fn from(node: &'a crate::ExprDict<'ast>) -> Self {
        Self::Dict(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprSet<'ast>> for ExprRef<'a> {
    fn from(node: &'a crate::ExprSet<'ast>) -> Self {
        Self::Set(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprListComp<'ast>> for ExprRef<'a> {
    fn from(node: &'a crate::ExprListComp<'ast>) -> Self {
        Self::ListComp(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprSetComp<'ast>> for ExprRef<'a> {
    fn from(node: &'a crate::ExprSetComp<'ast>) -> Self {
        Self::SetComp(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprDictComp<'ast>> for ExprRef<'a> {
    fn from(node: &'a crate::ExprDictComp<'ast>) -> Self {
        Self::DictComp(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprGenerator<'ast>> for ExprRef<'a> {
    fn from(node: &'a crate::ExprGenerator<'ast>) -> Self {
        Self::Generator(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprAwait<'ast>> for ExprRef<'a> {
    fn from(node: &'a crate::ExprAwait<'ast>) -> Self {
        Self::Await(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprYield<'ast>> for ExprRef<'a> {
    fn from(node: &'a crate::ExprYield<'ast>) -> Self {
        Self::Yield(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprYieldFrom<'ast>> for ExprRef<'a> {
    fn from(node: &'a crate::ExprYieldFrom<'ast>) -> Self {
        Self::YieldFrom(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprCompare<'ast>> for ExprRef<'a> {
    fn from(node: &'a crate::ExprCompare<'ast>) -> Self {
        Self::Compare(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprCall<'ast>> for ExprRef<'a> {
    fn from(node: &'a crate::ExprCall<'ast>) -> Self {
        Self::Call(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprFString<'ast>> for ExprRef<'a> {
    fn from(node: &'a crate::ExprFString<'ast>) -> Self {
        Self::FString(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprTString<'ast>> for ExprRef<'a> {
    fn from(node: &'a crate::ExprTString<'ast>) -> Self {
        Self::TString(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprStringLiteral<'ast>> for ExprRef<'a> {
    fn from(node: &'a crate::ExprStringLiteral<'ast>) -> Self {
        Self::StringLiteral(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprBytesLiteral<'ast>> for ExprRef<'a> {
    fn from(node: &'a crate::ExprBytesLiteral<'ast>) -> Self {
        Self::BytesLiteral(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprNumberLiteral<'ast>> for ExprRef<'a> {
    fn from(node: &'a crate::ExprNumberLiteral<'ast>) -> Self {
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

impl<'a, 'ast: 'a> From<&'a crate::ExprAttribute<'ast>> for ExprRef<'a> {
    fn from(node: &'a crate::ExprAttribute<'ast>) -> Self {
        Self::Attribute(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprSubscript<'ast>> for ExprRef<'a> {
    fn from(node: &'a crate::ExprSubscript<'ast>) -> Self {
        Self::Subscript(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprStarred<'ast>> for ExprRef<'a> {
    fn from(node: &'a crate::ExprStarred<'ast>) -> Self {
        Self::Starred(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprName<'ast>> for ExprRef<'a> {
    fn from(node: &'a crate::ExprName<'ast>) -> Self {
        Self::Name(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprList<'ast>> for ExprRef<'a> {
    fn from(node: &'a crate::ExprList<'ast>) -> Self {
        Self::List(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprTuple<'ast>> for ExprRef<'a> {
    fn from(node: &'a crate::ExprTuple<'ast>) -> Self {
        Self::Tuple(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprSlice<'ast>> for ExprRef<'a> {
    fn from(node: &'a crate::ExprSlice<'ast>) -> Self {
        Self::Slice(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprIpyEscapeCommand<'ast>> for ExprRef<'a> {
    fn from(node: &'a crate::ExprIpyEscapeCommand<'ast>) -> Self {
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
            Self::TString(node) => node.range(),
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

impl crate::HasNodeIndex for ExprRef<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        match self {
            Self::BoolOp(node) => node.node_index(),
            Self::Named(node) => node.node_index(),
            Self::BinOp(node) => node.node_index(),
            Self::UnaryOp(node) => node.node_index(),
            Self::Lambda(node) => node.node_index(),
            Self::If(node) => node.node_index(),
            Self::Dict(node) => node.node_index(),
            Self::Set(node) => node.node_index(),
            Self::ListComp(node) => node.node_index(),
            Self::SetComp(node) => node.node_index(),
            Self::DictComp(node) => node.node_index(),
            Self::Generator(node) => node.node_index(),
            Self::Await(node) => node.node_index(),
            Self::Yield(node) => node.node_index(),
            Self::YieldFrom(node) => node.node_index(),
            Self::Compare(node) => node.node_index(),
            Self::Call(node) => node.node_index(),
            Self::FString(node) => node.node_index(),
            Self::TString(node) => node.node_index(),
            Self::StringLiteral(node) => node.node_index(),
            Self::BytesLiteral(node) => node.node_index(),
            Self::NumberLiteral(node) => node.node_index(),
            Self::BooleanLiteral(node) => node.node_index(),
            Self::NoneLiteral(node) => node.node_index(),
            Self::EllipsisLiteral(node) => node.node_index(),
            Self::Attribute(node) => node.node_index(),
            Self::Subscript(node) => node.node_index(),
            Self::Starred(node) => node.node_index(),
            Self::Name(node) => node.node_index(),
            Self::List(node) => node.node_index(),
            Self::Tuple(node) => node.node_index(),
            Self::Slice(node) => node.node_index(),
            Self::IpyEscapeCommand(node) => node.node_index(),
        }
    }
}

/// See also [excepthandler](https://docs.python.org/3/library/ast.html#ast.excepthandler)
#[derive(Clone, Copy, Debug, PartialEq, is_macro::Is)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub enum ExceptHandlerRef<'a> {
    ExceptHandler(&'a crate::ExceptHandlerExceptHandler<'a>),
}

impl<'a, 'ast: 'a> From<&'a ExceptHandler<'ast>> for ExceptHandlerRef<'a> {
    fn from(node: &'a ExceptHandler<'ast>) -> Self {
        match node {
            ExceptHandler::ExceptHandler(node) => ExceptHandlerRef::ExceptHandler(node),
        }
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExceptHandlerExceptHandler<'ast>> for ExceptHandlerRef<'a> {
    fn from(node: &'a crate::ExceptHandlerExceptHandler<'ast>) -> Self {
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

impl crate::HasNodeIndex for ExceptHandlerRef<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        match self {
            Self::ExceptHandler(node) => node.node_index(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, is_macro::Is)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub enum InterpolatedStringElementRef<'a> {
    Interpolation(&'a crate::InterpolatedElement<'a>),
    Literal(&'a crate::InterpolatedStringLiteralElement<'a>),
}

impl<'a, 'ast: 'a> From<&'a InterpolatedStringElement<'ast>> for InterpolatedStringElementRef<'a> {
    fn from(node: &'a InterpolatedStringElement<'ast>) -> Self {
        match node {
            InterpolatedStringElement::Interpolation(node) => {
                InterpolatedStringElementRef::Interpolation(node)
            }
            InterpolatedStringElement::Literal(node) => InterpolatedStringElementRef::Literal(node),
        }
    }
}

impl<'a, 'ast: 'a> From<&'a crate::InterpolatedElement<'ast>> for InterpolatedStringElementRef<'a> {
    fn from(node: &'a crate::InterpolatedElement<'ast>) -> Self {
        Self::Interpolation(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::InterpolatedStringLiteralElement<'ast>>
    for InterpolatedStringElementRef<'a>
{
    fn from(node: &'a crate::InterpolatedStringLiteralElement<'ast>) -> Self {
        Self::Literal(node)
    }
}

impl ruff_text_size::Ranged for InterpolatedStringElementRef<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        match self {
            Self::Interpolation(node) => node.range(),
            Self::Literal(node) => node.range(),
        }
    }
}

impl crate::HasNodeIndex for InterpolatedStringElementRef<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        match self {
            Self::Interpolation(node) => node.node_index(),
            Self::Literal(node) => node.node_index(),
        }
    }
}

/// See also [pattern](https://docs.python.org/3/library/ast.html#ast.pattern)
#[derive(Clone, Copy, Debug, PartialEq, is_macro::Is)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub enum PatternRef<'a> {
    MatchValue(&'a crate::PatternMatchValue<'a>),
    MatchSingleton(&'a crate::PatternMatchSingleton),
    MatchSequence(&'a crate::PatternMatchSequence<'a>),
    MatchMapping(&'a crate::PatternMatchMapping<'a>),
    MatchClass(&'a crate::PatternMatchClass<'a>),
    MatchStar(&'a crate::PatternMatchStar<'a>),
    MatchAs(&'a crate::PatternMatchAs<'a>),
    MatchOr(&'a crate::PatternMatchOr<'a>),
}

impl<'a, 'ast: 'a> From<&'a Pattern<'ast>> for PatternRef<'a> {
    fn from(node: &'a Pattern<'ast>) -> Self {
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

impl<'a, 'ast: 'a> From<&'a crate::PatternMatchValue<'ast>> for PatternRef<'a> {
    fn from(node: &'a crate::PatternMatchValue<'ast>) -> Self {
        Self::MatchValue(node)
    }
}

impl<'a> From<&'a crate::PatternMatchSingleton> for PatternRef<'a> {
    fn from(node: &'a crate::PatternMatchSingleton) -> Self {
        Self::MatchSingleton(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::PatternMatchSequence<'ast>> for PatternRef<'a> {
    fn from(node: &'a crate::PatternMatchSequence<'ast>) -> Self {
        Self::MatchSequence(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::PatternMatchMapping<'ast>> for PatternRef<'a> {
    fn from(node: &'a crate::PatternMatchMapping<'ast>) -> Self {
        Self::MatchMapping(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::PatternMatchClass<'ast>> for PatternRef<'a> {
    fn from(node: &'a crate::PatternMatchClass<'ast>) -> Self {
        Self::MatchClass(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::PatternMatchStar<'ast>> for PatternRef<'a> {
    fn from(node: &'a crate::PatternMatchStar<'ast>) -> Self {
        Self::MatchStar(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::PatternMatchAs<'ast>> for PatternRef<'a> {
    fn from(node: &'a crate::PatternMatchAs<'ast>) -> Self {
        Self::MatchAs(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::PatternMatchOr<'ast>> for PatternRef<'a> {
    fn from(node: &'a crate::PatternMatchOr<'ast>) -> Self {
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

impl crate::HasNodeIndex for PatternRef<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        match self {
            Self::MatchValue(node) => node.node_index(),
            Self::MatchSingleton(node) => node.node_index(),
            Self::MatchSequence(node) => node.node_index(),
            Self::MatchMapping(node) => node.node_index(),
            Self::MatchClass(node) => node.node_index(),
            Self::MatchStar(node) => node.node_index(),
            Self::MatchAs(node) => node.node_index(),
            Self::MatchOr(node) => node.node_index(),
        }
    }
}

/// See also [type_param](https://docs.python.org/3/library/ast.html#ast.type_param)
#[derive(Clone, Copy, Debug, PartialEq, is_macro::Is)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub enum TypeParamRef<'a> {
    TypeVar(&'a crate::TypeParamTypeVar<'a>),
    TypeVarTuple(&'a crate::TypeParamTypeVarTuple<'a>),
    ParamSpec(&'a crate::TypeParamParamSpec<'a>),
}

impl<'a, 'ast: 'a> From<&'a TypeParam<'ast>> for TypeParamRef<'a> {
    fn from(node: &'a TypeParam<'ast>) -> Self {
        match node {
            TypeParam::TypeVar(node) => TypeParamRef::TypeVar(node),
            TypeParam::TypeVarTuple(node) => TypeParamRef::TypeVarTuple(node),
            TypeParam::ParamSpec(node) => TypeParamRef::ParamSpec(node),
        }
    }
}

impl<'a, 'ast: 'a> From<&'a crate::TypeParamTypeVar<'ast>> for TypeParamRef<'a> {
    fn from(node: &'a crate::TypeParamTypeVar<'ast>) -> Self {
        Self::TypeVar(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::TypeParamTypeVarTuple<'ast>> for TypeParamRef<'a> {
    fn from(node: &'a crate::TypeParamTypeVarTuple<'ast>) -> Self {
        Self::TypeVarTuple(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::TypeParamParamSpec<'ast>> for TypeParamRef<'a> {
    fn from(node: &'a crate::TypeParamParamSpec<'ast>) -> Self {
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

impl crate::HasNodeIndex for TypeParamRef<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        match self {
            Self::TypeVar(node) => node.node_index(),
            Self::TypeVarTuple(node) => node.node_index(),
            Self::ParamSpec(node) => node.node_index(),
        }
    }
}

/// A flattened enumeration of all AST nodes.
#[derive(Copy, Clone, Debug, is_macro::Is, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub enum AnyNodeRef<'a> {
    ModModule(&'a crate::ModModule<'a>),
    ModExpression(&'a crate::ModExpression<'a>),
    StmtFunctionDef(&'a crate::StmtFunctionDef<'a>),
    StmtClassDef(&'a crate::StmtClassDef<'a>),
    StmtReturn(&'a crate::StmtReturn<'a>),
    StmtDelete(&'a crate::StmtDelete<'a>),
    StmtTypeAlias(&'a crate::StmtTypeAlias<'a>),
    StmtAssign(&'a crate::StmtAssign<'a>),
    StmtAugAssign(&'a crate::StmtAugAssign<'a>),
    StmtAnnAssign(&'a crate::StmtAnnAssign<'a>),
    StmtFor(&'a crate::StmtFor<'a>),
    StmtWhile(&'a crate::StmtWhile<'a>),
    StmtIf(&'a crate::StmtIf<'a>),
    StmtWith(&'a crate::StmtWith<'a>),
    StmtMatch(&'a crate::StmtMatch<'a>),
    StmtRaise(&'a crate::StmtRaise<'a>),
    StmtTry(&'a crate::StmtTry<'a>),
    StmtAssert(&'a crate::StmtAssert<'a>),
    StmtImport(&'a crate::StmtImport<'a>),
    StmtImportFrom(&'a crate::StmtImportFrom<'a>),
    StmtGlobal(&'a crate::StmtGlobal<'a>),
    StmtNonlocal(&'a crate::StmtNonlocal<'a>),
    StmtExpr(&'a crate::StmtExpr<'a>),
    StmtPass(&'a crate::StmtPass),
    StmtBreak(&'a crate::StmtBreak),
    StmtContinue(&'a crate::StmtContinue),
    StmtIpyEscapeCommand(&'a crate::StmtIpyEscapeCommand<'a>),
    ExprBoolOp(&'a crate::ExprBoolOp<'a>),
    ExprNamed(&'a crate::ExprNamed<'a>),
    ExprBinOp(&'a crate::ExprBinOp<'a>),
    ExprUnaryOp(&'a crate::ExprUnaryOp<'a>),
    ExprLambda(&'a crate::ExprLambda<'a>),
    ExprIf(&'a crate::ExprIf<'a>),
    ExprDict(&'a crate::ExprDict<'a>),
    ExprSet(&'a crate::ExprSet<'a>),
    ExprListComp(&'a crate::ExprListComp<'a>),
    ExprSetComp(&'a crate::ExprSetComp<'a>),
    ExprDictComp(&'a crate::ExprDictComp<'a>),
    ExprGenerator(&'a crate::ExprGenerator<'a>),
    ExprAwait(&'a crate::ExprAwait<'a>),
    ExprYield(&'a crate::ExprYield<'a>),
    ExprYieldFrom(&'a crate::ExprYieldFrom<'a>),
    ExprCompare(&'a crate::ExprCompare<'a>),
    ExprCall(&'a crate::ExprCall<'a>),
    ExprFString(&'a crate::ExprFString<'a>),
    ExprTString(&'a crate::ExprTString<'a>),
    ExprStringLiteral(&'a crate::ExprStringLiteral<'a>),
    ExprBytesLiteral(&'a crate::ExprBytesLiteral<'a>),
    ExprNumberLiteral(&'a crate::ExprNumberLiteral<'a>),
    ExprBooleanLiteral(&'a crate::ExprBooleanLiteral),
    ExprNoneLiteral(&'a crate::ExprNoneLiteral),
    ExprEllipsisLiteral(&'a crate::ExprEllipsisLiteral),
    ExprAttribute(&'a crate::ExprAttribute<'a>),
    ExprSubscript(&'a crate::ExprSubscript<'a>),
    ExprStarred(&'a crate::ExprStarred<'a>),
    ExprName(&'a crate::ExprName<'a>),
    ExprList(&'a crate::ExprList<'a>),
    ExprTuple(&'a crate::ExprTuple<'a>),
    ExprSlice(&'a crate::ExprSlice<'a>),
    ExprIpyEscapeCommand(&'a crate::ExprIpyEscapeCommand<'a>),
    ExceptHandlerExceptHandler(&'a crate::ExceptHandlerExceptHandler<'a>),
    InterpolatedElement(&'a crate::InterpolatedElement<'a>),
    InterpolatedStringLiteralElement(&'a crate::InterpolatedStringLiteralElement<'a>),
    PatternMatchValue(&'a crate::PatternMatchValue<'a>),
    PatternMatchSingleton(&'a crate::PatternMatchSingleton),
    PatternMatchSequence(&'a crate::PatternMatchSequence<'a>),
    PatternMatchMapping(&'a crate::PatternMatchMapping<'a>),
    PatternMatchClass(&'a crate::PatternMatchClass<'a>),
    PatternMatchStar(&'a crate::PatternMatchStar<'a>),
    PatternMatchAs(&'a crate::PatternMatchAs<'a>),
    PatternMatchOr(&'a crate::PatternMatchOr<'a>),
    TypeParamTypeVar(&'a crate::TypeParamTypeVar<'a>),
    TypeParamTypeVarTuple(&'a crate::TypeParamTypeVarTuple<'a>),
    TypeParamParamSpec(&'a crate::TypeParamParamSpec<'a>),
    InterpolatedStringFormatSpec(&'a crate::InterpolatedStringFormatSpec<'a>),
    PatternArguments(&'a crate::PatternArguments<'a>),
    PatternKeyword(&'a crate::PatternKeyword<'a>),
    Comprehension(&'a crate::Comprehension<'a>),
    Arguments(&'a crate::Arguments<'a>),
    Parameters(&'a crate::Parameters<'a>),
    Parameter(&'a crate::Parameter<'a>),
    ParameterWithDefault(&'a crate::ParameterWithDefault<'a>),
    Keyword(&'a crate::Keyword<'a>),
    Alias(&'a crate::Alias<'a>),
    WithItem(&'a crate::WithItem<'a>),
    MatchCase(&'a crate::MatchCase<'a>),
    Decorator(&'a crate::Decorator<'a>),
    ElifElseClause(&'a crate::ElifElseClause<'a>),
    TypeParams(&'a crate::TypeParams<'a>),
    FString(&'a crate::FString<'a>),
    TString(&'a crate::TString<'a>),
    StringLiteral(&'a crate::StringLiteral<'a>),
    BytesLiteral(&'a crate::BytesLiteral<'a>),
    Identifier(&'a crate::Identifier<'a>),
}

impl<'a, 'ast: 'a> From<&'a Mod<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a Mod<'ast>) -> AnyNodeRef<'a> {
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

impl<'a> AnyNodeRef<'a> {
    pub fn as_mod_ref(self) -> Option<ModRef<'a>> {
        match self {
            Self::ModModule(node) => Some(ModRef::Module(node)),
            Self::ModExpression(node) => Some(ModRef::Expression(node)),

            _ => None,
        }
    }
}

impl<'a, 'ast: 'a> From<&'a Stmt<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a Stmt<'ast>) -> AnyNodeRef<'a> {
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

impl<'a> AnyNodeRef<'a> {
    pub fn as_stmt_ref(self) -> Option<StmtRef<'a>> {
        match self {
            Self::StmtFunctionDef(node) => Some(StmtRef::FunctionDef(node)),
            Self::StmtClassDef(node) => Some(StmtRef::ClassDef(node)),
            Self::StmtReturn(node) => Some(StmtRef::Return(node)),
            Self::StmtDelete(node) => Some(StmtRef::Delete(node)),
            Self::StmtTypeAlias(node) => Some(StmtRef::TypeAlias(node)),
            Self::StmtAssign(node) => Some(StmtRef::Assign(node)),
            Self::StmtAugAssign(node) => Some(StmtRef::AugAssign(node)),
            Self::StmtAnnAssign(node) => Some(StmtRef::AnnAssign(node)),
            Self::StmtFor(node) => Some(StmtRef::For(node)),
            Self::StmtWhile(node) => Some(StmtRef::While(node)),
            Self::StmtIf(node) => Some(StmtRef::If(node)),
            Self::StmtWith(node) => Some(StmtRef::With(node)),
            Self::StmtMatch(node) => Some(StmtRef::Match(node)),
            Self::StmtRaise(node) => Some(StmtRef::Raise(node)),
            Self::StmtTry(node) => Some(StmtRef::Try(node)),
            Self::StmtAssert(node) => Some(StmtRef::Assert(node)),
            Self::StmtImport(node) => Some(StmtRef::Import(node)),
            Self::StmtImportFrom(node) => Some(StmtRef::ImportFrom(node)),
            Self::StmtGlobal(node) => Some(StmtRef::Global(node)),
            Self::StmtNonlocal(node) => Some(StmtRef::Nonlocal(node)),
            Self::StmtExpr(node) => Some(StmtRef::Expr(node)),
            Self::StmtPass(node) => Some(StmtRef::Pass(node)),
            Self::StmtBreak(node) => Some(StmtRef::Break(node)),
            Self::StmtContinue(node) => Some(StmtRef::Continue(node)),
            Self::StmtIpyEscapeCommand(node) => Some(StmtRef::IpyEscapeCommand(node)),

            _ => None,
        }
    }
}

impl<'a, 'ast: 'a> From<&'a Expr<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a Expr<'ast>) -> AnyNodeRef<'a> {
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
            Expr::TString(node) => AnyNodeRef::ExprTString(node),
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
            ExprRef::TString(node) => AnyNodeRef::ExprTString(node),
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

impl<'a> AnyNodeRef<'a> {
    pub fn as_expr_ref(self) -> Option<ExprRef<'a>> {
        match self {
            Self::ExprBoolOp(node) => Some(ExprRef::BoolOp(node)),
            Self::ExprNamed(node) => Some(ExprRef::Named(node)),
            Self::ExprBinOp(node) => Some(ExprRef::BinOp(node)),
            Self::ExprUnaryOp(node) => Some(ExprRef::UnaryOp(node)),
            Self::ExprLambda(node) => Some(ExprRef::Lambda(node)),
            Self::ExprIf(node) => Some(ExprRef::If(node)),
            Self::ExprDict(node) => Some(ExprRef::Dict(node)),
            Self::ExprSet(node) => Some(ExprRef::Set(node)),
            Self::ExprListComp(node) => Some(ExprRef::ListComp(node)),
            Self::ExprSetComp(node) => Some(ExprRef::SetComp(node)),
            Self::ExprDictComp(node) => Some(ExprRef::DictComp(node)),
            Self::ExprGenerator(node) => Some(ExprRef::Generator(node)),
            Self::ExprAwait(node) => Some(ExprRef::Await(node)),
            Self::ExprYield(node) => Some(ExprRef::Yield(node)),
            Self::ExprYieldFrom(node) => Some(ExprRef::YieldFrom(node)),
            Self::ExprCompare(node) => Some(ExprRef::Compare(node)),
            Self::ExprCall(node) => Some(ExprRef::Call(node)),
            Self::ExprFString(node) => Some(ExprRef::FString(node)),
            Self::ExprTString(node) => Some(ExprRef::TString(node)),
            Self::ExprStringLiteral(node) => Some(ExprRef::StringLiteral(node)),
            Self::ExprBytesLiteral(node) => Some(ExprRef::BytesLiteral(node)),
            Self::ExprNumberLiteral(node) => Some(ExprRef::NumberLiteral(node)),
            Self::ExprBooleanLiteral(node) => Some(ExprRef::BooleanLiteral(node)),
            Self::ExprNoneLiteral(node) => Some(ExprRef::NoneLiteral(node)),
            Self::ExprEllipsisLiteral(node) => Some(ExprRef::EllipsisLiteral(node)),
            Self::ExprAttribute(node) => Some(ExprRef::Attribute(node)),
            Self::ExprSubscript(node) => Some(ExprRef::Subscript(node)),
            Self::ExprStarred(node) => Some(ExprRef::Starred(node)),
            Self::ExprName(node) => Some(ExprRef::Name(node)),
            Self::ExprList(node) => Some(ExprRef::List(node)),
            Self::ExprTuple(node) => Some(ExprRef::Tuple(node)),
            Self::ExprSlice(node) => Some(ExprRef::Slice(node)),
            Self::ExprIpyEscapeCommand(node) => Some(ExprRef::IpyEscapeCommand(node)),

            _ => None,
        }
    }
}

impl<'a, 'ast: 'a> From<&'a ExceptHandler<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a ExceptHandler<'ast>) -> AnyNodeRef<'a> {
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

impl<'a> AnyNodeRef<'a> {
    pub fn as_except_handler_ref(self) -> Option<ExceptHandlerRef<'a>> {
        match self {
            Self::ExceptHandlerExceptHandler(node) => Some(ExceptHandlerRef::ExceptHandler(node)),

            _ => None,
        }
    }
}

impl<'a, 'ast: 'a> From<&'a InterpolatedStringElement<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a InterpolatedStringElement<'ast>) -> AnyNodeRef<'a> {
        match node {
            InterpolatedStringElement::Interpolation(node) => AnyNodeRef::InterpolatedElement(node),
            InterpolatedStringElement::Literal(node) => {
                AnyNodeRef::InterpolatedStringLiteralElement(node)
            }
        }
    }
}

impl<'a> From<InterpolatedStringElementRef<'a>> for AnyNodeRef<'a> {
    fn from(node: InterpolatedStringElementRef<'a>) -> AnyNodeRef<'a> {
        match node {
            InterpolatedStringElementRef::Interpolation(node) => {
                AnyNodeRef::InterpolatedElement(node)
            }
            InterpolatedStringElementRef::Literal(node) => {
                AnyNodeRef::InterpolatedStringLiteralElement(node)
            }
        }
    }
}

impl<'a> AnyNodeRef<'a> {
    pub fn as_interpolated_string_element_ref(self) -> Option<InterpolatedStringElementRef<'a>> {
        match self {
            Self::InterpolatedElement(node) => {
                Some(InterpolatedStringElementRef::Interpolation(node))
            }
            Self::InterpolatedStringLiteralElement(node) => {
                Some(InterpolatedStringElementRef::Literal(node))
            }

            _ => None,
        }
    }
}

impl<'a, 'ast: 'a> From<&'a Pattern<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a Pattern<'ast>) -> AnyNodeRef<'a> {
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

impl<'a> AnyNodeRef<'a> {
    pub fn as_pattern_ref(self) -> Option<PatternRef<'a>> {
        match self {
            Self::PatternMatchValue(node) => Some(PatternRef::MatchValue(node)),
            Self::PatternMatchSingleton(node) => Some(PatternRef::MatchSingleton(node)),
            Self::PatternMatchSequence(node) => Some(PatternRef::MatchSequence(node)),
            Self::PatternMatchMapping(node) => Some(PatternRef::MatchMapping(node)),
            Self::PatternMatchClass(node) => Some(PatternRef::MatchClass(node)),
            Self::PatternMatchStar(node) => Some(PatternRef::MatchStar(node)),
            Self::PatternMatchAs(node) => Some(PatternRef::MatchAs(node)),
            Self::PatternMatchOr(node) => Some(PatternRef::MatchOr(node)),

            _ => None,
        }
    }
}

impl<'a, 'ast: 'a> From<&'a TypeParam<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a TypeParam<'ast>) -> AnyNodeRef<'a> {
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

impl<'a> AnyNodeRef<'a> {
    pub fn as_type_param_ref(self) -> Option<TypeParamRef<'a>> {
        match self {
            Self::TypeParamTypeVar(node) => Some(TypeParamRef::TypeVar(node)),
            Self::TypeParamTypeVarTuple(node) => Some(TypeParamRef::TypeVarTuple(node)),
            Self::TypeParamParamSpec(node) => Some(TypeParamRef::ParamSpec(node)),

            _ => None,
        }
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ModModule<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ModModule<'ast>) -> Self {
        AnyNodeRef::ModModule(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ModExpression<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ModExpression<'ast>) -> Self {
        AnyNodeRef::ModExpression(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtFunctionDef<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtFunctionDef<'ast>) -> Self {
        AnyNodeRef::StmtFunctionDef(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtClassDef<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtClassDef<'ast>) -> Self {
        AnyNodeRef::StmtClassDef(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtReturn<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtReturn<'ast>) -> Self {
        AnyNodeRef::StmtReturn(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtDelete<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtDelete<'ast>) -> Self {
        AnyNodeRef::StmtDelete(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtTypeAlias<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtTypeAlias<'ast>) -> Self {
        AnyNodeRef::StmtTypeAlias(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtAssign<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtAssign<'ast>) -> Self {
        AnyNodeRef::StmtAssign(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtAugAssign<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtAugAssign<'ast>) -> Self {
        AnyNodeRef::StmtAugAssign(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtAnnAssign<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtAnnAssign<'ast>) -> Self {
        AnyNodeRef::StmtAnnAssign(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtFor<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtFor<'ast>) -> Self {
        AnyNodeRef::StmtFor(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtWhile<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtWhile<'ast>) -> Self {
        AnyNodeRef::StmtWhile(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtIf<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtIf<'ast>) -> Self {
        AnyNodeRef::StmtIf(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtWith<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtWith<'ast>) -> Self {
        AnyNodeRef::StmtWith(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtMatch<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtMatch<'ast>) -> Self {
        AnyNodeRef::StmtMatch(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtRaise<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtRaise<'ast>) -> Self {
        AnyNodeRef::StmtRaise(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtTry<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtTry<'ast>) -> Self {
        AnyNodeRef::StmtTry(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtAssert<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtAssert<'ast>) -> Self {
        AnyNodeRef::StmtAssert(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtImport<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtImport<'ast>) -> Self {
        AnyNodeRef::StmtImport(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtImportFrom<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtImportFrom<'ast>) -> Self {
        AnyNodeRef::StmtImportFrom(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtGlobal<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtGlobal<'ast>) -> Self {
        AnyNodeRef::StmtGlobal(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtNonlocal<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtNonlocal<'ast>) -> Self {
        AnyNodeRef::StmtNonlocal(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtExpr<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtExpr<'ast>) -> Self {
        AnyNodeRef::StmtExpr(node)
    }
}

impl<'a> From<&'a crate::StmtPass> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtPass) -> Self {
        AnyNodeRef::StmtPass(node)
    }
}

impl<'a> From<&'a crate::StmtBreak> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtBreak) -> Self {
        AnyNodeRef::StmtBreak(node)
    }
}

impl<'a> From<&'a crate::StmtContinue> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtContinue) -> Self {
        AnyNodeRef::StmtContinue(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StmtIpyEscapeCommand<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StmtIpyEscapeCommand<'ast>) -> Self {
        AnyNodeRef::StmtIpyEscapeCommand(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprBoolOp<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprBoolOp<'ast>) -> Self {
        AnyNodeRef::ExprBoolOp(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprNamed<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprNamed<'ast>) -> Self {
        AnyNodeRef::ExprNamed(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprBinOp<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprBinOp<'ast>) -> Self {
        AnyNodeRef::ExprBinOp(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprUnaryOp<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprUnaryOp<'ast>) -> Self {
        AnyNodeRef::ExprUnaryOp(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprLambda<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprLambda<'ast>) -> Self {
        AnyNodeRef::ExprLambda(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprIf<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprIf<'ast>) -> Self {
        AnyNodeRef::ExprIf(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprDict<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprDict<'ast>) -> Self {
        AnyNodeRef::ExprDict(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprSet<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprSet<'ast>) -> Self {
        AnyNodeRef::ExprSet(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprListComp<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprListComp<'ast>) -> Self {
        AnyNodeRef::ExprListComp(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprSetComp<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprSetComp<'ast>) -> Self {
        AnyNodeRef::ExprSetComp(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprDictComp<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprDictComp<'ast>) -> Self {
        AnyNodeRef::ExprDictComp(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprGenerator<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprGenerator<'ast>) -> Self {
        AnyNodeRef::ExprGenerator(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprAwait<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprAwait<'ast>) -> Self {
        AnyNodeRef::ExprAwait(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprYield<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprYield<'ast>) -> Self {
        AnyNodeRef::ExprYield(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprYieldFrom<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprYieldFrom<'ast>) -> Self {
        AnyNodeRef::ExprYieldFrom(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprCompare<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprCompare<'ast>) -> Self {
        AnyNodeRef::ExprCompare(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprCall<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprCall<'ast>) -> Self {
        AnyNodeRef::ExprCall(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprFString<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprFString<'ast>) -> Self {
        AnyNodeRef::ExprFString(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprTString<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprTString<'ast>) -> Self {
        AnyNodeRef::ExprTString(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprStringLiteral<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprStringLiteral<'ast>) -> Self {
        AnyNodeRef::ExprStringLiteral(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprBytesLiteral<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprBytesLiteral<'ast>) -> Self {
        AnyNodeRef::ExprBytesLiteral(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprNumberLiteral<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprNumberLiteral<'ast>) -> Self {
        AnyNodeRef::ExprNumberLiteral(node)
    }
}

impl<'a> From<&'a crate::ExprBooleanLiteral> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprBooleanLiteral) -> Self {
        AnyNodeRef::ExprBooleanLiteral(node)
    }
}

impl<'a> From<&'a crate::ExprNoneLiteral> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprNoneLiteral) -> Self {
        AnyNodeRef::ExprNoneLiteral(node)
    }
}

impl<'a> From<&'a crate::ExprEllipsisLiteral> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprEllipsisLiteral) -> Self {
        AnyNodeRef::ExprEllipsisLiteral(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprAttribute<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprAttribute<'ast>) -> Self {
        AnyNodeRef::ExprAttribute(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprSubscript<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprSubscript<'ast>) -> Self {
        AnyNodeRef::ExprSubscript(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprStarred<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprStarred<'ast>) -> Self {
        AnyNodeRef::ExprStarred(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprName<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprName<'ast>) -> Self {
        AnyNodeRef::ExprName(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprList<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprList<'ast>) -> Self {
        AnyNodeRef::ExprList(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprTuple<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprTuple<'ast>) -> Self {
        AnyNodeRef::ExprTuple(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprSlice<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprSlice<'ast>) -> Self {
        AnyNodeRef::ExprSlice(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExprIpyEscapeCommand<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExprIpyEscapeCommand<'ast>) -> Self {
        AnyNodeRef::ExprIpyEscapeCommand(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ExceptHandlerExceptHandler<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExceptHandlerExceptHandler<'ast>) -> Self {
        AnyNodeRef::ExceptHandlerExceptHandler(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::InterpolatedElement<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::InterpolatedElement<'ast>) -> Self {
        AnyNodeRef::InterpolatedElement(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::InterpolatedStringLiteralElement<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::InterpolatedStringLiteralElement<'ast>) -> Self {
        AnyNodeRef::InterpolatedStringLiteralElement(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::PatternMatchValue<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::PatternMatchValue<'ast>) -> Self {
        AnyNodeRef::PatternMatchValue(node)
    }
}

impl<'a> From<&'a crate::PatternMatchSingleton> for AnyNodeRef<'a> {
    fn from(node: &'a crate::PatternMatchSingleton) -> Self {
        AnyNodeRef::PatternMatchSingleton(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::PatternMatchSequence<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::PatternMatchSequence<'ast>) -> Self {
        AnyNodeRef::PatternMatchSequence(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::PatternMatchMapping<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::PatternMatchMapping<'ast>) -> Self {
        AnyNodeRef::PatternMatchMapping(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::PatternMatchClass<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::PatternMatchClass<'ast>) -> Self {
        AnyNodeRef::PatternMatchClass(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::PatternMatchStar<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::PatternMatchStar<'ast>) -> Self {
        AnyNodeRef::PatternMatchStar(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::PatternMatchAs<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::PatternMatchAs<'ast>) -> Self {
        AnyNodeRef::PatternMatchAs(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::PatternMatchOr<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::PatternMatchOr<'ast>) -> Self {
        AnyNodeRef::PatternMatchOr(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::TypeParamTypeVar<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::TypeParamTypeVar<'ast>) -> Self {
        AnyNodeRef::TypeParamTypeVar(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::TypeParamTypeVarTuple<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::TypeParamTypeVarTuple<'ast>) -> Self {
        AnyNodeRef::TypeParamTypeVarTuple(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::TypeParamParamSpec<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::TypeParamParamSpec<'ast>) -> Self {
        AnyNodeRef::TypeParamParamSpec(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::InterpolatedStringFormatSpec<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::InterpolatedStringFormatSpec<'ast>) -> Self {
        AnyNodeRef::InterpolatedStringFormatSpec(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::PatternArguments<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::PatternArguments<'ast>) -> Self {
        AnyNodeRef::PatternArguments(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::PatternKeyword<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::PatternKeyword<'ast>) -> Self {
        AnyNodeRef::PatternKeyword(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::Comprehension<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::Comprehension<'ast>) -> Self {
        AnyNodeRef::Comprehension(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::Arguments<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::Arguments<'ast>) -> Self {
        AnyNodeRef::Arguments(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::Parameters<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::Parameters<'ast>) -> Self {
        AnyNodeRef::Parameters(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::Parameter<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::Parameter<'ast>) -> Self {
        AnyNodeRef::Parameter(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ParameterWithDefault<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ParameterWithDefault<'ast>) -> Self {
        AnyNodeRef::ParameterWithDefault(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::Keyword<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::Keyword<'ast>) -> Self {
        AnyNodeRef::Keyword(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::Alias<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::Alias<'ast>) -> Self {
        AnyNodeRef::Alias(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::WithItem<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::WithItem<'ast>) -> Self {
        AnyNodeRef::WithItem(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::MatchCase<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::MatchCase<'ast>) -> Self {
        AnyNodeRef::MatchCase(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::Decorator<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::Decorator<'ast>) -> Self {
        AnyNodeRef::Decorator(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ElifElseClause<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ElifElseClause<'ast>) -> Self {
        AnyNodeRef::ElifElseClause(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::TypeParams<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::TypeParams<'ast>) -> Self {
        AnyNodeRef::TypeParams(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::FString<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::FString<'ast>) -> Self {
        AnyNodeRef::FString(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::TString<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::TString<'ast>) -> Self {
        AnyNodeRef::TString(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StringLiteral<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::StringLiteral<'ast>) -> Self {
        AnyNodeRef::StringLiteral(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::BytesLiteral<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::BytesLiteral<'ast>) -> Self {
        AnyNodeRef::BytesLiteral(node)
    }
}

impl<'a, 'ast: 'a> From<&'a crate::Identifier<'ast>> for AnyNodeRef<'a> {
    fn from(node: &'a crate::Identifier<'ast>) -> Self {
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
            AnyNodeRef::ExprTString(node) => node.range(),
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
            AnyNodeRef::InterpolatedElement(node) => node.range(),
            AnyNodeRef::InterpolatedStringLiteralElement(node) => node.range(),
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
            AnyNodeRef::InterpolatedStringFormatSpec(node) => node.range(),
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
            AnyNodeRef::TString(node) => node.range(),
            AnyNodeRef::StringLiteral(node) => node.range(),
            AnyNodeRef::BytesLiteral(node) => node.range(),
            AnyNodeRef::Identifier(node) => node.range(),
        }
    }
}

impl crate::HasNodeIndex for AnyNodeRef<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        match self {
            AnyNodeRef::ModModule(node) => node.node_index(),
            AnyNodeRef::ModExpression(node) => node.node_index(),
            AnyNodeRef::StmtFunctionDef(node) => node.node_index(),
            AnyNodeRef::StmtClassDef(node) => node.node_index(),
            AnyNodeRef::StmtReturn(node) => node.node_index(),
            AnyNodeRef::StmtDelete(node) => node.node_index(),
            AnyNodeRef::StmtTypeAlias(node) => node.node_index(),
            AnyNodeRef::StmtAssign(node) => node.node_index(),
            AnyNodeRef::StmtAugAssign(node) => node.node_index(),
            AnyNodeRef::StmtAnnAssign(node) => node.node_index(),
            AnyNodeRef::StmtFor(node) => node.node_index(),
            AnyNodeRef::StmtWhile(node) => node.node_index(),
            AnyNodeRef::StmtIf(node) => node.node_index(),
            AnyNodeRef::StmtWith(node) => node.node_index(),
            AnyNodeRef::StmtMatch(node) => node.node_index(),
            AnyNodeRef::StmtRaise(node) => node.node_index(),
            AnyNodeRef::StmtTry(node) => node.node_index(),
            AnyNodeRef::StmtAssert(node) => node.node_index(),
            AnyNodeRef::StmtImport(node) => node.node_index(),
            AnyNodeRef::StmtImportFrom(node) => node.node_index(),
            AnyNodeRef::StmtGlobal(node) => node.node_index(),
            AnyNodeRef::StmtNonlocal(node) => node.node_index(),
            AnyNodeRef::StmtExpr(node) => node.node_index(),
            AnyNodeRef::StmtPass(node) => node.node_index(),
            AnyNodeRef::StmtBreak(node) => node.node_index(),
            AnyNodeRef::StmtContinue(node) => node.node_index(),
            AnyNodeRef::StmtIpyEscapeCommand(node) => node.node_index(),
            AnyNodeRef::ExprBoolOp(node) => node.node_index(),
            AnyNodeRef::ExprNamed(node) => node.node_index(),
            AnyNodeRef::ExprBinOp(node) => node.node_index(),
            AnyNodeRef::ExprUnaryOp(node) => node.node_index(),
            AnyNodeRef::ExprLambda(node) => node.node_index(),
            AnyNodeRef::ExprIf(node) => node.node_index(),
            AnyNodeRef::ExprDict(node) => node.node_index(),
            AnyNodeRef::ExprSet(node) => node.node_index(),
            AnyNodeRef::ExprListComp(node) => node.node_index(),
            AnyNodeRef::ExprSetComp(node) => node.node_index(),
            AnyNodeRef::ExprDictComp(node) => node.node_index(),
            AnyNodeRef::ExprGenerator(node) => node.node_index(),
            AnyNodeRef::ExprAwait(node) => node.node_index(),
            AnyNodeRef::ExprYield(node) => node.node_index(),
            AnyNodeRef::ExprYieldFrom(node) => node.node_index(),
            AnyNodeRef::ExprCompare(node) => node.node_index(),
            AnyNodeRef::ExprCall(node) => node.node_index(),
            AnyNodeRef::ExprFString(node) => node.node_index(),
            AnyNodeRef::ExprTString(node) => node.node_index(),
            AnyNodeRef::ExprStringLiteral(node) => node.node_index(),
            AnyNodeRef::ExprBytesLiteral(node) => node.node_index(),
            AnyNodeRef::ExprNumberLiteral(node) => node.node_index(),
            AnyNodeRef::ExprBooleanLiteral(node) => node.node_index(),
            AnyNodeRef::ExprNoneLiteral(node) => node.node_index(),
            AnyNodeRef::ExprEllipsisLiteral(node) => node.node_index(),
            AnyNodeRef::ExprAttribute(node) => node.node_index(),
            AnyNodeRef::ExprSubscript(node) => node.node_index(),
            AnyNodeRef::ExprStarred(node) => node.node_index(),
            AnyNodeRef::ExprName(node) => node.node_index(),
            AnyNodeRef::ExprList(node) => node.node_index(),
            AnyNodeRef::ExprTuple(node) => node.node_index(),
            AnyNodeRef::ExprSlice(node) => node.node_index(),
            AnyNodeRef::ExprIpyEscapeCommand(node) => node.node_index(),
            AnyNodeRef::ExceptHandlerExceptHandler(node) => node.node_index(),
            AnyNodeRef::InterpolatedElement(node) => node.node_index(),
            AnyNodeRef::InterpolatedStringLiteralElement(node) => node.node_index(),
            AnyNodeRef::PatternMatchValue(node) => node.node_index(),
            AnyNodeRef::PatternMatchSingleton(node) => node.node_index(),
            AnyNodeRef::PatternMatchSequence(node) => node.node_index(),
            AnyNodeRef::PatternMatchMapping(node) => node.node_index(),
            AnyNodeRef::PatternMatchClass(node) => node.node_index(),
            AnyNodeRef::PatternMatchStar(node) => node.node_index(),
            AnyNodeRef::PatternMatchAs(node) => node.node_index(),
            AnyNodeRef::PatternMatchOr(node) => node.node_index(),
            AnyNodeRef::TypeParamTypeVar(node) => node.node_index(),
            AnyNodeRef::TypeParamTypeVarTuple(node) => node.node_index(),
            AnyNodeRef::TypeParamParamSpec(node) => node.node_index(),
            AnyNodeRef::InterpolatedStringFormatSpec(node) => node.node_index(),
            AnyNodeRef::PatternArguments(node) => node.node_index(),
            AnyNodeRef::PatternKeyword(node) => node.node_index(),
            AnyNodeRef::Comprehension(node) => node.node_index(),
            AnyNodeRef::Arguments(node) => node.node_index(),
            AnyNodeRef::Parameters(node) => node.node_index(),
            AnyNodeRef::Parameter(node) => node.node_index(),
            AnyNodeRef::ParameterWithDefault(node) => node.node_index(),
            AnyNodeRef::Keyword(node) => node.node_index(),
            AnyNodeRef::Alias(node) => node.node_index(),
            AnyNodeRef::WithItem(node) => node.node_index(),
            AnyNodeRef::MatchCase(node) => node.node_index(),
            AnyNodeRef::Decorator(node) => node.node_index(),
            AnyNodeRef::ElifElseClause(node) => node.node_index(),
            AnyNodeRef::TypeParams(node) => node.node_index(),
            AnyNodeRef::FString(node) => node.node_index(),
            AnyNodeRef::TString(node) => node.node_index(),
            AnyNodeRef::StringLiteral(node) => node.node_index(),
            AnyNodeRef::BytesLiteral(node) => node.node_index(),
            AnyNodeRef::Identifier(node) => node.node_index(),
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
            AnyNodeRef::ExprTString(node) => std::ptr::NonNull::from(*node).cast(),
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
            AnyNodeRef::InterpolatedElement(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::InterpolatedStringLiteralElement(node) => {
                std::ptr::NonNull::from(*node).cast()
            }
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
            AnyNodeRef::InterpolatedStringFormatSpec(node) => std::ptr::NonNull::from(*node).cast(),
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
            AnyNodeRef::TString(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::StringLiteral(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::BytesLiteral(node) => std::ptr::NonNull::from(*node).cast(),
            AnyNodeRef::Identifier(node) => std::ptr::NonNull::from(*node).cast(),
        }
    }
}

impl<'a> AnyNodeRef<'a> {
    pub fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: crate::visitor::source_order::SourceOrderVisitor<'a> + ?Sized,
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
            AnyNodeRef::ExprTString(node) => node.visit_source_order(visitor),
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
            AnyNodeRef::InterpolatedElement(node) => node.visit_source_order(visitor),
            AnyNodeRef::InterpolatedStringLiteralElement(node) => node.visit_source_order(visitor),
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
            AnyNodeRef::InterpolatedStringFormatSpec(node) => node.visit_source_order(visitor),
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
            AnyNodeRef::TString(node) => node.visit_source_order(visitor),
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
                | AnyNodeRef::ExprTString(_)
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
    pub const fn is_interpolated_string_element(self) -> bool {
        matches!(
            self,
            AnyNodeRef::InterpolatedElement(_) | AnyNodeRef::InterpolatedStringLiteralElement(_)
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

/// An enumeration of all AST nodes.
///
/// Unlike `AnyNodeRef`, this type does not flatten nested enums, so its variants only
/// consist of the "root" AST node types. This is useful as it exposes references to the
/// original enums, not just references to their inner values.
///
/// For example, `AnyRootNodeRef::Mod` contains a reference to the `Mod` enum, while
/// `AnyNodeRef` has top-level `AnyNodeRef::ModModule` and `AnyNodeRef::ModExpression`
/// variants.
#[derive(Copy, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub enum AnyRootNodeRef<'a> {
    Mod(&'a Mod<'a>),
    Stmt(&'a Stmt<'a>),
    Expr(&'a Expr<'a>),
    ExceptHandler(&'a ExceptHandler<'a>),
    InterpolatedStringElement(&'a InterpolatedStringElement<'a>),
    Pattern(&'a Pattern<'a>),
    TypeParam(&'a TypeParam<'a>),
    InterpolatedStringFormatSpec(&'a crate::InterpolatedStringFormatSpec<'a>),
    PatternArguments(&'a crate::PatternArguments<'a>),
    PatternKeyword(&'a crate::PatternKeyword<'a>),
    Comprehension(&'a crate::Comprehension<'a>),
    Arguments(&'a crate::Arguments<'a>),
    Parameters(&'a crate::Parameters<'a>),
    Parameter(&'a crate::Parameter<'a>),
    ParameterWithDefault(&'a crate::ParameterWithDefault<'a>),
    Keyword(&'a crate::Keyword<'a>),
    Alias(&'a crate::Alias<'a>),
    WithItem(&'a crate::WithItem<'a>),
    MatchCase(&'a crate::MatchCase<'a>),
    Decorator(&'a crate::Decorator<'a>),
    ElifElseClause(&'a crate::ElifElseClause<'a>),
    TypeParams(&'a crate::TypeParams<'a>),
    FString(&'a crate::FString<'a>),
    TString(&'a crate::TString<'a>),
    StringLiteral(&'a crate::StringLiteral<'a>),
    BytesLiteral(&'a crate::BytesLiteral<'a>),
    Identifier(&'a crate::Identifier<'a>),
}

impl<'a, 'ast: 'a> From<&'a Mod<'ast>> for AnyRootNodeRef<'a> {
    fn from(node: &'a Mod<'ast>) -> AnyRootNodeRef<'a> {
        AnyRootNodeRef::Mod(node)
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a Mod<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a Mod<'a>, ()> {
        match node {
            AnyRootNodeRef::Mod(node) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ModModule<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ModModule<'a>, ()> {
        match node {
            AnyRootNodeRef::Mod(Mod::Module(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ModExpression<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ModExpression<'a>, ()> {
        match node {
            AnyRootNodeRef::Mod(Mod::Expression(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a, 'ast: 'a> From<&'a Stmt<'ast>> for AnyRootNodeRef<'a> {
    fn from(node: &'a Stmt<'ast>) -> AnyRootNodeRef<'a> {
        AnyRootNodeRef::Stmt(node)
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a Stmt<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a Stmt<'a>, ()> {
        match node {
            AnyRootNodeRef::Stmt(node) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::StmtFunctionDef<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::StmtFunctionDef<'a>, ()> {
        match node {
            AnyRootNodeRef::Stmt(Stmt::FunctionDef(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::StmtClassDef<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::StmtClassDef<'a>, ()> {
        match node {
            AnyRootNodeRef::Stmt(Stmt::ClassDef(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::StmtReturn<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::StmtReturn<'a>, ()> {
        match node {
            AnyRootNodeRef::Stmt(Stmt::Return(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::StmtDelete<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::StmtDelete<'a>, ()> {
        match node {
            AnyRootNodeRef::Stmt(Stmt::Delete(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::StmtTypeAlias<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::StmtTypeAlias<'a>, ()> {
        match node {
            AnyRootNodeRef::Stmt(Stmt::TypeAlias(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::StmtAssign<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::StmtAssign<'a>, ()> {
        match node {
            AnyRootNodeRef::Stmt(Stmt::Assign(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::StmtAugAssign<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::StmtAugAssign<'a>, ()> {
        match node {
            AnyRootNodeRef::Stmt(Stmt::AugAssign(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::StmtAnnAssign<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::StmtAnnAssign<'a>, ()> {
        match node {
            AnyRootNodeRef::Stmt(Stmt::AnnAssign(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::StmtFor<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::StmtFor<'a>, ()> {
        match node {
            AnyRootNodeRef::Stmt(Stmt::For(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::StmtWhile<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::StmtWhile<'a>, ()> {
        match node {
            AnyRootNodeRef::Stmt(Stmt::While(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::StmtIf<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::StmtIf<'a>, ()> {
        match node {
            AnyRootNodeRef::Stmt(Stmt::If(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::StmtWith<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::StmtWith<'a>, ()> {
        match node {
            AnyRootNodeRef::Stmt(Stmt::With(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::StmtMatch<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::StmtMatch<'a>, ()> {
        match node {
            AnyRootNodeRef::Stmt(Stmt::Match(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::StmtRaise<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::StmtRaise<'a>, ()> {
        match node {
            AnyRootNodeRef::Stmt(Stmt::Raise(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::StmtTry<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::StmtTry<'a>, ()> {
        match node {
            AnyRootNodeRef::Stmt(Stmt::Try(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::StmtAssert<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::StmtAssert<'a>, ()> {
        match node {
            AnyRootNodeRef::Stmt(Stmt::Assert(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::StmtImport<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::StmtImport<'a>, ()> {
        match node {
            AnyRootNodeRef::Stmt(Stmt::Import(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::StmtImportFrom<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::StmtImportFrom<'a>, ()> {
        match node {
            AnyRootNodeRef::Stmt(Stmt::ImportFrom(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::StmtGlobal<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::StmtGlobal<'a>, ()> {
        match node {
            AnyRootNodeRef::Stmt(Stmt::Global(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::StmtNonlocal<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::StmtNonlocal<'a>, ()> {
        match node {
            AnyRootNodeRef::Stmt(Stmt::Nonlocal(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::StmtExpr<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::StmtExpr<'a>, ()> {
        match node {
            AnyRootNodeRef::Stmt(Stmt::Expr(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::StmtPass {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::StmtPass, ()> {
        match node {
            AnyRootNodeRef::Stmt(Stmt::Pass(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::StmtBreak {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::StmtBreak, ()> {
        match node {
            AnyRootNodeRef::Stmt(Stmt::Break(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::StmtContinue {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::StmtContinue, ()> {
        match node {
            AnyRootNodeRef::Stmt(Stmt::Continue(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::StmtIpyEscapeCommand<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::StmtIpyEscapeCommand<'a>, ()> {
        match node {
            AnyRootNodeRef::Stmt(Stmt::IpyEscapeCommand(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a, 'ast: 'a> From<&'a Expr<'ast>> for AnyRootNodeRef<'a> {
    fn from(node: &'a Expr<'ast>) -> AnyRootNodeRef<'a> {
        AnyRootNodeRef::Expr(node)
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a Expr<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a Expr<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(node) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprBoolOp<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprBoolOp<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::BoolOp(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprNamed<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprNamed<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::Named(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprBinOp<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprBinOp<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::BinOp(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprUnaryOp<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprUnaryOp<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::UnaryOp(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprLambda<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprLambda<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::Lambda(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprIf<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprIf<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::If(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprDict<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprDict<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::Dict(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprSet<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprSet<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::Set(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprListComp<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprListComp<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::ListComp(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprSetComp<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprSetComp<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::SetComp(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprDictComp<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprDictComp<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::DictComp(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprGenerator<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprGenerator<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::Generator(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprAwait<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprAwait<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::Await(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprYield<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprYield<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::Yield(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprYieldFrom<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprYieldFrom<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::YieldFrom(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprCompare<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprCompare<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::Compare(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprCall<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprCall<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::Call(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprFString<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprFString<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::FString(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprTString<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprTString<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::TString(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprStringLiteral<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprStringLiteral<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::StringLiteral(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprBytesLiteral<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprBytesLiteral<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::BytesLiteral(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprNumberLiteral<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprNumberLiteral<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::NumberLiteral(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprBooleanLiteral {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprBooleanLiteral, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::BooleanLiteral(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprNoneLiteral {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprNoneLiteral, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::NoneLiteral(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprEllipsisLiteral {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprEllipsisLiteral, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::EllipsisLiteral(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprAttribute<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprAttribute<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::Attribute(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprSubscript<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprSubscript<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::Subscript(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprStarred<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprStarred<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::Starred(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprName<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprName<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::Name(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprList<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprList<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::List(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprTuple<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprTuple<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::Tuple(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprSlice<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprSlice<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::Slice(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExprIpyEscapeCommand<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExprIpyEscapeCommand<'a>, ()> {
        match node {
            AnyRootNodeRef::Expr(Expr::IpyEscapeCommand(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a, 'ast: 'a> From<&'a ExceptHandler<'ast>> for AnyRootNodeRef<'a> {
    fn from(node: &'a ExceptHandler<'ast>) -> AnyRootNodeRef<'a> {
        AnyRootNodeRef::ExceptHandler(node)
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a ExceptHandler<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a ExceptHandler<'a>, ()> {
        match node {
            AnyRootNodeRef::ExceptHandler(node) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ExceptHandlerExceptHandler<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ExceptHandlerExceptHandler<'a>, ()> {
        match node {
            AnyRootNodeRef::ExceptHandler(ExceptHandler::ExceptHandler(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a, 'ast: 'a> From<&'a InterpolatedStringElement<'ast>> for AnyRootNodeRef<'a> {
    fn from(node: &'a InterpolatedStringElement<'ast>) -> AnyRootNodeRef<'a> {
        AnyRootNodeRef::InterpolatedStringElement(node)
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a InterpolatedStringElement<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a InterpolatedStringElement<'a>, ()> {
        match node {
            AnyRootNodeRef::InterpolatedStringElement(node) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::InterpolatedElement<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::InterpolatedElement<'a>, ()> {
        match node {
            AnyRootNodeRef::InterpolatedStringElement(
                InterpolatedStringElement::Interpolation(node),
            ) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::InterpolatedStringLiteralElement<'a> {
    type Error = ();
    fn try_from(
        node: AnyRootNodeRef<'a>,
    ) -> Result<&'a crate::InterpolatedStringLiteralElement<'a>, ()> {
        match node {
            AnyRootNodeRef::InterpolatedStringElement(InterpolatedStringElement::Literal(node)) => {
                Ok(node)
            }
            _ => Err(()),
        }
    }
}

impl<'a, 'ast: 'a> From<&'a Pattern<'ast>> for AnyRootNodeRef<'a> {
    fn from(node: &'a Pattern<'ast>) -> AnyRootNodeRef<'a> {
        AnyRootNodeRef::Pattern(node)
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a Pattern<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a Pattern<'a>, ()> {
        match node {
            AnyRootNodeRef::Pattern(node) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::PatternMatchValue<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::PatternMatchValue<'a>, ()> {
        match node {
            AnyRootNodeRef::Pattern(Pattern::MatchValue(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::PatternMatchSingleton {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::PatternMatchSingleton, ()> {
        match node {
            AnyRootNodeRef::Pattern(Pattern::MatchSingleton(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::PatternMatchSequence<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::PatternMatchSequence<'a>, ()> {
        match node {
            AnyRootNodeRef::Pattern(Pattern::MatchSequence(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::PatternMatchMapping<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::PatternMatchMapping<'a>, ()> {
        match node {
            AnyRootNodeRef::Pattern(Pattern::MatchMapping(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::PatternMatchClass<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::PatternMatchClass<'a>, ()> {
        match node {
            AnyRootNodeRef::Pattern(Pattern::MatchClass(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::PatternMatchStar<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::PatternMatchStar<'a>, ()> {
        match node {
            AnyRootNodeRef::Pattern(Pattern::MatchStar(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::PatternMatchAs<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::PatternMatchAs<'a>, ()> {
        match node {
            AnyRootNodeRef::Pattern(Pattern::MatchAs(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::PatternMatchOr<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::PatternMatchOr<'a>, ()> {
        match node {
            AnyRootNodeRef::Pattern(Pattern::MatchOr(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a, 'ast: 'a> From<&'a TypeParam<'ast>> for AnyRootNodeRef<'a> {
    fn from(node: &'a TypeParam<'ast>) -> AnyRootNodeRef<'a> {
        AnyRootNodeRef::TypeParam(node)
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a TypeParam<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a TypeParam<'a>, ()> {
        match node {
            AnyRootNodeRef::TypeParam(node) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::TypeParamTypeVar<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::TypeParamTypeVar<'a>, ()> {
        match node {
            AnyRootNodeRef::TypeParam(TypeParam::TypeVar(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::TypeParamTypeVarTuple<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::TypeParamTypeVarTuple<'a>, ()> {
        match node {
            AnyRootNodeRef::TypeParam(TypeParam::TypeVarTuple(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::TypeParamParamSpec<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::TypeParamParamSpec<'a>, ()> {
        match node {
            AnyRootNodeRef::TypeParam(TypeParam::ParamSpec(node)) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a, 'ast: 'a> From<&'a crate::InterpolatedStringFormatSpec<'ast>> for AnyRootNodeRef<'a> {
    fn from(node: &'a crate::InterpolatedStringFormatSpec<'ast>) -> Self {
        AnyRootNodeRef::InterpolatedStringFormatSpec(node)
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::InterpolatedStringFormatSpec<'a> {
    type Error = ();
    fn try_from(
        node: AnyRootNodeRef<'a>,
    ) -> Result<&'a crate::InterpolatedStringFormatSpec<'a>, ()> {
        match node {
            AnyRootNodeRef::InterpolatedStringFormatSpec(node) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a, 'ast: 'a> From<&'a crate::PatternArguments<'ast>> for AnyRootNodeRef<'a> {
    fn from(node: &'a crate::PatternArguments<'ast>) -> Self {
        AnyRootNodeRef::PatternArguments(node)
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::PatternArguments<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::PatternArguments<'a>, ()> {
        match node {
            AnyRootNodeRef::PatternArguments(node) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a, 'ast: 'a> From<&'a crate::PatternKeyword<'ast>> for AnyRootNodeRef<'a> {
    fn from(node: &'a crate::PatternKeyword<'ast>) -> Self {
        AnyRootNodeRef::PatternKeyword(node)
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::PatternKeyword<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::PatternKeyword<'a>, ()> {
        match node {
            AnyRootNodeRef::PatternKeyword(node) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a, 'ast: 'a> From<&'a crate::Comprehension<'ast>> for AnyRootNodeRef<'a> {
    fn from(node: &'a crate::Comprehension<'ast>) -> Self {
        AnyRootNodeRef::Comprehension(node)
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::Comprehension<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::Comprehension<'a>, ()> {
        match node {
            AnyRootNodeRef::Comprehension(node) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a, 'ast: 'a> From<&'a crate::Arguments<'ast>> for AnyRootNodeRef<'a> {
    fn from(node: &'a crate::Arguments<'ast>) -> Self {
        AnyRootNodeRef::Arguments(node)
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::Arguments<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::Arguments<'a>, ()> {
        match node {
            AnyRootNodeRef::Arguments(node) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a, 'ast: 'a> From<&'a crate::Parameters<'ast>> for AnyRootNodeRef<'a> {
    fn from(node: &'a crate::Parameters<'ast>) -> Self {
        AnyRootNodeRef::Parameters(node)
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::Parameters<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::Parameters<'a>, ()> {
        match node {
            AnyRootNodeRef::Parameters(node) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a, 'ast: 'a> From<&'a crate::Parameter<'ast>> for AnyRootNodeRef<'a> {
    fn from(node: &'a crate::Parameter<'ast>) -> Self {
        AnyRootNodeRef::Parameter(node)
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::Parameter<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::Parameter<'a>, ()> {
        match node {
            AnyRootNodeRef::Parameter(node) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ParameterWithDefault<'ast>> for AnyRootNodeRef<'a> {
    fn from(node: &'a crate::ParameterWithDefault<'ast>) -> Self {
        AnyRootNodeRef::ParameterWithDefault(node)
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ParameterWithDefault<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ParameterWithDefault<'a>, ()> {
        match node {
            AnyRootNodeRef::ParameterWithDefault(node) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a, 'ast: 'a> From<&'a crate::Keyword<'ast>> for AnyRootNodeRef<'a> {
    fn from(node: &'a crate::Keyword<'ast>) -> Self {
        AnyRootNodeRef::Keyword(node)
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::Keyword<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::Keyword<'a>, ()> {
        match node {
            AnyRootNodeRef::Keyword(node) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a, 'ast: 'a> From<&'a crate::Alias<'ast>> for AnyRootNodeRef<'a> {
    fn from(node: &'a crate::Alias<'ast>) -> Self {
        AnyRootNodeRef::Alias(node)
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::Alias<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::Alias<'a>, ()> {
        match node {
            AnyRootNodeRef::Alias(node) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a, 'ast: 'a> From<&'a crate::WithItem<'ast>> for AnyRootNodeRef<'a> {
    fn from(node: &'a crate::WithItem<'ast>) -> Self {
        AnyRootNodeRef::WithItem(node)
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::WithItem<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::WithItem<'a>, ()> {
        match node {
            AnyRootNodeRef::WithItem(node) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a, 'ast: 'a> From<&'a crate::MatchCase<'ast>> for AnyRootNodeRef<'a> {
    fn from(node: &'a crate::MatchCase<'ast>) -> Self {
        AnyRootNodeRef::MatchCase(node)
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::MatchCase<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::MatchCase<'a>, ()> {
        match node {
            AnyRootNodeRef::MatchCase(node) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a, 'ast: 'a> From<&'a crate::Decorator<'ast>> for AnyRootNodeRef<'a> {
    fn from(node: &'a crate::Decorator<'ast>) -> Self {
        AnyRootNodeRef::Decorator(node)
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::Decorator<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::Decorator<'a>, ()> {
        match node {
            AnyRootNodeRef::Decorator(node) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a, 'ast: 'a> From<&'a crate::ElifElseClause<'ast>> for AnyRootNodeRef<'a> {
    fn from(node: &'a crate::ElifElseClause<'ast>) -> Self {
        AnyRootNodeRef::ElifElseClause(node)
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::ElifElseClause<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::ElifElseClause<'a>, ()> {
        match node {
            AnyRootNodeRef::ElifElseClause(node) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a, 'ast: 'a> From<&'a crate::TypeParams<'ast>> for AnyRootNodeRef<'a> {
    fn from(node: &'a crate::TypeParams<'ast>) -> Self {
        AnyRootNodeRef::TypeParams(node)
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::TypeParams<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::TypeParams<'a>, ()> {
        match node {
            AnyRootNodeRef::TypeParams(node) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a, 'ast: 'a> From<&'a crate::FString<'ast>> for AnyRootNodeRef<'a> {
    fn from(node: &'a crate::FString<'ast>) -> Self {
        AnyRootNodeRef::FString(node)
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::FString<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::FString<'a>, ()> {
        match node {
            AnyRootNodeRef::FString(node) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a, 'ast: 'a> From<&'a crate::TString<'ast>> for AnyRootNodeRef<'a> {
    fn from(node: &'a crate::TString<'ast>) -> Self {
        AnyRootNodeRef::TString(node)
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::TString<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::TString<'a>, ()> {
        match node {
            AnyRootNodeRef::TString(node) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a, 'ast: 'a> From<&'a crate::StringLiteral<'ast>> for AnyRootNodeRef<'a> {
    fn from(node: &'a crate::StringLiteral<'ast>) -> Self {
        AnyRootNodeRef::StringLiteral(node)
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::StringLiteral<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::StringLiteral<'a>, ()> {
        match node {
            AnyRootNodeRef::StringLiteral(node) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a, 'ast: 'a> From<&'a crate::BytesLiteral<'ast>> for AnyRootNodeRef<'a> {
    fn from(node: &'a crate::BytesLiteral<'ast>) -> Self {
        AnyRootNodeRef::BytesLiteral(node)
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::BytesLiteral<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::BytesLiteral<'a>, ()> {
        match node {
            AnyRootNodeRef::BytesLiteral(node) => Ok(node),
            _ => Err(()),
        }
    }
}

impl<'a, 'ast: 'a> From<&'a crate::Identifier<'ast>> for AnyRootNodeRef<'a> {
    fn from(node: &'a crate::Identifier<'ast>) -> Self {
        AnyRootNodeRef::Identifier(node)
    }
}

impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a crate::Identifier<'a> {
    type Error = ();
    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a crate::Identifier<'a>, ()> {
        match node {
            AnyRootNodeRef::Identifier(node) => Ok(node),
            _ => Err(()),
        }
    }
}

impl ruff_text_size::Ranged for AnyRootNodeRef<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        match self {
            AnyRootNodeRef::Mod(node) => node.range(),
            AnyRootNodeRef::Stmt(node) => node.range(),
            AnyRootNodeRef::Expr(node) => node.range(),
            AnyRootNodeRef::ExceptHandler(node) => node.range(),
            AnyRootNodeRef::InterpolatedStringElement(node) => node.range(),
            AnyRootNodeRef::Pattern(node) => node.range(),
            AnyRootNodeRef::TypeParam(node) => node.range(),
            AnyRootNodeRef::InterpolatedStringFormatSpec(node) => node.range(),
            AnyRootNodeRef::PatternArguments(node) => node.range(),
            AnyRootNodeRef::PatternKeyword(node) => node.range(),
            AnyRootNodeRef::Comprehension(node) => node.range(),
            AnyRootNodeRef::Arguments(node) => node.range(),
            AnyRootNodeRef::Parameters(node) => node.range(),
            AnyRootNodeRef::Parameter(node) => node.range(),
            AnyRootNodeRef::ParameterWithDefault(node) => node.range(),
            AnyRootNodeRef::Keyword(node) => node.range(),
            AnyRootNodeRef::Alias(node) => node.range(),
            AnyRootNodeRef::WithItem(node) => node.range(),
            AnyRootNodeRef::MatchCase(node) => node.range(),
            AnyRootNodeRef::Decorator(node) => node.range(),
            AnyRootNodeRef::ElifElseClause(node) => node.range(),
            AnyRootNodeRef::TypeParams(node) => node.range(),
            AnyRootNodeRef::FString(node) => node.range(),
            AnyRootNodeRef::TString(node) => node.range(),
            AnyRootNodeRef::StringLiteral(node) => node.range(),
            AnyRootNodeRef::BytesLiteral(node) => node.range(),
            AnyRootNodeRef::Identifier(node) => node.range(),
        }
    }
}

impl crate::HasNodeIndex for AnyRootNodeRef<'_> {
    fn node_index(&self) -> &crate::AtomicNodeIndex {
        match self {
            AnyRootNodeRef::Mod(node) => node.node_index(),
            AnyRootNodeRef::Stmt(node) => node.node_index(),
            AnyRootNodeRef::Expr(node) => node.node_index(),
            AnyRootNodeRef::ExceptHandler(node) => node.node_index(),
            AnyRootNodeRef::InterpolatedStringElement(node) => node.node_index(),
            AnyRootNodeRef::Pattern(node) => node.node_index(),
            AnyRootNodeRef::TypeParam(node) => node.node_index(),
            AnyRootNodeRef::InterpolatedStringFormatSpec(node) => node.node_index(),
            AnyRootNodeRef::PatternArguments(node) => node.node_index(),
            AnyRootNodeRef::PatternKeyword(node) => node.node_index(),
            AnyRootNodeRef::Comprehension(node) => node.node_index(),
            AnyRootNodeRef::Arguments(node) => node.node_index(),
            AnyRootNodeRef::Parameters(node) => node.node_index(),
            AnyRootNodeRef::Parameter(node) => node.node_index(),
            AnyRootNodeRef::ParameterWithDefault(node) => node.node_index(),
            AnyRootNodeRef::Keyword(node) => node.node_index(),
            AnyRootNodeRef::Alias(node) => node.node_index(),
            AnyRootNodeRef::WithItem(node) => node.node_index(),
            AnyRootNodeRef::MatchCase(node) => node.node_index(),
            AnyRootNodeRef::Decorator(node) => node.node_index(),
            AnyRootNodeRef::ElifElseClause(node) => node.node_index(),
            AnyRootNodeRef::TypeParams(node) => node.node_index(),
            AnyRootNodeRef::FString(node) => node.node_index(),
            AnyRootNodeRef::TString(node) => node.node_index(),
            AnyRootNodeRef::StringLiteral(node) => node.node_index(),
            AnyRootNodeRef::BytesLiteral(node) => node.node_index(),
            AnyRootNodeRef::Identifier(node) => node.node_index(),
        }
    }
}

impl<'a> AnyRootNodeRef<'a> {
    pub fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: crate::visitor::source_order::SourceOrderVisitor<'a> + ?Sized,
    {
        match self {
            AnyRootNodeRef::Mod(node) => node.visit_source_order(visitor),
            AnyRootNodeRef::Stmt(node) => node.visit_source_order(visitor),
            AnyRootNodeRef::Expr(node) => node.visit_source_order(visitor),
            AnyRootNodeRef::ExceptHandler(node) => node.visit_source_order(visitor),
            AnyRootNodeRef::InterpolatedStringElement(node) => node.visit_source_order(visitor),
            AnyRootNodeRef::Pattern(node) => node.visit_source_order(visitor),
            AnyRootNodeRef::TypeParam(node) => node.visit_source_order(visitor),
            AnyRootNodeRef::InterpolatedStringFormatSpec(node) => node.visit_source_order(visitor),
            AnyRootNodeRef::PatternArguments(node) => node.visit_source_order(visitor),
            AnyRootNodeRef::PatternKeyword(node) => node.visit_source_order(visitor),
            AnyRootNodeRef::Comprehension(node) => node.visit_source_order(visitor),
            AnyRootNodeRef::Arguments(node) => node.visit_source_order(visitor),
            AnyRootNodeRef::Parameters(node) => node.visit_source_order(visitor),
            AnyRootNodeRef::Parameter(node) => node.visit_source_order(visitor),
            AnyRootNodeRef::ParameterWithDefault(node) => node.visit_source_order(visitor),
            AnyRootNodeRef::Keyword(node) => node.visit_source_order(visitor),
            AnyRootNodeRef::Alias(node) => node.visit_source_order(visitor),
            AnyRootNodeRef::WithItem(node) => node.visit_source_order(visitor),
            AnyRootNodeRef::MatchCase(node) => node.visit_source_order(visitor),
            AnyRootNodeRef::Decorator(node) => node.visit_source_order(visitor),
            AnyRootNodeRef::ElifElseClause(node) => node.visit_source_order(visitor),
            AnyRootNodeRef::TypeParams(node) => node.visit_source_order(visitor),
            AnyRootNodeRef::FString(node) => node.visit_source_order(visitor),
            AnyRootNodeRef::TString(node) => node.visit_source_order(visitor),
            AnyRootNodeRef::StringLiteral(node) => node.visit_source_order(visitor),
            AnyRootNodeRef::BytesLiteral(node) => node.visit_source_order(visitor),
            AnyRootNodeRef::Identifier(node) => node.visit_source_order(visitor),
        }
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
    ExprTString,
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
    InterpolatedElement,
    InterpolatedStringLiteralElement,
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
    InterpolatedStringFormatSpec,
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
    TString,
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
            AnyNodeRef::ExprTString(_) => NodeKind::ExprTString,
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
            AnyNodeRef::InterpolatedElement(_) => NodeKind::InterpolatedElement,
            AnyNodeRef::InterpolatedStringLiteralElement(_) => {
                NodeKind::InterpolatedStringLiteralElement
            }
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
            AnyNodeRef::InterpolatedStringFormatSpec(_) => NodeKind::InterpolatedStringFormatSpec,
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
            AnyNodeRef::TString(_) => NodeKind::TString,
            AnyNodeRef::StringLiteral(_) => NodeKind::StringLiteral,
            AnyNodeRef::BytesLiteral(_) => NodeKind::BytesLiteral,
            AnyNodeRef::Identifier(_) => NodeKind::Identifier,
        }
    }
}

/// See also [Module](https://docs.python.org/3/library/ast.html#ast.Module)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ModModule<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub body: ArenaSlice<'ast, Stmt<'ast>>,
}

/// See also [Module](https://docs.python.org/3/library/ast.html#ast.Module)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ModExpression<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub body: ArenaBox<'ast, Expr<'ast>>,
}

/// See also [FunctionDef](https://docs.python.org/3/library/ast.html#ast.FunctionDef)
/// and [AsyncFunctionDef](https://docs.python.org/3/library/ast.html#ast.AsyncFunctionDef).
///
/// This type differs from the original Python AST, as it collapses the synchronous and asynchronous variants into a single type.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct StmtFunctionDef<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub is_async: bool,
    pub decorator_list: ArenaSlice<'ast, crate::Decorator<'ast>>,
    pub name: crate::Identifier<'ast>,
    pub type_params: Option<ArenaBox<'ast, crate::TypeParams<'ast>>>,
    pub parameters: ArenaBox<'ast, crate::Parameters<'ast>>,
    pub returns: Option<ArenaBox<'ast, Expr<'ast>>>,
    pub body: ArenaSlice<'ast, Stmt<'ast>>,
}

/// See also [ClassDef](https://docs.python.org/3/library/ast.html#ast.ClassDef)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct StmtClassDef<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub decorator_list: ArenaSlice<'ast, crate::Decorator<'ast>>,
    pub name: crate::Identifier<'ast>,
    pub type_params: Option<ArenaBox<'ast, crate::TypeParams<'ast>>>,
    pub arguments: Option<ArenaBox<'ast, crate::Arguments<'ast>>>,
    pub body: ArenaSlice<'ast, Stmt<'ast>>,
}

/// See also [Return](https://docs.python.org/3/library/ast.html#ast.Return)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct StmtReturn<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub value: Option<ArenaBox<'ast, Expr<'ast>>>,
}

/// See also [Delete](https://docs.python.org/3/library/ast.html#ast.Delete)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct StmtDelete<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub targets: ArenaSlice<'ast, Expr<'ast>>,
}

/// See also [TypeAlias](https://docs.python.org/3/library/ast.html#ast.TypeAlias)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct StmtTypeAlias<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub name: ArenaBox<'ast, Expr<'ast>>,
    pub type_params: Option<ArenaBox<'ast, crate::TypeParams<'ast>>>,
    pub value: ArenaBox<'ast, Expr<'ast>>,
}

/// See also [Assign](https://docs.python.org/3/library/ast.html#ast.Assign)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct StmtAssign<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub targets: ArenaSlice<'ast, Expr<'ast>>,
    pub value: ArenaBox<'ast, Expr<'ast>>,
}

/// See also [AugAssign](https://docs.python.org/3/library/ast.html#ast.AugAssign)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct StmtAugAssign<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub target: ArenaBox<'ast, Expr<'ast>>,
    pub op: crate::Operator,
    pub value: ArenaBox<'ast, Expr<'ast>>,
}

/// See also [AnnAssign](https://docs.python.org/3/library/ast.html#ast.AnnAssign)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct StmtAnnAssign<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub target: ArenaBox<'ast, Expr<'ast>>,
    pub annotation: ArenaBox<'ast, Expr<'ast>>,
    pub value: Option<ArenaBox<'ast, Expr<'ast>>>,
    pub simple: bool,
}

/// See also [For](https://docs.python.org/3/library/ast.html#ast.For)
/// and [AsyncFor](https://docs.python.org/3/library/ast.html#ast.AsyncFor).
///
/// This type differs from the original Python AST, as it collapses the synchronous and asynchronous variants into a single type.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct StmtFor<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub is_async: bool,
    pub target: ArenaBox<'ast, Expr<'ast>>,
    pub iter: ArenaBox<'ast, Expr<'ast>>,
    pub body: ArenaSlice<'ast, Stmt<'ast>>,
    pub orelse: ArenaSlice<'ast, Stmt<'ast>>,
}

/// See also [While](https://docs.python.org/3/library/ast.html#ast.While)
/// and [AsyncWhile](https://docs.python.org/3/library/ast.html#ast.AsyncWhile).
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct StmtWhile<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub test: ArenaBox<'ast, Expr<'ast>>,
    pub body: ArenaSlice<'ast, Stmt<'ast>>,
    pub orelse: ArenaSlice<'ast, Stmt<'ast>>,
}

/// See also [If](https://docs.python.org/3/library/ast.html#ast.If)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct StmtIf<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub test: ArenaBox<'ast, Expr<'ast>>,
    pub body: ArenaSlice<'ast, Stmt<'ast>>,
    pub elif_else_clauses: ArenaSlice<'ast, crate::ElifElseClause<'ast>>,
}

/// See also [With](https://docs.python.org/3/library/ast.html#ast.With)
/// and [AsyncWith](https://docs.python.org/3/library/ast.html#ast.AsyncWith).
///
/// This type differs from the original Python AST, as it collapses the synchronous and asynchronous variants into a single type.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct StmtWith<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub is_async: bool,
    pub items: ArenaSlice<'ast, crate::WithItem<'ast>>,
    pub body: ArenaSlice<'ast, Stmt<'ast>>,
}

/// See also [Match](https://docs.python.org/3/library/ast.html#ast.Match)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct StmtMatch<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub subject: ArenaBox<'ast, Expr<'ast>>,
    pub cases: ArenaSlice<'ast, crate::MatchCase<'ast>>,
}

/// See also [Raise](https://docs.python.org/3/library/ast.html#ast.Raise)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct StmtRaise<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub exc: Option<ArenaBox<'ast, Expr<'ast>>>,
    pub cause: Option<ArenaBox<'ast, Expr<'ast>>>,
}

/// See also [Try](https://docs.python.org/3/library/ast.html#ast.Try)
/// and [TryStar](https://docs.python.org/3/library/ast.html#ast.TryStar)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct StmtTry<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub body: ArenaSlice<'ast, Stmt<'ast>>,
    pub handlers: ArenaSlice<'ast, ExceptHandler<'ast>>,
    pub orelse: ArenaSlice<'ast, Stmt<'ast>>,
    pub finalbody: ArenaSlice<'ast, Stmt<'ast>>,
    pub is_star: bool,
}

/// See also [Assert](https://docs.python.org/3/library/ast.html#ast.Assert)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct StmtAssert<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub test: ArenaBox<'ast, Expr<'ast>>,
    pub msg: Option<ArenaBox<'ast, Expr<'ast>>>,
}

/// See also [Import](https://docs.python.org/3/library/ast.html#ast.Import)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct StmtImport<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub names: ArenaSlice<'ast, crate::Alias<'ast>>,
    pub is_lazy: bool,
}

/// See also [ImportFrom](https://docs.python.org/3/library/ast.html#ast.ImportFrom)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct StmtImportFrom<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub module: Option<crate::Identifier<'ast>>,
    pub names: ArenaSlice<'ast, crate::Alias<'ast>>,
    pub level: u32,
    pub is_lazy: bool,
}

/// See also [Global](https://docs.python.org/3/library/ast.html#ast.Global)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct StmtGlobal<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub names: ArenaSlice<'ast, crate::Identifier<'ast>>,
}

/// See also [Nonlocal](https://docs.python.org/3/library/ast.html#ast.Nonlocal)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct StmtNonlocal<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub names: ArenaSlice<'ast, crate::Identifier<'ast>>,
}

/// See also [Expr](https://docs.python.org/3/library/ast.html#ast.Expr)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct StmtExpr<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub value: ArenaBox<'ast, Expr<'ast>>,
}

/// See also [Pass](https://docs.python.org/3/library/ast.html#ast.Pass)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct StmtPass {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
}

/// See also [Break](https://docs.python.org/3/library/ast.html#ast.Break)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct StmtBreak {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
}

/// See also [Continue](https://docs.python.org/3/library/ast.html#ast.Continue)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct StmtContinue {
    pub node_index: crate::AtomicNodeIndex,
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
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct StmtIpyEscapeCommand<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub kind: crate::IpyEscapeKind,
    pub value: ArenaBox<'ast, str>,
}

/// See also [BoolOp](https://docs.python.org/3/library/ast.html#ast.BoolOp)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprBoolOp<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub op: crate::BoolOp,
    pub values: ArenaSlice<'ast, Expr<'ast>>,
}

/// See also [NamedExpr](https://docs.python.org/3/library/ast.html#ast.NamedExpr)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprNamed<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub target: ArenaBox<'ast, Expr<'ast>>,
    pub value: ArenaBox<'ast, Expr<'ast>>,
}

/// See also [BinOp](https://docs.python.org/3/library/ast.html#ast.BinOp)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprBinOp<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub left: ArenaBox<'ast, Expr<'ast>>,
    pub op: crate::Operator,
    pub right: ArenaBox<'ast, Expr<'ast>>,
}

/// See also [UnaryOp](https://docs.python.org/3/library/ast.html#ast.UnaryOp)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprUnaryOp<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub op: crate::UnaryOp,
    pub operand: ArenaBox<'ast, Expr<'ast>>,
}

/// See also [Lambda](https://docs.python.org/3/library/ast.html#ast.Lambda)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprLambda<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub parameters: Option<ArenaBox<'ast, crate::Parameters<'ast>>>,
    pub body: ArenaBox<'ast, Expr<'ast>>,
}

/// See also [IfExp](https://docs.python.org/3/library/ast.html#ast.IfExp)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprIf<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub test: ArenaBox<'ast, Expr<'ast>>,
    pub body: ArenaBox<'ast, Expr<'ast>>,
    pub orelse: ArenaBox<'ast, Expr<'ast>>,
}

/// See also [Dict](https://docs.python.org/3/library/ast.html#ast.Dict)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprDict<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub items: ArenaSlice<'ast, crate::DictItem<'ast>>,
}

/// See also [Set](https://docs.python.org/3/library/ast.html#ast.Set)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprSet<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub elts: ArenaSlice<'ast, Expr<'ast>>,
}

/// See also [ListComp](https://docs.python.org/3/library/ast.html#ast.ListComp)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprListComp<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub elt: ArenaBox<'ast, Expr<'ast>>,
    pub generators: ArenaSlice<'ast, crate::Comprehension<'ast>>,
}

/// See also [SetComp](https://docs.python.org/3/library/ast.html#ast.SetComp)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprSetComp<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub elt: ArenaBox<'ast, Expr<'ast>>,
    pub generators: ArenaSlice<'ast, crate::Comprehension<'ast>>,
}

/// See also [DictComp](https://docs.python.org/3/library/ast.html#ast.DictComp)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprDictComp<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub key: Option<ArenaBox<'ast, Expr<'ast>>>,
    pub value: ArenaBox<'ast, Expr<'ast>>,
    pub generators: ArenaSlice<'ast, crate::Comprehension<'ast>>,
}

/// See also [GeneratorExp](https://docs.python.org/3/library/ast.html#ast.GeneratorExp)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprGenerator<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub elt: ArenaBox<'ast, Expr<'ast>>,
    pub generators: ArenaSlice<'ast, crate::Comprehension<'ast>>,
    pub parenthesized: bool,
}

/// See also [Await](https://docs.python.org/3/library/ast.html#ast.Await)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprAwait<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub value: ArenaBox<'ast, Expr<'ast>>,
}

/// See also [Yield](https://docs.python.org/3/library/ast.html#ast.Yield)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprYield<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub value: Option<ArenaBox<'ast, Expr<'ast>>>,
}

/// See also [YieldFrom](https://docs.python.org/3/library/ast.html#ast.YieldFrom)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprYieldFrom<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub value: ArenaBox<'ast, Expr<'ast>>,
}

/// See also [Compare](https://docs.python.org/3/library/ast.html#ast.Compare)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprCompare<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub left: ArenaBox<'ast, Expr<'ast>>,
    pub ops: ArenaSlice<'ast, crate::CmpOp>,
    pub comparators: ArenaSlice<'ast, Expr<'ast>>,
}

/// See also [Call](https://docs.python.org/3/library/ast.html#ast.Call)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprCall<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub func: ArenaBox<'ast, Expr<'ast>>,
    pub arguments: crate::Arguments<'ast>,
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
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprFString<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub value: crate::FStringValue<'ast>,
}

/// An AST node that represents either a single-part t-string literal
/// or an implicitly concatenated t-string literal.
///
/// This type differs from the original Python AST `TemplateStr` in that it
/// doesn't join the implicitly concatenated parts into a single string. Instead,
/// it keeps them separate and provide various methods to access the parts.
///
/// See also [TemplateStr](https://docs.python.org/3/library/ast.html#ast.TemplateStr)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprTString<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub value: crate::TStringValue<'ast>,
}

/// An AST node that represents either a single-part string literal
/// or an implicitly concatenated string literal.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprStringLiteral<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub value: crate::StringLiteralValue<'ast>,
}

/// An AST node that represents either a single-part bytestring literal
/// or an implicitly concatenated bytestring literal.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprBytesLiteral<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub value: crate::BytesLiteralValue<'ast>,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprNumberLiteral<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub value: crate::Number<'ast>,
}

#[derive(Clone, Debug, PartialEq, Default)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprBooleanLiteral {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub value: bool,
}

#[derive(Clone, Debug, PartialEq, Default)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprNoneLiteral {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
}

#[derive(Clone, Debug, PartialEq, Default)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprEllipsisLiteral {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
}

/// See also [Attribute](https://docs.python.org/3/library/ast.html#ast.Attribute)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprAttribute<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub value: ArenaBox<'ast, Expr<'ast>>,
    pub attr: crate::Identifier<'ast>,
    pub ctx: crate::ExprContext,
}

/// See also [Subscript](https://docs.python.org/3/library/ast.html#ast.Subscript)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprSubscript<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub value: ArenaBox<'ast, Expr<'ast>>,
    pub slice: ArenaBox<'ast, Expr<'ast>>,
    pub ctx: crate::ExprContext,
}

/// See also [Starred](https://docs.python.org/3/library/ast.html#ast.Starred)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprStarred<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub value: ArenaBox<'ast, Expr<'ast>>,
    pub ctx: crate::ExprContext,
}

/// See also [Name](https://docs.python.org/3/library/ast.html#ast.Name)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprName<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub id: AstName<'ast>,
    pub ctx: crate::ExprContext,
}

/// See also [List](https://docs.python.org/3/library/ast.html#ast.List)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprList<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub elts: ArenaSlice<'ast, Expr<'ast>>,
    pub ctx: crate::ExprContext,
}

/// See also [Tuple](https://docs.python.org/3/library/ast.html#ast.Tuple)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprTuple<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub elts: ArenaSlice<'ast, Expr<'ast>>,
    pub ctx: crate::ExprContext,
    pub parenthesized: bool,
}

/// See also [Slice](https://docs.python.org/3/library/ast.html#ast.Slice)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprSlice<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub lower: Option<ArenaBox<'ast, Expr<'ast>>>,
    pub upper: Option<ArenaBox<'ast, Expr<'ast>>>,
    pub step: Option<ArenaBox<'ast, Expr<'ast>>>,
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
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct ExprIpyEscapeCommand<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub kind: crate::IpyEscapeKind,
    pub value: ArenaBox<'ast, str>,
}

/// See also [MatchValue](https://docs.python.org/3/library/ast.html#ast.MatchValue)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct PatternMatchValue<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub value: ArenaBox<'ast, Expr<'ast>>,
}

/// See also [MatchSingleton](https://docs.python.org/3/library/ast.html#ast.MatchSingleton)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct PatternMatchSingleton {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub value: crate::Singleton,
}

/// See also [MatchSequence](https://docs.python.org/3/library/ast.html#ast.MatchSequence)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct PatternMatchSequence<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub patterns: ArenaSlice<'ast, Pattern<'ast>>,
}

/// See also [MatchMapping](https://docs.python.org/3/library/ast.html#ast.MatchMapping)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct PatternMatchMapping<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub keys: ArenaSlice<'ast, Expr<'ast>>,
    pub patterns: ArenaSlice<'ast, Pattern<'ast>>,
    pub rest: Option<crate::Identifier<'ast>>,
}

/// See also [MatchClass](https://docs.python.org/3/library/ast.html#ast.MatchClass)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct PatternMatchClass<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub cls: ArenaBox<'ast, Expr<'ast>>,
    pub arguments: crate::PatternArguments<'ast>,
}

/// See also [MatchStar](https://docs.python.org/3/library/ast.html#ast.MatchStar)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct PatternMatchStar<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub name: Option<crate::Identifier<'ast>>,
}

/// See also [MatchAs](https://docs.python.org/3/library/ast.html#ast.MatchAs)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct PatternMatchAs<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub pattern: Option<ArenaBox<'ast, Pattern<'ast>>>,
    pub name: Option<crate::Identifier<'ast>>,
}

/// See also [MatchOr](https://docs.python.org/3/library/ast.html#ast.MatchOr)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct PatternMatchOr<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub patterns: ArenaSlice<'ast, Pattern<'ast>>,
}

/// See also [TypeVar](https://docs.python.org/3/library/ast.html#ast.TypeVar)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct TypeParamTypeVar<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub name: crate::Identifier<'ast>,
    pub bound: Option<ArenaBox<'ast, Expr<'ast>>>,
    pub default: Option<ArenaBox<'ast, Expr<'ast>>>,
}

/// See also [TypeVarTuple](https://docs.python.org/3/library/ast.html#ast.TypeVarTuple)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct TypeParamTypeVarTuple<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub name: crate::Identifier<'ast>,
    pub default: Option<ArenaBox<'ast, Expr<'ast>>>,
}

/// See also [ParamSpec](https://docs.python.org/3/library/ast.html#ast.ParamSpec)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct TypeParamParamSpec<'ast> {
    pub node_index: crate::AtomicNodeIndex,
    pub range: ruff_text_size::TextRange,
    pub name: crate::Identifier<'ast>,
    pub default: Option<ArenaBox<'ast, Expr<'ast>>>,
}

impl<'ast> crate::ModModule<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ModModule {
            body,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_body(body);
    }
}

impl<'ast> crate::ModExpression<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ModExpression {
            body,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_expr(body);
    }
}

impl<'ast> crate::StmtFunctionDef<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let StmtFunctionDef {
            is_async: _,
            decorator_list,
            name,
            type_params,
            parameters,
            returns,
            body,
            range: _,
            node_index: _,
        } = self;

        for elm in decorator_list {
            visitor.visit_decorator(elm);
        }
        visitor.visit_identifier(name);

        if let Some(type_params) = type_params {
            visitor.visit_type_params(type_params);
        }

        visitor.visit_parameters(parameters);

        if let Some(returns) = returns {
            visitor.visit_annotation(returns);
        }

        visitor.visit_body(body);
    }
}

impl<'ast> crate::StmtClassDef<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let StmtClassDef {
            decorator_list,
            name,
            type_params,
            arguments,
            body,
            range: _,
            node_index: _,
        } = self;

        for elm in decorator_list {
            visitor.visit_decorator(elm);
        }
        visitor.visit_identifier(name);

        if let Some(type_params) = type_params {
            visitor.visit_type_params(type_params);
        }

        if let Some(arguments) = arguments {
            visitor.visit_arguments(arguments);
        }

        visitor.visit_body(body);
    }
}

impl<'ast> crate::StmtReturn<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let StmtReturn {
            value,
            range: _,
            node_index: _,
        } = self;

        if let Some(value) = value {
            visitor.visit_expr(value);
        }
    }
}

impl<'ast> crate::StmtDelete<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let StmtDelete {
            targets,
            range: _,
            node_index: _,
        } = self;

        for elm in targets {
            visitor.visit_expr(elm);
        }
    }
}

impl<'ast> crate::StmtTypeAlias<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let StmtTypeAlias {
            name,
            type_params,
            value,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_expr(name);

        if let Some(type_params) = type_params {
            visitor.visit_type_params(type_params);
        }

        visitor.visit_expr(value);
    }
}

impl<'ast> crate::StmtAssign<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let StmtAssign {
            targets,
            value,
            range: _,
            node_index: _,
        } = self;

        for elm in targets {
            visitor.visit_expr(elm);
        }
        visitor.visit_expr(value);
    }
}

impl<'ast> crate::StmtAugAssign<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let StmtAugAssign {
            target,
            op,
            value,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_expr(target);
        visitor.visit_operator(op);
        visitor.visit_expr(value);
    }
}

impl<'ast> crate::StmtAnnAssign<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let StmtAnnAssign {
            target,
            annotation,
            value,
            simple: _,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_expr(target);
        visitor.visit_annotation(annotation);

        if let Some(value) = value {
            visitor.visit_expr(value);
        }
    }
}

impl<'ast> crate::StmtFor<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let StmtFor {
            is_async: _,
            target,
            iter,
            body,
            orelse,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_expr(target);
        visitor.visit_expr(iter);
        visitor.visit_body(body);
        visitor.visit_body(orelse);
    }
}

impl<'ast> crate::StmtWhile<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let StmtWhile {
            test,
            body,
            orelse,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_expr(test);
        visitor.visit_body(body);
        visitor.visit_body(orelse);
    }
}

impl<'ast> crate::StmtIf<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let StmtIf {
            test,
            body,
            elif_else_clauses,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_expr(test);
        visitor.visit_body(body);

        for elm in elif_else_clauses {
            visitor.visit_elif_else_clause(elm);
        }
    }
}

impl<'ast> crate::StmtWith<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let StmtWith {
            is_async: _,
            items,
            body,
            range: _,
            node_index: _,
        } = self;

        for elm in items {
            visitor.visit_with_item(elm);
        }
        visitor.visit_body(body);
    }
}

impl<'ast> crate::StmtMatch<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let StmtMatch {
            subject,
            cases,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_expr(subject);

        for elm in cases {
            visitor.visit_match_case(elm);
        }
    }
}

impl<'ast> crate::StmtRaise<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let StmtRaise {
            exc,
            cause,
            range: _,
            node_index: _,
        } = self;

        if let Some(exc) = exc {
            visitor.visit_expr(exc);
        }

        if let Some(cause) = cause {
            visitor.visit_expr(cause);
        }
    }
}

impl<'ast> crate::StmtTry<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
            is_star: _,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_body(body);

        for elm in handlers {
            visitor.visit_except_handler(elm);
        }
        visitor.visit_body(orelse);
        visitor.visit_body(finalbody);
    }
}

impl<'ast> crate::StmtAssert<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let StmtAssert {
            test,
            msg,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_expr(test);

        if let Some(msg) = msg {
            visitor.visit_expr(msg);
        }
    }
}

impl<'ast> crate::StmtImport<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let StmtImport {
            names,
            is_lazy: _,
            range: _,
            node_index: _,
        } = self;

        for elm in names {
            visitor.visit_alias(elm);
        }
    }
}

impl<'ast> crate::StmtImportFrom<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let StmtImportFrom {
            module,
            names,
            level: _,
            is_lazy: _,
            range: _,
            node_index: _,
        } = self;

        if let Some(module) = module {
            visitor.visit_identifier(module);
        }

        for elm in names {
            visitor.visit_alias(elm);
        }
    }
}

impl<'ast> crate::StmtGlobal<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let StmtGlobal {
            names,
            range: _,
            node_index: _,
        } = self;

        for elm in names {
            visitor.visit_identifier(elm);
        }
    }
}

impl<'ast> crate::StmtNonlocal<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let StmtNonlocal {
            names,
            range: _,
            node_index: _,
        } = self;

        for elm in names {
            visitor.visit_identifier(elm);
        }
    }
}

impl<'ast> crate::StmtExpr<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let StmtExpr {
            value,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_expr(value);
    }
}

impl crate::StmtPass {
    pub(crate) fn visit_source_order<'a, V>(&'a self, _: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let StmtPass {
            range: _,
            node_index: _,
        } = self;
    }
}

impl crate::StmtBreak {
    pub(crate) fn visit_source_order<'a, V>(&'a self, _: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let StmtBreak {
            range: _,
            node_index: _,
        } = self;
    }
}

impl crate::StmtContinue {
    pub(crate) fn visit_source_order<'a, V>(&'a self, _: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let StmtContinue {
            range: _,
            node_index: _,
        } = self;
    }
}

impl<'ast> crate::StmtIpyEscapeCommand<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, _: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let StmtIpyEscapeCommand {
            kind: _,
            value: _,
            range: _,
            node_index: _,
        } = self;
    }
}

impl<'ast> crate::ExprNamed<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ExprNamed {
            target,
            value,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_expr(target);
        visitor.visit_expr(value);
    }
}

impl<'ast> crate::ExprBinOp<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ExprBinOp {
            left,
            op,
            right,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_expr(left);
        visitor.visit_operator(op);
        visitor.visit_expr(right);
    }
}

impl<'ast> crate::ExprUnaryOp<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ExprUnaryOp {
            op,
            operand,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_unary_op(op);
        visitor.visit_expr(operand);
    }
}

impl<'ast> crate::ExprLambda<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ExprLambda {
            parameters,
            body,
            range: _,
            node_index: _,
        } = self;

        if let Some(parameters) = parameters {
            visitor.visit_parameters(parameters);
        }

        visitor.visit_expr(body);
    }
}

impl<'ast> crate::ExprIf<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ExprIf {
            test,
            body,
            orelse,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_expr(body);
        visitor.visit_expr(test);
        visitor.visit_expr(orelse);
    }
}

impl<'ast> crate::ExprSet<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ExprSet {
            elts,
            range: _,
            node_index: _,
        } = self;

        for elm in elts {
            visitor.visit_expr(elm);
        }
    }
}

impl<'ast> crate::ExprListComp<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ExprListComp {
            elt,
            generators,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_expr(elt);

        for elm in generators {
            visitor.visit_comprehension(elm);
        }
    }
}

impl<'ast> crate::ExprSetComp<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ExprSetComp {
            elt,
            generators,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_expr(elt);

        for elm in generators {
            visitor.visit_comprehension(elm);
        }
    }
}

impl<'ast> crate::ExprDictComp<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ExprDictComp {
            key,
            value,
            generators,
            range: _,
            node_index: _,
        } = self;

        if let Some(key) = key {
            visitor.visit_expr(key);
        }

        visitor.visit_expr(value);

        for elm in generators {
            visitor.visit_comprehension(elm);
        }
    }
}

impl<'ast> crate::ExprGenerator<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ExprGenerator {
            elt,
            generators,
            parenthesized: _,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_expr(elt);

        for elm in generators {
            visitor.visit_comprehension(elm);
        }
    }
}

impl<'ast> crate::ExprAwait<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ExprAwait {
            value,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_expr(value);
    }
}

impl<'ast> crate::ExprYield<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ExprYield {
            value,
            range: _,
            node_index: _,
        } = self;

        if let Some(value) = value {
            visitor.visit_expr(value);
        }
    }
}

impl<'ast> crate::ExprYieldFrom<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ExprYieldFrom {
            value,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_expr(value);
    }
}

impl<'ast> crate::ExprCall<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ExprCall {
            func,
            arguments,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_expr(func);
        visitor.visit_arguments(arguments);
    }
}

impl<'ast> crate::ExprNumberLiteral<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, _: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ExprNumberLiteral {
            value: _,
            range: _,
            node_index: _,
        } = self;
    }
}

impl crate::ExprBooleanLiteral {
    pub(crate) fn visit_source_order<'a, V>(&'a self, _: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ExprBooleanLiteral {
            value: _,
            range: _,
            node_index: _,
        } = self;
    }
}

impl crate::ExprNoneLiteral {
    pub(crate) fn visit_source_order<'a, V>(&'a self, _: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ExprNoneLiteral {
            range: _,
            node_index: _,
        } = self;
    }
}

impl crate::ExprEllipsisLiteral {
    pub(crate) fn visit_source_order<'a, V>(&'a self, _: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ExprEllipsisLiteral {
            range: _,
            node_index: _,
        } = self;
    }
}

impl<'ast> crate::ExprAttribute<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ExprAttribute {
            value,
            attr,
            ctx: _,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_expr(value);
        visitor.visit_identifier(attr);
    }
}

impl<'ast> crate::ExprSubscript<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ExprSubscript {
            value,
            slice,
            ctx: _,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_expr(value);
        visitor.visit_expr(slice);
    }
}

impl<'ast> crate::ExprStarred<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ExprStarred {
            value,
            ctx: _,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_expr(value);
    }
}

impl<'ast> crate::ExprName<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, _: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ExprName {
            id: _,
            ctx: _,
            range: _,
            node_index: _,
        } = self;
    }
}

impl<'ast> crate::ExprList<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ExprList {
            elts,
            ctx: _,
            range: _,
            node_index: _,
        } = self;

        for elm in elts {
            visitor.visit_expr(elm);
        }
    }
}

impl<'ast> crate::ExprTuple<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ExprTuple {
            elts,
            ctx: _,
            parenthesized: _,
            range: _,
            node_index: _,
        } = self;

        for elm in elts {
            visitor.visit_expr(elm);
        }
    }
}

impl<'ast> crate::ExprSlice<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ExprSlice {
            lower,
            upper,
            step,
            range: _,
            node_index: _,
        } = self;

        if let Some(lower) = lower {
            visitor.visit_expr(lower);
        }

        if let Some(upper) = upper {
            visitor.visit_expr(upper);
        }

        if let Some(step) = step {
            visitor.visit_expr(step);
        }
    }
}

impl<'ast> crate::ExprIpyEscapeCommand<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, _: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ExprIpyEscapeCommand {
            kind: _,
            value: _,
            range: _,
            node_index: _,
        } = self;
    }
}

impl<'ast> crate::PatternMatchValue<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let PatternMatchValue {
            value,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_expr(value);
    }
}

impl crate::PatternMatchSingleton {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let PatternMatchSingleton {
            value,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_singleton(value);
    }
}

impl<'ast> crate::PatternMatchSequence<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let PatternMatchSequence {
            patterns,
            range: _,
            node_index: _,
        } = self;

        for elm in patterns {
            visitor.visit_pattern(elm);
        }
    }
}

impl<'ast> crate::PatternMatchClass<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let PatternMatchClass {
            cls,
            arguments,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_expr(cls);
        visitor.visit_pattern_arguments(arguments);
    }
}

impl<'ast> crate::PatternMatchStar<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let PatternMatchStar {
            name,
            range: _,
            node_index: _,
        } = self;

        if let Some(name) = name {
            visitor.visit_identifier(name);
        }
    }
}

impl<'ast> crate::PatternMatchAs<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let PatternMatchAs {
            pattern,
            name,
            range: _,
            node_index: _,
        } = self;

        if let Some(pattern) = pattern {
            visitor.visit_pattern(pattern);
        }

        if let Some(name) = name {
            visitor.visit_identifier(name);
        }
    }
}

impl<'ast> crate::PatternMatchOr<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let PatternMatchOr {
            patterns,
            range: _,
            node_index: _,
        } = self;

        for elm in patterns {
            visitor.visit_pattern(elm);
        }
    }
}

impl<'ast> crate::TypeParamTypeVar<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let TypeParamTypeVar {
            name,
            bound,
            default,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_identifier(name);

        if let Some(bound) = bound {
            visitor.visit_expr(bound);
        }

        if let Some(default) = default {
            visitor.visit_expr(default);
        }
    }
}

impl<'ast> crate::TypeParamTypeVarTuple<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let TypeParamTypeVarTuple {
            name,
            default,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_identifier(name);

        if let Some(default) = default {
            visitor.visit_expr(default);
        }
    }
}

impl<'ast> crate::TypeParamParamSpec<'ast> {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        'ast: 'a,
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let TypeParamParamSpec {
            name,
            default,
            range: _,
            node_index: _,
        } = self;
        visitor.visit_identifier(name);

        if let Some(default) = default {
            visitor.visit_expr(default);
        }
    }
}
