//! This is a generated file. Don't modify it by hand! Run `crates/ruff_python_formatter/generate.py` to re-generate the file.
#![allow(unknown_lints, clippy::default_constructed_unit_structs)]

use crate::context::PyFormatContext;
use crate::{FormatNodeRule, IntoFormat, PyFormatter};
use ruff_formatter::{FormatOwnedWithRule, FormatResult, FormatRule};
use ruff_python_ast as ast;
use ruff_python_ast::flat::View;

impl FormatRule<View<'_, ast::ModModule>, PyFormatContext<'_>>
    for crate::module::mod_module::FormatModModule
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ModModule>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ModModule>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ModModule> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::module::mod_module::FormatModModule,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::module::mod_module::FormatModModule::default())
    }
}

impl FormatRule<View<'_, ast::ModExpression>, PyFormatContext<'_>>
    for crate::module::mod_expression::FormatModExpression
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ModExpression>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ModExpression>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ModExpression> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::module::mod_expression::FormatModExpression,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::module::mod_expression::FormatModExpression::default(),
        )
    }
}

impl FormatRule<View<'_, ast::StmtFunctionDef>, PyFormatContext<'_>>
    for crate::statement::stmt_function_def::FormatStmtFunctionDef
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::StmtFunctionDef>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtFunctionDef>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::StmtFunctionDef> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::statement::stmt_function_def::FormatStmtFunctionDef,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_function_def::FormatStmtFunctionDef::default(),
        )
    }
}

impl FormatRule<View<'_, ast::StmtClassDef>, PyFormatContext<'_>>
    for crate::statement::stmt_class_def::FormatStmtClassDef
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::StmtClassDef>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtClassDef>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::StmtClassDef> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::statement::stmt_class_def::FormatStmtClassDef,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_class_def::FormatStmtClassDef::default(),
        )
    }
}

impl FormatRule<View<'_, ast::StmtReturn>, PyFormatContext<'_>>
    for crate::statement::stmt_return::FormatStmtReturn
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::StmtReturn>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtReturn>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::StmtReturn> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::statement::stmt_return::FormatStmtReturn,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_return::FormatStmtReturn::default(),
        )
    }
}

impl FormatRule<View<'_, ast::StmtDelete>, PyFormatContext<'_>>
    for crate::statement::stmt_delete::FormatStmtDelete
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::StmtDelete>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtDelete>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::StmtDelete> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::statement::stmt_delete::FormatStmtDelete,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_delete::FormatStmtDelete::default(),
        )
    }
}

impl FormatRule<View<'_, ast::StmtTypeAlias>, PyFormatContext<'_>>
    for crate::statement::stmt_type_alias::FormatStmtTypeAlias
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::StmtTypeAlias>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtTypeAlias>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::StmtTypeAlias> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::statement::stmt_type_alias::FormatStmtTypeAlias,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_type_alias::FormatStmtTypeAlias::default(),
        )
    }
}

impl FormatRule<View<'_, ast::StmtAssign>, PyFormatContext<'_>>
    for crate::statement::stmt_assign::FormatStmtAssign
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::StmtAssign>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtAssign>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::StmtAssign> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::statement::stmt_assign::FormatStmtAssign,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_assign::FormatStmtAssign::default(),
        )
    }
}

impl FormatRule<View<'_, ast::StmtAugAssign>, PyFormatContext<'_>>
    for crate::statement::stmt_aug_assign::FormatStmtAugAssign
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::StmtAugAssign>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtAugAssign>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::StmtAugAssign> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::statement::stmt_aug_assign::FormatStmtAugAssign,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_aug_assign::FormatStmtAugAssign::default(),
        )
    }
}

impl FormatRule<View<'_, ast::StmtAnnAssign>, PyFormatContext<'_>>
    for crate::statement::stmt_ann_assign::FormatStmtAnnAssign
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::StmtAnnAssign>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtAnnAssign>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::StmtAnnAssign> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::statement::stmt_ann_assign::FormatStmtAnnAssign,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_ann_assign::FormatStmtAnnAssign::default(),
        )
    }
}

impl FormatRule<View<'_, ast::StmtFor>, PyFormatContext<'_>>
    for crate::statement::stmt_for::FormatStmtFor
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::StmtFor>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtFor>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::StmtFor> {
    type Format =
        FormatOwnedWithRule<Self, crate::statement::stmt_for::FormatStmtFor, PyFormatContext<'ast>>;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::statement::stmt_for::FormatStmtFor::default())
    }
}

impl FormatRule<View<'_, ast::StmtWhile>, PyFormatContext<'_>>
    for crate::statement::stmt_while::FormatStmtWhile
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::StmtWhile>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtWhile>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::StmtWhile> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::statement::stmt_while::FormatStmtWhile,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_while::FormatStmtWhile::default(),
        )
    }
}

impl FormatRule<View<'_, ast::StmtIf>, PyFormatContext<'_>>
    for crate::statement::stmt_if::FormatStmtIf
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::StmtIf>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtIf>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::StmtIf> {
    type Format =
        FormatOwnedWithRule<Self, crate::statement::stmt_if::FormatStmtIf, PyFormatContext<'ast>>;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::statement::stmt_if::FormatStmtIf::default())
    }
}

impl FormatRule<View<'_, ast::StmtWith>, PyFormatContext<'_>>
    for crate::statement::stmt_with::FormatStmtWith
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::StmtWith>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtWith>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::StmtWith> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::statement::stmt_with::FormatStmtWith,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::statement::stmt_with::FormatStmtWith::default())
    }
}

impl FormatRule<View<'_, ast::StmtMatch>, PyFormatContext<'_>>
    for crate::statement::stmt_match::FormatStmtMatch
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::StmtMatch>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtMatch>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::StmtMatch> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::statement::stmt_match::FormatStmtMatch,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_match::FormatStmtMatch::default(),
        )
    }
}

impl FormatRule<View<'_, ast::StmtRaise>, PyFormatContext<'_>>
    for crate::statement::stmt_raise::FormatStmtRaise
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::StmtRaise>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtRaise>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::StmtRaise> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::statement::stmt_raise::FormatStmtRaise,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_raise::FormatStmtRaise::default(),
        )
    }
}

impl FormatRule<View<'_, ast::StmtTry>, PyFormatContext<'_>>
    for crate::statement::stmt_try::FormatStmtTry
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::StmtTry>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtTry>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::StmtTry> {
    type Format =
        FormatOwnedWithRule<Self, crate::statement::stmt_try::FormatStmtTry, PyFormatContext<'ast>>;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::statement::stmt_try::FormatStmtTry::default())
    }
}

impl FormatRule<View<'_, ast::StmtAssert>, PyFormatContext<'_>>
    for crate::statement::stmt_assert::FormatStmtAssert
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::StmtAssert>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtAssert>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::StmtAssert> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::statement::stmt_assert::FormatStmtAssert,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_assert::FormatStmtAssert::default(),
        )
    }
}

impl FormatRule<View<'_, ast::StmtImport>, PyFormatContext<'_>>
    for crate::statement::stmt_import::FormatStmtImport
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::StmtImport>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtImport>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::StmtImport> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::statement::stmt_import::FormatStmtImport,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_import::FormatStmtImport::default(),
        )
    }
}

impl FormatRule<View<'_, ast::StmtImportFrom>, PyFormatContext<'_>>
    for crate::statement::stmt_import_from::FormatStmtImportFrom
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::StmtImportFrom>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtImportFrom>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::StmtImportFrom> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::statement::stmt_import_from::FormatStmtImportFrom,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_import_from::FormatStmtImportFrom::default(),
        )
    }
}

impl FormatRule<View<'_, ast::StmtGlobal>, PyFormatContext<'_>>
    for crate::statement::stmt_global::FormatStmtGlobal
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::StmtGlobal>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtGlobal>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::StmtGlobal> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::statement::stmt_global::FormatStmtGlobal,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_global::FormatStmtGlobal::default(),
        )
    }
}

impl FormatRule<View<'_, ast::StmtNonlocal>, PyFormatContext<'_>>
    for crate::statement::stmt_nonlocal::FormatStmtNonlocal
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::StmtNonlocal>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtNonlocal>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::StmtNonlocal> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::statement::stmt_nonlocal::FormatStmtNonlocal,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_nonlocal::FormatStmtNonlocal::default(),
        )
    }
}

impl FormatRule<View<'_, ast::StmtExpr>, PyFormatContext<'_>>
    for crate::statement::stmt_expr::FormatStmtExpr
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::StmtExpr>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtExpr>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::StmtExpr> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::statement::stmt_expr::FormatStmtExpr,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::statement::stmt_expr::FormatStmtExpr::default())
    }
}

impl FormatRule<View<'_, ast::StmtPass>, PyFormatContext<'_>>
    for crate::statement::stmt_pass::FormatStmtPass
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::StmtPass>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtPass>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::StmtPass> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::statement::stmt_pass::FormatStmtPass,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::statement::stmt_pass::FormatStmtPass::default())
    }
}

impl FormatRule<View<'_, ast::StmtBreak>, PyFormatContext<'_>>
    for crate::statement::stmt_break::FormatStmtBreak
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::StmtBreak>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtBreak>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::StmtBreak> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::statement::stmt_break::FormatStmtBreak,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_break::FormatStmtBreak::default(),
        )
    }
}

impl FormatRule<View<'_, ast::StmtContinue>, PyFormatContext<'_>>
    for crate::statement::stmt_continue::FormatStmtContinue
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::StmtContinue>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtContinue>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::StmtContinue> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::statement::stmt_continue::FormatStmtContinue,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_continue::FormatStmtContinue::default(),
        )
    }
}

impl FormatRule<View<'_, ast::StmtIpyEscapeCommand>, PyFormatContext<'_>>
    for crate::statement::stmt_ipy_escape_command::FormatStmtIpyEscapeCommand
{
    #[inline]
    fn fmt(
        &self,
        node: &View<'_, ast::StmtIpyEscapeCommand>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtIpyEscapeCommand>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::StmtIpyEscapeCommand> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::statement::stmt_ipy_escape_command::FormatStmtIpyEscapeCommand,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_ipy_escape_command::FormatStmtIpyEscapeCommand::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprBoolOp>, PyFormatContext<'_>>
    for crate::expression::expr_bool_op::FormatExprBoolOp
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ExprBoolOp>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprBoolOp>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprBoolOp> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_bool_op::FormatExprBoolOp,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_bool_op::FormatExprBoolOp::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprNamed>, PyFormatContext<'_>>
    for crate::expression::expr_named::FormatExprNamed
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ExprNamed>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprNamed>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprNamed> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_named::FormatExprNamed,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_named::FormatExprNamed::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprBinOp>, PyFormatContext<'_>>
    for crate::expression::expr_bin_op::FormatExprBinOp
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ExprBinOp>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprBinOp>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprBinOp> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_bin_op::FormatExprBinOp,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_bin_op::FormatExprBinOp::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprUnaryOp>, PyFormatContext<'_>>
    for crate::expression::expr_unary_op::FormatExprUnaryOp
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ExprUnaryOp>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprUnaryOp>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprUnaryOp> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_unary_op::FormatExprUnaryOp,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_unary_op::FormatExprUnaryOp::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprLambda>, PyFormatContext<'_>>
    for crate::expression::expr_lambda::FormatExprLambda
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ExprLambda>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprLambda>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprLambda> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_lambda::FormatExprLambda,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_lambda::FormatExprLambda::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprIf>, PyFormatContext<'_>>
    for crate::expression::expr_if::FormatExprIf
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ExprIf>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprIf>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprIf> {
    type Format =
        FormatOwnedWithRule<Self, crate::expression::expr_if::FormatExprIf, PyFormatContext<'ast>>;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::expression::expr_if::FormatExprIf::default())
    }
}

impl FormatRule<View<'_, ast::ExprDict>, PyFormatContext<'_>>
    for crate::expression::expr_dict::FormatExprDict
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ExprDict>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprDict>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprDict> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_dict::FormatExprDict,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_dict::FormatExprDict::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprSet>, PyFormatContext<'_>>
    for crate::expression::expr_set::FormatExprSet
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ExprSet>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprSet>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprSet> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_set::FormatExprSet,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::expression::expr_set::FormatExprSet::default())
    }
}

impl FormatRule<View<'_, ast::ExprListComp>, PyFormatContext<'_>>
    for crate::expression::expr_list_comp::FormatExprListComp
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ExprListComp>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprListComp>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprListComp> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_list_comp::FormatExprListComp,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_list_comp::FormatExprListComp::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprSetComp>, PyFormatContext<'_>>
    for crate::expression::expr_set_comp::FormatExprSetComp
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ExprSetComp>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprSetComp>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprSetComp> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_set_comp::FormatExprSetComp,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_set_comp::FormatExprSetComp::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprDictComp>, PyFormatContext<'_>>
    for crate::expression::expr_dict_comp::FormatExprDictComp
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ExprDictComp>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprDictComp>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprDictComp> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_dict_comp::FormatExprDictComp,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_dict_comp::FormatExprDictComp::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprGenerator>, PyFormatContext<'_>>
    for crate::expression::expr_generator::FormatExprGenerator
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ExprGenerator>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprGenerator>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprGenerator> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_generator::FormatExprGenerator,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_generator::FormatExprGenerator::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprAwait>, PyFormatContext<'_>>
    for crate::expression::expr_await::FormatExprAwait
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ExprAwait>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprAwait>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprAwait> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_await::FormatExprAwait,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_await::FormatExprAwait::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprYield>, PyFormatContext<'_>>
    for crate::expression::expr_yield::FormatExprYield
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ExprYield>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprYield>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprYield> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_yield::FormatExprYield,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_yield::FormatExprYield::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprYieldFrom>, PyFormatContext<'_>>
    for crate::expression::expr_yield_from::FormatExprYieldFrom
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ExprYieldFrom>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprYieldFrom>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprYieldFrom> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_yield_from::FormatExprYieldFrom,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_yield_from::FormatExprYieldFrom::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprCompare>, PyFormatContext<'_>>
    for crate::expression::expr_compare::FormatExprCompare
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ExprCompare>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprCompare>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprCompare> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_compare::FormatExprCompare,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_compare::FormatExprCompare::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprCall>, PyFormatContext<'_>>
    for crate::expression::expr_call::FormatExprCall
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ExprCall>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprCall>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprCall> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_call::FormatExprCall,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_call::FormatExprCall::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprFString>, PyFormatContext<'_>>
    for crate::expression::expr_f_string::FormatExprFString
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ExprFString>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprFString>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprFString> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_f_string::FormatExprFString,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_f_string::FormatExprFString::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprTString>, PyFormatContext<'_>>
    for crate::expression::expr_t_string::FormatExprTString
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ExprTString>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprTString>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprTString> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_t_string::FormatExprTString,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_t_string::FormatExprTString::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprStringLiteral>, PyFormatContext<'_>>
    for crate::expression::expr_string_literal::FormatExprStringLiteral
{
    #[inline]
    fn fmt(
        &self,
        node: &View<'_, ast::ExprStringLiteral>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprStringLiteral>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprStringLiteral> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_string_literal::FormatExprStringLiteral,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_string_literal::FormatExprStringLiteral::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprBytesLiteral>, PyFormatContext<'_>>
    for crate::expression::expr_bytes_literal::FormatExprBytesLiteral
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ExprBytesLiteral>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprBytesLiteral>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprBytesLiteral> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_bytes_literal::FormatExprBytesLiteral,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_bytes_literal::FormatExprBytesLiteral::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprNumberLiteral>, PyFormatContext<'_>>
    for crate::expression::expr_number_literal::FormatExprNumberLiteral
{
    #[inline]
    fn fmt(
        &self,
        node: &View<'_, ast::ExprNumberLiteral>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprNumberLiteral>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprNumberLiteral> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_number_literal::FormatExprNumberLiteral,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_number_literal::FormatExprNumberLiteral::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprBooleanLiteral>, PyFormatContext<'_>>
    for crate::expression::expr_boolean_literal::FormatExprBooleanLiteral
{
    #[inline]
    fn fmt(
        &self,
        node: &View<'_, ast::ExprBooleanLiteral>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprBooleanLiteral>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprBooleanLiteral> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_boolean_literal::FormatExprBooleanLiteral,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_boolean_literal::FormatExprBooleanLiteral::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprNoneLiteral>, PyFormatContext<'_>>
    for crate::expression::expr_none_literal::FormatExprNoneLiteral
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ExprNoneLiteral>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprNoneLiteral>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprNoneLiteral> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_none_literal::FormatExprNoneLiteral,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_none_literal::FormatExprNoneLiteral::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprEllipsisLiteral>, PyFormatContext<'_>>
    for crate::expression::expr_ellipsis_literal::FormatExprEllipsisLiteral
{
    #[inline]
    fn fmt(
        &self,
        node: &View<'_, ast::ExprEllipsisLiteral>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprEllipsisLiteral>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprEllipsisLiteral> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_ellipsis_literal::FormatExprEllipsisLiteral,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_ellipsis_literal::FormatExprEllipsisLiteral::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprAttribute>, PyFormatContext<'_>>
    for crate::expression::expr_attribute::FormatExprAttribute
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ExprAttribute>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprAttribute>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprAttribute> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_attribute::FormatExprAttribute,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_attribute::FormatExprAttribute::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprSubscript>, PyFormatContext<'_>>
    for crate::expression::expr_subscript::FormatExprSubscript
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ExprSubscript>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprSubscript>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprSubscript> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_subscript::FormatExprSubscript,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_subscript::FormatExprSubscript::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprStarred>, PyFormatContext<'_>>
    for crate::expression::expr_starred::FormatExprStarred
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ExprStarred>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprStarred>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprStarred> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_starred::FormatExprStarred,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_starred::FormatExprStarred::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprName>, PyFormatContext<'_>>
    for crate::expression::expr_name::FormatExprName
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ExprName>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprName>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprName> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_name::FormatExprName,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_name::FormatExprName::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprList>, PyFormatContext<'_>>
    for crate::expression::expr_list::FormatExprList
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ExprList>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprList>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprList> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_list::FormatExprList,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_list::FormatExprList::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprTuple>, PyFormatContext<'_>>
    for crate::expression::expr_tuple::FormatExprTuple
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ExprTuple>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprTuple>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprTuple> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_tuple::FormatExprTuple,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_tuple::FormatExprTuple::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprSlice>, PyFormatContext<'_>>
    for crate::expression::expr_slice::FormatExprSlice
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ExprSlice>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprSlice>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprSlice> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_slice::FormatExprSlice,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_slice::FormatExprSlice::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExprIpyEscapeCommand>, PyFormatContext<'_>>
    for crate::expression::expr_ipy_escape_command::FormatExprIpyEscapeCommand
{
    #[inline]
    fn fmt(
        &self,
        node: &View<'_, ast::ExprIpyEscapeCommand>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprIpyEscapeCommand>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExprIpyEscapeCommand> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_ipy_escape_command::FormatExprIpyEscapeCommand,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_ipy_escape_command::FormatExprIpyEscapeCommand::default(),
        )
    }
}

impl FormatRule<View<'_, ast::ExceptHandlerExceptHandler>, PyFormatContext<'_>>
    for crate::other::except_handler_except_handler::FormatExceptHandlerExceptHandler
{
    #[inline]
    fn fmt(
        &self,
        node: &View<'_, ast::ExceptHandlerExceptHandler>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        FormatNodeRule::<ast::ExceptHandlerExceptHandler>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ExceptHandlerExceptHandler> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::other::except_handler_except_handler::FormatExceptHandlerExceptHandler,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::other::except_handler_except_handler::FormatExceptHandlerExceptHandler::default(
            ),
        )
    }
}

impl FormatRule<View<'_, ast::PatternMatchValue>, PyFormatContext<'_>>
    for crate::pattern::pattern_match_value::FormatPatternMatchValue
{
    #[inline]
    fn fmt(
        &self,
        node: &View<'_, ast::PatternMatchValue>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        FormatNodeRule::<ast::PatternMatchValue>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::PatternMatchValue> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::pattern::pattern_match_value::FormatPatternMatchValue,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::pattern::pattern_match_value::FormatPatternMatchValue::default(),
        )
    }
}

impl FormatRule<View<'_, ast::PatternMatchSingleton>, PyFormatContext<'_>>
    for crate::pattern::pattern_match_singleton::FormatPatternMatchSingleton
{
    #[inline]
    fn fmt(
        &self,
        node: &View<'_, ast::PatternMatchSingleton>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        FormatNodeRule::<ast::PatternMatchSingleton>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::PatternMatchSingleton> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::pattern::pattern_match_singleton::FormatPatternMatchSingleton,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::pattern::pattern_match_singleton::FormatPatternMatchSingleton::default(),
        )
    }
}

impl FormatRule<View<'_, ast::PatternMatchSequence>, PyFormatContext<'_>>
    for crate::pattern::pattern_match_sequence::FormatPatternMatchSequence
{
    #[inline]
    fn fmt(
        &self,
        node: &View<'_, ast::PatternMatchSequence>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        FormatNodeRule::<ast::PatternMatchSequence>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::PatternMatchSequence> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::pattern::pattern_match_sequence::FormatPatternMatchSequence,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::pattern::pattern_match_sequence::FormatPatternMatchSequence::default(),
        )
    }
}

impl FormatRule<View<'_, ast::PatternMatchMapping>, PyFormatContext<'_>>
    for crate::pattern::pattern_match_mapping::FormatPatternMatchMapping
{
    #[inline]
    fn fmt(
        &self,
        node: &View<'_, ast::PatternMatchMapping>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        FormatNodeRule::<ast::PatternMatchMapping>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::PatternMatchMapping> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::pattern::pattern_match_mapping::FormatPatternMatchMapping,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::pattern::pattern_match_mapping::FormatPatternMatchMapping::default(),
        )
    }
}

impl FormatRule<View<'_, ast::PatternMatchClass>, PyFormatContext<'_>>
    for crate::pattern::pattern_match_class::FormatPatternMatchClass
{
    #[inline]
    fn fmt(
        &self,
        node: &View<'_, ast::PatternMatchClass>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        FormatNodeRule::<ast::PatternMatchClass>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::PatternMatchClass> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::pattern::pattern_match_class::FormatPatternMatchClass,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::pattern::pattern_match_class::FormatPatternMatchClass::default(),
        )
    }
}

impl FormatRule<View<'_, ast::PatternMatchStar>, PyFormatContext<'_>>
    for crate::pattern::pattern_match_star::FormatPatternMatchStar
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::PatternMatchStar>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::PatternMatchStar>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::PatternMatchStar> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::pattern::pattern_match_star::FormatPatternMatchStar,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::pattern::pattern_match_star::FormatPatternMatchStar::default(),
        )
    }
}

impl FormatRule<View<'_, ast::PatternMatchAs>, PyFormatContext<'_>>
    for crate::pattern::pattern_match_as::FormatPatternMatchAs
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::PatternMatchAs>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::PatternMatchAs>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::PatternMatchAs> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::pattern::pattern_match_as::FormatPatternMatchAs,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::pattern::pattern_match_as::FormatPatternMatchAs::default(),
        )
    }
}

impl FormatRule<View<'_, ast::PatternMatchOr>, PyFormatContext<'_>>
    for crate::pattern::pattern_match_or::FormatPatternMatchOr
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::PatternMatchOr>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::PatternMatchOr>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::PatternMatchOr> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::pattern::pattern_match_or::FormatPatternMatchOr,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::pattern::pattern_match_or::FormatPatternMatchOr::default(),
        )
    }
}

impl FormatRule<View<'_, ast::TypeParamTypeVar>, PyFormatContext<'_>>
    for crate::type_param::type_param_type_var::FormatTypeParamTypeVar
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::TypeParamTypeVar>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::TypeParamTypeVar>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::TypeParamTypeVar> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::type_param::type_param_type_var::FormatTypeParamTypeVar,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::type_param::type_param_type_var::FormatTypeParamTypeVar::default(),
        )
    }
}

impl FormatRule<View<'_, ast::TypeParamTypeVarTuple>, PyFormatContext<'_>>
    for crate::type_param::type_param_type_var_tuple::FormatTypeParamTypeVarTuple
{
    #[inline]
    fn fmt(
        &self,
        node: &View<'_, ast::TypeParamTypeVarTuple>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        FormatNodeRule::<ast::TypeParamTypeVarTuple>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::TypeParamTypeVarTuple> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::type_param::type_param_type_var_tuple::FormatTypeParamTypeVarTuple,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::type_param::type_param_type_var_tuple::FormatTypeParamTypeVarTuple::default(),
        )
    }
}

impl FormatRule<View<'_, ast::TypeParamParamSpec>, PyFormatContext<'_>>
    for crate::type_param::type_param_param_spec::FormatTypeParamParamSpec
{
    #[inline]
    fn fmt(
        &self,
        node: &View<'_, ast::TypeParamParamSpec>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        FormatNodeRule::<ast::TypeParamParamSpec>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::TypeParamParamSpec> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::type_param::type_param_param_spec::FormatTypeParamParamSpec,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::type_param::type_param_param_spec::FormatTypeParamParamSpec::default(),
        )
    }
}

impl FormatRule<View<'_, ast::PatternArguments>, PyFormatContext<'_>>
    for crate::pattern::pattern_arguments::FormatPatternArguments
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::PatternArguments>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::PatternArguments>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::PatternArguments> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::pattern::pattern_arguments::FormatPatternArguments,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::pattern::pattern_arguments::FormatPatternArguments::default(),
        )
    }
}

impl FormatRule<View<'_, ast::PatternKeyword>, PyFormatContext<'_>>
    for crate::pattern::pattern_keyword::FormatPatternKeyword
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::PatternKeyword>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::PatternKeyword>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::PatternKeyword> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::pattern::pattern_keyword::FormatPatternKeyword,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::pattern::pattern_keyword::FormatPatternKeyword::default(),
        )
    }
}

impl FormatRule<View<'_, ast::Comprehension>, PyFormatContext<'_>>
    for crate::other::comprehension::FormatComprehension
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::Comprehension>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::Comprehension>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::Comprehension> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::other::comprehension::FormatComprehension,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::other::comprehension::FormatComprehension::default(),
        )
    }
}

impl FormatRule<View<'_, ast::Arguments>, PyFormatContext<'_>>
    for crate::other::arguments::FormatArguments
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::Arguments>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::Arguments>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::Arguments> {
    type Format =
        FormatOwnedWithRule<Self, crate::other::arguments::FormatArguments, PyFormatContext<'ast>>;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::arguments::FormatArguments::default())
    }
}

impl FormatRule<View<'_, ast::Parameters>, PyFormatContext<'_>>
    for crate::other::parameters::FormatParameters
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::Parameters>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::Parameters>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::Parameters> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::other::parameters::FormatParameters,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::parameters::FormatParameters::default())
    }
}

impl FormatRule<View<'_, ast::Parameter>, PyFormatContext<'_>>
    for crate::other::parameter::FormatParameter
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::Parameter>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::Parameter>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::Parameter> {
    type Format =
        FormatOwnedWithRule<Self, crate::other::parameter::FormatParameter, PyFormatContext<'ast>>;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::parameter::FormatParameter::default())
    }
}

impl FormatRule<View<'_, ast::ParameterWithDefault>, PyFormatContext<'_>>
    for crate::other::parameter_with_default::FormatParameterWithDefault
{
    #[inline]
    fn fmt(
        &self,
        node: &View<'_, ast::ParameterWithDefault>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        FormatNodeRule::<ast::ParameterWithDefault>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ParameterWithDefault> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::other::parameter_with_default::FormatParameterWithDefault,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::other::parameter_with_default::FormatParameterWithDefault::default(),
        )
    }
}

impl FormatRule<View<'_, ast::Keyword>, PyFormatContext<'_>>
    for crate::other::keyword::FormatKeyword
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::Keyword>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::Keyword>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::Keyword> {
    type Format =
        FormatOwnedWithRule<Self, crate::other::keyword::FormatKeyword, PyFormatContext<'ast>>;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::keyword::FormatKeyword::default())
    }
}

impl FormatRule<View<'_, ast::Alias>, PyFormatContext<'_>> for crate::other::alias::FormatAlias {
    #[inline]
    fn fmt(&self, node: &View<'_, ast::Alias>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::Alias>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::Alias> {
    type Format =
        FormatOwnedWithRule<Self, crate::other::alias::FormatAlias, PyFormatContext<'ast>>;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::alias::FormatAlias::default())
    }
}

impl FormatRule<View<'_, ast::WithItem>, PyFormatContext<'_>>
    for crate::other::with_item::FormatWithItem
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::WithItem>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::WithItem>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::WithItem> {
    type Format =
        FormatOwnedWithRule<Self, crate::other::with_item::FormatWithItem, PyFormatContext<'ast>>;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::with_item::FormatWithItem::default())
    }
}

impl FormatRule<View<'_, ast::MatchCase>, PyFormatContext<'_>>
    for crate::other::match_case::FormatMatchCase
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::MatchCase>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::MatchCase>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::MatchCase> {
    type Format =
        FormatOwnedWithRule<Self, crate::other::match_case::FormatMatchCase, PyFormatContext<'ast>>;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::match_case::FormatMatchCase::default())
    }
}

impl FormatRule<View<'_, ast::Decorator>, PyFormatContext<'_>>
    for crate::other::decorator::FormatDecorator
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::Decorator>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::Decorator>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::Decorator> {
    type Format =
        FormatOwnedWithRule<Self, crate::other::decorator::FormatDecorator, PyFormatContext<'ast>>;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::decorator::FormatDecorator::default())
    }
}

impl FormatRule<View<'_, ast::ElifElseClause>, PyFormatContext<'_>>
    for crate::other::elif_else_clause::FormatElifElseClause
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::ElifElseClause>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ElifElseClause>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::ElifElseClause> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::other::elif_else_clause::FormatElifElseClause,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::other::elif_else_clause::FormatElifElseClause::default(),
        )
    }
}

impl FormatRule<View<'_, ast::TypeParams>, PyFormatContext<'_>>
    for crate::type_param::type_params::FormatTypeParams
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::TypeParams>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::TypeParams>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::TypeParams> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::type_param::type_params::FormatTypeParams,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::type_param::type_params::FormatTypeParams::default(),
        )
    }
}

impl FormatRule<View<'_, ast::FString>, PyFormatContext<'_>>
    for crate::other::f_string::FormatFString
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::FString>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::FString>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::FString> {
    type Format =
        FormatOwnedWithRule<Self, crate::other::f_string::FormatFString, PyFormatContext<'ast>>;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::f_string::FormatFString::default())
    }
}

impl FormatRule<View<'_, ast::TString>, PyFormatContext<'_>>
    for crate::other::t_string::FormatTString
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::TString>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::TString>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::TString> {
    type Format =
        FormatOwnedWithRule<Self, crate::other::t_string::FormatTString, PyFormatContext<'ast>>;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::t_string::FormatTString::default())
    }
}

impl FormatRule<View<'_, ast::StringLiteral>, PyFormatContext<'_>>
    for crate::other::string_literal::FormatStringLiteral
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::StringLiteral>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StringLiteral>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::StringLiteral> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::other::string_literal::FormatStringLiteral,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::other::string_literal::FormatStringLiteral::default(),
        )
    }
}

impl FormatRule<View<'_, ast::BytesLiteral>, PyFormatContext<'_>>
    for crate::other::bytes_literal::FormatBytesLiteral
{
    #[inline]
    fn fmt(&self, node: &View<'_, ast::BytesLiteral>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::BytesLiteral>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for View<'_, ast::BytesLiteral> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::other::bytes_literal::FormatBytesLiteral,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::other::bytes_literal::FormatBytesLiteral::default(),
        )
    }
}
