// This is a generated file. Don't modify it by hand!
// Run `crates/ruff_python_ast/generate.py` to re-generate the file.

/// See also [mod](https://docs.python.org/3/library/ast.html#ast.mod)
#[derive(Clone, Debug, PartialEq, is_macro::Is)]
pub enum Mod {
    Module(crate::ModModule),
    Expression(crate::ModExpression),
}

/// See also [stmt](https://docs.python.org/3/library/ast.html#ast.stmt)
#[derive(Clone, Debug, PartialEq, is_macro::Is)]
pub enum Stmt {
    #[is(name = "function_def_stmt")]
    FunctionDef(crate::StmtFunctionDef),
    #[is(name = "class_def_stmt")]
    ClassDef(crate::StmtClassDef),
    #[is(name = "return_stmt")]
    Return(crate::StmtReturn),
    #[is(name = "delete_stmt")]
    Delete(crate::StmtDelete),
    #[is(name = "type_alias_stmt")]
    TypeAlias(crate::StmtTypeAlias),
    #[is(name = "assign_stmt")]
    Assign(crate::StmtAssign),
    #[is(name = "aug_assign_stmt")]
    AugAssign(crate::StmtAugAssign),
    #[is(name = "ann_assign_stmt")]
    AnnAssign(crate::StmtAnnAssign),
    #[is(name = "for_stmt")]
    For(crate::StmtFor),
    #[is(name = "while_stmt")]
    While(crate::StmtWhile),
    #[is(name = "if_stmt")]
    If(crate::StmtIf),
    #[is(name = "with_stmt")]
    With(crate::StmtWith),
    #[is(name = "match_stmt")]
    Match(crate::StmtMatch),
    #[is(name = "raise_stmt")]
    Raise(crate::StmtRaise),
    #[is(name = "try_stmt")]
    Try(crate::StmtTry),
    #[is(name = "assert_stmt")]
    Assert(crate::StmtAssert),
    #[is(name = "import_stmt")]
    Import(crate::StmtImport),
    #[is(name = "import_from_stmt")]
    ImportFrom(crate::StmtImportFrom),
    #[is(name = "global_stmt")]
    Global(crate::StmtGlobal),
    #[is(name = "nonlocal_stmt")]
    Nonlocal(crate::StmtNonlocal),
    #[is(name = "expr_stmt")]
    Expr(crate::StmtExpr),
    #[is(name = "pass_stmt")]
    Pass(crate::StmtPass),
    #[is(name = "break_stmt")]
    Break(crate::StmtBreak),
    #[is(name = "continue_stmt")]
    Continue(crate::StmtContinue),
    #[is(name = "ipy_escape_command_stmt")]
    IpyEscapeCommand(crate::StmtIpyEscapeCommand),
}

/// See also [expr](https://docs.python.org/3/library/ast.html#ast.expr)
#[derive(Clone, Debug, PartialEq, is_macro::Is)]
pub enum Expr {
    #[is(name = "bool_op_expr")]
    BoolOp(crate::ExprBoolOp),
    #[is(name = "named_expr")]
    Named(crate::ExprNamed),
    #[is(name = "bin_op_expr")]
    BinOp(crate::ExprBinOp),
    #[is(name = "unary_op_expr")]
    UnaryOp(crate::ExprUnaryOp),
    #[is(name = "lambda_expr")]
    Lambda(crate::ExprLambda),
    #[is(name = "if_expr")]
    If(crate::ExprIf),
    #[is(name = "dict_expr")]
    Dict(crate::ExprDict),
    #[is(name = "set_expr")]
    Set(crate::ExprSet),
    #[is(name = "list_comp_expr")]
    ListComp(crate::ExprListComp),
    #[is(name = "set_comp_expr")]
    SetComp(crate::ExprSetComp),
    #[is(name = "dict_comp_expr")]
    DictComp(crate::ExprDictComp),
    #[is(name = "generator_expr")]
    Generator(crate::ExprGenerator),
    #[is(name = "await_expr")]
    Await(crate::ExprAwait),
    #[is(name = "yield_expr")]
    Yield(crate::ExprYield),
    #[is(name = "yield_from_expr")]
    YieldFrom(crate::ExprYieldFrom),
    #[is(name = "compare_expr")]
    Compare(crate::ExprCompare),
    #[is(name = "call_expr")]
    Call(crate::ExprCall),
    #[is(name = "f_string_expr")]
    FString(crate::ExprFString),
    #[is(name = "string_literal_expr")]
    StringLiteral(crate::ExprStringLiteral),
    #[is(name = "bytes_literal_expr")]
    BytesLiteral(crate::ExprBytesLiteral),
    #[is(name = "number_literal_expr")]
    NumberLiteral(crate::ExprNumberLiteral),
    #[is(name = "boolean_literal_expr")]
    BooleanLiteral(crate::ExprBooleanLiteral),
    #[is(name = "none_literal_expr")]
    NoneLiteral(crate::ExprNoneLiteral),
    #[is(name = "ellipsis_literal_expr")]
    EllipsisLiteral(crate::ExprEllipsisLiteral),
    #[is(name = "attribute_expr")]
    Attribute(crate::ExprAttribute),
    #[is(name = "subscript_expr")]
    Subscript(crate::ExprSubscript),
    #[is(name = "starred_expr")]
    Starred(crate::ExprStarred),
    #[is(name = "name_expr")]
    Name(crate::ExprName),
    #[is(name = "list_expr")]
    List(crate::ExprList),
    #[is(name = "tuple_expr")]
    Tuple(crate::ExprTuple),
    #[is(name = "slice_expr")]
    Slice(crate::ExprSlice),
    #[is(name = "ipy_escape_command_expr")]
    IpyEscapeCommand(crate::ExprIpyEscapeCommand),
}

/// See also [excepthandler](https://docs.python.org/3/library/ast.html#ast.excepthandler)
#[derive(Clone, Debug, PartialEq, is_macro::Is)]
pub enum ExceptHandler {
    ExceptHandler(crate::ExceptHandlerExceptHandler),
}

#[derive(Clone, Debug, PartialEq, is_macro::Is)]
pub enum FStringElement {
    Expression(crate::FStringExpressionElement),
    Literal(crate::FStringLiteralElement),
}

/// See also [pattern](https://docs.python.org/3/library/ast.html#ast.pattern)
#[derive(Clone, Debug, PartialEq, is_macro::Is)]
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

/// See also [type_param](https://docs.python.org/3/library/ast.html#ast.type_param)
#[derive(Clone, Debug, PartialEq, is_macro::Is)]
pub enum TypeParam {
    TypeVar(crate::TypeParamTypeVar),
    TypeVarTuple(crate::TypeParamTypeVarTuple),
    ParamSpec(crate::TypeParamParamSpec),
}
