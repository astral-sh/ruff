//! This is a generated file. Don't modify it by hand! Run `crates/ruff_python_formatter/generate.py` to re-generate the file.
#![allow(unknown_lints, clippy::default_constructed_unit_structs)]

use crate::context::PyFormatContext;
use crate::{AsFormat, FormatNodeRule, IntoFormat, PyFormatter};
use ruff_formatter::{FormatOwnedWithRule, FormatRefWithRule, FormatResult, FormatRule};
use ruff_python_ast as ast;

impl FormatRule<ast::ModModule, PyFormatContext<'_>>
    for crate::module::mod_module::FormatModModule
{
    #[inline]
    fn fmt(&self, node: &ast::ModModule, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ModModule>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ModModule {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ModModule,
        crate::module::mod_module::FormatModModule,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::module::mod_module::FormatModModule::default())
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ModModule {
    type Format = FormatOwnedWithRule<
        ast::ModModule,
        crate::module::mod_module::FormatModModule,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::module::mod_module::FormatModModule::default())
    }
}

impl FormatRule<ast::ModExpression, PyFormatContext<'_>>
    for crate::module::mod_expression::FormatModExpression
{
    #[inline]
    fn fmt(&self, node: &ast::ModExpression, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ModExpression>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ModExpression {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ModExpression,
        crate::module::mod_expression::FormatModExpression,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::module::mod_expression::FormatModExpression::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ModExpression {
    type Format = FormatOwnedWithRule<
        ast::ModExpression,
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

impl FormatRule<ast::StmtFunctionDef, PyFormatContext<'_>>
    for crate::statement::stmt_function_def::FormatStmtFunctionDef
{
    #[inline]
    fn fmt(&self, node: &ast::StmtFunctionDef, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtFunctionDef>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::StmtFunctionDef {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::StmtFunctionDef,
        crate::statement::stmt_function_def::FormatStmtFunctionDef,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_function_def::FormatStmtFunctionDef::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtFunctionDef {
    type Format = FormatOwnedWithRule<
        ast::StmtFunctionDef,
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

impl FormatRule<ast::StmtClassDef, PyFormatContext<'_>>
    for crate::statement::stmt_class_def::FormatStmtClassDef
{
    #[inline]
    fn fmt(&self, node: &ast::StmtClassDef, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtClassDef>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::StmtClassDef {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::StmtClassDef,
        crate::statement::stmt_class_def::FormatStmtClassDef,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_class_def::FormatStmtClassDef::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtClassDef {
    type Format = FormatOwnedWithRule<
        ast::StmtClassDef,
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

impl FormatRule<ast::StmtReturn, PyFormatContext<'_>>
    for crate::statement::stmt_return::FormatStmtReturn
{
    #[inline]
    fn fmt(&self, node: &ast::StmtReturn, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtReturn>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::StmtReturn {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::StmtReturn,
        crate::statement::stmt_return::FormatStmtReturn,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_return::FormatStmtReturn::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtReturn {
    type Format = FormatOwnedWithRule<
        ast::StmtReturn,
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

impl FormatRule<ast::StmtDelete, PyFormatContext<'_>>
    for crate::statement::stmt_delete::FormatStmtDelete
{
    #[inline]
    fn fmt(&self, node: &ast::StmtDelete, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtDelete>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::StmtDelete {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::StmtDelete,
        crate::statement::stmt_delete::FormatStmtDelete,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_delete::FormatStmtDelete::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtDelete {
    type Format = FormatOwnedWithRule<
        ast::StmtDelete,
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

impl FormatRule<ast::StmtTypeAlias, PyFormatContext<'_>>
    for crate::statement::stmt_type_alias::FormatStmtTypeAlias
{
    #[inline]
    fn fmt(&self, node: &ast::StmtTypeAlias, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtTypeAlias>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::StmtTypeAlias {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::StmtTypeAlias,
        crate::statement::stmt_type_alias::FormatStmtTypeAlias,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_type_alias::FormatStmtTypeAlias::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtTypeAlias {
    type Format = FormatOwnedWithRule<
        ast::StmtTypeAlias,
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

impl FormatRule<ast::StmtAssign, PyFormatContext<'_>>
    for crate::statement::stmt_assign::FormatStmtAssign
{
    #[inline]
    fn fmt(&self, node: &ast::StmtAssign, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtAssign>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::StmtAssign {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::StmtAssign,
        crate::statement::stmt_assign::FormatStmtAssign,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_assign::FormatStmtAssign::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtAssign {
    type Format = FormatOwnedWithRule<
        ast::StmtAssign,
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

impl FormatRule<ast::StmtAugAssign, PyFormatContext<'_>>
    for crate::statement::stmt_aug_assign::FormatStmtAugAssign
{
    #[inline]
    fn fmt(&self, node: &ast::StmtAugAssign, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtAugAssign>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::StmtAugAssign {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::StmtAugAssign,
        crate::statement::stmt_aug_assign::FormatStmtAugAssign,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_aug_assign::FormatStmtAugAssign::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtAugAssign {
    type Format = FormatOwnedWithRule<
        ast::StmtAugAssign,
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

impl FormatRule<ast::StmtAnnAssign, PyFormatContext<'_>>
    for crate::statement::stmt_ann_assign::FormatStmtAnnAssign
{
    #[inline]
    fn fmt(&self, node: &ast::StmtAnnAssign, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtAnnAssign>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::StmtAnnAssign {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::StmtAnnAssign,
        crate::statement::stmt_ann_assign::FormatStmtAnnAssign,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_ann_assign::FormatStmtAnnAssign::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtAnnAssign {
    type Format = FormatOwnedWithRule<
        ast::StmtAnnAssign,
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

impl FormatRule<ast::StmtFor, PyFormatContext<'_>> for crate::statement::stmt_for::FormatStmtFor {
    #[inline]
    fn fmt(&self, node: &ast::StmtFor, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtFor>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::StmtFor {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::StmtFor,
        crate::statement::stmt_for::FormatStmtFor,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::statement::stmt_for::FormatStmtFor::default())
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtFor {
    type Format = FormatOwnedWithRule<
        ast::StmtFor,
        crate::statement::stmt_for::FormatStmtFor,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::statement::stmt_for::FormatStmtFor::default())
    }
}

impl FormatRule<ast::StmtWhile, PyFormatContext<'_>>
    for crate::statement::stmt_while::FormatStmtWhile
{
    #[inline]
    fn fmt(&self, node: &ast::StmtWhile, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtWhile>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::StmtWhile {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::StmtWhile,
        crate::statement::stmt_while::FormatStmtWhile,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_while::FormatStmtWhile::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtWhile {
    type Format = FormatOwnedWithRule<
        ast::StmtWhile,
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

impl FormatRule<ast::StmtIf, PyFormatContext<'_>> for crate::statement::stmt_if::FormatStmtIf {
    #[inline]
    fn fmt(&self, node: &ast::StmtIf, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtIf>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::StmtIf {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::StmtIf,
        crate::statement::stmt_if::FormatStmtIf,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::statement::stmt_if::FormatStmtIf::default())
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtIf {
    type Format = FormatOwnedWithRule<
        ast::StmtIf,
        crate::statement::stmt_if::FormatStmtIf,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::statement::stmt_if::FormatStmtIf::default())
    }
}

impl FormatRule<ast::StmtWith, PyFormatContext<'_>>
    for crate::statement::stmt_with::FormatStmtWith
{
    #[inline]
    fn fmt(&self, node: &ast::StmtWith, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtWith>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::StmtWith {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::StmtWith,
        crate::statement::stmt_with::FormatStmtWith,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::statement::stmt_with::FormatStmtWith::default())
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtWith {
    type Format = FormatOwnedWithRule<
        ast::StmtWith,
        crate::statement::stmt_with::FormatStmtWith,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::statement::stmt_with::FormatStmtWith::default())
    }
}

impl FormatRule<ast::StmtMatch, PyFormatContext<'_>>
    for crate::statement::stmt_match::FormatStmtMatch
{
    #[inline]
    fn fmt(&self, node: &ast::StmtMatch, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtMatch>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::StmtMatch {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::StmtMatch,
        crate::statement::stmt_match::FormatStmtMatch,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_match::FormatStmtMatch::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtMatch {
    type Format = FormatOwnedWithRule<
        ast::StmtMatch,
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

impl FormatRule<ast::StmtRaise, PyFormatContext<'_>>
    for crate::statement::stmt_raise::FormatStmtRaise
{
    #[inline]
    fn fmt(&self, node: &ast::StmtRaise, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtRaise>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::StmtRaise {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::StmtRaise,
        crate::statement::stmt_raise::FormatStmtRaise,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_raise::FormatStmtRaise::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtRaise {
    type Format = FormatOwnedWithRule<
        ast::StmtRaise,
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

impl FormatRule<ast::StmtTry, PyFormatContext<'_>> for crate::statement::stmt_try::FormatStmtTry {
    #[inline]
    fn fmt(&self, node: &ast::StmtTry, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtTry>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::StmtTry {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::StmtTry,
        crate::statement::stmt_try::FormatStmtTry,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::statement::stmt_try::FormatStmtTry::default())
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtTry {
    type Format = FormatOwnedWithRule<
        ast::StmtTry,
        crate::statement::stmt_try::FormatStmtTry,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::statement::stmt_try::FormatStmtTry::default())
    }
}

impl FormatRule<ast::StmtAssert, PyFormatContext<'_>>
    for crate::statement::stmt_assert::FormatStmtAssert
{
    #[inline]
    fn fmt(&self, node: &ast::StmtAssert, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtAssert>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::StmtAssert {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::StmtAssert,
        crate::statement::stmt_assert::FormatStmtAssert,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_assert::FormatStmtAssert::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtAssert {
    type Format = FormatOwnedWithRule<
        ast::StmtAssert,
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

impl FormatRule<ast::StmtImport, PyFormatContext<'_>>
    for crate::statement::stmt_import::FormatStmtImport
{
    #[inline]
    fn fmt(&self, node: &ast::StmtImport, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtImport>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::StmtImport {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::StmtImport,
        crate::statement::stmt_import::FormatStmtImport,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_import::FormatStmtImport::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtImport {
    type Format = FormatOwnedWithRule<
        ast::StmtImport,
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

impl FormatRule<ast::StmtImportFrom, PyFormatContext<'_>>
    for crate::statement::stmt_import_from::FormatStmtImportFrom
{
    #[inline]
    fn fmt(&self, node: &ast::StmtImportFrom, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtImportFrom>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::StmtImportFrom {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::StmtImportFrom,
        crate::statement::stmt_import_from::FormatStmtImportFrom,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_import_from::FormatStmtImportFrom::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtImportFrom {
    type Format = FormatOwnedWithRule<
        ast::StmtImportFrom,
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

impl FormatRule<ast::StmtGlobal, PyFormatContext<'_>>
    for crate::statement::stmt_global::FormatStmtGlobal
{
    #[inline]
    fn fmt(&self, node: &ast::StmtGlobal, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtGlobal>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::StmtGlobal {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::StmtGlobal,
        crate::statement::stmt_global::FormatStmtGlobal,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_global::FormatStmtGlobal::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtGlobal {
    type Format = FormatOwnedWithRule<
        ast::StmtGlobal,
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

impl FormatRule<ast::StmtNonlocal, PyFormatContext<'_>>
    for crate::statement::stmt_nonlocal::FormatStmtNonlocal
{
    #[inline]
    fn fmt(&self, node: &ast::StmtNonlocal, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtNonlocal>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::StmtNonlocal {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::StmtNonlocal,
        crate::statement::stmt_nonlocal::FormatStmtNonlocal,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_nonlocal::FormatStmtNonlocal::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtNonlocal {
    type Format = FormatOwnedWithRule<
        ast::StmtNonlocal,
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

impl FormatRule<ast::StmtExpr, PyFormatContext<'_>>
    for crate::statement::stmt_expr::FormatStmtExpr
{
    #[inline]
    fn fmt(&self, node: &ast::StmtExpr, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtExpr>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::StmtExpr {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::StmtExpr,
        crate::statement::stmt_expr::FormatStmtExpr,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::statement::stmt_expr::FormatStmtExpr::default())
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtExpr {
    type Format = FormatOwnedWithRule<
        ast::StmtExpr,
        crate::statement::stmt_expr::FormatStmtExpr,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::statement::stmt_expr::FormatStmtExpr::default())
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
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::StmtPass {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::StmtPass,
        crate::statement::stmt_pass::FormatStmtPass,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::statement::stmt_pass::FormatStmtPass::default())
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtPass {
    type Format = FormatOwnedWithRule<
        ast::StmtPass,
        crate::statement::stmt_pass::FormatStmtPass,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::statement::stmt_pass::FormatStmtPass::default())
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
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::StmtBreak {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::StmtBreak,
        crate::statement::stmt_break::FormatStmtBreak,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_break::FormatStmtBreak::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtBreak {
    type Format = FormatOwnedWithRule<
        ast::StmtBreak,
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

impl FormatRule<ast::StmtContinue, PyFormatContext<'_>>
    for crate::statement::stmt_continue::FormatStmtContinue
{
    #[inline]
    fn fmt(&self, node: &ast::StmtContinue, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtContinue>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::StmtContinue {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::StmtContinue,
        crate::statement::stmt_continue::FormatStmtContinue,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_continue::FormatStmtContinue::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtContinue {
    type Format = FormatOwnedWithRule<
        ast::StmtContinue,
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

impl FormatRule<ast::StmtIpyEscapeCommand, PyFormatContext<'_>>
    for crate::statement::stmt_ipy_escape_command::FormatStmtIpyEscapeCommand
{
    #[inline]
    fn fmt(&self, node: &ast::StmtIpyEscapeCommand, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StmtIpyEscapeCommand>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::StmtIpyEscapeCommand {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::StmtIpyEscapeCommand,
        crate::statement::stmt_ipy_escape_command::FormatStmtIpyEscapeCommand,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::statement::stmt_ipy_escape_command::FormatStmtIpyEscapeCommand::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StmtIpyEscapeCommand {
    type Format = FormatOwnedWithRule<
        ast::StmtIpyEscapeCommand,
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

impl FormatRule<ast::ExprBoolOp, PyFormatContext<'_>>
    for crate::expression::expr_bool_op::FormatExprBoolOp
{
    #[inline]
    fn fmt(&self, node: &ast::ExprBoolOp, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprBoolOp>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprBoolOp {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprBoolOp,
        crate::expression::expr_bool_op::FormatExprBoolOp,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_bool_op::FormatExprBoolOp::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprBoolOp {
    type Format = FormatOwnedWithRule<
        ast::ExprBoolOp,
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

impl FormatRule<ast::ExprNamed, PyFormatContext<'_>>
    for crate::expression::expr_named::FormatExprNamed
{
    #[inline]
    fn fmt(&self, node: &ast::ExprNamed, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprNamed>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprNamed {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprNamed,
        crate::expression::expr_named::FormatExprNamed,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_named::FormatExprNamed::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprNamed {
    type Format = FormatOwnedWithRule<
        ast::ExprNamed,
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

impl FormatRule<ast::ExprBinOp, PyFormatContext<'_>>
    for crate::expression::expr_bin_op::FormatExprBinOp
{
    #[inline]
    fn fmt(&self, node: &ast::ExprBinOp, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprBinOp>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprBinOp {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprBinOp,
        crate::expression::expr_bin_op::FormatExprBinOp,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_bin_op::FormatExprBinOp::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprBinOp {
    type Format = FormatOwnedWithRule<
        ast::ExprBinOp,
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

impl FormatRule<ast::ExprUnaryOp, PyFormatContext<'_>>
    for crate::expression::expr_unary_op::FormatExprUnaryOp
{
    #[inline]
    fn fmt(&self, node: &ast::ExprUnaryOp, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprUnaryOp>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprUnaryOp {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprUnaryOp,
        crate::expression::expr_unary_op::FormatExprUnaryOp,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_unary_op::FormatExprUnaryOp::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprUnaryOp {
    type Format = FormatOwnedWithRule<
        ast::ExprUnaryOp,
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

impl FormatRule<ast::ExprLambda, PyFormatContext<'_>>
    for crate::expression::expr_lambda::FormatExprLambda
{
    #[inline]
    fn fmt(&self, node: &ast::ExprLambda, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprLambda>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprLambda {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprLambda,
        crate::expression::expr_lambda::FormatExprLambda,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_lambda::FormatExprLambda::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprLambda {
    type Format = FormatOwnedWithRule<
        ast::ExprLambda,
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

impl FormatRule<ast::ExprIf, PyFormatContext<'_>> for crate::expression::expr_if::FormatExprIf {
    #[inline]
    fn fmt(&self, node: &ast::ExprIf, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprIf>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprIf {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprIf,
        crate::expression::expr_if::FormatExprIf,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::expression::expr_if::FormatExprIf::default())
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprIf {
    type Format = FormatOwnedWithRule<
        ast::ExprIf,
        crate::expression::expr_if::FormatExprIf,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::expression::expr_if::FormatExprIf::default())
    }
}

impl FormatRule<ast::ExprDict, PyFormatContext<'_>>
    for crate::expression::expr_dict::FormatExprDict
{
    #[inline]
    fn fmt(&self, node: &ast::ExprDict, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprDict>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprDict {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprDict,
        crate::expression::expr_dict::FormatExprDict,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_dict::FormatExprDict::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprDict {
    type Format = FormatOwnedWithRule<
        ast::ExprDict,
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

impl FormatRule<ast::ExprSet, PyFormatContext<'_>> for crate::expression::expr_set::FormatExprSet {
    #[inline]
    fn fmt(&self, node: &ast::ExprSet, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprSet>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprSet {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprSet,
        crate::expression::expr_set::FormatExprSet,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::expression::expr_set::FormatExprSet::default())
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprSet {
    type Format = FormatOwnedWithRule<
        ast::ExprSet,
        crate::expression::expr_set::FormatExprSet,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::expression::expr_set::FormatExprSet::default())
    }
}

impl FormatRule<ast::ExprListComp, PyFormatContext<'_>>
    for crate::expression::expr_list_comp::FormatExprListComp
{
    #[inline]
    fn fmt(&self, node: &ast::ExprListComp, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprListComp>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprListComp {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprListComp,
        crate::expression::expr_list_comp::FormatExprListComp,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_list_comp::FormatExprListComp::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprListComp {
    type Format = FormatOwnedWithRule<
        ast::ExprListComp,
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

impl FormatRule<ast::ExprSetComp, PyFormatContext<'_>>
    for crate::expression::expr_set_comp::FormatExprSetComp
{
    #[inline]
    fn fmt(&self, node: &ast::ExprSetComp, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprSetComp>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprSetComp {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprSetComp,
        crate::expression::expr_set_comp::FormatExprSetComp,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_set_comp::FormatExprSetComp::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprSetComp {
    type Format = FormatOwnedWithRule<
        ast::ExprSetComp,
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

impl FormatRule<ast::ExprDictComp, PyFormatContext<'_>>
    for crate::expression::expr_dict_comp::FormatExprDictComp
{
    #[inline]
    fn fmt(&self, node: &ast::ExprDictComp, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprDictComp>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprDictComp {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprDictComp,
        crate::expression::expr_dict_comp::FormatExprDictComp,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_dict_comp::FormatExprDictComp::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprDictComp {
    type Format = FormatOwnedWithRule<
        ast::ExprDictComp,
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

impl FormatRule<ast::ExprGenerator, PyFormatContext<'_>>
    for crate::expression::expr_generator::FormatExprGenerator
{
    #[inline]
    fn fmt(&self, node: &ast::ExprGenerator, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprGenerator>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprGenerator {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprGenerator,
        crate::expression::expr_generator::FormatExprGenerator,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_generator::FormatExprGenerator::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprGenerator {
    type Format = FormatOwnedWithRule<
        ast::ExprGenerator,
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

impl FormatRule<ast::ExprAwait, PyFormatContext<'_>>
    for crate::expression::expr_await::FormatExprAwait
{
    #[inline]
    fn fmt(&self, node: &ast::ExprAwait, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprAwait>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprAwait {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprAwait,
        crate::expression::expr_await::FormatExprAwait,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_await::FormatExprAwait::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprAwait {
    type Format = FormatOwnedWithRule<
        ast::ExprAwait,
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

impl FormatRule<ast::ExprYield, PyFormatContext<'_>>
    for crate::expression::expr_yield::FormatExprYield
{
    #[inline]
    fn fmt(&self, node: &ast::ExprYield, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprYield>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprYield {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprYield,
        crate::expression::expr_yield::FormatExprYield,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_yield::FormatExprYield::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprYield {
    type Format = FormatOwnedWithRule<
        ast::ExprYield,
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

impl FormatRule<ast::ExprYieldFrom, PyFormatContext<'_>>
    for crate::expression::expr_yield_from::FormatExprYieldFrom
{
    #[inline]
    fn fmt(&self, node: &ast::ExprYieldFrom, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprYieldFrom>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprYieldFrom {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprYieldFrom,
        crate::expression::expr_yield_from::FormatExprYieldFrom,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_yield_from::FormatExprYieldFrom::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprYieldFrom {
    type Format = FormatOwnedWithRule<
        ast::ExprYieldFrom,
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

impl FormatRule<ast::ExprCompare, PyFormatContext<'_>>
    for crate::expression::expr_compare::FormatExprCompare
{
    #[inline]
    fn fmt(&self, node: &ast::ExprCompare, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprCompare>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprCompare {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprCompare,
        crate::expression::expr_compare::FormatExprCompare,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_compare::FormatExprCompare::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprCompare {
    type Format = FormatOwnedWithRule<
        ast::ExprCompare,
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

impl FormatRule<ast::ExprCall, PyFormatContext<'_>>
    for crate::expression::expr_call::FormatExprCall
{
    #[inline]
    fn fmt(&self, node: &ast::ExprCall, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprCall>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprCall {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprCall,
        crate::expression::expr_call::FormatExprCall,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_call::FormatExprCall::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprCall {
    type Format = FormatOwnedWithRule<
        ast::ExprCall,
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

impl FormatRule<ast::ExprFString, PyFormatContext<'_>>
    for crate::expression::expr_f_string::FormatExprFString
{
    #[inline]
    fn fmt(&self, node: &ast::ExprFString, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprFString>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprFString {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprFString,
        crate::expression::expr_f_string::FormatExprFString,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_f_string::FormatExprFString::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprFString {
    type Format = FormatOwnedWithRule<
        ast::ExprFString,
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

impl FormatRule<ast::ExprStringLiteral, PyFormatContext<'_>>
    for crate::expression::expr_string_literal::FormatExprStringLiteral
{
    #[inline]
    fn fmt(&self, node: &ast::ExprStringLiteral, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprStringLiteral>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprStringLiteral {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprStringLiteral,
        crate::expression::expr_string_literal::FormatExprStringLiteral,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_string_literal::FormatExprStringLiteral::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprStringLiteral {
    type Format = FormatOwnedWithRule<
        ast::ExprStringLiteral,
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

impl FormatRule<ast::ExprBytesLiteral, PyFormatContext<'_>>
    for crate::expression::expr_bytes_literal::FormatExprBytesLiteral
{
    #[inline]
    fn fmt(&self, node: &ast::ExprBytesLiteral, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprBytesLiteral>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprBytesLiteral {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprBytesLiteral,
        crate::expression::expr_bytes_literal::FormatExprBytesLiteral,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_bytes_literal::FormatExprBytesLiteral::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprBytesLiteral {
    type Format = FormatOwnedWithRule<
        ast::ExprBytesLiteral,
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

impl FormatRule<ast::ExprNumberLiteral, PyFormatContext<'_>>
    for crate::expression::expr_number_literal::FormatExprNumberLiteral
{
    #[inline]
    fn fmt(&self, node: &ast::ExprNumberLiteral, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprNumberLiteral>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprNumberLiteral {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprNumberLiteral,
        crate::expression::expr_number_literal::FormatExprNumberLiteral,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_number_literal::FormatExprNumberLiteral::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprNumberLiteral {
    type Format = FormatOwnedWithRule<
        ast::ExprNumberLiteral,
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

impl FormatRule<ast::ExprBooleanLiteral, PyFormatContext<'_>>
    for crate::expression::expr_boolean_literal::FormatExprBooleanLiteral
{
    #[inline]
    fn fmt(&self, node: &ast::ExprBooleanLiteral, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprBooleanLiteral>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprBooleanLiteral {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprBooleanLiteral,
        crate::expression::expr_boolean_literal::FormatExprBooleanLiteral,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_boolean_literal::FormatExprBooleanLiteral::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprBooleanLiteral {
    type Format = FormatOwnedWithRule<
        ast::ExprBooleanLiteral,
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

impl FormatRule<ast::ExprNoneLiteral, PyFormatContext<'_>>
    for crate::expression::expr_none_literal::FormatExprNoneLiteral
{
    #[inline]
    fn fmt(&self, node: &ast::ExprNoneLiteral, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprNoneLiteral>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprNoneLiteral {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprNoneLiteral,
        crate::expression::expr_none_literal::FormatExprNoneLiteral,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_none_literal::FormatExprNoneLiteral::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprNoneLiteral {
    type Format = FormatOwnedWithRule<
        ast::ExprNoneLiteral,
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

impl FormatRule<ast::ExprEllipsisLiteral, PyFormatContext<'_>>
    for crate::expression::expr_ellipsis_literal::FormatExprEllipsisLiteral
{
    #[inline]
    fn fmt(&self, node: &ast::ExprEllipsisLiteral, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprEllipsisLiteral>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprEllipsisLiteral {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprEllipsisLiteral,
        crate::expression::expr_ellipsis_literal::FormatExprEllipsisLiteral,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_ellipsis_literal::FormatExprEllipsisLiteral::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprEllipsisLiteral {
    type Format = FormatOwnedWithRule<
        ast::ExprEllipsisLiteral,
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

impl FormatRule<ast::ExprAttribute, PyFormatContext<'_>>
    for crate::expression::expr_attribute::FormatExprAttribute
{
    #[inline]
    fn fmt(&self, node: &ast::ExprAttribute, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprAttribute>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprAttribute {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprAttribute,
        crate::expression::expr_attribute::FormatExprAttribute,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_attribute::FormatExprAttribute::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprAttribute {
    type Format = FormatOwnedWithRule<
        ast::ExprAttribute,
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

impl FormatRule<ast::ExprSubscript, PyFormatContext<'_>>
    for crate::expression::expr_subscript::FormatExprSubscript
{
    #[inline]
    fn fmt(&self, node: &ast::ExprSubscript, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprSubscript>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprSubscript {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprSubscript,
        crate::expression::expr_subscript::FormatExprSubscript,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_subscript::FormatExprSubscript::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprSubscript {
    type Format = FormatOwnedWithRule<
        ast::ExprSubscript,
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

impl FormatRule<ast::ExprStarred, PyFormatContext<'_>>
    for crate::expression::expr_starred::FormatExprStarred
{
    #[inline]
    fn fmt(&self, node: &ast::ExprStarred, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprStarred>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprStarred {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprStarred,
        crate::expression::expr_starred::FormatExprStarred,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_starred::FormatExprStarred::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprStarred {
    type Format = FormatOwnedWithRule<
        ast::ExprStarred,
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

impl FormatRule<ast::ExprName, PyFormatContext<'_>>
    for crate::expression::expr_name::FormatExprName
{
    #[inline]
    fn fmt(&self, node: &ast::ExprName, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprName>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprName {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprName,
        crate::expression::expr_name::FormatExprName,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_name::FormatExprName::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprName {
    type Format = FormatOwnedWithRule<
        ast::ExprName,
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

impl FormatRule<ast::ExprList, PyFormatContext<'_>>
    for crate::expression::expr_list::FormatExprList
{
    #[inline]
    fn fmt(&self, node: &ast::ExprList, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprList>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprList {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprList,
        crate::expression::expr_list::FormatExprList,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_list::FormatExprList::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprList {
    type Format = FormatOwnedWithRule<
        ast::ExprList,
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

impl FormatRule<ast::ExprTuple, PyFormatContext<'_>>
    for crate::expression::expr_tuple::FormatExprTuple
{
    #[inline]
    fn fmt(&self, node: &ast::ExprTuple, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprTuple>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprTuple {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprTuple,
        crate::expression::expr_tuple::FormatExprTuple,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_tuple::FormatExprTuple::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprTuple {
    type Format = FormatOwnedWithRule<
        ast::ExprTuple,
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

impl FormatRule<ast::ExprSlice, PyFormatContext<'_>>
    for crate::expression::expr_slice::FormatExprSlice
{
    #[inline]
    fn fmt(&self, node: &ast::ExprSlice, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprSlice>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprSlice {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprSlice,
        crate::expression::expr_slice::FormatExprSlice,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_slice::FormatExprSlice::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprSlice {
    type Format = FormatOwnedWithRule<
        ast::ExprSlice,
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

impl FormatRule<ast::ExprIpyEscapeCommand, PyFormatContext<'_>>
    for crate::expression::expr_ipy_escape_command::FormatExprIpyEscapeCommand
{
    #[inline]
    fn fmt(&self, node: &ast::ExprIpyEscapeCommand, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExprIpyEscapeCommand>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExprIpyEscapeCommand {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExprIpyEscapeCommand,
        crate::expression::expr_ipy_escape_command::FormatExprIpyEscapeCommand,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::expression::expr_ipy_escape_command::FormatExprIpyEscapeCommand::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExprIpyEscapeCommand {
    type Format = FormatOwnedWithRule<
        ast::ExprIpyEscapeCommand,
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

impl FormatRule<ast::ExceptHandlerExceptHandler, PyFormatContext<'_>>
    for crate::other::except_handler_except_handler::FormatExceptHandlerExceptHandler
{
    #[inline]
    fn fmt(&self, node: &ast::ExceptHandlerExceptHandler, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ExceptHandlerExceptHandler>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ExceptHandlerExceptHandler {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ExceptHandlerExceptHandler,
        crate::other::except_handler_except_handler::FormatExceptHandlerExceptHandler,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::other::except_handler_except_handler::FormatExceptHandlerExceptHandler::default(
            ),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ExceptHandlerExceptHandler {
    type Format = FormatOwnedWithRule<
        ast::ExceptHandlerExceptHandler,
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

impl FormatRule<ast::PatternMatchValue, PyFormatContext<'_>>
    for crate::pattern::pattern_match_value::FormatPatternMatchValue
{
    #[inline]
    fn fmt(&self, node: &ast::PatternMatchValue, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::PatternMatchValue>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::PatternMatchValue {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::PatternMatchValue,
        crate::pattern::pattern_match_value::FormatPatternMatchValue,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::pattern::pattern_match_value::FormatPatternMatchValue::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::PatternMatchValue {
    type Format = FormatOwnedWithRule<
        ast::PatternMatchValue,
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

impl FormatRule<ast::PatternMatchSingleton, PyFormatContext<'_>>
    for crate::pattern::pattern_match_singleton::FormatPatternMatchSingleton
{
    #[inline]
    fn fmt(&self, node: &ast::PatternMatchSingleton, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::PatternMatchSingleton>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::PatternMatchSingleton {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::PatternMatchSingleton,
        crate::pattern::pattern_match_singleton::FormatPatternMatchSingleton,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::pattern::pattern_match_singleton::FormatPatternMatchSingleton::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::PatternMatchSingleton {
    type Format = FormatOwnedWithRule<
        ast::PatternMatchSingleton,
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

impl FormatRule<ast::PatternMatchSequence, PyFormatContext<'_>>
    for crate::pattern::pattern_match_sequence::FormatPatternMatchSequence
{
    #[inline]
    fn fmt(&self, node: &ast::PatternMatchSequence, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::PatternMatchSequence>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::PatternMatchSequence {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::PatternMatchSequence,
        crate::pattern::pattern_match_sequence::FormatPatternMatchSequence,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::pattern::pattern_match_sequence::FormatPatternMatchSequence::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::PatternMatchSequence {
    type Format = FormatOwnedWithRule<
        ast::PatternMatchSequence,
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

impl FormatRule<ast::PatternMatchMapping, PyFormatContext<'_>>
    for crate::pattern::pattern_match_mapping::FormatPatternMatchMapping
{
    #[inline]
    fn fmt(&self, node: &ast::PatternMatchMapping, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::PatternMatchMapping>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::PatternMatchMapping {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::PatternMatchMapping,
        crate::pattern::pattern_match_mapping::FormatPatternMatchMapping,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::pattern::pattern_match_mapping::FormatPatternMatchMapping::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::PatternMatchMapping {
    type Format = FormatOwnedWithRule<
        ast::PatternMatchMapping,
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

impl FormatRule<ast::PatternMatchClass, PyFormatContext<'_>>
    for crate::pattern::pattern_match_class::FormatPatternMatchClass
{
    #[inline]
    fn fmt(&self, node: &ast::PatternMatchClass, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::PatternMatchClass>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::PatternMatchClass {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::PatternMatchClass,
        crate::pattern::pattern_match_class::FormatPatternMatchClass,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::pattern::pattern_match_class::FormatPatternMatchClass::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::PatternMatchClass {
    type Format = FormatOwnedWithRule<
        ast::PatternMatchClass,
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

impl FormatRule<ast::PatternMatchStar, PyFormatContext<'_>>
    for crate::pattern::pattern_match_star::FormatPatternMatchStar
{
    #[inline]
    fn fmt(&self, node: &ast::PatternMatchStar, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::PatternMatchStar>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::PatternMatchStar {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::PatternMatchStar,
        crate::pattern::pattern_match_star::FormatPatternMatchStar,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::pattern::pattern_match_star::FormatPatternMatchStar::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::PatternMatchStar {
    type Format = FormatOwnedWithRule<
        ast::PatternMatchStar,
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

impl FormatRule<ast::PatternMatchAs, PyFormatContext<'_>>
    for crate::pattern::pattern_match_as::FormatPatternMatchAs
{
    #[inline]
    fn fmt(&self, node: &ast::PatternMatchAs, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::PatternMatchAs>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::PatternMatchAs {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::PatternMatchAs,
        crate::pattern::pattern_match_as::FormatPatternMatchAs,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::pattern::pattern_match_as::FormatPatternMatchAs::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::PatternMatchAs {
    type Format = FormatOwnedWithRule<
        ast::PatternMatchAs,
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

impl FormatRule<ast::PatternMatchOr, PyFormatContext<'_>>
    for crate::pattern::pattern_match_or::FormatPatternMatchOr
{
    #[inline]
    fn fmt(&self, node: &ast::PatternMatchOr, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::PatternMatchOr>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::PatternMatchOr {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::PatternMatchOr,
        crate::pattern::pattern_match_or::FormatPatternMatchOr,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::pattern::pattern_match_or::FormatPatternMatchOr::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::PatternMatchOr {
    type Format = FormatOwnedWithRule<
        ast::PatternMatchOr,
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

impl FormatRule<ast::TypeParamTypeVar, PyFormatContext<'_>>
    for crate::type_param::type_param_type_var::FormatTypeParamTypeVar
{
    #[inline]
    fn fmt(&self, node: &ast::TypeParamTypeVar, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::TypeParamTypeVar>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::TypeParamTypeVar {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::TypeParamTypeVar,
        crate::type_param::type_param_type_var::FormatTypeParamTypeVar,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::type_param::type_param_type_var::FormatTypeParamTypeVar::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::TypeParamTypeVar {
    type Format = FormatOwnedWithRule<
        ast::TypeParamTypeVar,
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

impl FormatRule<ast::TypeParamTypeVarTuple, PyFormatContext<'_>>
    for crate::type_param::type_param_type_var_tuple::FormatTypeParamTypeVarTuple
{
    #[inline]
    fn fmt(&self, node: &ast::TypeParamTypeVarTuple, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::TypeParamTypeVarTuple>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::TypeParamTypeVarTuple {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::TypeParamTypeVarTuple,
        crate::type_param::type_param_type_var_tuple::FormatTypeParamTypeVarTuple,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::type_param::type_param_type_var_tuple::FormatTypeParamTypeVarTuple::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::TypeParamTypeVarTuple {
    type Format = FormatOwnedWithRule<
        ast::TypeParamTypeVarTuple,
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

impl FormatRule<ast::TypeParamParamSpec, PyFormatContext<'_>>
    for crate::type_param::type_param_param_spec::FormatTypeParamParamSpec
{
    #[inline]
    fn fmt(&self, node: &ast::TypeParamParamSpec, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::TypeParamParamSpec>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::TypeParamParamSpec {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::TypeParamParamSpec,
        crate::type_param::type_param_param_spec::FormatTypeParamParamSpec,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::type_param::type_param_param_spec::FormatTypeParamParamSpec::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::TypeParamParamSpec {
    type Format = FormatOwnedWithRule<
        ast::TypeParamParamSpec,
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

impl FormatRule<ast::PatternArguments, PyFormatContext<'_>>
    for crate::pattern::pattern_arguments::FormatPatternArguments
{
    #[inline]
    fn fmt(&self, node: &ast::PatternArguments, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::PatternArguments>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::PatternArguments {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::PatternArguments,
        crate::pattern::pattern_arguments::FormatPatternArguments,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::pattern::pattern_arguments::FormatPatternArguments::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::PatternArguments {
    type Format = FormatOwnedWithRule<
        ast::PatternArguments,
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

impl FormatRule<ast::PatternKeyword, PyFormatContext<'_>>
    for crate::pattern::pattern_keyword::FormatPatternKeyword
{
    #[inline]
    fn fmt(&self, node: &ast::PatternKeyword, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::PatternKeyword>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::PatternKeyword {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::PatternKeyword,
        crate::pattern::pattern_keyword::FormatPatternKeyword,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::pattern::pattern_keyword::FormatPatternKeyword::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::PatternKeyword {
    type Format = FormatOwnedWithRule<
        ast::PatternKeyword,
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

impl FormatRule<ast::Comprehension, PyFormatContext<'_>>
    for crate::other::comprehension::FormatComprehension
{
    #[inline]
    fn fmt(&self, node: &ast::Comprehension, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::Comprehension>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::Comprehension {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::Comprehension,
        crate::other::comprehension::FormatComprehension,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::other::comprehension::FormatComprehension::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::Comprehension {
    type Format = FormatOwnedWithRule<
        ast::Comprehension,
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

impl FormatRule<ast::Arguments, PyFormatContext<'_>> for crate::other::arguments::FormatArguments {
    #[inline]
    fn fmt(&self, node: &ast::Arguments, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::Arguments>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::Arguments {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::Arguments,
        crate::other::arguments::FormatArguments,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::other::arguments::FormatArguments::default())
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::Arguments {
    type Format = FormatOwnedWithRule<
        ast::Arguments,
        crate::other::arguments::FormatArguments,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::arguments::FormatArguments::default())
    }
}

impl FormatRule<ast::Parameters, PyFormatContext<'_>>
    for crate::other::parameters::FormatParameters
{
    #[inline]
    fn fmt(&self, node: &ast::Parameters, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::Parameters>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::Parameters {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::Parameters,
        crate::other::parameters::FormatParameters,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::other::parameters::FormatParameters::default())
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::Parameters {
    type Format = FormatOwnedWithRule<
        ast::Parameters,
        crate::other::parameters::FormatParameters,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::parameters::FormatParameters::default())
    }
}

impl FormatRule<ast::Parameter, PyFormatContext<'_>> for crate::other::parameter::FormatParameter {
    #[inline]
    fn fmt(&self, node: &ast::Parameter, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::Parameter>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::Parameter {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::Parameter,
        crate::other::parameter::FormatParameter,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::other::parameter::FormatParameter::default())
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::Parameter {
    type Format = FormatOwnedWithRule<
        ast::Parameter,
        crate::other::parameter::FormatParameter,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::parameter::FormatParameter::default())
    }
}

impl FormatRule<ast::ParameterWithDefault, PyFormatContext<'_>>
    for crate::other::parameter_with_default::FormatParameterWithDefault
{
    #[inline]
    fn fmt(&self, node: &ast::ParameterWithDefault, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ParameterWithDefault>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ParameterWithDefault {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ParameterWithDefault,
        crate::other::parameter_with_default::FormatParameterWithDefault,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::other::parameter_with_default::FormatParameterWithDefault::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ParameterWithDefault {
    type Format = FormatOwnedWithRule<
        ast::ParameterWithDefault,
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

impl FormatRule<ast::Keyword, PyFormatContext<'_>> for crate::other::keyword::FormatKeyword {
    #[inline]
    fn fmt(&self, node: &ast::Keyword, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::Keyword>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::Keyword {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::Keyword,
        crate::other::keyword::FormatKeyword,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::other::keyword::FormatKeyword::default())
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::Keyword {
    type Format = FormatOwnedWithRule<
        ast::Keyword,
        crate::other::keyword::FormatKeyword,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::keyword::FormatKeyword::default())
    }
}

impl FormatRule<ast::Alias, PyFormatContext<'_>> for crate::other::alias::FormatAlias {
    #[inline]
    fn fmt(&self, node: &ast::Alias, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::Alias>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::Alias {
    type Format<'a> =
        FormatRefWithRule<'a, ast::Alias, crate::other::alias::FormatAlias, PyFormatContext<'ast>>;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::other::alias::FormatAlias::default())
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::Alias {
    type Format =
        FormatOwnedWithRule<ast::Alias, crate::other::alias::FormatAlias, PyFormatContext<'ast>>;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::alias::FormatAlias::default())
    }
}

impl FormatRule<ast::WithItem, PyFormatContext<'_>> for crate::other::with_item::FormatWithItem {
    #[inline]
    fn fmt(&self, node: &ast::WithItem, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::WithItem>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::WithItem {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::WithItem,
        crate::other::with_item::FormatWithItem,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::other::with_item::FormatWithItem::default())
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::WithItem {
    type Format = FormatOwnedWithRule<
        ast::WithItem,
        crate::other::with_item::FormatWithItem,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::with_item::FormatWithItem::default())
    }
}

impl FormatRule<ast::MatchCase, PyFormatContext<'_>> for crate::other::match_case::FormatMatchCase {
    #[inline]
    fn fmt(&self, node: &ast::MatchCase, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::MatchCase>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::MatchCase {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::MatchCase,
        crate::other::match_case::FormatMatchCase,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::other::match_case::FormatMatchCase::default())
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::MatchCase {
    type Format = FormatOwnedWithRule<
        ast::MatchCase,
        crate::other::match_case::FormatMatchCase,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::match_case::FormatMatchCase::default())
    }
}

impl FormatRule<ast::Decorator, PyFormatContext<'_>> for crate::other::decorator::FormatDecorator {
    #[inline]
    fn fmt(&self, node: &ast::Decorator, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::Decorator>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::Decorator {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::Decorator,
        crate::other::decorator::FormatDecorator,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::other::decorator::FormatDecorator::default())
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::Decorator {
    type Format = FormatOwnedWithRule<
        ast::Decorator,
        crate::other::decorator::FormatDecorator,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::decorator::FormatDecorator::default())
    }
}

impl FormatRule<ast::ElifElseClause, PyFormatContext<'_>>
    for crate::other::elif_else_clause::FormatElifElseClause
{
    #[inline]
    fn fmt(&self, node: &ast::ElifElseClause, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::ElifElseClause>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::ElifElseClause {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::ElifElseClause,
        crate::other::elif_else_clause::FormatElifElseClause,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::other::elif_else_clause::FormatElifElseClause::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::ElifElseClause {
    type Format = FormatOwnedWithRule<
        ast::ElifElseClause,
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

impl FormatRule<ast::TypeParams, PyFormatContext<'_>>
    for crate::type_param::type_params::FormatTypeParams
{
    #[inline]
    fn fmt(&self, node: &ast::TypeParams, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::TypeParams>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::TypeParams {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::TypeParams,
        crate::type_param::type_params::FormatTypeParams,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::type_param::type_params::FormatTypeParams::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::TypeParams {
    type Format = FormatOwnedWithRule<
        ast::TypeParams,
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

impl FormatRule<ast::FString, PyFormatContext<'_>> for crate::other::f_string::FormatFString {
    #[inline]
    fn fmt(&self, node: &ast::FString, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::FString>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::FString {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::FString,
        crate::other::f_string::FormatFString,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, crate::other::f_string::FormatFString::default())
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::FString {
    type Format = FormatOwnedWithRule<
        ast::FString,
        crate::other::f_string::FormatFString,
        PyFormatContext<'ast>,
    >;
    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, crate::other::f_string::FormatFString::default())
    }
}

impl FormatRule<ast::StringLiteral, PyFormatContext<'_>>
    for crate::other::string_literal::FormatStringLiteral
{
    #[inline]
    fn fmt(&self, node: &ast::StringLiteral, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::StringLiteral>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::StringLiteral {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::StringLiteral,
        crate::other::string_literal::FormatStringLiteral,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::other::string_literal::FormatStringLiteral::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::StringLiteral {
    type Format = FormatOwnedWithRule<
        ast::StringLiteral,
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

impl FormatRule<ast::BytesLiteral, PyFormatContext<'_>>
    for crate::other::bytes_literal::FormatBytesLiteral
{
    #[inline]
    fn fmt(&self, node: &ast::BytesLiteral, f: &mut PyFormatter) -> FormatResult<()> {
        FormatNodeRule::<ast::BytesLiteral>::fmt(self, node, f)
    }
}
impl<'ast> AsFormat<PyFormatContext<'ast>> for ast::BytesLiteral {
    type Format<'a> = FormatRefWithRule<
        'a,
        ast::BytesLiteral,
        crate::other::bytes_literal::FormatBytesLiteral,
        PyFormatContext<'ast>,
    >;
    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(
            self,
            crate::other::bytes_literal::FormatBytesLiteral::default(),
        )
    }
}
impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::BytesLiteral {
    type Format = FormatOwnedWithRule<
        ast::BytesLiteral,
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
