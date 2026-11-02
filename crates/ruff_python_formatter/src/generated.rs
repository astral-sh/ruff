//! This is a generated file. Don't modify it by hand! Run `crates/ruff_python_formatter/generate.py` to re-generate the file.
#![allow(unknown_lints, clippy::default_constructed_unit_structs)]

use crate::context::PyFormatContext;
use crate::{AsFormat, FormatNodeRule, IntoFormat, PyFormatter};
use ruff_formatter::{FormatOwnedWithRule, FormatResult, FormatRule};
use ruff_python_ast as ast;

macro_rules! impl_as_format {
    ($($node:ident),* $(,)?) => { $(
        impl<C> AsFormat<C> for ast::$node<'_> where Self: IntoFormat<C> {
            type Format<'a> = <Self as IntoFormat<C>>::Format where Self: 'a;
            fn format(&self) -> Self::Format<'_> { (*self).into_format() }
        }
    )* };
}
impl_as_format!(
    Mod,
    Stmt,
    Expr,
    ExceptHandler,
    InterpolatedStringElement,
    Pattern,
    TypeParam,
    Identifier,
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
    PatternArguments,
    PatternKeyword,
    Comprehension,
    Arguments,
    Parameters,
    Parameter,
    ParameterWithDefault,
    DictItem,
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
    BytesLiteral
);

impl FormatRule<ast::ModModule<'_>, PyFormatContext<'_>>
    for crate::module::mod_module::FormatModModule
{
    #[inline]
    fn fmt(&self, node: &ast::ModModule<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ModModule>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ModModule<'_> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::module::mod_module::FormatModModule,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::module::mod_module::FormatModModule::default())
    }
}

impl FormatRule<ast::ModExpression<'_>, PyFormatContext<'_>>
    for crate::module::mod_expression::FormatModExpression
{
    #[inline]
    fn fmt(&self, node: &ast::ModExpression<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ModExpression>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ModExpression<'_> {
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

impl FormatRule<ast::StmtFunctionDef<'_>, PyFormatContext<'_>>
    for crate::statement::stmt_function_def::FormatStmtFunctionDef
{
    #[inline]
    fn fmt(&self, node: &ast::StmtFunctionDef<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::StmtFunctionDef>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtFunctionDef<'_> {
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

impl FormatRule<ast::StmtClassDef<'_>, PyFormatContext<'_>>
    for crate::statement::stmt_class_def::FormatStmtClassDef
{
    #[inline]
    fn fmt(&self, node: &ast::StmtClassDef<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::StmtClassDef>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtClassDef<'_> {
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

impl FormatRule<ast::StmtReturn<'_>, PyFormatContext<'_>>
    for crate::statement::stmt_return::FormatStmtReturn
{
    #[inline]
    fn fmt(&self, node: &ast::StmtReturn<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::StmtReturn>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtReturn<'_> {
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

impl FormatRule<ast::StmtDelete<'_>, PyFormatContext<'_>>
    for crate::statement::stmt_delete::FormatStmtDelete
{
    #[inline]
    fn fmt(&self, node: &ast::StmtDelete<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::StmtDelete>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtDelete<'_> {
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

impl FormatRule<ast::StmtTypeAlias<'_>, PyFormatContext<'_>>
    for crate::statement::stmt_type_alias::FormatStmtTypeAlias
{
    #[inline]
    fn fmt(&self, node: &ast::StmtTypeAlias<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::StmtTypeAlias>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtTypeAlias<'_> {
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

impl FormatRule<ast::StmtAssign<'_>, PyFormatContext<'_>>
    for crate::statement::stmt_assign::FormatStmtAssign
{
    #[inline]
    fn fmt(&self, node: &ast::StmtAssign<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::StmtAssign>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtAssign<'_> {
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

impl FormatRule<ast::StmtAugAssign<'_>, PyFormatContext<'_>>
    for crate::statement::stmt_aug_assign::FormatStmtAugAssign
{
    #[inline]
    fn fmt(&self, node: &ast::StmtAugAssign<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::StmtAugAssign>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtAugAssign<'_> {
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

impl FormatRule<ast::StmtAnnAssign<'_>, PyFormatContext<'_>>
    for crate::statement::stmt_ann_assign::FormatStmtAnnAssign
{
    #[inline]
    fn fmt(&self, node: &ast::StmtAnnAssign<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::StmtAnnAssign>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtAnnAssign<'_> {
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

impl FormatRule<ast::StmtFor<'_>, PyFormatContext<'_>>
    for crate::statement::stmt_for::FormatStmtFor
{
    #[inline]
    fn fmt(&self, node: &ast::StmtFor<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::StmtFor>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtFor<'_> {
    type Format =
        FormatOwnedWithRule<Self, crate::statement::stmt_for::FormatStmtFor, PyFormatContext<'ast>>;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::statement::stmt_for::FormatStmtFor::default())
    }
}

impl FormatRule<ast::StmtWhile<'_>, PyFormatContext<'_>>
    for crate::statement::stmt_while::FormatStmtWhile
{
    #[inline]
    fn fmt(&self, node: &ast::StmtWhile<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::StmtWhile>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtWhile<'_> {
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

impl FormatRule<ast::StmtIf<'_>, PyFormatContext<'_>> for crate::statement::stmt_if::FormatStmtIf {
    #[inline]
    fn fmt(&self, node: &ast::StmtIf<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::StmtIf>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtIf<'_> {
    type Format =
        FormatOwnedWithRule<Self, crate::statement::stmt_if::FormatStmtIf, PyFormatContext<'ast>>;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::statement::stmt_if::FormatStmtIf::default())
    }
}

impl FormatRule<ast::StmtWith<'_>, PyFormatContext<'_>>
    for crate::statement::stmt_with::FormatStmtWith
{
    #[inline]
    fn fmt(&self, node: &ast::StmtWith<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::StmtWith>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtWith<'_> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::statement::stmt_with::FormatStmtWith,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::statement::stmt_with::FormatStmtWith::default())
    }
}

impl FormatRule<ast::StmtMatch<'_>, PyFormatContext<'_>>
    for crate::statement::stmt_match::FormatStmtMatch
{
    #[inline]
    fn fmt(&self, node: &ast::StmtMatch<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::StmtMatch>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtMatch<'_> {
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

impl FormatRule<ast::StmtRaise<'_>, PyFormatContext<'_>>
    for crate::statement::stmt_raise::FormatStmtRaise
{
    #[inline]
    fn fmt(&self, node: &ast::StmtRaise<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::StmtRaise>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtRaise<'_> {
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

impl FormatRule<ast::StmtTry<'_>, PyFormatContext<'_>>
    for crate::statement::stmt_try::FormatStmtTry
{
    #[inline]
    fn fmt(&self, node: &ast::StmtTry<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::StmtTry>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtTry<'_> {
    type Format =
        FormatOwnedWithRule<Self, crate::statement::stmt_try::FormatStmtTry, PyFormatContext<'ast>>;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::statement::stmt_try::FormatStmtTry::default())
    }
}

impl FormatRule<ast::StmtAssert<'_>, PyFormatContext<'_>>
    for crate::statement::stmt_assert::FormatStmtAssert
{
    #[inline]
    fn fmt(&self, node: &ast::StmtAssert<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::StmtAssert>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtAssert<'_> {
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

impl FormatRule<ast::StmtImport<'_>, PyFormatContext<'_>>
    for crate::statement::stmt_import::FormatStmtImport
{
    #[inline]
    fn fmt(&self, node: &ast::StmtImport<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::StmtImport>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtImport<'_> {
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

impl FormatRule<ast::StmtImportFrom<'_>, PyFormatContext<'_>>
    for crate::statement::stmt_import_from::FormatStmtImportFrom
{
    #[inline]
    fn fmt(&self, node: &ast::StmtImportFrom<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::StmtImportFrom>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtImportFrom<'_> {
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

impl FormatRule<ast::StmtGlobal<'_>, PyFormatContext<'_>>
    for crate::statement::stmt_global::FormatStmtGlobal
{
    #[inline]
    fn fmt(&self, node: &ast::StmtGlobal<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::StmtGlobal>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtGlobal<'_> {
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

impl FormatRule<ast::StmtNonlocal<'_>, PyFormatContext<'_>>
    for crate::statement::stmt_nonlocal::FormatStmtNonlocal
{
    #[inline]
    fn fmt(&self, node: &ast::StmtNonlocal<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::StmtNonlocal>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtNonlocal<'_> {
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

impl FormatRule<ast::StmtExpr<'_>, PyFormatContext<'_>>
    for crate::statement::stmt_expr::FormatStmtExpr
{
    #[inline]
    fn fmt(&self, node: &ast::StmtExpr<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::StmtExpr>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtExpr<'_> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::statement::stmt_expr::FormatStmtExpr,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::statement::stmt_expr::FormatStmtExpr::default())
    }
}

impl FormatRule<ast::StmtPass<'_>, PyFormatContext<'_>>
    for crate::statement::stmt_pass::FormatStmtPass
{
    #[inline]
    fn fmt(&self, node: &ast::StmtPass<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::StmtPass>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtPass<'_> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::statement::stmt_pass::FormatStmtPass,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::statement::stmt_pass::FormatStmtPass::default())
    }
}

impl FormatRule<ast::StmtBreak<'_>, PyFormatContext<'_>>
    for crate::statement::stmt_break::FormatStmtBreak
{
    #[inline]
    fn fmt(&self, node: &ast::StmtBreak<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::StmtBreak>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtBreak<'_> {
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

impl FormatRule<ast::StmtContinue<'_>, PyFormatContext<'_>>
    for crate::statement::stmt_continue::FormatStmtContinue
{
    #[inline]
    fn fmt(&self, node: &ast::StmtContinue<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::StmtContinue>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtContinue<'_> {
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

impl FormatRule<ast::StmtIpyEscapeCommand<'_>, PyFormatContext<'_>>
    for crate::statement::stmt_ipy_escape_command::FormatStmtIpyEscapeCommand
{
    #[inline]
    fn fmt(&self, node: &ast::StmtIpyEscapeCommand<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::StmtIpyEscapeCommand>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtIpyEscapeCommand<'_> {
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

impl FormatRule<ast::ExprBoolOp<'_>, PyFormatContext<'_>>
    for crate::expression::expr_bool_op::FormatExprBoolOp
{
    #[inline]
    fn fmt(&self, node: &ast::ExprBoolOp<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprBoolOp>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprBoolOp<'_> {
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

impl FormatRule<ast::ExprNamed<'_>, PyFormatContext<'_>>
    for crate::expression::expr_named::FormatExprNamed
{
    #[inline]
    fn fmt(&self, node: &ast::ExprNamed<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprNamed>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprNamed<'_> {
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

impl FormatRule<ast::ExprBinOp<'_>, PyFormatContext<'_>>
    for crate::expression::expr_bin_op::FormatExprBinOp
{
    #[inline]
    fn fmt(&self, node: &ast::ExprBinOp<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprBinOp>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprBinOp<'_> {
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

impl FormatRule<ast::ExprUnaryOp<'_>, PyFormatContext<'_>>
    for crate::expression::expr_unary_op::FormatExprUnaryOp
{
    #[inline]
    fn fmt(&self, node: &ast::ExprUnaryOp<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprUnaryOp>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprUnaryOp<'_> {
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

impl FormatRule<ast::ExprLambda<'_>, PyFormatContext<'_>>
    for crate::expression::expr_lambda::FormatExprLambda
{
    #[inline]
    fn fmt(&self, node: &ast::ExprLambda<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprLambda>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprLambda<'_> {
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

impl FormatRule<ast::ExprIf<'_>, PyFormatContext<'_>> for crate::expression::expr_if::FormatExprIf {
    #[inline]
    fn fmt(&self, node: &ast::ExprIf<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprIf>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprIf<'_> {
    type Format =
        FormatOwnedWithRule<Self, crate::expression::expr_if::FormatExprIf, PyFormatContext<'ast>>;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::expression::expr_if::FormatExprIf::default())
    }
}

impl FormatRule<ast::ExprDict<'_>, PyFormatContext<'_>>
    for crate::expression::expr_dict::FormatExprDict
{
    #[inline]
    fn fmt(&self, node: &ast::ExprDict<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprDict>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprDict<'_> {
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

impl FormatRule<ast::ExprSet<'_>, PyFormatContext<'_>>
    for crate::expression::expr_set::FormatExprSet
{
    #[inline]
    fn fmt(&self, node: &ast::ExprSet<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprSet>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprSet<'_> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::expression::expr_set::FormatExprSet,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::expression::expr_set::FormatExprSet::default())
    }
}

impl FormatRule<ast::ExprListComp<'_>, PyFormatContext<'_>>
    for crate::expression::expr_list_comp::FormatExprListComp
{
    #[inline]
    fn fmt(&self, node: &ast::ExprListComp<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprListComp>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprListComp<'_> {
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

impl FormatRule<ast::ExprSetComp<'_>, PyFormatContext<'_>>
    for crate::expression::expr_set_comp::FormatExprSetComp
{
    #[inline]
    fn fmt(&self, node: &ast::ExprSetComp<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprSetComp>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprSetComp<'_> {
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

impl FormatRule<ast::ExprDictComp<'_>, PyFormatContext<'_>>
    for crate::expression::expr_dict_comp::FormatExprDictComp
{
    #[inline]
    fn fmt(&self, node: &ast::ExprDictComp<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprDictComp>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprDictComp<'_> {
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

impl FormatRule<ast::ExprGenerator<'_>, PyFormatContext<'_>>
    for crate::expression::expr_generator::FormatExprGenerator
{
    #[inline]
    fn fmt(&self, node: &ast::ExprGenerator<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprGenerator>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprGenerator<'_> {
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

impl FormatRule<ast::ExprAwait<'_>, PyFormatContext<'_>>
    for crate::expression::expr_await::FormatExprAwait
{
    #[inline]
    fn fmt(&self, node: &ast::ExprAwait<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprAwait>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprAwait<'_> {
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

impl FormatRule<ast::ExprYield<'_>, PyFormatContext<'_>>
    for crate::expression::expr_yield::FormatExprYield
{
    #[inline]
    fn fmt(&self, node: &ast::ExprYield<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprYield>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprYield<'_> {
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

impl FormatRule<ast::ExprYieldFrom<'_>, PyFormatContext<'_>>
    for crate::expression::expr_yield_from::FormatExprYieldFrom
{
    #[inline]
    fn fmt(&self, node: &ast::ExprYieldFrom<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprYieldFrom>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprYieldFrom<'_> {
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

impl FormatRule<ast::ExprCompare<'_>, PyFormatContext<'_>>
    for crate::expression::expr_compare::FormatExprCompare
{
    #[inline]
    fn fmt(&self, node: &ast::ExprCompare<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprCompare>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprCompare<'_> {
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

impl FormatRule<ast::ExprCall<'_>, PyFormatContext<'_>>
    for crate::expression::expr_call::FormatExprCall
{
    #[inline]
    fn fmt(&self, node: &ast::ExprCall<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprCall>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprCall<'_> {
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

impl FormatRule<ast::ExprFString<'_>, PyFormatContext<'_>>
    for crate::expression::expr_f_string::FormatExprFString
{
    #[inline]
    fn fmt(&self, node: &ast::ExprFString<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprFString>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprFString<'_> {
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

impl FormatRule<ast::ExprTString<'_>, PyFormatContext<'_>>
    for crate::expression::expr_t_string::FormatExprTString
{
    #[inline]
    fn fmt(&self, node: &ast::ExprTString<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprTString>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprTString<'_> {
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

impl FormatRule<ast::ExprStringLiteral<'_>, PyFormatContext<'_>>
    for crate::expression::expr_string_literal::FormatExprStringLiteral
{
    #[inline]
    fn fmt(&self, node: &ast::ExprStringLiteral<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprStringLiteral>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprStringLiteral<'_> {
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

impl FormatRule<ast::ExprBytesLiteral<'_>, PyFormatContext<'_>>
    for crate::expression::expr_bytes_literal::FormatExprBytesLiteral
{
    #[inline]
    fn fmt(&self, node: &ast::ExprBytesLiteral<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprBytesLiteral>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprBytesLiteral<'_> {
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

impl FormatRule<ast::ExprNumberLiteral<'_>, PyFormatContext<'_>>
    for crate::expression::expr_number_literal::FormatExprNumberLiteral
{
    #[inline]
    fn fmt(&self, node: &ast::ExprNumberLiteral<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprNumberLiteral>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprNumberLiteral<'_> {
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

impl FormatRule<ast::ExprBooleanLiteral<'_>, PyFormatContext<'_>>
    for crate::expression::expr_boolean_literal::FormatExprBooleanLiteral
{
    #[inline]
    fn fmt(&self, node: &ast::ExprBooleanLiteral<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprBooleanLiteral>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprBooleanLiteral<'_> {
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

impl FormatRule<ast::ExprNoneLiteral<'_>, PyFormatContext<'_>>
    for crate::expression::expr_none_literal::FormatExprNoneLiteral
{
    #[inline]
    fn fmt(&self, node: &ast::ExprNoneLiteral<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprNoneLiteral>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprNoneLiteral<'_> {
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

impl FormatRule<ast::ExprEllipsisLiteral<'_>, PyFormatContext<'_>>
    for crate::expression::expr_ellipsis_literal::FormatExprEllipsisLiteral
{
    #[inline]
    fn fmt(&self, node: &ast::ExprEllipsisLiteral<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprEllipsisLiteral>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprEllipsisLiteral<'_> {
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

impl FormatRule<ast::ExprAttribute<'_>, PyFormatContext<'_>>
    for crate::expression::expr_attribute::FormatExprAttribute
{
    #[inline]
    fn fmt(&self, node: &ast::ExprAttribute<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprAttribute>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprAttribute<'_> {
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

impl FormatRule<ast::ExprSubscript<'_>, PyFormatContext<'_>>
    for crate::expression::expr_subscript::FormatExprSubscript
{
    #[inline]
    fn fmt(&self, node: &ast::ExprSubscript<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprSubscript>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprSubscript<'_> {
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

impl FormatRule<ast::ExprStarred<'_>, PyFormatContext<'_>>
    for crate::expression::expr_starred::FormatExprStarred
{
    #[inline]
    fn fmt(&self, node: &ast::ExprStarred<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprStarred>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprStarred<'_> {
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

impl FormatRule<ast::ExprName<'_>, PyFormatContext<'_>>
    for crate::expression::expr_name::FormatExprName
{
    #[inline]
    fn fmt(&self, node: &ast::ExprName<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprName>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprName<'_> {
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

impl FormatRule<ast::ExprList<'_>, PyFormatContext<'_>>
    for crate::expression::expr_list::FormatExprList
{
    #[inline]
    fn fmt(&self, node: &ast::ExprList<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprList>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprList<'_> {
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

impl FormatRule<ast::ExprTuple<'_>, PyFormatContext<'_>>
    for crate::expression::expr_tuple::FormatExprTuple
{
    #[inline]
    fn fmt(&self, node: &ast::ExprTuple<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprTuple>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprTuple<'_> {
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

impl FormatRule<ast::ExprSlice<'_>, PyFormatContext<'_>>
    for crate::expression::expr_slice::FormatExprSlice
{
    #[inline]
    fn fmt(&self, node: &ast::ExprSlice<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprSlice>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprSlice<'_> {
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

impl FormatRule<ast::ExprIpyEscapeCommand<'_>, PyFormatContext<'_>>
    for crate::expression::expr_ipy_escape_command::FormatExprIpyEscapeCommand
{
    #[inline]
    fn fmt(&self, node: &ast::ExprIpyEscapeCommand<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExprIpyEscapeCommand>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprIpyEscapeCommand<'_> {
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

impl FormatRule<ast::ExceptHandlerExceptHandler<'_>, PyFormatContext<'_>>
    for crate::other::except_handler_except_handler::FormatExceptHandlerExceptHandler
{
    #[inline]
    fn fmt(
        &self,
        node: &ast::ExceptHandlerExceptHandler<'_>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ExceptHandlerExceptHandler>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExceptHandlerExceptHandler<'_> {
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

impl FormatRule<ast::PatternMatchValue<'_>, PyFormatContext<'_>>
    for crate::pattern::pattern_match_value::FormatPatternMatchValue
{
    #[inline]
    fn fmt(&self, node: &ast::PatternMatchValue<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::PatternMatchValue>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::PatternMatchValue<'_> {
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

impl FormatRule<ast::PatternMatchSingleton<'_>, PyFormatContext<'_>>
    for crate::pattern::pattern_match_singleton::FormatPatternMatchSingleton
{
    #[inline]
    fn fmt(&self, node: &ast::PatternMatchSingleton<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::PatternMatchSingleton>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::PatternMatchSingleton<'_> {
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

impl FormatRule<ast::PatternMatchSequence<'_>, PyFormatContext<'_>>
    for crate::pattern::pattern_match_sequence::FormatPatternMatchSequence
{
    #[inline]
    fn fmt(&self, node: &ast::PatternMatchSequence<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::PatternMatchSequence>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::PatternMatchSequence<'_> {
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

impl FormatRule<ast::PatternMatchMapping<'_>, PyFormatContext<'_>>
    for crate::pattern::pattern_match_mapping::FormatPatternMatchMapping
{
    #[inline]
    fn fmt(&self, node: &ast::PatternMatchMapping<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::PatternMatchMapping>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::PatternMatchMapping<'_> {
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

impl FormatRule<ast::PatternMatchClass<'_>, PyFormatContext<'_>>
    for crate::pattern::pattern_match_class::FormatPatternMatchClass
{
    #[inline]
    fn fmt(&self, node: &ast::PatternMatchClass<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::PatternMatchClass>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::PatternMatchClass<'_> {
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

impl FormatRule<ast::PatternMatchStar<'_>, PyFormatContext<'_>>
    for crate::pattern::pattern_match_star::FormatPatternMatchStar
{
    #[inline]
    fn fmt(&self, node: &ast::PatternMatchStar<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::PatternMatchStar>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::PatternMatchStar<'_> {
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

impl FormatRule<ast::PatternMatchAs<'_>, PyFormatContext<'_>>
    for crate::pattern::pattern_match_as::FormatPatternMatchAs
{
    #[inline]
    fn fmt(&self, node: &ast::PatternMatchAs<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::PatternMatchAs>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::PatternMatchAs<'_> {
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

impl FormatRule<ast::PatternMatchOr<'_>, PyFormatContext<'_>>
    for crate::pattern::pattern_match_or::FormatPatternMatchOr
{
    #[inline]
    fn fmt(&self, node: &ast::PatternMatchOr<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::PatternMatchOr>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::PatternMatchOr<'_> {
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

impl FormatRule<ast::TypeParamTypeVar<'_>, PyFormatContext<'_>>
    for crate::type_param::type_param_type_var::FormatTypeParamTypeVar
{
    #[inline]
    fn fmt(&self, node: &ast::TypeParamTypeVar<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::TypeParamTypeVar>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::TypeParamTypeVar<'_> {
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

impl FormatRule<ast::TypeParamTypeVarTuple<'_>, PyFormatContext<'_>>
    for crate::type_param::type_param_type_var_tuple::FormatTypeParamTypeVarTuple
{
    #[inline]
    fn fmt(&self, node: &ast::TypeParamTypeVarTuple<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::TypeParamTypeVarTuple>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::TypeParamTypeVarTuple<'_> {
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

impl FormatRule<ast::TypeParamParamSpec<'_>, PyFormatContext<'_>>
    for crate::type_param::type_param_param_spec::FormatTypeParamParamSpec
{
    #[inline]
    fn fmt(&self, node: &ast::TypeParamParamSpec<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::TypeParamParamSpec>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::TypeParamParamSpec<'_> {
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

impl FormatRule<ast::PatternArguments<'_>, PyFormatContext<'_>>
    for crate::pattern::pattern_arguments::FormatPatternArguments
{
    #[inline]
    fn fmt(&self, node: &ast::PatternArguments<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::PatternArguments>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::PatternArguments<'_> {
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

impl FormatRule<ast::PatternKeyword<'_>, PyFormatContext<'_>>
    for crate::pattern::pattern_keyword::FormatPatternKeyword
{
    #[inline]
    fn fmt(&self, node: &ast::PatternKeyword<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::PatternKeyword>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::PatternKeyword<'_> {
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

impl FormatRule<ast::Comprehension<'_>, PyFormatContext<'_>>
    for crate::other::comprehension::FormatComprehension
{
    #[inline]
    fn fmt(&self, node: &ast::Comprehension<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::Comprehension>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::Comprehension<'_> {
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

impl FormatRule<ast::Arguments<'_>, PyFormatContext<'_>>
    for crate::other::arguments::FormatArguments
{
    #[inline]
    fn fmt(&self, node: &ast::Arguments<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::Arguments>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::Arguments<'_> {
    type Format =
        FormatOwnedWithRule<Self, crate::other::arguments::FormatArguments, PyFormatContext<'ast>>;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::arguments::FormatArguments::default())
    }
}

impl FormatRule<ast::Parameters<'_>, PyFormatContext<'_>>
    for crate::other::parameters::FormatParameters
{
    #[inline]
    fn fmt(&self, node: &ast::Parameters<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::Parameters>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::Parameters<'_> {
    type Format = FormatOwnedWithRule<
        Self,
        crate::other::parameters::FormatParameters,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::parameters::FormatParameters::default())
    }
}

impl FormatRule<ast::Parameter<'_>, PyFormatContext<'_>>
    for crate::other::parameter::FormatParameter
{
    #[inline]
    fn fmt(&self, node: &ast::Parameter<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::Parameter>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::Parameter<'_> {
    type Format =
        FormatOwnedWithRule<Self, crate::other::parameter::FormatParameter, PyFormatContext<'ast>>;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::parameter::FormatParameter::default())
    }
}

impl FormatRule<ast::ParameterWithDefault<'_>, PyFormatContext<'_>>
    for crate::other::parameter_with_default::FormatParameterWithDefault
{
    #[inline]
    fn fmt(&self, node: &ast::ParameterWithDefault<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ParameterWithDefault>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ParameterWithDefault<'_> {
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

impl FormatRule<ast::DictItem<'_>, PyFormatContext<'_>>
    for crate::other::dict_item::FormatDictItem
{
    #[inline]
    fn fmt(&self, node: &ast::DictItem<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::DictItem>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::DictItem<'_> {
    type Format =
        FormatOwnedWithRule<Self, crate::other::dict_item::FormatDictItem, PyFormatContext<'ast>>;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::dict_item::FormatDictItem::default())
    }
}

impl FormatRule<ast::Keyword<'_>, PyFormatContext<'_>> for crate::other::keyword::FormatKeyword {
    #[inline]
    fn fmt(&self, node: &ast::Keyword<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::Keyword>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::Keyword<'_> {
    type Format =
        FormatOwnedWithRule<Self, crate::other::keyword::FormatKeyword, PyFormatContext<'ast>>;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::keyword::FormatKeyword::default())
    }
}

impl FormatRule<ast::Alias<'_>, PyFormatContext<'_>> for crate::other::alias::FormatAlias {
    #[inline]
    fn fmt(&self, node: &ast::Alias<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::Alias>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::Alias<'_> {
    type Format =
        FormatOwnedWithRule<Self, crate::other::alias::FormatAlias, PyFormatContext<'ast>>;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::alias::FormatAlias::default())
    }
}

impl FormatRule<ast::WithItem<'_>, PyFormatContext<'_>>
    for crate::other::with_item::FormatWithItem
{
    #[inline]
    fn fmt(&self, node: &ast::WithItem<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::WithItem>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::WithItem<'_> {
    type Format =
        FormatOwnedWithRule<Self, crate::other::with_item::FormatWithItem, PyFormatContext<'ast>>;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::with_item::FormatWithItem::default())
    }
}

impl FormatRule<ast::MatchCase<'_>, PyFormatContext<'_>>
    for crate::other::match_case::FormatMatchCase
{
    #[inline]
    fn fmt(&self, node: &ast::MatchCase<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::MatchCase>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::MatchCase<'_> {
    type Format =
        FormatOwnedWithRule<Self, crate::other::match_case::FormatMatchCase, PyFormatContext<'ast>>;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::match_case::FormatMatchCase::default())
    }
}

impl FormatRule<ast::Decorator<'_>, PyFormatContext<'_>>
    for crate::other::decorator::FormatDecorator
{
    #[inline]
    fn fmt(&self, node: &ast::Decorator<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::Decorator>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::Decorator<'_> {
    type Format =
        FormatOwnedWithRule<Self, crate::other::decorator::FormatDecorator, PyFormatContext<'ast>>;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::decorator::FormatDecorator::default())
    }
}

impl FormatRule<ast::ElifElseClause<'_>, PyFormatContext<'_>>
    for crate::other::elif_else_clause::FormatElifElseClause
{
    #[inline]
    fn fmt(&self, node: &ast::ElifElseClause<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::ElifElseClause>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ElifElseClause<'_> {
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

impl FormatRule<ast::TypeParams<'_>, PyFormatContext<'_>>
    for crate::type_param::type_params::FormatTypeParams
{
    #[inline]
    fn fmt(&self, node: &ast::TypeParams<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::TypeParams>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::TypeParams<'_> {
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

impl FormatRule<ast::FString<'_>, PyFormatContext<'_>> for crate::other::f_string::FormatFString {
    #[inline]
    fn fmt(&self, node: &ast::FString<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::FString>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::FString<'_> {
    type Format =
        FormatOwnedWithRule<Self, crate::other::f_string::FormatFString, PyFormatContext<'ast>>;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::f_string::FormatFString::default())
    }
}

impl FormatRule<ast::TString<'_>, PyFormatContext<'_>> for crate::other::t_string::FormatTString {
    #[inline]
    fn fmt(&self, node: &ast::TString<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::TString>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::TString<'_> {
    type Format =
        FormatOwnedWithRule<Self, crate::other::t_string::FormatTString, PyFormatContext<'ast>>;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::t_string::FormatTString::default())
    }
}

impl FormatRule<ast::StringLiteral<'_>, PyFormatContext<'_>>
    for crate::other::string_literal::FormatStringLiteral
{
    #[inline]
    fn fmt(&self, node: &ast::StringLiteral<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::StringLiteral>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StringLiteral<'_> {
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

impl FormatRule<ast::BytesLiteral<'_>, PyFormatContext<'_>>
    for crate::other::bytes_literal::FormatBytesLiteral
{
    #[inline]
    fn fmt(&self, node: &ast::BytesLiteral<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::kind::BytesLiteral>::fmt(self, *node, f)
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::BytesLiteral<'_> {
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
