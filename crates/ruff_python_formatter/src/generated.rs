//! This is a generated file. Don't modify it by hand! Run `crates/ruff_python_formatter/generate.py` to re-generate the file.
#![allow(unknown_lints, clippy::default_constructed_unit_structs)]

use crate::context::PyFormatContext;
use crate::{AsFormat, FormatNodeRule, FormattableNode, IntoFormat, PyFormatter};
use ruff_formatter::{FormatOwnedWithRule, FormatRefWithRule, FormatResult, FormatRule};
use ruff_python_ast as ast;

impl FormattableNode for ast::ModModule<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ModModule<'ast>, PyFormatContext<'_>>
    for crate::module::mod_module::FormatModModule
{
    #[inline]
    fn fmt(&self, node: &ast::ModModule<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ModModule<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ModModule<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ModModule<'ast>,
        crate::module::mod_module::FormatModModule,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::module::mod_module::FormatModModule::default())
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ModModule<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ModModule<'ast>,
        crate::module::mod_module::FormatModModule,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::module::mod_module::FormatModModule::default())
    }
}

impl FormattableNode for ast::ModExpression<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ModExpression<'ast>, PyFormatContext<'_>>
    for crate::module::mod_expression::FormatModExpression
{
    #[inline]
    fn fmt(&self, node: &ast::ModExpression<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ModExpression<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ModExpression<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ModExpression<'ast>,
        crate::module::mod_expression::FormatModExpression,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::module::mod_expression::FormatModExpression::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ModExpression<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ModExpression<'ast>,
        crate::module::mod_expression::FormatModExpression,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::module::mod_expression::FormatModExpression::default(),
        )
    }
}

impl FormattableNode for ast::StmtFunctionDef<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::StmtFunctionDef<'ast>, PyFormatContext<'_>>
    for crate::statement::stmt_function_def::FormatStmtFunctionDef
{
    #[inline]
    fn fmt(&self, node: &ast::StmtFunctionDef<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtFunctionDef<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::StmtFunctionDef<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::StmtFunctionDef<'ast>,
        crate::statement::stmt_function_def::FormatStmtFunctionDef,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_function_def::FormatStmtFunctionDef::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::StmtFunctionDef<'ast> {
    type Format = FormatOwnedWithRule<
        ast::StmtFunctionDef<'ast>,
        crate::statement::stmt_function_def::FormatStmtFunctionDef,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_function_def::FormatStmtFunctionDef::default(),
        )
    }
}

impl FormattableNode for ast::StmtClassDef<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::StmtClassDef<'ast>, PyFormatContext<'_>>
    for crate::statement::stmt_class_def::FormatStmtClassDef
{
    #[inline]
    fn fmt(&self, node: &ast::StmtClassDef<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtClassDef<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::StmtClassDef<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::StmtClassDef<'ast>,
        crate::statement::stmt_class_def::FormatStmtClassDef,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_class_def::FormatStmtClassDef::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::StmtClassDef<'ast> {
    type Format = FormatOwnedWithRule<
        ast::StmtClassDef<'ast>,
        crate::statement::stmt_class_def::FormatStmtClassDef,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_class_def::FormatStmtClassDef::default(),
        )
    }
}

impl FormattableNode for ast::StmtReturn<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::StmtReturn<'ast>, PyFormatContext<'_>>
    for crate::statement::stmt_return::FormatStmtReturn
{
    #[inline]
    fn fmt(&self, node: &ast::StmtReturn<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtReturn<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::StmtReturn<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::StmtReturn<'ast>,
        crate::statement::stmt_return::FormatStmtReturn,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_return::FormatStmtReturn::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::StmtReturn<'ast> {
    type Format = FormatOwnedWithRule<
        ast::StmtReturn<'ast>,
        crate::statement::stmt_return::FormatStmtReturn,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_return::FormatStmtReturn::default(),
        )
    }
}

impl FormattableNode for ast::StmtDelete<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::StmtDelete<'ast>, PyFormatContext<'_>>
    for crate::statement::stmt_delete::FormatStmtDelete
{
    #[inline]
    fn fmt(&self, node: &ast::StmtDelete<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtDelete<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::StmtDelete<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::StmtDelete<'ast>,
        crate::statement::stmt_delete::FormatStmtDelete,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_delete::FormatStmtDelete::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::StmtDelete<'ast> {
    type Format = FormatOwnedWithRule<
        ast::StmtDelete<'ast>,
        crate::statement::stmt_delete::FormatStmtDelete,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_delete::FormatStmtDelete::default(),
        )
    }
}

impl FormattableNode for ast::StmtTypeAlias<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::StmtTypeAlias<'ast>, PyFormatContext<'_>>
    for crate::statement::stmt_type_alias::FormatStmtTypeAlias
{
    #[inline]
    fn fmt(&self, node: &ast::StmtTypeAlias<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtTypeAlias<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::StmtTypeAlias<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::StmtTypeAlias<'ast>,
        crate::statement::stmt_type_alias::FormatStmtTypeAlias,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_type_alias::FormatStmtTypeAlias::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::StmtTypeAlias<'ast> {
    type Format = FormatOwnedWithRule<
        ast::StmtTypeAlias<'ast>,
        crate::statement::stmt_type_alias::FormatStmtTypeAlias,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_type_alias::FormatStmtTypeAlias::default(),
        )
    }
}

impl FormattableNode for ast::StmtAssign<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::StmtAssign<'ast>, PyFormatContext<'_>>
    for crate::statement::stmt_assign::FormatStmtAssign
{
    #[inline]
    fn fmt(&self, node: &ast::StmtAssign<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtAssign<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::StmtAssign<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::StmtAssign<'ast>,
        crate::statement::stmt_assign::FormatStmtAssign,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_assign::FormatStmtAssign::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::StmtAssign<'ast> {
    type Format = FormatOwnedWithRule<
        ast::StmtAssign<'ast>,
        crate::statement::stmt_assign::FormatStmtAssign,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_assign::FormatStmtAssign::default(),
        )
    }
}

impl FormattableNode for ast::StmtAugAssign<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::StmtAugAssign<'ast>, PyFormatContext<'_>>
    for crate::statement::stmt_aug_assign::FormatStmtAugAssign
{
    #[inline]
    fn fmt(&self, node: &ast::StmtAugAssign<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtAugAssign<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::StmtAugAssign<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::StmtAugAssign<'ast>,
        crate::statement::stmt_aug_assign::FormatStmtAugAssign,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_aug_assign::FormatStmtAugAssign::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::StmtAugAssign<'ast> {
    type Format = FormatOwnedWithRule<
        ast::StmtAugAssign<'ast>,
        crate::statement::stmt_aug_assign::FormatStmtAugAssign,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_aug_assign::FormatStmtAugAssign::default(),
        )
    }
}

impl FormattableNode for ast::StmtAnnAssign<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::StmtAnnAssign<'ast>, PyFormatContext<'_>>
    for crate::statement::stmt_ann_assign::FormatStmtAnnAssign
{
    #[inline]
    fn fmt(&self, node: &ast::StmtAnnAssign<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtAnnAssign<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::StmtAnnAssign<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::StmtAnnAssign<'ast>,
        crate::statement::stmt_ann_assign::FormatStmtAnnAssign,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_ann_assign::FormatStmtAnnAssign::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::StmtAnnAssign<'ast> {
    type Format = FormatOwnedWithRule<
        ast::StmtAnnAssign<'ast>,
        crate::statement::stmt_ann_assign::FormatStmtAnnAssign,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_ann_assign::FormatStmtAnnAssign::default(),
        )
    }
}

impl FormattableNode for ast::StmtFor<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::StmtFor<'ast>, PyFormatContext<'_>>
    for crate::statement::stmt_for::FormatStmtFor
{
    #[inline]
    fn fmt(&self, node: &ast::StmtFor<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtFor<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::StmtFor<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::StmtFor<'ast>,
        crate::statement::stmt_for::FormatStmtFor,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::statement::stmt_for::FormatStmtFor::default())
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::StmtFor<'ast> {
    type Format = FormatOwnedWithRule<
        ast::StmtFor<'ast>,
        crate::statement::stmt_for::FormatStmtFor,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::statement::stmt_for::FormatStmtFor::default())
    }
}

impl FormattableNode for ast::StmtWhile<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::StmtWhile<'ast>, PyFormatContext<'_>>
    for crate::statement::stmt_while::FormatStmtWhile
{
    #[inline]
    fn fmt(&self, node: &ast::StmtWhile<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtWhile<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::StmtWhile<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::StmtWhile<'ast>,
        crate::statement::stmt_while::FormatStmtWhile,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_while::FormatStmtWhile::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::StmtWhile<'ast> {
    type Format = FormatOwnedWithRule<
        ast::StmtWhile<'ast>,
        crate::statement::stmt_while::FormatStmtWhile,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_while::FormatStmtWhile::default(),
        )
    }
}

impl FormattableNode for ast::StmtIf<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::StmtIf<'ast>, PyFormatContext<'_>>
    for crate::statement::stmt_if::FormatStmtIf
{
    #[inline]
    fn fmt(&self, node: &ast::StmtIf<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtIf<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::StmtIf<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::StmtIf<'ast>,
        crate::statement::stmt_if::FormatStmtIf,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::statement::stmt_if::FormatStmtIf::default())
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::StmtIf<'ast> {
    type Format = FormatOwnedWithRule<
        ast::StmtIf<'ast>,
        crate::statement::stmt_if::FormatStmtIf,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::statement::stmt_if::FormatStmtIf::default())
    }
}

impl FormattableNode for ast::StmtWith<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::StmtWith<'ast>, PyFormatContext<'_>>
    for crate::statement::stmt_with::FormatStmtWith
{
    #[inline]
    fn fmt(&self, node: &ast::StmtWith<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtWith<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::StmtWith<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::StmtWith<'ast>,
        crate::statement::stmt_with::FormatStmtWith,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::statement::stmt_with::FormatStmtWith::default())
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::StmtWith<'ast> {
    type Format = FormatOwnedWithRule<
        ast::StmtWith<'ast>,
        crate::statement::stmt_with::FormatStmtWith,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::statement::stmt_with::FormatStmtWith::default())
    }
}

impl FormattableNode for ast::StmtMatch<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::StmtMatch<'ast>, PyFormatContext<'_>>
    for crate::statement::stmt_match::FormatStmtMatch
{
    #[inline]
    fn fmt(&self, node: &ast::StmtMatch<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtMatch<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::StmtMatch<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::StmtMatch<'ast>,
        crate::statement::stmt_match::FormatStmtMatch,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_match::FormatStmtMatch::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::StmtMatch<'ast> {
    type Format = FormatOwnedWithRule<
        ast::StmtMatch<'ast>,
        crate::statement::stmt_match::FormatStmtMatch,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_match::FormatStmtMatch::default(),
        )
    }
}

impl FormattableNode for ast::StmtRaise<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::StmtRaise<'ast>, PyFormatContext<'_>>
    for crate::statement::stmt_raise::FormatStmtRaise
{
    #[inline]
    fn fmt(&self, node: &ast::StmtRaise<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtRaise<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::StmtRaise<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::StmtRaise<'ast>,
        crate::statement::stmt_raise::FormatStmtRaise,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_raise::FormatStmtRaise::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::StmtRaise<'ast> {
    type Format = FormatOwnedWithRule<
        ast::StmtRaise<'ast>,
        crate::statement::stmt_raise::FormatStmtRaise,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_raise::FormatStmtRaise::default(),
        )
    }
}

impl FormattableNode for ast::StmtTry<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::StmtTry<'ast>, PyFormatContext<'_>>
    for crate::statement::stmt_try::FormatStmtTry
{
    #[inline]
    fn fmt(&self, node: &ast::StmtTry<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtTry<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::StmtTry<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::StmtTry<'ast>,
        crate::statement::stmt_try::FormatStmtTry,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::statement::stmt_try::FormatStmtTry::default())
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::StmtTry<'ast> {
    type Format = FormatOwnedWithRule<
        ast::StmtTry<'ast>,
        crate::statement::stmt_try::FormatStmtTry,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::statement::stmt_try::FormatStmtTry::default())
    }
}

impl FormattableNode for ast::StmtAssert<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::StmtAssert<'ast>, PyFormatContext<'_>>
    for crate::statement::stmt_assert::FormatStmtAssert
{
    #[inline]
    fn fmt(&self, node: &ast::StmtAssert<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtAssert<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::StmtAssert<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::StmtAssert<'ast>,
        crate::statement::stmt_assert::FormatStmtAssert,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_assert::FormatStmtAssert::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::StmtAssert<'ast> {
    type Format = FormatOwnedWithRule<
        ast::StmtAssert<'ast>,
        crate::statement::stmt_assert::FormatStmtAssert,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_assert::FormatStmtAssert::default(),
        )
    }
}

impl FormattableNode for ast::StmtImport<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::StmtImport<'ast>, PyFormatContext<'_>>
    for crate::statement::stmt_import::FormatStmtImport
{
    #[inline]
    fn fmt(&self, node: &ast::StmtImport<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtImport<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::StmtImport<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::StmtImport<'ast>,
        crate::statement::stmt_import::FormatStmtImport,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_import::FormatStmtImport::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::StmtImport<'ast> {
    type Format = FormatOwnedWithRule<
        ast::StmtImport<'ast>,
        crate::statement::stmt_import::FormatStmtImport,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_import::FormatStmtImport::default(),
        )
    }
}

impl FormattableNode for ast::StmtImportFrom<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::StmtImportFrom<'ast>, PyFormatContext<'_>>
    for crate::statement::stmt_import_from::FormatStmtImportFrom
{
    #[inline]
    fn fmt(&self, node: &ast::StmtImportFrom<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtImportFrom<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::StmtImportFrom<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::StmtImportFrom<'ast>,
        crate::statement::stmt_import_from::FormatStmtImportFrom,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_import_from::FormatStmtImportFrom::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::StmtImportFrom<'ast> {
    type Format = FormatOwnedWithRule<
        ast::StmtImportFrom<'ast>,
        crate::statement::stmt_import_from::FormatStmtImportFrom,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_import_from::FormatStmtImportFrom::default(),
        )
    }
}

impl FormattableNode for ast::StmtGlobal<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::StmtGlobal<'ast>, PyFormatContext<'_>>
    for crate::statement::stmt_global::FormatStmtGlobal
{
    #[inline]
    fn fmt(&self, node: &ast::StmtGlobal<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtGlobal<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::StmtGlobal<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::StmtGlobal<'ast>,
        crate::statement::stmt_global::FormatStmtGlobal,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_global::FormatStmtGlobal::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::StmtGlobal<'ast> {
    type Format = FormatOwnedWithRule<
        ast::StmtGlobal<'ast>,
        crate::statement::stmt_global::FormatStmtGlobal,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_global::FormatStmtGlobal::default(),
        )
    }
}

impl FormattableNode for ast::StmtNonlocal<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::StmtNonlocal<'ast>, PyFormatContext<'_>>
    for crate::statement::stmt_nonlocal::FormatStmtNonlocal
{
    #[inline]
    fn fmt(&self, node: &ast::StmtNonlocal<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtNonlocal<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::StmtNonlocal<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::StmtNonlocal<'ast>,
        crate::statement::stmt_nonlocal::FormatStmtNonlocal,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_nonlocal::FormatStmtNonlocal::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::StmtNonlocal<'ast> {
    type Format = FormatOwnedWithRule<
        ast::StmtNonlocal<'ast>,
        crate::statement::stmt_nonlocal::FormatStmtNonlocal,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_nonlocal::FormatStmtNonlocal::default(),
        )
    }
}

impl FormattableNode for ast::StmtExpr<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::StmtExpr<'ast>, PyFormatContext<'_>>
    for crate::statement::stmt_expr::FormatStmtExpr
{
    #[inline]
    fn fmt(&self, node: &ast::StmtExpr<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtExpr<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::StmtExpr<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::StmtExpr<'ast>,
        crate::statement::stmt_expr::FormatStmtExpr,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::statement::stmt_expr::FormatStmtExpr::default())
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::StmtExpr<'ast> {
    type Format = FormatOwnedWithRule<
        ast::StmtExpr<'ast>,
        crate::statement::stmt_expr::FormatStmtExpr,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::statement::stmt_expr::FormatStmtExpr::default())
    }
}

impl FormattableNode for ast::StmtPass {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl FormatRule<ast::StmtPass, PyFormatContext<'_>>
    for crate::statement::stmt_pass::FormatStmtPass
{
    #[inline]
    fn fmt(&self, node: &ast::StmtPass, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtPass>::fmt(self, node, f)
    }
}
impl<'context> AsFormat<PyFormatContext<'context>> for ast::StmtPass {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::StmtPass,
        crate::statement::stmt_pass::FormatStmtPass,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::statement::stmt_pass::FormatStmtPass::default())
    }
}
impl<'context> IntoFormat<PyFormatContext<'context>> for ast::StmtPass {
    type Format = FormatOwnedWithRule<
        ast::StmtPass,
        crate::statement::stmt_pass::FormatStmtPass,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::statement::stmt_pass::FormatStmtPass::default())
    }
}

impl FormattableNode for ast::StmtBreak {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl FormatRule<ast::StmtBreak, PyFormatContext<'_>>
    for crate::statement::stmt_break::FormatStmtBreak
{
    #[inline]
    fn fmt(&self, node: &ast::StmtBreak, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtBreak>::fmt(self, node, f)
    }
}
impl<'context> AsFormat<PyFormatContext<'context>> for ast::StmtBreak {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::StmtBreak,
        crate::statement::stmt_break::FormatStmtBreak,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_break::FormatStmtBreak::default(),
        )
    }
}
impl<'context> IntoFormat<PyFormatContext<'context>> for ast::StmtBreak {
    type Format = FormatOwnedWithRule<
        ast::StmtBreak,
        crate::statement::stmt_break::FormatStmtBreak,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_break::FormatStmtBreak::default(),
        )
    }
}

impl FormattableNode for ast::StmtContinue {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl FormatRule<ast::StmtContinue, PyFormatContext<'_>>
    for crate::statement::stmt_continue::FormatStmtContinue
{
    #[inline]
    fn fmt(&self, node: &ast::StmtContinue, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtContinue>::fmt(self, node, f)
    }
}
impl<'context> AsFormat<PyFormatContext<'context>> for ast::StmtContinue {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::StmtContinue,
        crate::statement::stmt_continue::FormatStmtContinue,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_continue::FormatStmtContinue::default(),
        )
    }
}
impl<'context> IntoFormat<PyFormatContext<'context>> for ast::StmtContinue {
    type Format = FormatOwnedWithRule<
        ast::StmtContinue,
        crate::statement::stmt_continue::FormatStmtContinue,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_continue::FormatStmtContinue::default(),
        )
    }
}

impl FormattableNode for ast::StmtIpyEscapeCommand<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::StmtIpyEscapeCommand<'ast>, PyFormatContext<'_>>
    for crate::statement::stmt_ipy_escape_command::FormatStmtIpyEscapeCommand
{
    #[inline]
    fn fmt(&self, node: &ast::StmtIpyEscapeCommand<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtIpyEscapeCommand<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::StmtIpyEscapeCommand<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::StmtIpyEscapeCommand<'ast>,
        crate::statement::stmt_ipy_escape_command::FormatStmtIpyEscapeCommand,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_ipy_escape_command::FormatStmtIpyEscapeCommand::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::StmtIpyEscapeCommand<'ast> {
    type Format = FormatOwnedWithRule<
        ast::StmtIpyEscapeCommand<'ast>,
        crate::statement::stmt_ipy_escape_command::FormatStmtIpyEscapeCommand,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::statement::stmt_ipy_escape_command::FormatStmtIpyEscapeCommand::default(),
        )
    }
}

impl FormattableNode for ast::ExprBoolOp<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExprBoolOp<'ast>, PyFormatContext<'_>>
    for crate::expression::expr_bool_op::FormatExprBoolOp
{
    #[inline]
    fn fmt(&self, node: &ast::ExprBoolOp<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprBoolOp<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExprBoolOp<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprBoolOp<'ast>,
        crate::expression::expr_bool_op::FormatExprBoolOp,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_bool_op::FormatExprBoolOp::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ExprBoolOp<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ExprBoolOp<'ast>,
        crate::expression::expr_bool_op::FormatExprBoolOp,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_bool_op::FormatExprBoolOp::default(),
        )
    }
}

impl FormattableNode for ast::ExprNamed<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExprNamed<'ast>, PyFormatContext<'_>>
    for crate::expression::expr_named::FormatExprNamed
{
    #[inline]
    fn fmt(&self, node: &ast::ExprNamed<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprNamed<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExprNamed<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprNamed<'ast>,
        crate::expression::expr_named::FormatExprNamed,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_named::FormatExprNamed::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ExprNamed<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ExprNamed<'ast>,
        crate::expression::expr_named::FormatExprNamed,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_named::FormatExprNamed::default(),
        )
    }
}

impl FormattableNode for ast::ExprBinOp<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExprBinOp<'ast>, PyFormatContext<'_>>
    for crate::expression::expr_bin_op::FormatExprBinOp
{
    #[inline]
    fn fmt(&self, node: &ast::ExprBinOp<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprBinOp<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExprBinOp<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprBinOp<'ast>,
        crate::expression::expr_bin_op::FormatExprBinOp,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_bin_op::FormatExprBinOp::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ExprBinOp<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ExprBinOp<'ast>,
        crate::expression::expr_bin_op::FormatExprBinOp,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_bin_op::FormatExprBinOp::default(),
        )
    }
}

impl FormattableNode for ast::ExprUnaryOp<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExprUnaryOp<'ast>, PyFormatContext<'_>>
    for crate::expression::expr_unary_op::FormatExprUnaryOp
{
    #[inline]
    fn fmt(&self, node: &ast::ExprUnaryOp<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprUnaryOp<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExprUnaryOp<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprUnaryOp<'ast>,
        crate::expression::expr_unary_op::FormatExprUnaryOp,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_unary_op::FormatExprUnaryOp::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ExprUnaryOp<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ExprUnaryOp<'ast>,
        crate::expression::expr_unary_op::FormatExprUnaryOp,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_unary_op::FormatExprUnaryOp::default(),
        )
    }
}

impl FormattableNode for ast::ExprLambda<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExprLambda<'ast>, PyFormatContext<'_>>
    for crate::expression::expr_lambda::FormatExprLambda
{
    #[inline]
    fn fmt(&self, node: &ast::ExprLambda<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprLambda<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExprLambda<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprLambda<'ast>,
        crate::expression::expr_lambda::FormatExprLambda,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_lambda::FormatExprLambda::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ExprLambda<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ExprLambda<'ast>,
        crate::expression::expr_lambda::FormatExprLambda,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_lambda::FormatExprLambda::default(),
        )
    }
}

impl FormattableNode for ast::ExprIf<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExprIf<'ast>, PyFormatContext<'_>>
    for crate::expression::expr_if::FormatExprIf
{
    #[inline]
    fn fmt(&self, node: &ast::ExprIf<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprIf<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExprIf<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprIf<'ast>,
        crate::expression::expr_if::FormatExprIf,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::expression::expr_if::FormatExprIf::default())
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ExprIf<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ExprIf<'ast>,
        crate::expression::expr_if::FormatExprIf,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::expression::expr_if::FormatExprIf::default())
    }
}

impl FormattableNode for ast::ExprDict<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExprDict<'ast>, PyFormatContext<'_>>
    for crate::expression::expr_dict::FormatExprDict
{
    #[inline]
    fn fmt(&self, node: &ast::ExprDict<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprDict<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExprDict<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprDict<'ast>,
        crate::expression::expr_dict::FormatExprDict,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_dict::FormatExprDict::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ExprDict<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ExprDict<'ast>,
        crate::expression::expr_dict::FormatExprDict,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_dict::FormatExprDict::default(),
        )
    }
}

impl FormattableNode for ast::ExprSet<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExprSet<'ast>, PyFormatContext<'_>>
    for crate::expression::expr_set::FormatExprSet
{
    #[inline]
    fn fmt(&self, node: &ast::ExprSet<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprSet<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExprSet<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprSet<'ast>,
        crate::expression::expr_set::FormatExprSet,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::expression::expr_set::FormatExprSet::default())
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ExprSet<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ExprSet<'ast>,
        crate::expression::expr_set::FormatExprSet,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::expression::expr_set::FormatExprSet::default())
    }
}

impl FormattableNode for ast::ExprListComp<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExprListComp<'ast>, PyFormatContext<'_>>
    for crate::expression::expr_list_comp::FormatExprListComp
{
    #[inline]
    fn fmt(&self, node: &ast::ExprListComp<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprListComp<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExprListComp<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprListComp<'ast>,
        crate::expression::expr_list_comp::FormatExprListComp,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_list_comp::FormatExprListComp::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ExprListComp<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ExprListComp<'ast>,
        crate::expression::expr_list_comp::FormatExprListComp,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_list_comp::FormatExprListComp::default(),
        )
    }
}

impl FormattableNode for ast::ExprSetComp<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExprSetComp<'ast>, PyFormatContext<'_>>
    for crate::expression::expr_set_comp::FormatExprSetComp
{
    #[inline]
    fn fmt(&self, node: &ast::ExprSetComp<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprSetComp<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExprSetComp<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprSetComp<'ast>,
        crate::expression::expr_set_comp::FormatExprSetComp,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_set_comp::FormatExprSetComp::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ExprSetComp<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ExprSetComp<'ast>,
        crate::expression::expr_set_comp::FormatExprSetComp,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_set_comp::FormatExprSetComp::default(),
        )
    }
}

impl FormattableNode for ast::ExprDictComp<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExprDictComp<'ast>, PyFormatContext<'_>>
    for crate::expression::expr_dict_comp::FormatExprDictComp
{
    #[inline]
    fn fmt(&self, node: &ast::ExprDictComp<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprDictComp<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExprDictComp<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprDictComp<'ast>,
        crate::expression::expr_dict_comp::FormatExprDictComp,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_dict_comp::FormatExprDictComp::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ExprDictComp<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ExprDictComp<'ast>,
        crate::expression::expr_dict_comp::FormatExprDictComp,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_dict_comp::FormatExprDictComp::default(),
        )
    }
}

impl FormattableNode for ast::ExprGenerator<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExprGenerator<'ast>, PyFormatContext<'_>>
    for crate::expression::expr_generator::FormatExprGenerator
{
    #[inline]
    fn fmt(&self, node: &ast::ExprGenerator<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprGenerator<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExprGenerator<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprGenerator<'ast>,
        crate::expression::expr_generator::FormatExprGenerator,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_generator::FormatExprGenerator::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ExprGenerator<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ExprGenerator<'ast>,
        crate::expression::expr_generator::FormatExprGenerator,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_generator::FormatExprGenerator::default(),
        )
    }
}

impl FormattableNode for ast::ExprAwait<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExprAwait<'ast>, PyFormatContext<'_>>
    for crate::expression::expr_await::FormatExprAwait
{
    #[inline]
    fn fmt(&self, node: &ast::ExprAwait<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprAwait<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExprAwait<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprAwait<'ast>,
        crate::expression::expr_await::FormatExprAwait,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_await::FormatExprAwait::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ExprAwait<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ExprAwait<'ast>,
        crate::expression::expr_await::FormatExprAwait,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_await::FormatExprAwait::default(),
        )
    }
}

impl FormattableNode for ast::ExprYield<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExprYield<'ast>, PyFormatContext<'_>>
    for crate::expression::expr_yield::FormatExprYield
{
    #[inline]
    fn fmt(&self, node: &ast::ExprYield<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprYield<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExprYield<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprYield<'ast>,
        crate::expression::expr_yield::FormatExprYield,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_yield::FormatExprYield::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ExprYield<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ExprYield<'ast>,
        crate::expression::expr_yield::FormatExprYield,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_yield::FormatExprYield::default(),
        )
    }
}

impl FormattableNode for ast::ExprYieldFrom<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExprYieldFrom<'ast>, PyFormatContext<'_>>
    for crate::expression::expr_yield_from::FormatExprYieldFrom
{
    #[inline]
    fn fmt(&self, node: &ast::ExprYieldFrom<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprYieldFrom<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExprYieldFrom<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprYieldFrom<'ast>,
        crate::expression::expr_yield_from::FormatExprYieldFrom,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_yield_from::FormatExprYieldFrom::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ExprYieldFrom<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ExprYieldFrom<'ast>,
        crate::expression::expr_yield_from::FormatExprYieldFrom,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_yield_from::FormatExprYieldFrom::default(),
        )
    }
}

impl FormattableNode for ast::ExprCompare<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExprCompare<'ast>, PyFormatContext<'_>>
    for crate::expression::expr_compare::FormatExprCompare
{
    #[inline]
    fn fmt(&self, node: &ast::ExprCompare<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprCompare<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExprCompare<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprCompare<'ast>,
        crate::expression::expr_compare::FormatExprCompare,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_compare::FormatExprCompare::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ExprCompare<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ExprCompare<'ast>,
        crate::expression::expr_compare::FormatExprCompare,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_compare::FormatExprCompare::default(),
        )
    }
}

impl FormattableNode for ast::ExprCall<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExprCall<'ast>, PyFormatContext<'_>>
    for crate::expression::expr_call::FormatExprCall
{
    #[inline]
    fn fmt(&self, node: &ast::ExprCall<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprCall<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExprCall<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprCall<'ast>,
        crate::expression::expr_call::FormatExprCall,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_call::FormatExprCall::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ExprCall<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ExprCall<'ast>,
        crate::expression::expr_call::FormatExprCall,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_call::FormatExprCall::default(),
        )
    }
}

impl FormattableNode for ast::ExprFString<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExprFString<'ast>, PyFormatContext<'_>>
    for crate::expression::expr_f_string::FormatExprFString
{
    #[inline]
    fn fmt(&self, node: &ast::ExprFString<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprFString<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExprFString<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprFString<'ast>,
        crate::expression::expr_f_string::FormatExprFString,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_f_string::FormatExprFString::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ExprFString<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ExprFString<'ast>,
        crate::expression::expr_f_string::FormatExprFString,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_f_string::FormatExprFString::default(),
        )
    }
}

impl FormattableNode for ast::ExprTString<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExprTString<'ast>, PyFormatContext<'_>>
    for crate::expression::expr_t_string::FormatExprTString
{
    #[inline]
    fn fmt(&self, node: &ast::ExprTString<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprTString<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExprTString<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprTString<'ast>,
        crate::expression::expr_t_string::FormatExprTString,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_t_string::FormatExprTString::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ExprTString<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ExprTString<'ast>,
        crate::expression::expr_t_string::FormatExprTString,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_t_string::FormatExprTString::default(),
        )
    }
}

impl FormattableNode for ast::ExprStringLiteral<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExprStringLiteral<'ast>, PyFormatContext<'_>>
    for crate::expression::expr_string_literal::FormatExprStringLiteral
{
    #[inline]
    fn fmt(&self, node: &ast::ExprStringLiteral<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprStringLiteral<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExprStringLiteral<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprStringLiteral<'ast>,
        crate::expression::expr_string_literal::FormatExprStringLiteral,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_string_literal::FormatExprStringLiteral::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ExprStringLiteral<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ExprStringLiteral<'ast>,
        crate::expression::expr_string_literal::FormatExprStringLiteral,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_string_literal::FormatExprStringLiteral::default(),
        )
    }
}

impl FormattableNode for ast::ExprBytesLiteral<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExprBytesLiteral<'ast>, PyFormatContext<'_>>
    for crate::expression::expr_bytes_literal::FormatExprBytesLiteral
{
    #[inline]
    fn fmt(&self, node: &ast::ExprBytesLiteral<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprBytesLiteral<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExprBytesLiteral<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprBytesLiteral<'ast>,
        crate::expression::expr_bytes_literal::FormatExprBytesLiteral,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_bytes_literal::FormatExprBytesLiteral::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ExprBytesLiteral<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ExprBytesLiteral<'ast>,
        crate::expression::expr_bytes_literal::FormatExprBytesLiteral,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_bytes_literal::FormatExprBytesLiteral::default(),
        )
    }
}

impl FormattableNode for ast::ExprNumberLiteral<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExprNumberLiteral<'ast>, PyFormatContext<'_>>
    for crate::expression::expr_number_literal::FormatExprNumberLiteral
{
    #[inline]
    fn fmt(&self, node: &ast::ExprNumberLiteral<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprNumberLiteral<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExprNumberLiteral<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprNumberLiteral<'ast>,
        crate::expression::expr_number_literal::FormatExprNumberLiteral,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_number_literal::FormatExprNumberLiteral::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ExprNumberLiteral<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ExprNumberLiteral<'ast>,
        crate::expression::expr_number_literal::FormatExprNumberLiteral,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_number_literal::FormatExprNumberLiteral::default(),
        )
    }
}

impl FormattableNode for ast::ExprBooleanLiteral {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl FormatRule<ast::ExprBooleanLiteral, PyFormatContext<'_>>
    for crate::expression::expr_boolean_literal::FormatExprBooleanLiteral
{
    #[inline]
    fn fmt(&self, node: &ast::ExprBooleanLiteral, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprBooleanLiteral>::fmt(self, node, f)
    }
}
impl<'context> AsFormat<PyFormatContext<'context>> for ast::ExprBooleanLiteral {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprBooleanLiteral,
        crate::expression::expr_boolean_literal::FormatExprBooleanLiteral,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_boolean_literal::FormatExprBooleanLiteral::default(),
        )
    }
}
impl<'context> IntoFormat<PyFormatContext<'context>> for ast::ExprBooleanLiteral {
    type Format = FormatOwnedWithRule<
        ast::ExprBooleanLiteral,
        crate::expression::expr_boolean_literal::FormatExprBooleanLiteral,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_boolean_literal::FormatExprBooleanLiteral::default(),
        )
    }
}

impl FormattableNode for ast::ExprNoneLiteral {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl FormatRule<ast::ExprNoneLiteral, PyFormatContext<'_>>
    for crate::expression::expr_none_literal::FormatExprNoneLiteral
{
    #[inline]
    fn fmt(&self, node: &ast::ExprNoneLiteral, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprNoneLiteral>::fmt(self, node, f)
    }
}
impl<'context> AsFormat<PyFormatContext<'context>> for ast::ExprNoneLiteral {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprNoneLiteral,
        crate::expression::expr_none_literal::FormatExprNoneLiteral,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_none_literal::FormatExprNoneLiteral::default(),
        )
    }
}
impl<'context> IntoFormat<PyFormatContext<'context>> for ast::ExprNoneLiteral {
    type Format = FormatOwnedWithRule<
        ast::ExprNoneLiteral,
        crate::expression::expr_none_literal::FormatExprNoneLiteral,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_none_literal::FormatExprNoneLiteral::default(),
        )
    }
}

impl FormattableNode for ast::ExprEllipsisLiteral {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl FormatRule<ast::ExprEllipsisLiteral, PyFormatContext<'_>>
    for crate::expression::expr_ellipsis_literal::FormatExprEllipsisLiteral
{
    #[inline]
    fn fmt(&self, node: &ast::ExprEllipsisLiteral, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprEllipsisLiteral>::fmt(self, node, f)
    }
}
impl<'context> AsFormat<PyFormatContext<'context>> for ast::ExprEllipsisLiteral {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprEllipsisLiteral,
        crate::expression::expr_ellipsis_literal::FormatExprEllipsisLiteral,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_ellipsis_literal::FormatExprEllipsisLiteral::default(),
        )
    }
}
impl<'context> IntoFormat<PyFormatContext<'context>> for ast::ExprEllipsisLiteral {
    type Format = FormatOwnedWithRule<
        ast::ExprEllipsisLiteral,
        crate::expression::expr_ellipsis_literal::FormatExprEllipsisLiteral,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_ellipsis_literal::FormatExprEllipsisLiteral::default(),
        )
    }
}

impl FormattableNode for ast::ExprAttribute<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExprAttribute<'ast>, PyFormatContext<'_>>
    for crate::expression::expr_attribute::FormatExprAttribute
{
    #[inline]
    fn fmt(&self, node: &ast::ExprAttribute<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprAttribute<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExprAttribute<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprAttribute<'ast>,
        crate::expression::expr_attribute::FormatExprAttribute,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_attribute::FormatExprAttribute::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ExprAttribute<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ExprAttribute<'ast>,
        crate::expression::expr_attribute::FormatExprAttribute,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_attribute::FormatExprAttribute::default(),
        )
    }
}

impl FormattableNode for ast::ExprSubscript<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExprSubscript<'ast>, PyFormatContext<'_>>
    for crate::expression::expr_subscript::FormatExprSubscript
{
    #[inline]
    fn fmt(&self, node: &ast::ExprSubscript<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprSubscript<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExprSubscript<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprSubscript<'ast>,
        crate::expression::expr_subscript::FormatExprSubscript,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_subscript::FormatExprSubscript::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ExprSubscript<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ExprSubscript<'ast>,
        crate::expression::expr_subscript::FormatExprSubscript,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_subscript::FormatExprSubscript::default(),
        )
    }
}

impl FormattableNode for ast::ExprStarred<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExprStarred<'ast>, PyFormatContext<'_>>
    for crate::expression::expr_starred::FormatExprStarred
{
    #[inline]
    fn fmt(&self, node: &ast::ExprStarred<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprStarred<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExprStarred<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprStarred<'ast>,
        crate::expression::expr_starred::FormatExprStarred,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_starred::FormatExprStarred::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ExprStarred<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ExprStarred<'ast>,
        crate::expression::expr_starred::FormatExprStarred,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_starred::FormatExprStarred::default(),
        )
    }
}

impl FormattableNode for ast::ExprName<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExprName<'ast>, PyFormatContext<'_>>
    for crate::expression::expr_name::FormatExprName
{
    #[inline]
    fn fmt(&self, node: &ast::ExprName<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprName<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExprName<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprName<'ast>,
        crate::expression::expr_name::FormatExprName,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_name::FormatExprName::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ExprName<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ExprName<'ast>,
        crate::expression::expr_name::FormatExprName,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_name::FormatExprName::default(),
        )
    }
}

impl FormattableNode for ast::ExprList<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExprList<'ast>, PyFormatContext<'_>>
    for crate::expression::expr_list::FormatExprList
{
    #[inline]
    fn fmt(&self, node: &ast::ExprList<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprList<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExprList<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprList<'ast>,
        crate::expression::expr_list::FormatExprList,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_list::FormatExprList::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ExprList<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ExprList<'ast>,
        crate::expression::expr_list::FormatExprList,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_list::FormatExprList::default(),
        )
    }
}

impl FormattableNode for ast::ExprTuple<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExprTuple<'ast>, PyFormatContext<'_>>
    for crate::expression::expr_tuple::FormatExprTuple
{
    #[inline]
    fn fmt(&self, node: &ast::ExprTuple<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprTuple<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExprTuple<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprTuple<'ast>,
        crate::expression::expr_tuple::FormatExprTuple,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_tuple::FormatExprTuple::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ExprTuple<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ExprTuple<'ast>,
        crate::expression::expr_tuple::FormatExprTuple,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_tuple::FormatExprTuple::default(),
        )
    }
}

impl FormattableNode for ast::ExprSlice<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExprSlice<'ast>, PyFormatContext<'_>>
    for crate::expression::expr_slice::FormatExprSlice
{
    #[inline]
    fn fmt(&self, node: &ast::ExprSlice<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprSlice<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExprSlice<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprSlice<'ast>,
        crate::expression::expr_slice::FormatExprSlice,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_slice::FormatExprSlice::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ExprSlice<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ExprSlice<'ast>,
        crate::expression::expr_slice::FormatExprSlice,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_slice::FormatExprSlice::default(),
        )
    }
}

impl FormattableNode for ast::ExprIpyEscapeCommand<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExprIpyEscapeCommand<'ast>, PyFormatContext<'_>>
    for crate::expression::expr_ipy_escape_command::FormatExprIpyEscapeCommand
{
    #[inline]
    fn fmt(&self, node: &ast::ExprIpyEscapeCommand<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprIpyEscapeCommand<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExprIpyEscapeCommand<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExprIpyEscapeCommand<'ast>,
        crate::expression::expr_ipy_escape_command::FormatExprIpyEscapeCommand,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_ipy_escape_command::FormatExprIpyEscapeCommand::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ExprIpyEscapeCommand<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ExprIpyEscapeCommand<'ast>,
        crate::expression::expr_ipy_escape_command::FormatExprIpyEscapeCommand,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::expression::expr_ipy_escape_command::FormatExprIpyEscapeCommand::default(),
        )
    }
}

impl FormattableNode for ast::ExceptHandlerExceptHandler<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ExceptHandlerExceptHandler<'ast>, PyFormatContext<'_>>
    for crate::other::except_handler_except_handler::FormatExceptHandlerExceptHandler
{
    #[inline]
    fn fmt(
        &self,
        node: &ast::ExceptHandlerExceptHandler<'ast>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        FormatNodeRule::<ast::ExceptHandlerExceptHandler<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ExceptHandlerExceptHandler<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ExceptHandlerExceptHandler<'ast>,
        crate::other::except_handler_except_handler::FormatExceptHandlerExceptHandler,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::other::except_handler_except_handler::FormatExceptHandlerExceptHandler::default(
            ),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>>
    for ast::ExceptHandlerExceptHandler<'ast>
{
    type Format = FormatOwnedWithRule<
        ast::ExceptHandlerExceptHandler<'ast>,
        crate::other::except_handler_except_handler::FormatExceptHandlerExceptHandler,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::other::except_handler_except_handler::FormatExceptHandlerExceptHandler::default(
            ),
        )
    }
}

impl FormattableNode for ast::PatternMatchValue<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::PatternMatchValue<'ast>, PyFormatContext<'_>>
    for crate::pattern::pattern_match_value::FormatPatternMatchValue
{
    #[inline]
    fn fmt(&self, node: &ast::PatternMatchValue<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::PatternMatchValue<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::PatternMatchValue<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::PatternMatchValue<'ast>,
        crate::pattern::pattern_match_value::FormatPatternMatchValue,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::pattern::pattern_match_value::FormatPatternMatchValue::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::PatternMatchValue<'ast> {
    type Format = FormatOwnedWithRule<
        ast::PatternMatchValue<'ast>,
        crate::pattern::pattern_match_value::FormatPatternMatchValue,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::pattern::pattern_match_value::FormatPatternMatchValue::default(),
        )
    }
}

impl FormattableNode for ast::PatternMatchSingleton {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl FormatRule<ast::PatternMatchSingleton, PyFormatContext<'_>>
    for crate::pattern::pattern_match_singleton::FormatPatternMatchSingleton
{
    #[inline]
    fn fmt(&self, node: &ast::PatternMatchSingleton, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::PatternMatchSingleton>::fmt(self, node, f)
    }
}
impl<'context> AsFormat<PyFormatContext<'context>> for ast::PatternMatchSingleton {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::PatternMatchSingleton,
        crate::pattern::pattern_match_singleton::FormatPatternMatchSingleton,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::pattern::pattern_match_singleton::FormatPatternMatchSingleton::default(),
        )
    }
}
impl<'context> IntoFormat<PyFormatContext<'context>> for ast::PatternMatchSingleton {
    type Format = FormatOwnedWithRule<
        ast::PatternMatchSingleton,
        crate::pattern::pattern_match_singleton::FormatPatternMatchSingleton,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::pattern::pattern_match_singleton::FormatPatternMatchSingleton::default(),
        )
    }
}

impl FormattableNode for ast::PatternMatchSequence<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::PatternMatchSequence<'ast>, PyFormatContext<'_>>
    for crate::pattern::pattern_match_sequence::FormatPatternMatchSequence
{
    #[inline]
    fn fmt(&self, node: &ast::PatternMatchSequence<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::PatternMatchSequence<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::PatternMatchSequence<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::PatternMatchSequence<'ast>,
        crate::pattern::pattern_match_sequence::FormatPatternMatchSequence,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::pattern::pattern_match_sequence::FormatPatternMatchSequence::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::PatternMatchSequence<'ast> {
    type Format = FormatOwnedWithRule<
        ast::PatternMatchSequence<'ast>,
        crate::pattern::pattern_match_sequence::FormatPatternMatchSequence,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::pattern::pattern_match_sequence::FormatPatternMatchSequence::default(),
        )
    }
}

impl FormattableNode for ast::PatternMatchMapping<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::PatternMatchMapping<'ast>, PyFormatContext<'_>>
    for crate::pattern::pattern_match_mapping::FormatPatternMatchMapping
{
    #[inline]
    fn fmt(&self, node: &ast::PatternMatchMapping<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::PatternMatchMapping<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::PatternMatchMapping<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::PatternMatchMapping<'ast>,
        crate::pattern::pattern_match_mapping::FormatPatternMatchMapping,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::pattern::pattern_match_mapping::FormatPatternMatchMapping::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::PatternMatchMapping<'ast> {
    type Format = FormatOwnedWithRule<
        ast::PatternMatchMapping<'ast>,
        crate::pattern::pattern_match_mapping::FormatPatternMatchMapping,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::pattern::pattern_match_mapping::FormatPatternMatchMapping::default(),
        )
    }
}

impl FormattableNode for ast::PatternMatchClass<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::PatternMatchClass<'ast>, PyFormatContext<'_>>
    for crate::pattern::pattern_match_class::FormatPatternMatchClass
{
    #[inline]
    fn fmt(&self, node: &ast::PatternMatchClass<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::PatternMatchClass<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::PatternMatchClass<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::PatternMatchClass<'ast>,
        crate::pattern::pattern_match_class::FormatPatternMatchClass,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::pattern::pattern_match_class::FormatPatternMatchClass::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::PatternMatchClass<'ast> {
    type Format = FormatOwnedWithRule<
        ast::PatternMatchClass<'ast>,
        crate::pattern::pattern_match_class::FormatPatternMatchClass,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::pattern::pattern_match_class::FormatPatternMatchClass::default(),
        )
    }
}

impl FormattableNode for ast::PatternMatchStar<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::PatternMatchStar<'ast>, PyFormatContext<'_>>
    for crate::pattern::pattern_match_star::FormatPatternMatchStar
{
    #[inline]
    fn fmt(&self, node: &ast::PatternMatchStar<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::PatternMatchStar<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::PatternMatchStar<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::PatternMatchStar<'ast>,
        crate::pattern::pattern_match_star::FormatPatternMatchStar,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::pattern::pattern_match_star::FormatPatternMatchStar::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::PatternMatchStar<'ast> {
    type Format = FormatOwnedWithRule<
        ast::PatternMatchStar<'ast>,
        crate::pattern::pattern_match_star::FormatPatternMatchStar,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::pattern::pattern_match_star::FormatPatternMatchStar::default(),
        )
    }
}

impl FormattableNode for ast::PatternMatchAs<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::PatternMatchAs<'ast>, PyFormatContext<'_>>
    for crate::pattern::pattern_match_as::FormatPatternMatchAs
{
    #[inline]
    fn fmt(&self, node: &ast::PatternMatchAs<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::PatternMatchAs<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::PatternMatchAs<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::PatternMatchAs<'ast>,
        crate::pattern::pattern_match_as::FormatPatternMatchAs,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::pattern::pattern_match_as::FormatPatternMatchAs::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::PatternMatchAs<'ast> {
    type Format = FormatOwnedWithRule<
        ast::PatternMatchAs<'ast>,
        crate::pattern::pattern_match_as::FormatPatternMatchAs,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::pattern::pattern_match_as::FormatPatternMatchAs::default(),
        )
    }
}

impl FormattableNode for ast::PatternMatchOr<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::PatternMatchOr<'ast>, PyFormatContext<'_>>
    for crate::pattern::pattern_match_or::FormatPatternMatchOr
{
    #[inline]
    fn fmt(&self, node: &ast::PatternMatchOr<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::PatternMatchOr<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::PatternMatchOr<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::PatternMatchOr<'ast>,
        crate::pattern::pattern_match_or::FormatPatternMatchOr,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::pattern::pattern_match_or::FormatPatternMatchOr::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::PatternMatchOr<'ast> {
    type Format = FormatOwnedWithRule<
        ast::PatternMatchOr<'ast>,
        crate::pattern::pattern_match_or::FormatPatternMatchOr,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::pattern::pattern_match_or::FormatPatternMatchOr::default(),
        )
    }
}

impl FormattableNode for ast::TypeParamTypeVar<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::TypeParamTypeVar<'ast>, PyFormatContext<'_>>
    for crate::type_param::type_param_type_var::FormatTypeParamTypeVar
{
    #[inline]
    fn fmt(&self, node: &ast::TypeParamTypeVar<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::TypeParamTypeVar<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::TypeParamTypeVar<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::TypeParamTypeVar<'ast>,
        crate::type_param::type_param_type_var::FormatTypeParamTypeVar,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::type_param::type_param_type_var::FormatTypeParamTypeVar::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::TypeParamTypeVar<'ast> {
    type Format = FormatOwnedWithRule<
        ast::TypeParamTypeVar<'ast>,
        crate::type_param::type_param_type_var::FormatTypeParamTypeVar,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::type_param::type_param_type_var::FormatTypeParamTypeVar::default(),
        )
    }
}

impl FormattableNode for ast::TypeParamTypeVarTuple<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::TypeParamTypeVarTuple<'ast>, PyFormatContext<'_>>
    for crate::type_param::type_param_type_var_tuple::FormatTypeParamTypeVarTuple
{
    #[inline]
    fn fmt(
        &self,
        node: &ast::TypeParamTypeVarTuple<'ast>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        FormatNodeRule::<ast::TypeParamTypeVarTuple<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::TypeParamTypeVarTuple<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::TypeParamTypeVarTuple<'ast>,
        crate::type_param::type_param_type_var_tuple::FormatTypeParamTypeVarTuple,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::type_param::type_param_type_var_tuple::FormatTypeParamTypeVarTuple::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::TypeParamTypeVarTuple<'ast> {
    type Format = FormatOwnedWithRule<
        ast::TypeParamTypeVarTuple<'ast>,
        crate::type_param::type_param_type_var_tuple::FormatTypeParamTypeVarTuple,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::type_param::type_param_type_var_tuple::FormatTypeParamTypeVarTuple::default(),
        )
    }
}

impl FormattableNode for ast::TypeParamParamSpec<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::TypeParamParamSpec<'ast>, PyFormatContext<'_>>
    for crate::type_param::type_param_param_spec::FormatTypeParamParamSpec
{
    #[inline]
    fn fmt(&self, node: &ast::TypeParamParamSpec<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::TypeParamParamSpec<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::TypeParamParamSpec<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::TypeParamParamSpec<'ast>,
        crate::type_param::type_param_param_spec::FormatTypeParamParamSpec,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::type_param::type_param_param_spec::FormatTypeParamParamSpec::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::TypeParamParamSpec<'ast> {
    type Format = FormatOwnedWithRule<
        ast::TypeParamParamSpec<'ast>,
        crate::type_param::type_param_param_spec::FormatTypeParamParamSpec,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::type_param::type_param_param_spec::FormatTypeParamParamSpec::default(),
        )
    }
}

impl FormattableNode for ast::PatternArguments<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::PatternArguments<'ast>, PyFormatContext<'_>>
    for crate::pattern::pattern_arguments::FormatPatternArguments
{
    #[inline]
    fn fmt(&self, node: &ast::PatternArguments<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::PatternArguments<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::PatternArguments<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::PatternArguments<'ast>,
        crate::pattern::pattern_arguments::FormatPatternArguments,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::pattern::pattern_arguments::FormatPatternArguments::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::PatternArguments<'ast> {
    type Format = FormatOwnedWithRule<
        ast::PatternArguments<'ast>,
        crate::pattern::pattern_arguments::FormatPatternArguments,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::pattern::pattern_arguments::FormatPatternArguments::default(),
        )
    }
}

impl FormattableNode for ast::PatternKeyword<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::PatternKeyword<'ast>, PyFormatContext<'_>>
    for crate::pattern::pattern_keyword::FormatPatternKeyword
{
    #[inline]
    fn fmt(&self, node: &ast::PatternKeyword<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::PatternKeyword<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::PatternKeyword<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::PatternKeyword<'ast>,
        crate::pattern::pattern_keyword::FormatPatternKeyword,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::pattern::pattern_keyword::FormatPatternKeyword::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::PatternKeyword<'ast> {
    type Format = FormatOwnedWithRule<
        ast::PatternKeyword<'ast>,
        crate::pattern::pattern_keyword::FormatPatternKeyword,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::pattern::pattern_keyword::FormatPatternKeyword::default(),
        )
    }
}

impl FormattableNode for ast::Comprehension<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::Comprehension<'ast>, PyFormatContext<'_>>
    for crate::other::comprehension::FormatComprehension
{
    #[inline]
    fn fmt(&self, node: &ast::Comprehension<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::Comprehension<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::Comprehension<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::Comprehension<'ast>,
        crate::other::comprehension::FormatComprehension,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::other::comprehension::FormatComprehension::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::Comprehension<'ast> {
    type Format = FormatOwnedWithRule<
        ast::Comprehension<'ast>,
        crate::other::comprehension::FormatComprehension,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::other::comprehension::FormatComprehension::default(),
        )
    }
}

impl FormattableNode for ast::Arguments<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::Arguments<'ast>, PyFormatContext<'_>>
    for crate::other::arguments::FormatArguments
{
    #[inline]
    fn fmt(&self, node: &ast::Arguments<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::Arguments<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::Arguments<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::Arguments<'ast>,
        crate::other::arguments::FormatArguments,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::other::arguments::FormatArguments::default())
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::Arguments<'ast> {
    type Format = FormatOwnedWithRule<
        ast::Arguments<'ast>,
        crate::other::arguments::FormatArguments,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::arguments::FormatArguments::default())
    }
}

impl FormattableNode for ast::Parameters<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::Parameters<'ast>, PyFormatContext<'_>>
    for crate::other::parameters::FormatParameters
{
    #[inline]
    fn fmt(&self, node: &ast::Parameters<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::Parameters<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::Parameters<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::Parameters<'ast>,
        crate::other::parameters::FormatParameters,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::other::parameters::FormatParameters::default())
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::Parameters<'ast> {
    type Format = FormatOwnedWithRule<
        ast::Parameters<'ast>,
        crate::other::parameters::FormatParameters,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::parameters::FormatParameters::default())
    }
}

impl FormattableNode for ast::Parameter<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::Parameter<'ast>, PyFormatContext<'_>>
    for crate::other::parameter::FormatParameter
{
    #[inline]
    fn fmt(&self, node: &ast::Parameter<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::Parameter<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::Parameter<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::Parameter<'ast>,
        crate::other::parameter::FormatParameter,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::other::parameter::FormatParameter::default())
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::Parameter<'ast> {
    type Format = FormatOwnedWithRule<
        ast::Parameter<'ast>,
        crate::other::parameter::FormatParameter,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::parameter::FormatParameter::default())
    }
}

impl FormattableNode for ast::ParameterWithDefault<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ParameterWithDefault<'ast>, PyFormatContext<'_>>
    for crate::other::parameter_with_default::FormatParameterWithDefault
{
    #[inline]
    fn fmt(&self, node: &ast::ParameterWithDefault<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ParameterWithDefault<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ParameterWithDefault<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ParameterWithDefault<'ast>,
        crate::other::parameter_with_default::FormatParameterWithDefault,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::other::parameter_with_default::FormatParameterWithDefault::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ParameterWithDefault<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ParameterWithDefault<'ast>,
        crate::other::parameter_with_default::FormatParameterWithDefault,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::other::parameter_with_default::FormatParameterWithDefault::default(),
        )
    }
}

impl FormattableNode for ast::Keyword<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::Keyword<'ast>, PyFormatContext<'_>>
    for crate::other::keyword::FormatKeyword
{
    #[inline]
    fn fmt(&self, node: &ast::Keyword<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::Keyword<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::Keyword<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::Keyword<'ast>,
        crate::other::keyword::FormatKeyword,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::other::keyword::FormatKeyword::default())
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::Keyword<'ast> {
    type Format = FormatOwnedWithRule<
        ast::Keyword<'ast>,
        crate::other::keyword::FormatKeyword,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::keyword::FormatKeyword::default())
    }
}

impl FormattableNode for ast::Alias<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::Alias<'ast>, PyFormatContext<'_>> for crate::other::alias::FormatAlias {
    #[inline]
    fn fmt(&self, node: &ast::Alias<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::Alias<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::Alias<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::Alias<'ast>,
        crate::other::alias::FormatAlias,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::other::alias::FormatAlias::default())
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::Alias<'ast> {
    type Format = FormatOwnedWithRule<
        ast::Alias<'ast>,
        crate::other::alias::FormatAlias,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::alias::FormatAlias::default())
    }
}

impl FormattableNode for ast::WithItem<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::WithItem<'ast>, PyFormatContext<'_>>
    for crate::other::with_item::FormatWithItem
{
    #[inline]
    fn fmt(&self, node: &ast::WithItem<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::WithItem<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::WithItem<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::WithItem<'ast>,
        crate::other::with_item::FormatWithItem,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::other::with_item::FormatWithItem::default())
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::WithItem<'ast> {
    type Format = FormatOwnedWithRule<
        ast::WithItem<'ast>,
        crate::other::with_item::FormatWithItem,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::with_item::FormatWithItem::default())
    }
}

impl FormattableNode for ast::MatchCase<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::MatchCase<'ast>, PyFormatContext<'_>>
    for crate::other::match_case::FormatMatchCase
{
    #[inline]
    fn fmt(&self, node: &ast::MatchCase<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::MatchCase<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::MatchCase<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::MatchCase<'ast>,
        crate::other::match_case::FormatMatchCase,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::other::match_case::FormatMatchCase::default())
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::MatchCase<'ast> {
    type Format = FormatOwnedWithRule<
        ast::MatchCase<'ast>,
        crate::other::match_case::FormatMatchCase,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::match_case::FormatMatchCase::default())
    }
}

impl FormattableNode for ast::Decorator<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::Decorator<'ast>, PyFormatContext<'_>>
    for crate::other::decorator::FormatDecorator
{
    #[inline]
    fn fmt(&self, node: &ast::Decorator<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::Decorator<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::Decorator<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::Decorator<'ast>,
        crate::other::decorator::FormatDecorator,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::other::decorator::FormatDecorator::default())
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::Decorator<'ast> {
    type Format = FormatOwnedWithRule<
        ast::Decorator<'ast>,
        crate::other::decorator::FormatDecorator,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::decorator::FormatDecorator::default())
    }
}

impl FormattableNode for ast::ElifElseClause<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::ElifElseClause<'ast>, PyFormatContext<'_>>
    for crate::other::elif_else_clause::FormatElifElseClause
{
    #[inline]
    fn fmt(&self, node: &ast::ElifElseClause<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ElifElseClause<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::ElifElseClause<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::ElifElseClause<'ast>,
        crate::other::elif_else_clause::FormatElifElseClause,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::other::elif_else_clause::FormatElifElseClause::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::ElifElseClause<'ast> {
    type Format = FormatOwnedWithRule<
        ast::ElifElseClause<'ast>,
        crate::other::elif_else_clause::FormatElifElseClause,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::other::elif_else_clause::FormatElifElseClause::default(),
        )
    }
}

impl FormattableNode for ast::TypeParams<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::TypeParams<'ast>, PyFormatContext<'_>>
    for crate::type_param::type_params::FormatTypeParams
{
    #[inline]
    fn fmt(&self, node: &ast::TypeParams<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::TypeParams<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::TypeParams<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::TypeParams<'ast>,
        crate::type_param::type_params::FormatTypeParams,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::type_param::type_params::FormatTypeParams::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::TypeParams<'ast> {
    type Format = FormatOwnedWithRule<
        ast::TypeParams<'ast>,
        crate::type_param::type_params::FormatTypeParams,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::type_param::type_params::FormatTypeParams::default(),
        )
    }
}

impl FormattableNode for ast::FString<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::FString<'ast>, PyFormatContext<'_>>
    for crate::other::f_string::FormatFString
{
    #[inline]
    fn fmt(&self, node: &ast::FString<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::FString<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::FString<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::FString<'ast>,
        crate::other::f_string::FormatFString,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::other::f_string::FormatFString::default())
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::FString<'ast> {
    type Format = FormatOwnedWithRule<
        ast::FString<'ast>,
        crate::other::f_string::FormatFString,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::f_string::FormatFString::default())
    }
}

impl FormattableNode for ast::TString<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::TString<'ast>, PyFormatContext<'_>>
    for crate::other::t_string::FormatTString
{
    #[inline]
    fn fmt(&self, node: &ast::TString<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::TString<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::TString<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::TString<'ast>,
        crate::other::t_string::FormatTString,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::other::t_string::FormatTString::default())
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::TString<'ast> {
    type Format = FormatOwnedWithRule<
        ast::TString<'ast>,
        crate::other::t_string::FormatTString,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::t_string::FormatTString::default())
    }
}

impl FormattableNode for ast::StringLiteral<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::StringLiteral<'ast>, PyFormatContext<'_>>
    for crate::other::string_literal::FormatStringLiteral
{
    #[inline]
    fn fmt(&self, node: &ast::StringLiteral<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StringLiteral<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::StringLiteral<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::StringLiteral<'ast>,
        crate::other::string_literal::FormatStringLiteral,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::other::string_literal::FormatStringLiteral::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::StringLiteral<'ast> {
    type Format = FormatOwnedWithRule<
        ast::StringLiteral<'ast>,
        crate::other::string_literal::FormatStringLiteral,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::other::string_literal::FormatStringLiteral::default(),
        )
    }
}

impl FormattableNode for ast::BytesLiteral<'_> {
    fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {
        self.into()
    }
}
impl<'ast> FormatRule<ast::BytesLiteral<'ast>, PyFormatContext<'_>>
    for crate::other::bytes_literal::FormatBytesLiteral
{
    #[inline]
    fn fmt(&self, node: &ast::BytesLiteral<'ast>, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::BytesLiteral<'ast>>::fmt(self, node, f)
    }
}
impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for ast::BytesLiteral<'ast> {
    type Format<'a>
        = FormatRefWithRule<
        'a,
        ast::BytesLiteral<'ast>,
        crate::other::bytes_literal::FormatBytesLiteral,
        PyFormatContext<'context>,
    >
    where
        Self: 'a;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::other::bytes_literal::FormatBytesLiteral::default(),
        )
    }
}
impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for ast::BytesLiteral<'ast> {
    type Format = FormatOwnedWithRule<
        ast::BytesLiteral<'ast>,
        crate::other::bytes_literal::FormatBytesLiteral,
        PyFormatContext<'context>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(
            self,
            crate::other::bytes_literal::FormatBytesLiteral::default(),
        )
    }
}
