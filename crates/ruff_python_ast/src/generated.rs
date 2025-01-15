// This is a generated file. Don't modify it by hand!
// Run `crates/ruff_python_ast/generate.py` to re-generate the file.

/// See also [mod](https://docs.python.org/3/library/ast.html#ast.mod)
#[derive(Clone, Debug, PartialEq, is_macro::Is)]
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

/// See also [excepthandler](https://docs.python.org/3/library/ast.html#ast.excepthandler)
#[derive(Clone, Debug, PartialEq, is_macro::Is)]
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

#[derive(Clone, Debug, PartialEq, is_macro::Is)]
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

/// See also [type_param](https://docs.python.org/3/library/ast.html#ast.type_param)
#[derive(Clone, Debug, PartialEq, is_macro::Is)]
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
pub enum StatementRef<'a> {
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

impl<'a> From<&'a Stmt> for StatementRef<'a> {
    fn from(node: &'a Stmt) -> Self {
        match node {
            Stmt::FunctionDef(node) => StatementRef::FunctionDef(node),
            Stmt::ClassDef(node) => StatementRef::ClassDef(node),
            Stmt::Return(node) => StatementRef::Return(node),
            Stmt::Delete(node) => StatementRef::Delete(node),
            Stmt::TypeAlias(node) => StatementRef::TypeAlias(node),
            Stmt::Assign(node) => StatementRef::Assign(node),
            Stmt::AugAssign(node) => StatementRef::AugAssign(node),
            Stmt::AnnAssign(node) => StatementRef::AnnAssign(node),
            Stmt::For(node) => StatementRef::For(node),
            Stmt::While(node) => StatementRef::While(node),
            Stmt::If(node) => StatementRef::If(node),
            Stmt::With(node) => StatementRef::With(node),
            Stmt::Match(node) => StatementRef::Match(node),
            Stmt::Raise(node) => StatementRef::Raise(node),
            Stmt::Try(node) => StatementRef::Try(node),
            Stmt::Assert(node) => StatementRef::Assert(node),
            Stmt::Import(node) => StatementRef::Import(node),
            Stmt::ImportFrom(node) => StatementRef::ImportFrom(node),
            Stmt::Global(node) => StatementRef::Global(node),
            Stmt::Nonlocal(node) => StatementRef::Nonlocal(node),
            Stmt::Expr(node) => StatementRef::Expr(node),
            Stmt::Pass(node) => StatementRef::Pass(node),
            Stmt::Break(node) => StatementRef::Break(node),
            Stmt::Continue(node) => StatementRef::Continue(node),
            Stmt::IpyEscapeCommand(node) => StatementRef::IpyEscapeCommand(node),
        }
    }
}

impl<'a> From<&'a crate::StmtFunctionDef> for StatementRef<'a> {
    fn from(node: &'a crate::StmtFunctionDef) -> Self {
        Self::FunctionDef(node)
    }
}

impl<'a> From<&'a crate::StmtClassDef> for StatementRef<'a> {
    fn from(node: &'a crate::StmtClassDef) -> Self {
        Self::ClassDef(node)
    }
}

impl<'a> From<&'a crate::StmtReturn> for StatementRef<'a> {
    fn from(node: &'a crate::StmtReturn) -> Self {
        Self::Return(node)
    }
}

impl<'a> From<&'a crate::StmtDelete> for StatementRef<'a> {
    fn from(node: &'a crate::StmtDelete) -> Self {
        Self::Delete(node)
    }
}

impl<'a> From<&'a crate::StmtTypeAlias> for StatementRef<'a> {
    fn from(node: &'a crate::StmtTypeAlias) -> Self {
        Self::TypeAlias(node)
    }
}

impl<'a> From<&'a crate::StmtAssign> for StatementRef<'a> {
    fn from(node: &'a crate::StmtAssign) -> Self {
        Self::Assign(node)
    }
}

impl<'a> From<&'a crate::StmtAugAssign> for StatementRef<'a> {
    fn from(node: &'a crate::StmtAugAssign) -> Self {
        Self::AugAssign(node)
    }
}

impl<'a> From<&'a crate::StmtAnnAssign> for StatementRef<'a> {
    fn from(node: &'a crate::StmtAnnAssign) -> Self {
        Self::AnnAssign(node)
    }
}

impl<'a> From<&'a crate::StmtFor> for StatementRef<'a> {
    fn from(node: &'a crate::StmtFor) -> Self {
        Self::For(node)
    }
}

impl<'a> From<&'a crate::StmtWhile> for StatementRef<'a> {
    fn from(node: &'a crate::StmtWhile) -> Self {
        Self::While(node)
    }
}

impl<'a> From<&'a crate::StmtIf> for StatementRef<'a> {
    fn from(node: &'a crate::StmtIf) -> Self {
        Self::If(node)
    }
}

impl<'a> From<&'a crate::StmtWith> for StatementRef<'a> {
    fn from(node: &'a crate::StmtWith) -> Self {
        Self::With(node)
    }
}

impl<'a> From<&'a crate::StmtMatch> for StatementRef<'a> {
    fn from(node: &'a crate::StmtMatch) -> Self {
        Self::Match(node)
    }
}

impl<'a> From<&'a crate::StmtRaise> for StatementRef<'a> {
    fn from(node: &'a crate::StmtRaise) -> Self {
        Self::Raise(node)
    }
}

impl<'a> From<&'a crate::StmtTry> for StatementRef<'a> {
    fn from(node: &'a crate::StmtTry) -> Self {
        Self::Try(node)
    }
}

impl<'a> From<&'a crate::StmtAssert> for StatementRef<'a> {
    fn from(node: &'a crate::StmtAssert) -> Self {
        Self::Assert(node)
    }
}

impl<'a> From<&'a crate::StmtImport> for StatementRef<'a> {
    fn from(node: &'a crate::StmtImport) -> Self {
        Self::Import(node)
    }
}

impl<'a> From<&'a crate::StmtImportFrom> for StatementRef<'a> {
    fn from(node: &'a crate::StmtImportFrom) -> Self {
        Self::ImportFrom(node)
    }
}

impl<'a> From<&'a crate::StmtGlobal> for StatementRef<'a> {
    fn from(node: &'a crate::StmtGlobal) -> Self {
        Self::Global(node)
    }
}

impl<'a> From<&'a crate::StmtNonlocal> for StatementRef<'a> {
    fn from(node: &'a crate::StmtNonlocal) -> Self {
        Self::Nonlocal(node)
    }
}

impl<'a> From<&'a crate::StmtExpr> for StatementRef<'a> {
    fn from(node: &'a crate::StmtExpr) -> Self {
        Self::Expr(node)
    }
}

impl<'a> From<&'a crate::StmtPass> for StatementRef<'a> {
    fn from(node: &'a crate::StmtPass) -> Self {
        Self::Pass(node)
    }
}

impl<'a> From<&'a crate::StmtBreak> for StatementRef<'a> {
    fn from(node: &'a crate::StmtBreak) -> Self {
        Self::Break(node)
    }
}

impl<'a> From<&'a crate::StmtContinue> for StatementRef<'a> {
    fn from(node: &'a crate::StmtContinue) -> Self {
        Self::Continue(node)
    }
}

impl<'a> From<&'a crate::StmtIpyEscapeCommand> for StatementRef<'a> {
    fn from(node: &'a crate::StmtIpyEscapeCommand) -> Self {
        Self::IpyEscapeCommand(node)
    }
}

impl ruff_text_size::Ranged for StatementRef<'_> {
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
pub enum ExpressionRef<'a> {
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

impl<'a> From<&'a Expr> for ExpressionRef<'a> {
    fn from(node: &'a Expr) -> Self {
        match node {
            Expr::BoolOp(node) => ExpressionRef::BoolOp(node),
            Expr::Named(node) => ExpressionRef::Named(node),
            Expr::BinOp(node) => ExpressionRef::BinOp(node),
            Expr::UnaryOp(node) => ExpressionRef::UnaryOp(node),
            Expr::Lambda(node) => ExpressionRef::Lambda(node),
            Expr::If(node) => ExpressionRef::If(node),
            Expr::Dict(node) => ExpressionRef::Dict(node),
            Expr::Set(node) => ExpressionRef::Set(node),
            Expr::ListComp(node) => ExpressionRef::ListComp(node),
            Expr::SetComp(node) => ExpressionRef::SetComp(node),
            Expr::DictComp(node) => ExpressionRef::DictComp(node),
            Expr::Generator(node) => ExpressionRef::Generator(node),
            Expr::Await(node) => ExpressionRef::Await(node),
            Expr::Yield(node) => ExpressionRef::Yield(node),
            Expr::YieldFrom(node) => ExpressionRef::YieldFrom(node),
            Expr::Compare(node) => ExpressionRef::Compare(node),
            Expr::Call(node) => ExpressionRef::Call(node),
            Expr::FString(node) => ExpressionRef::FString(node),
            Expr::StringLiteral(node) => ExpressionRef::StringLiteral(node),
            Expr::BytesLiteral(node) => ExpressionRef::BytesLiteral(node),
            Expr::NumberLiteral(node) => ExpressionRef::NumberLiteral(node),
            Expr::BooleanLiteral(node) => ExpressionRef::BooleanLiteral(node),
            Expr::NoneLiteral(node) => ExpressionRef::NoneLiteral(node),
            Expr::EllipsisLiteral(node) => ExpressionRef::EllipsisLiteral(node),
            Expr::Attribute(node) => ExpressionRef::Attribute(node),
            Expr::Subscript(node) => ExpressionRef::Subscript(node),
            Expr::Starred(node) => ExpressionRef::Starred(node),
            Expr::Name(node) => ExpressionRef::Name(node),
            Expr::List(node) => ExpressionRef::List(node),
            Expr::Tuple(node) => ExpressionRef::Tuple(node),
            Expr::Slice(node) => ExpressionRef::Slice(node),
            Expr::IpyEscapeCommand(node) => ExpressionRef::IpyEscapeCommand(node),
        }
    }
}

impl<'a> From<&'a crate::ExprBoolOp> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprBoolOp) -> Self {
        Self::BoolOp(node)
    }
}

impl<'a> From<&'a crate::ExprNamed> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprNamed) -> Self {
        Self::Named(node)
    }
}

impl<'a> From<&'a crate::ExprBinOp> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprBinOp) -> Self {
        Self::BinOp(node)
    }
}

impl<'a> From<&'a crate::ExprUnaryOp> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprUnaryOp) -> Self {
        Self::UnaryOp(node)
    }
}

impl<'a> From<&'a crate::ExprLambda> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprLambda) -> Self {
        Self::Lambda(node)
    }
}

impl<'a> From<&'a crate::ExprIf> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprIf) -> Self {
        Self::If(node)
    }
}

impl<'a> From<&'a crate::ExprDict> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprDict) -> Self {
        Self::Dict(node)
    }
}

impl<'a> From<&'a crate::ExprSet> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprSet) -> Self {
        Self::Set(node)
    }
}

impl<'a> From<&'a crate::ExprListComp> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprListComp) -> Self {
        Self::ListComp(node)
    }
}

impl<'a> From<&'a crate::ExprSetComp> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprSetComp) -> Self {
        Self::SetComp(node)
    }
}

impl<'a> From<&'a crate::ExprDictComp> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprDictComp) -> Self {
        Self::DictComp(node)
    }
}

impl<'a> From<&'a crate::ExprGenerator> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprGenerator) -> Self {
        Self::Generator(node)
    }
}

impl<'a> From<&'a crate::ExprAwait> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprAwait) -> Self {
        Self::Await(node)
    }
}

impl<'a> From<&'a crate::ExprYield> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprYield) -> Self {
        Self::Yield(node)
    }
}

impl<'a> From<&'a crate::ExprYieldFrom> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprYieldFrom) -> Self {
        Self::YieldFrom(node)
    }
}

impl<'a> From<&'a crate::ExprCompare> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprCompare) -> Self {
        Self::Compare(node)
    }
}

impl<'a> From<&'a crate::ExprCall> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprCall) -> Self {
        Self::Call(node)
    }
}

impl<'a> From<&'a crate::ExprFString> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprFString) -> Self {
        Self::FString(node)
    }
}

impl<'a> From<&'a crate::ExprStringLiteral> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprStringLiteral) -> Self {
        Self::StringLiteral(node)
    }
}

impl<'a> From<&'a crate::ExprBytesLiteral> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprBytesLiteral) -> Self {
        Self::BytesLiteral(node)
    }
}

impl<'a> From<&'a crate::ExprNumberLiteral> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprNumberLiteral) -> Self {
        Self::NumberLiteral(node)
    }
}

impl<'a> From<&'a crate::ExprBooleanLiteral> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprBooleanLiteral) -> Self {
        Self::BooleanLiteral(node)
    }
}

impl<'a> From<&'a crate::ExprNoneLiteral> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprNoneLiteral) -> Self {
        Self::NoneLiteral(node)
    }
}

impl<'a> From<&'a crate::ExprEllipsisLiteral> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprEllipsisLiteral) -> Self {
        Self::EllipsisLiteral(node)
    }
}

impl<'a> From<&'a crate::ExprAttribute> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprAttribute) -> Self {
        Self::Attribute(node)
    }
}

impl<'a> From<&'a crate::ExprSubscript> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprSubscript) -> Self {
        Self::Subscript(node)
    }
}

impl<'a> From<&'a crate::ExprStarred> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprStarred) -> Self {
        Self::Starred(node)
    }
}

impl<'a> From<&'a crate::ExprName> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprName) -> Self {
        Self::Name(node)
    }
}

impl<'a> From<&'a crate::ExprList> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprList) -> Self {
        Self::List(node)
    }
}

impl<'a> From<&'a crate::ExprTuple> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprTuple) -> Self {
        Self::Tuple(node)
    }
}

impl<'a> From<&'a crate::ExprSlice> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprSlice) -> Self {
        Self::Slice(node)
    }
}

impl<'a> From<&'a crate::ExprIpyEscapeCommand> for ExpressionRef<'a> {
    fn from(node: &'a crate::ExprIpyEscapeCommand) -> Self {
        Self::IpyEscapeCommand(node)
    }
}

impl ruff_text_size::Ranged for ExpressionRef<'_> {
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

#[derive(Clone, Debug, is_macro::Is, PartialEq)]
pub enum AnyNode {
    ModModule(crate::ModModule),
    ModExpression(crate::ModExpression),
    StmtFunctionDef(crate::StmtFunctionDef),
    StmtClassDef(crate::StmtClassDef),
    StmtReturn(crate::StmtReturn),
    StmtDelete(crate::StmtDelete),
    StmtTypeAlias(crate::StmtTypeAlias),
    StmtAssign(crate::StmtAssign),
    StmtAugAssign(crate::StmtAugAssign),
    StmtAnnAssign(crate::StmtAnnAssign),
    StmtFor(crate::StmtFor),
    StmtWhile(crate::StmtWhile),
    StmtIf(crate::StmtIf),
    StmtWith(crate::StmtWith),
    StmtMatch(crate::StmtMatch),
    StmtRaise(crate::StmtRaise),
    StmtTry(crate::StmtTry),
    StmtAssert(crate::StmtAssert),
    StmtImport(crate::StmtImport),
    StmtImportFrom(crate::StmtImportFrom),
    StmtGlobal(crate::StmtGlobal),
    StmtNonlocal(crate::StmtNonlocal),
    StmtExpr(crate::StmtExpr),
    StmtPass(crate::StmtPass),
    StmtBreak(crate::StmtBreak),
    StmtContinue(crate::StmtContinue),
    StmtIpyEscapeCommand(crate::StmtIpyEscapeCommand),
    ExprBoolOp(crate::ExprBoolOp),
    ExprNamed(crate::ExprNamed),
    ExprBinOp(crate::ExprBinOp),
    ExprUnaryOp(crate::ExprUnaryOp),
    ExprLambda(crate::ExprLambda),
    ExprIf(crate::ExprIf),
    ExprDict(crate::ExprDict),
    ExprSet(crate::ExprSet),
    ExprListComp(crate::ExprListComp),
    ExprSetComp(crate::ExprSetComp),
    ExprDictComp(crate::ExprDictComp),
    ExprGenerator(crate::ExprGenerator),
    ExprAwait(crate::ExprAwait),
    ExprYield(crate::ExprYield),
    ExprYieldFrom(crate::ExprYieldFrom),
    ExprCompare(crate::ExprCompare),
    ExprCall(crate::ExprCall),
    ExprFString(crate::ExprFString),
    ExprStringLiteral(crate::ExprStringLiteral),
    ExprBytesLiteral(crate::ExprBytesLiteral),
    ExprNumberLiteral(crate::ExprNumberLiteral),
    ExprBooleanLiteral(crate::ExprBooleanLiteral),
    ExprNoneLiteral(crate::ExprNoneLiteral),
    ExprEllipsisLiteral(crate::ExprEllipsisLiteral),
    ExprAttribute(crate::ExprAttribute),
    ExprSubscript(crate::ExprSubscript),
    ExprStarred(crate::ExprStarred),
    ExprName(crate::ExprName),
    ExprList(crate::ExprList),
    ExprTuple(crate::ExprTuple),
    ExprSlice(crate::ExprSlice),
    ExprIpyEscapeCommand(crate::ExprIpyEscapeCommand),
    ExceptHandlerExceptHandler(crate::ExceptHandlerExceptHandler),
    FStringExpressionElement(crate::FStringExpressionElement),
    FStringLiteralElement(crate::FStringLiteralElement),
    PatternMatchValue(crate::PatternMatchValue),
    PatternMatchSingleton(crate::PatternMatchSingleton),
    PatternMatchSequence(crate::PatternMatchSequence),
    PatternMatchMapping(crate::PatternMatchMapping),
    PatternMatchClass(crate::PatternMatchClass),
    PatternMatchStar(crate::PatternMatchStar),
    PatternMatchAs(crate::PatternMatchAs),
    PatternMatchOr(crate::PatternMatchOr),
    TypeParamTypeVar(crate::TypeParamTypeVar),
    TypeParamTypeVarTuple(crate::TypeParamTypeVarTuple),
    TypeParamParamSpec(crate::TypeParamParamSpec),
    FStringFormatSpec(crate::FStringFormatSpec),
    PatternArguments(crate::PatternArguments),
    PatternKeyword(crate::PatternKeyword),
    Comprehension(crate::Comprehension),
    Arguments(crate::Arguments),
    Parameters(crate::Parameters),
    Parameter(crate::Parameter),
    ParameterWithDefault(crate::ParameterWithDefault),
    Keyword(crate::Keyword),
    Alias(crate::Alias),
    WithItem(crate::WithItem),
    MatchCase(crate::MatchCase),
    Decorator(crate::Decorator),
    ElifElseClause(crate::ElifElseClause),
    TypeParams(crate::TypeParams),
    FString(crate::FString),
    StringLiteral(crate::StringLiteral),
    BytesLiteral(crate::BytesLiteral),
    Identifier(crate::Identifier),
}

impl From<Mod> for AnyNode {
    fn from(node: Mod) -> AnyNode {
        match node {
            Mod::Module(node) => AnyNode::ModModule(node),
            Mod::Expression(node) => AnyNode::ModExpression(node),
        }
    }
}

impl From<crate::ModModule> for AnyNode {
    fn from(node: crate::ModModule) -> AnyNode {
        AnyNode::ModModule(node)
    }
}

impl From<crate::ModExpression> for AnyNode {
    fn from(node: crate::ModExpression) -> AnyNode {
        AnyNode::ModExpression(node)
    }
}

impl From<Stmt> for AnyNode {
    fn from(node: Stmt) -> AnyNode {
        match node {
            Stmt::FunctionDef(node) => AnyNode::StmtFunctionDef(node),
            Stmt::ClassDef(node) => AnyNode::StmtClassDef(node),
            Stmt::Return(node) => AnyNode::StmtReturn(node),
            Stmt::Delete(node) => AnyNode::StmtDelete(node),
            Stmt::TypeAlias(node) => AnyNode::StmtTypeAlias(node),
            Stmt::Assign(node) => AnyNode::StmtAssign(node),
            Stmt::AugAssign(node) => AnyNode::StmtAugAssign(node),
            Stmt::AnnAssign(node) => AnyNode::StmtAnnAssign(node),
            Stmt::For(node) => AnyNode::StmtFor(node),
            Stmt::While(node) => AnyNode::StmtWhile(node),
            Stmt::If(node) => AnyNode::StmtIf(node),
            Stmt::With(node) => AnyNode::StmtWith(node),
            Stmt::Match(node) => AnyNode::StmtMatch(node),
            Stmt::Raise(node) => AnyNode::StmtRaise(node),
            Stmt::Try(node) => AnyNode::StmtTry(node),
            Stmt::Assert(node) => AnyNode::StmtAssert(node),
            Stmt::Import(node) => AnyNode::StmtImport(node),
            Stmt::ImportFrom(node) => AnyNode::StmtImportFrom(node),
            Stmt::Global(node) => AnyNode::StmtGlobal(node),
            Stmt::Nonlocal(node) => AnyNode::StmtNonlocal(node),
            Stmt::Expr(node) => AnyNode::StmtExpr(node),
            Stmt::Pass(node) => AnyNode::StmtPass(node),
            Stmt::Break(node) => AnyNode::StmtBreak(node),
            Stmt::Continue(node) => AnyNode::StmtContinue(node),
            Stmt::IpyEscapeCommand(node) => AnyNode::StmtIpyEscapeCommand(node),
        }
    }
}

impl From<crate::StmtFunctionDef> for AnyNode {
    fn from(node: crate::StmtFunctionDef) -> AnyNode {
        AnyNode::StmtFunctionDef(node)
    }
}

impl From<crate::StmtClassDef> for AnyNode {
    fn from(node: crate::StmtClassDef) -> AnyNode {
        AnyNode::StmtClassDef(node)
    }
}

impl From<crate::StmtReturn> for AnyNode {
    fn from(node: crate::StmtReturn) -> AnyNode {
        AnyNode::StmtReturn(node)
    }
}

impl From<crate::StmtDelete> for AnyNode {
    fn from(node: crate::StmtDelete) -> AnyNode {
        AnyNode::StmtDelete(node)
    }
}

impl From<crate::StmtTypeAlias> for AnyNode {
    fn from(node: crate::StmtTypeAlias) -> AnyNode {
        AnyNode::StmtTypeAlias(node)
    }
}

impl From<crate::StmtAssign> for AnyNode {
    fn from(node: crate::StmtAssign) -> AnyNode {
        AnyNode::StmtAssign(node)
    }
}

impl From<crate::StmtAugAssign> for AnyNode {
    fn from(node: crate::StmtAugAssign) -> AnyNode {
        AnyNode::StmtAugAssign(node)
    }
}

impl From<crate::StmtAnnAssign> for AnyNode {
    fn from(node: crate::StmtAnnAssign) -> AnyNode {
        AnyNode::StmtAnnAssign(node)
    }
}

impl From<crate::StmtFor> for AnyNode {
    fn from(node: crate::StmtFor) -> AnyNode {
        AnyNode::StmtFor(node)
    }
}

impl From<crate::StmtWhile> for AnyNode {
    fn from(node: crate::StmtWhile) -> AnyNode {
        AnyNode::StmtWhile(node)
    }
}

impl From<crate::StmtIf> for AnyNode {
    fn from(node: crate::StmtIf) -> AnyNode {
        AnyNode::StmtIf(node)
    }
}

impl From<crate::StmtWith> for AnyNode {
    fn from(node: crate::StmtWith) -> AnyNode {
        AnyNode::StmtWith(node)
    }
}

impl From<crate::StmtMatch> for AnyNode {
    fn from(node: crate::StmtMatch) -> AnyNode {
        AnyNode::StmtMatch(node)
    }
}

impl From<crate::StmtRaise> for AnyNode {
    fn from(node: crate::StmtRaise) -> AnyNode {
        AnyNode::StmtRaise(node)
    }
}

impl From<crate::StmtTry> for AnyNode {
    fn from(node: crate::StmtTry) -> AnyNode {
        AnyNode::StmtTry(node)
    }
}

impl From<crate::StmtAssert> for AnyNode {
    fn from(node: crate::StmtAssert) -> AnyNode {
        AnyNode::StmtAssert(node)
    }
}

impl From<crate::StmtImport> for AnyNode {
    fn from(node: crate::StmtImport) -> AnyNode {
        AnyNode::StmtImport(node)
    }
}

impl From<crate::StmtImportFrom> for AnyNode {
    fn from(node: crate::StmtImportFrom) -> AnyNode {
        AnyNode::StmtImportFrom(node)
    }
}

impl From<crate::StmtGlobal> for AnyNode {
    fn from(node: crate::StmtGlobal) -> AnyNode {
        AnyNode::StmtGlobal(node)
    }
}

impl From<crate::StmtNonlocal> for AnyNode {
    fn from(node: crate::StmtNonlocal) -> AnyNode {
        AnyNode::StmtNonlocal(node)
    }
}

impl From<crate::StmtExpr> for AnyNode {
    fn from(node: crate::StmtExpr) -> AnyNode {
        AnyNode::StmtExpr(node)
    }
}

impl From<crate::StmtPass> for AnyNode {
    fn from(node: crate::StmtPass) -> AnyNode {
        AnyNode::StmtPass(node)
    }
}

impl From<crate::StmtBreak> for AnyNode {
    fn from(node: crate::StmtBreak) -> AnyNode {
        AnyNode::StmtBreak(node)
    }
}

impl From<crate::StmtContinue> for AnyNode {
    fn from(node: crate::StmtContinue) -> AnyNode {
        AnyNode::StmtContinue(node)
    }
}

impl From<crate::StmtIpyEscapeCommand> for AnyNode {
    fn from(node: crate::StmtIpyEscapeCommand) -> AnyNode {
        AnyNode::StmtIpyEscapeCommand(node)
    }
}

impl From<Expr> for AnyNode {
    fn from(node: Expr) -> AnyNode {
        match node {
            Expr::BoolOp(node) => AnyNode::ExprBoolOp(node),
            Expr::Named(node) => AnyNode::ExprNamed(node),
            Expr::BinOp(node) => AnyNode::ExprBinOp(node),
            Expr::UnaryOp(node) => AnyNode::ExprUnaryOp(node),
            Expr::Lambda(node) => AnyNode::ExprLambda(node),
            Expr::If(node) => AnyNode::ExprIf(node),
            Expr::Dict(node) => AnyNode::ExprDict(node),
            Expr::Set(node) => AnyNode::ExprSet(node),
            Expr::ListComp(node) => AnyNode::ExprListComp(node),
            Expr::SetComp(node) => AnyNode::ExprSetComp(node),
            Expr::DictComp(node) => AnyNode::ExprDictComp(node),
            Expr::Generator(node) => AnyNode::ExprGenerator(node),
            Expr::Await(node) => AnyNode::ExprAwait(node),
            Expr::Yield(node) => AnyNode::ExprYield(node),
            Expr::YieldFrom(node) => AnyNode::ExprYieldFrom(node),
            Expr::Compare(node) => AnyNode::ExprCompare(node),
            Expr::Call(node) => AnyNode::ExprCall(node),
            Expr::FString(node) => AnyNode::ExprFString(node),
            Expr::StringLiteral(node) => AnyNode::ExprStringLiteral(node),
            Expr::BytesLiteral(node) => AnyNode::ExprBytesLiteral(node),
            Expr::NumberLiteral(node) => AnyNode::ExprNumberLiteral(node),
            Expr::BooleanLiteral(node) => AnyNode::ExprBooleanLiteral(node),
            Expr::NoneLiteral(node) => AnyNode::ExprNoneLiteral(node),
            Expr::EllipsisLiteral(node) => AnyNode::ExprEllipsisLiteral(node),
            Expr::Attribute(node) => AnyNode::ExprAttribute(node),
            Expr::Subscript(node) => AnyNode::ExprSubscript(node),
            Expr::Starred(node) => AnyNode::ExprStarred(node),
            Expr::Name(node) => AnyNode::ExprName(node),
            Expr::List(node) => AnyNode::ExprList(node),
            Expr::Tuple(node) => AnyNode::ExprTuple(node),
            Expr::Slice(node) => AnyNode::ExprSlice(node),
            Expr::IpyEscapeCommand(node) => AnyNode::ExprIpyEscapeCommand(node),
        }
    }
}

impl From<crate::ExprBoolOp> for AnyNode {
    fn from(node: crate::ExprBoolOp) -> AnyNode {
        AnyNode::ExprBoolOp(node)
    }
}

impl From<crate::ExprNamed> for AnyNode {
    fn from(node: crate::ExprNamed) -> AnyNode {
        AnyNode::ExprNamed(node)
    }
}

impl From<crate::ExprBinOp> for AnyNode {
    fn from(node: crate::ExprBinOp) -> AnyNode {
        AnyNode::ExprBinOp(node)
    }
}

impl From<crate::ExprUnaryOp> for AnyNode {
    fn from(node: crate::ExprUnaryOp) -> AnyNode {
        AnyNode::ExprUnaryOp(node)
    }
}

impl From<crate::ExprLambda> for AnyNode {
    fn from(node: crate::ExprLambda) -> AnyNode {
        AnyNode::ExprLambda(node)
    }
}

impl From<crate::ExprIf> for AnyNode {
    fn from(node: crate::ExprIf) -> AnyNode {
        AnyNode::ExprIf(node)
    }
}

impl From<crate::ExprDict> for AnyNode {
    fn from(node: crate::ExprDict) -> AnyNode {
        AnyNode::ExprDict(node)
    }
}

impl From<crate::ExprSet> for AnyNode {
    fn from(node: crate::ExprSet) -> AnyNode {
        AnyNode::ExprSet(node)
    }
}

impl From<crate::ExprListComp> for AnyNode {
    fn from(node: crate::ExprListComp) -> AnyNode {
        AnyNode::ExprListComp(node)
    }
}

impl From<crate::ExprSetComp> for AnyNode {
    fn from(node: crate::ExprSetComp) -> AnyNode {
        AnyNode::ExprSetComp(node)
    }
}

impl From<crate::ExprDictComp> for AnyNode {
    fn from(node: crate::ExprDictComp) -> AnyNode {
        AnyNode::ExprDictComp(node)
    }
}

impl From<crate::ExprGenerator> for AnyNode {
    fn from(node: crate::ExprGenerator) -> AnyNode {
        AnyNode::ExprGenerator(node)
    }
}

impl From<crate::ExprAwait> for AnyNode {
    fn from(node: crate::ExprAwait) -> AnyNode {
        AnyNode::ExprAwait(node)
    }
}

impl From<crate::ExprYield> for AnyNode {
    fn from(node: crate::ExprYield) -> AnyNode {
        AnyNode::ExprYield(node)
    }
}

impl From<crate::ExprYieldFrom> for AnyNode {
    fn from(node: crate::ExprYieldFrom) -> AnyNode {
        AnyNode::ExprYieldFrom(node)
    }
}

impl From<crate::ExprCompare> for AnyNode {
    fn from(node: crate::ExprCompare) -> AnyNode {
        AnyNode::ExprCompare(node)
    }
}

impl From<crate::ExprCall> for AnyNode {
    fn from(node: crate::ExprCall) -> AnyNode {
        AnyNode::ExprCall(node)
    }
}

impl From<crate::ExprFString> for AnyNode {
    fn from(node: crate::ExprFString) -> AnyNode {
        AnyNode::ExprFString(node)
    }
}

impl From<crate::ExprStringLiteral> for AnyNode {
    fn from(node: crate::ExprStringLiteral) -> AnyNode {
        AnyNode::ExprStringLiteral(node)
    }
}

impl From<crate::ExprBytesLiteral> for AnyNode {
    fn from(node: crate::ExprBytesLiteral) -> AnyNode {
        AnyNode::ExprBytesLiteral(node)
    }
}

impl From<crate::ExprNumberLiteral> for AnyNode {
    fn from(node: crate::ExprNumberLiteral) -> AnyNode {
        AnyNode::ExprNumberLiteral(node)
    }
}

impl From<crate::ExprBooleanLiteral> for AnyNode {
    fn from(node: crate::ExprBooleanLiteral) -> AnyNode {
        AnyNode::ExprBooleanLiteral(node)
    }
}

impl From<crate::ExprNoneLiteral> for AnyNode {
    fn from(node: crate::ExprNoneLiteral) -> AnyNode {
        AnyNode::ExprNoneLiteral(node)
    }
}

impl From<crate::ExprEllipsisLiteral> for AnyNode {
    fn from(node: crate::ExprEllipsisLiteral) -> AnyNode {
        AnyNode::ExprEllipsisLiteral(node)
    }
}

impl From<crate::ExprAttribute> for AnyNode {
    fn from(node: crate::ExprAttribute) -> AnyNode {
        AnyNode::ExprAttribute(node)
    }
}

impl From<crate::ExprSubscript> for AnyNode {
    fn from(node: crate::ExprSubscript) -> AnyNode {
        AnyNode::ExprSubscript(node)
    }
}

impl From<crate::ExprStarred> for AnyNode {
    fn from(node: crate::ExprStarred) -> AnyNode {
        AnyNode::ExprStarred(node)
    }
}

impl From<crate::ExprName> for AnyNode {
    fn from(node: crate::ExprName) -> AnyNode {
        AnyNode::ExprName(node)
    }
}

impl From<crate::ExprList> for AnyNode {
    fn from(node: crate::ExprList) -> AnyNode {
        AnyNode::ExprList(node)
    }
}

impl From<crate::ExprTuple> for AnyNode {
    fn from(node: crate::ExprTuple) -> AnyNode {
        AnyNode::ExprTuple(node)
    }
}

impl From<crate::ExprSlice> for AnyNode {
    fn from(node: crate::ExprSlice) -> AnyNode {
        AnyNode::ExprSlice(node)
    }
}

impl From<crate::ExprIpyEscapeCommand> for AnyNode {
    fn from(node: crate::ExprIpyEscapeCommand) -> AnyNode {
        AnyNode::ExprIpyEscapeCommand(node)
    }
}

impl From<ExceptHandler> for AnyNode {
    fn from(node: ExceptHandler) -> AnyNode {
        match node {
            ExceptHandler::ExceptHandler(node) => AnyNode::ExceptHandlerExceptHandler(node),
        }
    }
}

impl From<crate::ExceptHandlerExceptHandler> for AnyNode {
    fn from(node: crate::ExceptHandlerExceptHandler) -> AnyNode {
        AnyNode::ExceptHandlerExceptHandler(node)
    }
}

impl From<FStringElement> for AnyNode {
    fn from(node: FStringElement) -> AnyNode {
        match node {
            FStringElement::Expression(node) => AnyNode::FStringExpressionElement(node),
            FStringElement::Literal(node) => AnyNode::FStringLiteralElement(node),
        }
    }
}

impl From<crate::FStringExpressionElement> for AnyNode {
    fn from(node: crate::FStringExpressionElement) -> AnyNode {
        AnyNode::FStringExpressionElement(node)
    }
}

impl From<crate::FStringLiteralElement> for AnyNode {
    fn from(node: crate::FStringLiteralElement) -> AnyNode {
        AnyNode::FStringLiteralElement(node)
    }
}

impl From<Pattern> for AnyNode {
    fn from(node: Pattern) -> AnyNode {
        match node {
            Pattern::MatchValue(node) => AnyNode::PatternMatchValue(node),
            Pattern::MatchSingleton(node) => AnyNode::PatternMatchSingleton(node),
            Pattern::MatchSequence(node) => AnyNode::PatternMatchSequence(node),
            Pattern::MatchMapping(node) => AnyNode::PatternMatchMapping(node),
            Pattern::MatchClass(node) => AnyNode::PatternMatchClass(node),
            Pattern::MatchStar(node) => AnyNode::PatternMatchStar(node),
            Pattern::MatchAs(node) => AnyNode::PatternMatchAs(node),
            Pattern::MatchOr(node) => AnyNode::PatternMatchOr(node),
        }
    }
}

impl From<crate::PatternMatchValue> for AnyNode {
    fn from(node: crate::PatternMatchValue) -> AnyNode {
        AnyNode::PatternMatchValue(node)
    }
}

impl From<crate::PatternMatchSingleton> for AnyNode {
    fn from(node: crate::PatternMatchSingleton) -> AnyNode {
        AnyNode::PatternMatchSingleton(node)
    }
}

impl From<crate::PatternMatchSequence> for AnyNode {
    fn from(node: crate::PatternMatchSequence) -> AnyNode {
        AnyNode::PatternMatchSequence(node)
    }
}

impl From<crate::PatternMatchMapping> for AnyNode {
    fn from(node: crate::PatternMatchMapping) -> AnyNode {
        AnyNode::PatternMatchMapping(node)
    }
}

impl From<crate::PatternMatchClass> for AnyNode {
    fn from(node: crate::PatternMatchClass) -> AnyNode {
        AnyNode::PatternMatchClass(node)
    }
}

impl From<crate::PatternMatchStar> for AnyNode {
    fn from(node: crate::PatternMatchStar) -> AnyNode {
        AnyNode::PatternMatchStar(node)
    }
}

impl From<crate::PatternMatchAs> for AnyNode {
    fn from(node: crate::PatternMatchAs) -> AnyNode {
        AnyNode::PatternMatchAs(node)
    }
}

impl From<crate::PatternMatchOr> for AnyNode {
    fn from(node: crate::PatternMatchOr) -> AnyNode {
        AnyNode::PatternMatchOr(node)
    }
}

impl From<TypeParam> for AnyNode {
    fn from(node: TypeParam) -> AnyNode {
        match node {
            TypeParam::TypeVar(node) => AnyNode::TypeParamTypeVar(node),
            TypeParam::TypeVarTuple(node) => AnyNode::TypeParamTypeVarTuple(node),
            TypeParam::ParamSpec(node) => AnyNode::TypeParamParamSpec(node),
        }
    }
}

impl From<crate::TypeParamTypeVar> for AnyNode {
    fn from(node: crate::TypeParamTypeVar) -> AnyNode {
        AnyNode::TypeParamTypeVar(node)
    }
}

impl From<crate::TypeParamTypeVarTuple> for AnyNode {
    fn from(node: crate::TypeParamTypeVarTuple) -> AnyNode {
        AnyNode::TypeParamTypeVarTuple(node)
    }
}

impl From<crate::TypeParamParamSpec> for AnyNode {
    fn from(node: crate::TypeParamParamSpec) -> AnyNode {
        AnyNode::TypeParamParamSpec(node)
    }
}

impl From<crate::FStringFormatSpec> for AnyNode {
    fn from(node: crate::FStringFormatSpec) -> AnyNode {
        AnyNode::FStringFormatSpec(node)
    }
}

impl From<crate::PatternArguments> for AnyNode {
    fn from(node: crate::PatternArguments) -> AnyNode {
        AnyNode::PatternArguments(node)
    }
}

impl From<crate::PatternKeyword> for AnyNode {
    fn from(node: crate::PatternKeyword) -> AnyNode {
        AnyNode::PatternKeyword(node)
    }
}

impl From<crate::Comprehension> for AnyNode {
    fn from(node: crate::Comprehension) -> AnyNode {
        AnyNode::Comprehension(node)
    }
}

impl From<crate::Arguments> for AnyNode {
    fn from(node: crate::Arguments) -> AnyNode {
        AnyNode::Arguments(node)
    }
}

impl From<crate::Parameters> for AnyNode {
    fn from(node: crate::Parameters) -> AnyNode {
        AnyNode::Parameters(node)
    }
}

impl From<crate::Parameter> for AnyNode {
    fn from(node: crate::Parameter) -> AnyNode {
        AnyNode::Parameter(node)
    }
}

impl From<crate::ParameterWithDefault> for AnyNode {
    fn from(node: crate::ParameterWithDefault) -> AnyNode {
        AnyNode::ParameterWithDefault(node)
    }
}

impl From<crate::Keyword> for AnyNode {
    fn from(node: crate::Keyword) -> AnyNode {
        AnyNode::Keyword(node)
    }
}

impl From<crate::Alias> for AnyNode {
    fn from(node: crate::Alias) -> AnyNode {
        AnyNode::Alias(node)
    }
}

impl From<crate::WithItem> for AnyNode {
    fn from(node: crate::WithItem) -> AnyNode {
        AnyNode::WithItem(node)
    }
}

impl From<crate::MatchCase> for AnyNode {
    fn from(node: crate::MatchCase) -> AnyNode {
        AnyNode::MatchCase(node)
    }
}

impl From<crate::Decorator> for AnyNode {
    fn from(node: crate::Decorator) -> AnyNode {
        AnyNode::Decorator(node)
    }
}

impl From<crate::ElifElseClause> for AnyNode {
    fn from(node: crate::ElifElseClause) -> AnyNode {
        AnyNode::ElifElseClause(node)
    }
}

impl From<crate::TypeParams> for AnyNode {
    fn from(node: crate::TypeParams) -> AnyNode {
        AnyNode::TypeParams(node)
    }
}

impl From<crate::FString> for AnyNode {
    fn from(node: crate::FString) -> AnyNode {
        AnyNode::FString(node)
    }
}

impl From<crate::StringLiteral> for AnyNode {
    fn from(node: crate::StringLiteral) -> AnyNode {
        AnyNode::StringLiteral(node)
    }
}

impl From<crate::BytesLiteral> for AnyNode {
    fn from(node: crate::BytesLiteral) -> AnyNode {
        AnyNode::BytesLiteral(node)
    }
}

impl From<crate::Identifier> for AnyNode {
    fn from(node: crate::Identifier) -> AnyNode {
        AnyNode::Identifier(node)
    }
}

impl ruff_text_size::Ranged for AnyNode {
    fn range(&self) -> ruff_text_size::TextRange {
        match self {
            AnyNode::ModModule(node) => node.range(),
            AnyNode::ModExpression(node) => node.range(),
            AnyNode::StmtFunctionDef(node) => node.range(),
            AnyNode::StmtClassDef(node) => node.range(),
            AnyNode::StmtReturn(node) => node.range(),
            AnyNode::StmtDelete(node) => node.range(),
            AnyNode::StmtTypeAlias(node) => node.range(),
            AnyNode::StmtAssign(node) => node.range(),
            AnyNode::StmtAugAssign(node) => node.range(),
            AnyNode::StmtAnnAssign(node) => node.range(),
            AnyNode::StmtFor(node) => node.range(),
            AnyNode::StmtWhile(node) => node.range(),
            AnyNode::StmtIf(node) => node.range(),
            AnyNode::StmtWith(node) => node.range(),
            AnyNode::StmtMatch(node) => node.range(),
            AnyNode::StmtRaise(node) => node.range(),
            AnyNode::StmtTry(node) => node.range(),
            AnyNode::StmtAssert(node) => node.range(),
            AnyNode::StmtImport(node) => node.range(),
            AnyNode::StmtImportFrom(node) => node.range(),
            AnyNode::StmtGlobal(node) => node.range(),
            AnyNode::StmtNonlocal(node) => node.range(),
            AnyNode::StmtExpr(node) => node.range(),
            AnyNode::StmtPass(node) => node.range(),
            AnyNode::StmtBreak(node) => node.range(),
            AnyNode::StmtContinue(node) => node.range(),
            AnyNode::StmtIpyEscapeCommand(node) => node.range(),
            AnyNode::ExprBoolOp(node) => node.range(),
            AnyNode::ExprNamed(node) => node.range(),
            AnyNode::ExprBinOp(node) => node.range(),
            AnyNode::ExprUnaryOp(node) => node.range(),
            AnyNode::ExprLambda(node) => node.range(),
            AnyNode::ExprIf(node) => node.range(),
            AnyNode::ExprDict(node) => node.range(),
            AnyNode::ExprSet(node) => node.range(),
            AnyNode::ExprListComp(node) => node.range(),
            AnyNode::ExprSetComp(node) => node.range(),
            AnyNode::ExprDictComp(node) => node.range(),
            AnyNode::ExprGenerator(node) => node.range(),
            AnyNode::ExprAwait(node) => node.range(),
            AnyNode::ExprYield(node) => node.range(),
            AnyNode::ExprYieldFrom(node) => node.range(),
            AnyNode::ExprCompare(node) => node.range(),
            AnyNode::ExprCall(node) => node.range(),
            AnyNode::ExprFString(node) => node.range(),
            AnyNode::ExprStringLiteral(node) => node.range(),
            AnyNode::ExprBytesLiteral(node) => node.range(),
            AnyNode::ExprNumberLiteral(node) => node.range(),
            AnyNode::ExprBooleanLiteral(node) => node.range(),
            AnyNode::ExprNoneLiteral(node) => node.range(),
            AnyNode::ExprEllipsisLiteral(node) => node.range(),
            AnyNode::ExprAttribute(node) => node.range(),
            AnyNode::ExprSubscript(node) => node.range(),
            AnyNode::ExprStarred(node) => node.range(),
            AnyNode::ExprName(node) => node.range(),
            AnyNode::ExprList(node) => node.range(),
            AnyNode::ExprTuple(node) => node.range(),
            AnyNode::ExprSlice(node) => node.range(),
            AnyNode::ExprIpyEscapeCommand(node) => node.range(),
            AnyNode::ExceptHandlerExceptHandler(node) => node.range(),
            AnyNode::FStringExpressionElement(node) => node.range(),
            AnyNode::FStringLiteralElement(node) => node.range(),
            AnyNode::PatternMatchValue(node) => node.range(),
            AnyNode::PatternMatchSingleton(node) => node.range(),
            AnyNode::PatternMatchSequence(node) => node.range(),
            AnyNode::PatternMatchMapping(node) => node.range(),
            AnyNode::PatternMatchClass(node) => node.range(),
            AnyNode::PatternMatchStar(node) => node.range(),
            AnyNode::PatternMatchAs(node) => node.range(),
            AnyNode::PatternMatchOr(node) => node.range(),
            AnyNode::TypeParamTypeVar(node) => node.range(),
            AnyNode::TypeParamTypeVarTuple(node) => node.range(),
            AnyNode::TypeParamParamSpec(node) => node.range(),
            AnyNode::FStringFormatSpec(node) => node.range(),
            AnyNode::PatternArguments(node) => node.range(),
            AnyNode::PatternKeyword(node) => node.range(),
            AnyNode::Comprehension(node) => node.range(),
            AnyNode::Arguments(node) => node.range(),
            AnyNode::Parameters(node) => node.range(),
            AnyNode::Parameter(node) => node.range(),
            AnyNode::ParameterWithDefault(node) => node.range(),
            AnyNode::Keyword(node) => node.range(),
            AnyNode::Alias(node) => node.range(),
            AnyNode::WithItem(node) => node.range(),
            AnyNode::MatchCase(node) => node.range(),
            AnyNode::Decorator(node) => node.range(),
            AnyNode::ElifElseClause(node) => node.range(),
            AnyNode::TypeParams(node) => node.range(),
            AnyNode::FString(node) => node.range(),
            AnyNode::StringLiteral(node) => node.range(),
            AnyNode::BytesLiteral(node) => node.range(),
            AnyNode::Identifier(node) => node.range(),
        }
    }
}

impl AnyNode {
    pub fn module(self) -> Option<Mod> {
        match self {
            AnyNode::ModModule(node) => Some(Mod::Module(node)),
            AnyNode::ModExpression(node) => Some(Mod::Expression(node)),

            _ => None,
        }
    }
}

impl AnyNode {
    pub fn statement(self) -> Option<Stmt> {
        match self {
            AnyNode::StmtFunctionDef(node) => Some(Stmt::FunctionDef(node)),
            AnyNode::StmtClassDef(node) => Some(Stmt::ClassDef(node)),
            AnyNode::StmtReturn(node) => Some(Stmt::Return(node)),
            AnyNode::StmtDelete(node) => Some(Stmt::Delete(node)),
            AnyNode::StmtTypeAlias(node) => Some(Stmt::TypeAlias(node)),
            AnyNode::StmtAssign(node) => Some(Stmt::Assign(node)),
            AnyNode::StmtAugAssign(node) => Some(Stmt::AugAssign(node)),
            AnyNode::StmtAnnAssign(node) => Some(Stmt::AnnAssign(node)),
            AnyNode::StmtFor(node) => Some(Stmt::For(node)),
            AnyNode::StmtWhile(node) => Some(Stmt::While(node)),
            AnyNode::StmtIf(node) => Some(Stmt::If(node)),
            AnyNode::StmtWith(node) => Some(Stmt::With(node)),
            AnyNode::StmtMatch(node) => Some(Stmt::Match(node)),
            AnyNode::StmtRaise(node) => Some(Stmt::Raise(node)),
            AnyNode::StmtTry(node) => Some(Stmt::Try(node)),
            AnyNode::StmtAssert(node) => Some(Stmt::Assert(node)),
            AnyNode::StmtImport(node) => Some(Stmt::Import(node)),
            AnyNode::StmtImportFrom(node) => Some(Stmt::ImportFrom(node)),
            AnyNode::StmtGlobal(node) => Some(Stmt::Global(node)),
            AnyNode::StmtNonlocal(node) => Some(Stmt::Nonlocal(node)),
            AnyNode::StmtExpr(node) => Some(Stmt::Expr(node)),
            AnyNode::StmtPass(node) => Some(Stmt::Pass(node)),
            AnyNode::StmtBreak(node) => Some(Stmt::Break(node)),
            AnyNode::StmtContinue(node) => Some(Stmt::Continue(node)),
            AnyNode::StmtIpyEscapeCommand(node) => Some(Stmt::IpyEscapeCommand(node)),

            _ => None,
        }
    }
}

impl AnyNode {
    pub fn expression(self) -> Option<Expr> {
        match self {
            AnyNode::ExprBoolOp(node) => Some(Expr::BoolOp(node)),
            AnyNode::ExprNamed(node) => Some(Expr::Named(node)),
            AnyNode::ExprBinOp(node) => Some(Expr::BinOp(node)),
            AnyNode::ExprUnaryOp(node) => Some(Expr::UnaryOp(node)),
            AnyNode::ExprLambda(node) => Some(Expr::Lambda(node)),
            AnyNode::ExprIf(node) => Some(Expr::If(node)),
            AnyNode::ExprDict(node) => Some(Expr::Dict(node)),
            AnyNode::ExprSet(node) => Some(Expr::Set(node)),
            AnyNode::ExprListComp(node) => Some(Expr::ListComp(node)),
            AnyNode::ExprSetComp(node) => Some(Expr::SetComp(node)),
            AnyNode::ExprDictComp(node) => Some(Expr::DictComp(node)),
            AnyNode::ExprGenerator(node) => Some(Expr::Generator(node)),
            AnyNode::ExprAwait(node) => Some(Expr::Await(node)),
            AnyNode::ExprYield(node) => Some(Expr::Yield(node)),
            AnyNode::ExprYieldFrom(node) => Some(Expr::YieldFrom(node)),
            AnyNode::ExprCompare(node) => Some(Expr::Compare(node)),
            AnyNode::ExprCall(node) => Some(Expr::Call(node)),
            AnyNode::ExprFString(node) => Some(Expr::FString(node)),
            AnyNode::ExprStringLiteral(node) => Some(Expr::StringLiteral(node)),
            AnyNode::ExprBytesLiteral(node) => Some(Expr::BytesLiteral(node)),
            AnyNode::ExprNumberLiteral(node) => Some(Expr::NumberLiteral(node)),
            AnyNode::ExprBooleanLiteral(node) => Some(Expr::BooleanLiteral(node)),
            AnyNode::ExprNoneLiteral(node) => Some(Expr::NoneLiteral(node)),
            AnyNode::ExprEllipsisLiteral(node) => Some(Expr::EllipsisLiteral(node)),
            AnyNode::ExprAttribute(node) => Some(Expr::Attribute(node)),
            AnyNode::ExprSubscript(node) => Some(Expr::Subscript(node)),
            AnyNode::ExprStarred(node) => Some(Expr::Starred(node)),
            AnyNode::ExprName(node) => Some(Expr::Name(node)),
            AnyNode::ExprList(node) => Some(Expr::List(node)),
            AnyNode::ExprTuple(node) => Some(Expr::Tuple(node)),
            AnyNode::ExprSlice(node) => Some(Expr::Slice(node)),
            AnyNode::ExprIpyEscapeCommand(node) => Some(Expr::IpyEscapeCommand(node)),

            _ => None,
        }
    }
}

impl AnyNode {
    pub fn except_handler(self) -> Option<ExceptHandler> {
        match self {
            AnyNode::ExceptHandlerExceptHandler(node) => Some(ExceptHandler::ExceptHandler(node)),

            _ => None,
        }
    }
}

impl AnyNode {
    pub fn f_string_element(self) -> Option<FStringElement> {
        match self {
            AnyNode::FStringExpressionElement(node) => Some(FStringElement::Expression(node)),
            AnyNode::FStringLiteralElement(node) => Some(FStringElement::Literal(node)),

            _ => None,
        }
    }
}

impl AnyNode {
    pub fn pattern(self) -> Option<Pattern> {
        match self {
            AnyNode::PatternMatchValue(node) => Some(Pattern::MatchValue(node)),
            AnyNode::PatternMatchSingleton(node) => Some(Pattern::MatchSingleton(node)),
            AnyNode::PatternMatchSequence(node) => Some(Pattern::MatchSequence(node)),
            AnyNode::PatternMatchMapping(node) => Some(Pattern::MatchMapping(node)),
            AnyNode::PatternMatchClass(node) => Some(Pattern::MatchClass(node)),
            AnyNode::PatternMatchStar(node) => Some(Pattern::MatchStar(node)),
            AnyNode::PatternMatchAs(node) => Some(Pattern::MatchAs(node)),
            AnyNode::PatternMatchOr(node) => Some(Pattern::MatchOr(node)),

            _ => None,
        }
    }
}

impl AnyNode {
    pub fn type_param(self) -> Option<TypeParam> {
        match self {
            AnyNode::TypeParamTypeVar(node) => Some(TypeParam::TypeVar(node)),
            AnyNode::TypeParamTypeVarTuple(node) => Some(TypeParam::TypeVarTuple(node)),
            AnyNode::TypeParamParamSpec(node) => Some(TypeParam::ParamSpec(node)),

            _ => None,
        }
    }
}

impl AnyNode {
    pub const fn is_module(&self) -> bool {
        matches!(self, AnyNode::ModModule(_) | AnyNode::ModExpression(_))
    }
}

impl AnyNode {
    pub const fn is_statement(&self) -> bool {
        matches!(
            self,
            AnyNode::StmtFunctionDef(_)
                | AnyNode::StmtClassDef(_)
                | AnyNode::StmtReturn(_)
                | AnyNode::StmtDelete(_)
                | AnyNode::StmtTypeAlias(_)
                | AnyNode::StmtAssign(_)
                | AnyNode::StmtAugAssign(_)
                | AnyNode::StmtAnnAssign(_)
                | AnyNode::StmtFor(_)
                | AnyNode::StmtWhile(_)
                | AnyNode::StmtIf(_)
                | AnyNode::StmtWith(_)
                | AnyNode::StmtMatch(_)
                | AnyNode::StmtRaise(_)
                | AnyNode::StmtTry(_)
                | AnyNode::StmtAssert(_)
                | AnyNode::StmtImport(_)
                | AnyNode::StmtImportFrom(_)
                | AnyNode::StmtGlobal(_)
                | AnyNode::StmtNonlocal(_)
                | AnyNode::StmtExpr(_)
                | AnyNode::StmtPass(_)
                | AnyNode::StmtBreak(_)
                | AnyNode::StmtContinue(_)
                | AnyNode::StmtIpyEscapeCommand(_)
        )
    }
}

impl AnyNode {
    pub const fn is_expression(&self) -> bool {
        matches!(
            self,
            AnyNode::ExprBoolOp(_)
                | AnyNode::ExprNamed(_)
                | AnyNode::ExprBinOp(_)
                | AnyNode::ExprUnaryOp(_)
                | AnyNode::ExprLambda(_)
                | AnyNode::ExprIf(_)
                | AnyNode::ExprDict(_)
                | AnyNode::ExprSet(_)
                | AnyNode::ExprListComp(_)
                | AnyNode::ExprSetComp(_)
                | AnyNode::ExprDictComp(_)
                | AnyNode::ExprGenerator(_)
                | AnyNode::ExprAwait(_)
                | AnyNode::ExprYield(_)
                | AnyNode::ExprYieldFrom(_)
                | AnyNode::ExprCompare(_)
                | AnyNode::ExprCall(_)
                | AnyNode::ExprFString(_)
                | AnyNode::ExprStringLiteral(_)
                | AnyNode::ExprBytesLiteral(_)
                | AnyNode::ExprNumberLiteral(_)
                | AnyNode::ExprBooleanLiteral(_)
                | AnyNode::ExprNoneLiteral(_)
                | AnyNode::ExprEllipsisLiteral(_)
                | AnyNode::ExprAttribute(_)
                | AnyNode::ExprSubscript(_)
                | AnyNode::ExprStarred(_)
                | AnyNode::ExprName(_)
                | AnyNode::ExprList(_)
                | AnyNode::ExprTuple(_)
                | AnyNode::ExprSlice(_)
                | AnyNode::ExprIpyEscapeCommand(_)
        )
    }
}

impl AnyNode {
    pub const fn is_except_handler(&self) -> bool {
        matches!(self, AnyNode::ExceptHandlerExceptHandler(_))
    }
}

impl AnyNode {
    pub const fn is_f_string_element(&self) -> bool {
        matches!(
            self,
            AnyNode::FStringExpressionElement(_) | AnyNode::FStringLiteralElement(_)
        )
    }
}

impl AnyNode {
    pub const fn is_pattern(&self) -> bool {
        matches!(
            self,
            AnyNode::PatternMatchValue(_)
                | AnyNode::PatternMatchSingleton(_)
                | AnyNode::PatternMatchSequence(_)
                | AnyNode::PatternMatchMapping(_)
                | AnyNode::PatternMatchClass(_)
                | AnyNode::PatternMatchStar(_)
                | AnyNode::PatternMatchAs(_)
                | AnyNode::PatternMatchOr(_)
        )
    }
}

impl AnyNode {
    pub const fn is_type_param(&self) -> bool {
        matches!(
            self,
            AnyNode::TypeParamTypeVar(_)
                | AnyNode::TypeParamTypeVarTuple(_)
                | AnyNode::TypeParamParamSpec(_)
        )
    }
}

impl AnyNode {
    pub const fn as_ref(&self) -> AnyNodeRef {
        match self {
            AnyNode::ModModule(node) => AnyNodeRef::ModModule(node),
            AnyNode::ModExpression(node) => AnyNodeRef::ModExpression(node),
            AnyNode::StmtFunctionDef(node) => AnyNodeRef::StmtFunctionDef(node),
            AnyNode::StmtClassDef(node) => AnyNodeRef::StmtClassDef(node),
            AnyNode::StmtReturn(node) => AnyNodeRef::StmtReturn(node),
            AnyNode::StmtDelete(node) => AnyNodeRef::StmtDelete(node),
            AnyNode::StmtTypeAlias(node) => AnyNodeRef::StmtTypeAlias(node),
            AnyNode::StmtAssign(node) => AnyNodeRef::StmtAssign(node),
            AnyNode::StmtAugAssign(node) => AnyNodeRef::StmtAugAssign(node),
            AnyNode::StmtAnnAssign(node) => AnyNodeRef::StmtAnnAssign(node),
            AnyNode::StmtFor(node) => AnyNodeRef::StmtFor(node),
            AnyNode::StmtWhile(node) => AnyNodeRef::StmtWhile(node),
            AnyNode::StmtIf(node) => AnyNodeRef::StmtIf(node),
            AnyNode::StmtWith(node) => AnyNodeRef::StmtWith(node),
            AnyNode::StmtMatch(node) => AnyNodeRef::StmtMatch(node),
            AnyNode::StmtRaise(node) => AnyNodeRef::StmtRaise(node),
            AnyNode::StmtTry(node) => AnyNodeRef::StmtTry(node),
            AnyNode::StmtAssert(node) => AnyNodeRef::StmtAssert(node),
            AnyNode::StmtImport(node) => AnyNodeRef::StmtImport(node),
            AnyNode::StmtImportFrom(node) => AnyNodeRef::StmtImportFrom(node),
            AnyNode::StmtGlobal(node) => AnyNodeRef::StmtGlobal(node),
            AnyNode::StmtNonlocal(node) => AnyNodeRef::StmtNonlocal(node),
            AnyNode::StmtExpr(node) => AnyNodeRef::StmtExpr(node),
            AnyNode::StmtPass(node) => AnyNodeRef::StmtPass(node),
            AnyNode::StmtBreak(node) => AnyNodeRef::StmtBreak(node),
            AnyNode::StmtContinue(node) => AnyNodeRef::StmtContinue(node),
            AnyNode::StmtIpyEscapeCommand(node) => AnyNodeRef::StmtIpyEscapeCommand(node),
            AnyNode::ExprBoolOp(node) => AnyNodeRef::ExprBoolOp(node),
            AnyNode::ExprNamed(node) => AnyNodeRef::ExprNamed(node),
            AnyNode::ExprBinOp(node) => AnyNodeRef::ExprBinOp(node),
            AnyNode::ExprUnaryOp(node) => AnyNodeRef::ExprUnaryOp(node),
            AnyNode::ExprLambda(node) => AnyNodeRef::ExprLambda(node),
            AnyNode::ExprIf(node) => AnyNodeRef::ExprIf(node),
            AnyNode::ExprDict(node) => AnyNodeRef::ExprDict(node),
            AnyNode::ExprSet(node) => AnyNodeRef::ExprSet(node),
            AnyNode::ExprListComp(node) => AnyNodeRef::ExprListComp(node),
            AnyNode::ExprSetComp(node) => AnyNodeRef::ExprSetComp(node),
            AnyNode::ExprDictComp(node) => AnyNodeRef::ExprDictComp(node),
            AnyNode::ExprGenerator(node) => AnyNodeRef::ExprGenerator(node),
            AnyNode::ExprAwait(node) => AnyNodeRef::ExprAwait(node),
            AnyNode::ExprYield(node) => AnyNodeRef::ExprYield(node),
            AnyNode::ExprYieldFrom(node) => AnyNodeRef::ExprYieldFrom(node),
            AnyNode::ExprCompare(node) => AnyNodeRef::ExprCompare(node),
            AnyNode::ExprCall(node) => AnyNodeRef::ExprCall(node),
            AnyNode::ExprFString(node) => AnyNodeRef::ExprFString(node),
            AnyNode::ExprStringLiteral(node) => AnyNodeRef::ExprStringLiteral(node),
            AnyNode::ExprBytesLiteral(node) => AnyNodeRef::ExprBytesLiteral(node),
            AnyNode::ExprNumberLiteral(node) => AnyNodeRef::ExprNumberLiteral(node),
            AnyNode::ExprBooleanLiteral(node) => AnyNodeRef::ExprBooleanLiteral(node),
            AnyNode::ExprNoneLiteral(node) => AnyNodeRef::ExprNoneLiteral(node),
            AnyNode::ExprEllipsisLiteral(node) => AnyNodeRef::ExprEllipsisLiteral(node),
            AnyNode::ExprAttribute(node) => AnyNodeRef::ExprAttribute(node),
            AnyNode::ExprSubscript(node) => AnyNodeRef::ExprSubscript(node),
            AnyNode::ExprStarred(node) => AnyNodeRef::ExprStarred(node),
            AnyNode::ExprName(node) => AnyNodeRef::ExprName(node),
            AnyNode::ExprList(node) => AnyNodeRef::ExprList(node),
            AnyNode::ExprTuple(node) => AnyNodeRef::ExprTuple(node),
            AnyNode::ExprSlice(node) => AnyNodeRef::ExprSlice(node),
            AnyNode::ExprIpyEscapeCommand(node) => AnyNodeRef::ExprIpyEscapeCommand(node),
            AnyNode::ExceptHandlerExceptHandler(node) => {
                AnyNodeRef::ExceptHandlerExceptHandler(node)
            }
            AnyNode::FStringExpressionElement(node) => AnyNodeRef::FStringExpressionElement(node),
            AnyNode::FStringLiteralElement(node) => AnyNodeRef::FStringLiteralElement(node),
            AnyNode::PatternMatchValue(node) => AnyNodeRef::PatternMatchValue(node),
            AnyNode::PatternMatchSingleton(node) => AnyNodeRef::PatternMatchSingleton(node),
            AnyNode::PatternMatchSequence(node) => AnyNodeRef::PatternMatchSequence(node),
            AnyNode::PatternMatchMapping(node) => AnyNodeRef::PatternMatchMapping(node),
            AnyNode::PatternMatchClass(node) => AnyNodeRef::PatternMatchClass(node),
            AnyNode::PatternMatchStar(node) => AnyNodeRef::PatternMatchStar(node),
            AnyNode::PatternMatchAs(node) => AnyNodeRef::PatternMatchAs(node),
            AnyNode::PatternMatchOr(node) => AnyNodeRef::PatternMatchOr(node),
            AnyNode::TypeParamTypeVar(node) => AnyNodeRef::TypeParamTypeVar(node),
            AnyNode::TypeParamTypeVarTuple(node) => AnyNodeRef::TypeParamTypeVarTuple(node),
            AnyNode::TypeParamParamSpec(node) => AnyNodeRef::TypeParamParamSpec(node),
            AnyNode::FStringFormatSpec(node) => AnyNodeRef::FStringFormatSpec(node),
            AnyNode::PatternArguments(node) => AnyNodeRef::PatternArguments(node),
            AnyNode::PatternKeyword(node) => AnyNodeRef::PatternKeyword(node),
            AnyNode::Comprehension(node) => AnyNodeRef::Comprehension(node),
            AnyNode::Arguments(node) => AnyNodeRef::Arguments(node),
            AnyNode::Parameters(node) => AnyNodeRef::Parameters(node),
            AnyNode::Parameter(node) => AnyNodeRef::Parameter(node),
            AnyNode::ParameterWithDefault(node) => AnyNodeRef::ParameterWithDefault(node),
            AnyNode::Keyword(node) => AnyNodeRef::Keyword(node),
            AnyNode::Alias(node) => AnyNodeRef::Alias(node),
            AnyNode::WithItem(node) => AnyNodeRef::WithItem(node),
            AnyNode::MatchCase(node) => AnyNodeRef::MatchCase(node),
            AnyNode::Decorator(node) => AnyNodeRef::Decorator(node),
            AnyNode::ElifElseClause(node) => AnyNodeRef::ElifElseClause(node),
            AnyNode::TypeParams(node) => AnyNodeRef::TypeParams(node),
            AnyNode::FString(node) => AnyNodeRef::FString(node),
            AnyNode::StringLiteral(node) => AnyNodeRef::StringLiteral(node),
            AnyNode::BytesLiteral(node) => AnyNodeRef::BytesLiteral(node),
            AnyNode::Identifier(node) => AnyNodeRef::Identifier(node),
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

impl<'a> From<StatementRef<'a>> for AnyNodeRef<'a> {
    fn from(node: StatementRef<'a>) -> AnyNodeRef<'a> {
        match node {
            StatementRef::FunctionDef(node) => AnyNodeRef::StmtFunctionDef(node),
            StatementRef::ClassDef(node) => AnyNodeRef::StmtClassDef(node),
            StatementRef::Return(node) => AnyNodeRef::StmtReturn(node),
            StatementRef::Delete(node) => AnyNodeRef::StmtDelete(node),
            StatementRef::TypeAlias(node) => AnyNodeRef::StmtTypeAlias(node),
            StatementRef::Assign(node) => AnyNodeRef::StmtAssign(node),
            StatementRef::AugAssign(node) => AnyNodeRef::StmtAugAssign(node),
            StatementRef::AnnAssign(node) => AnyNodeRef::StmtAnnAssign(node),
            StatementRef::For(node) => AnyNodeRef::StmtFor(node),
            StatementRef::While(node) => AnyNodeRef::StmtWhile(node),
            StatementRef::If(node) => AnyNodeRef::StmtIf(node),
            StatementRef::With(node) => AnyNodeRef::StmtWith(node),
            StatementRef::Match(node) => AnyNodeRef::StmtMatch(node),
            StatementRef::Raise(node) => AnyNodeRef::StmtRaise(node),
            StatementRef::Try(node) => AnyNodeRef::StmtTry(node),
            StatementRef::Assert(node) => AnyNodeRef::StmtAssert(node),
            StatementRef::Import(node) => AnyNodeRef::StmtImport(node),
            StatementRef::ImportFrom(node) => AnyNodeRef::StmtImportFrom(node),
            StatementRef::Global(node) => AnyNodeRef::StmtGlobal(node),
            StatementRef::Nonlocal(node) => AnyNodeRef::StmtNonlocal(node),
            StatementRef::Expr(node) => AnyNodeRef::StmtExpr(node),
            StatementRef::Pass(node) => AnyNodeRef::StmtPass(node),
            StatementRef::Break(node) => AnyNodeRef::StmtBreak(node),
            StatementRef::Continue(node) => AnyNodeRef::StmtContinue(node),
            StatementRef::IpyEscapeCommand(node) => AnyNodeRef::StmtIpyEscapeCommand(node),
        }
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

impl<'a> From<ExpressionRef<'a>> for AnyNodeRef<'a> {
    fn from(node: ExpressionRef<'a>) -> AnyNodeRef<'a> {
        match node {
            ExpressionRef::BoolOp(node) => AnyNodeRef::ExprBoolOp(node),
            ExpressionRef::Named(node) => AnyNodeRef::ExprNamed(node),
            ExpressionRef::BinOp(node) => AnyNodeRef::ExprBinOp(node),
            ExpressionRef::UnaryOp(node) => AnyNodeRef::ExprUnaryOp(node),
            ExpressionRef::Lambda(node) => AnyNodeRef::ExprLambda(node),
            ExpressionRef::If(node) => AnyNodeRef::ExprIf(node),
            ExpressionRef::Dict(node) => AnyNodeRef::ExprDict(node),
            ExpressionRef::Set(node) => AnyNodeRef::ExprSet(node),
            ExpressionRef::ListComp(node) => AnyNodeRef::ExprListComp(node),
            ExpressionRef::SetComp(node) => AnyNodeRef::ExprSetComp(node),
            ExpressionRef::DictComp(node) => AnyNodeRef::ExprDictComp(node),
            ExpressionRef::Generator(node) => AnyNodeRef::ExprGenerator(node),
            ExpressionRef::Await(node) => AnyNodeRef::ExprAwait(node),
            ExpressionRef::Yield(node) => AnyNodeRef::ExprYield(node),
            ExpressionRef::YieldFrom(node) => AnyNodeRef::ExprYieldFrom(node),
            ExpressionRef::Compare(node) => AnyNodeRef::ExprCompare(node),
            ExpressionRef::Call(node) => AnyNodeRef::ExprCall(node),
            ExpressionRef::FString(node) => AnyNodeRef::ExprFString(node),
            ExpressionRef::StringLiteral(node) => AnyNodeRef::ExprStringLiteral(node),
            ExpressionRef::BytesLiteral(node) => AnyNodeRef::ExprBytesLiteral(node),
            ExpressionRef::NumberLiteral(node) => AnyNodeRef::ExprNumberLiteral(node),
            ExpressionRef::BooleanLiteral(node) => AnyNodeRef::ExprBooleanLiteral(node),
            ExpressionRef::NoneLiteral(node) => AnyNodeRef::ExprNoneLiteral(node),
            ExpressionRef::EllipsisLiteral(node) => AnyNodeRef::ExprEllipsisLiteral(node),
            ExpressionRef::Attribute(node) => AnyNodeRef::ExprAttribute(node),
            ExpressionRef::Subscript(node) => AnyNodeRef::ExprSubscript(node),
            ExpressionRef::Starred(node) => AnyNodeRef::ExprStarred(node),
            ExpressionRef::Name(node) => AnyNodeRef::ExprName(node),
            ExpressionRef::List(node) => AnyNodeRef::ExprList(node),
            ExpressionRef::Tuple(node) => AnyNodeRef::ExprTuple(node),
            ExpressionRef::Slice(node) => AnyNodeRef::ExprSlice(node),
            ExpressionRef::IpyEscapeCommand(node) => AnyNodeRef::ExprIpyEscapeCommand(node),
        }
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

impl<'a> From<&'a crate::ExceptHandlerExceptHandler> for AnyNodeRef<'a> {
    fn from(node: &'a crate::ExceptHandlerExceptHandler) -> AnyNodeRef<'a> {
        AnyNodeRef::ExceptHandlerExceptHandler(node)
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

impl AnyNode {
    pub const fn kind(&self) -> NodeKind {
        match self {
            AnyNode::ModModule(_) => NodeKind::ModModule,
            AnyNode::ModExpression(_) => NodeKind::ModExpression,
            AnyNode::StmtFunctionDef(_) => NodeKind::StmtFunctionDef,
            AnyNode::StmtClassDef(_) => NodeKind::StmtClassDef,
            AnyNode::StmtReturn(_) => NodeKind::StmtReturn,
            AnyNode::StmtDelete(_) => NodeKind::StmtDelete,
            AnyNode::StmtTypeAlias(_) => NodeKind::StmtTypeAlias,
            AnyNode::StmtAssign(_) => NodeKind::StmtAssign,
            AnyNode::StmtAugAssign(_) => NodeKind::StmtAugAssign,
            AnyNode::StmtAnnAssign(_) => NodeKind::StmtAnnAssign,
            AnyNode::StmtFor(_) => NodeKind::StmtFor,
            AnyNode::StmtWhile(_) => NodeKind::StmtWhile,
            AnyNode::StmtIf(_) => NodeKind::StmtIf,
            AnyNode::StmtWith(_) => NodeKind::StmtWith,
            AnyNode::StmtMatch(_) => NodeKind::StmtMatch,
            AnyNode::StmtRaise(_) => NodeKind::StmtRaise,
            AnyNode::StmtTry(_) => NodeKind::StmtTry,
            AnyNode::StmtAssert(_) => NodeKind::StmtAssert,
            AnyNode::StmtImport(_) => NodeKind::StmtImport,
            AnyNode::StmtImportFrom(_) => NodeKind::StmtImportFrom,
            AnyNode::StmtGlobal(_) => NodeKind::StmtGlobal,
            AnyNode::StmtNonlocal(_) => NodeKind::StmtNonlocal,
            AnyNode::StmtExpr(_) => NodeKind::StmtExpr,
            AnyNode::StmtPass(_) => NodeKind::StmtPass,
            AnyNode::StmtBreak(_) => NodeKind::StmtBreak,
            AnyNode::StmtContinue(_) => NodeKind::StmtContinue,
            AnyNode::StmtIpyEscapeCommand(_) => NodeKind::StmtIpyEscapeCommand,
            AnyNode::ExprBoolOp(_) => NodeKind::ExprBoolOp,
            AnyNode::ExprNamed(_) => NodeKind::ExprNamed,
            AnyNode::ExprBinOp(_) => NodeKind::ExprBinOp,
            AnyNode::ExprUnaryOp(_) => NodeKind::ExprUnaryOp,
            AnyNode::ExprLambda(_) => NodeKind::ExprLambda,
            AnyNode::ExprIf(_) => NodeKind::ExprIf,
            AnyNode::ExprDict(_) => NodeKind::ExprDict,
            AnyNode::ExprSet(_) => NodeKind::ExprSet,
            AnyNode::ExprListComp(_) => NodeKind::ExprListComp,
            AnyNode::ExprSetComp(_) => NodeKind::ExprSetComp,
            AnyNode::ExprDictComp(_) => NodeKind::ExprDictComp,
            AnyNode::ExprGenerator(_) => NodeKind::ExprGenerator,
            AnyNode::ExprAwait(_) => NodeKind::ExprAwait,
            AnyNode::ExprYield(_) => NodeKind::ExprYield,
            AnyNode::ExprYieldFrom(_) => NodeKind::ExprYieldFrom,
            AnyNode::ExprCompare(_) => NodeKind::ExprCompare,
            AnyNode::ExprCall(_) => NodeKind::ExprCall,
            AnyNode::ExprFString(_) => NodeKind::ExprFString,
            AnyNode::ExprStringLiteral(_) => NodeKind::ExprStringLiteral,
            AnyNode::ExprBytesLiteral(_) => NodeKind::ExprBytesLiteral,
            AnyNode::ExprNumberLiteral(_) => NodeKind::ExprNumberLiteral,
            AnyNode::ExprBooleanLiteral(_) => NodeKind::ExprBooleanLiteral,
            AnyNode::ExprNoneLiteral(_) => NodeKind::ExprNoneLiteral,
            AnyNode::ExprEllipsisLiteral(_) => NodeKind::ExprEllipsisLiteral,
            AnyNode::ExprAttribute(_) => NodeKind::ExprAttribute,
            AnyNode::ExprSubscript(_) => NodeKind::ExprSubscript,
            AnyNode::ExprStarred(_) => NodeKind::ExprStarred,
            AnyNode::ExprName(_) => NodeKind::ExprName,
            AnyNode::ExprList(_) => NodeKind::ExprList,
            AnyNode::ExprTuple(_) => NodeKind::ExprTuple,
            AnyNode::ExprSlice(_) => NodeKind::ExprSlice,
            AnyNode::ExprIpyEscapeCommand(_) => NodeKind::ExprIpyEscapeCommand,
            AnyNode::ExceptHandlerExceptHandler(_) => NodeKind::ExceptHandlerExceptHandler,
            AnyNode::FStringExpressionElement(_) => NodeKind::FStringExpressionElement,
            AnyNode::FStringLiteralElement(_) => NodeKind::FStringLiteralElement,
            AnyNode::PatternMatchValue(_) => NodeKind::PatternMatchValue,
            AnyNode::PatternMatchSingleton(_) => NodeKind::PatternMatchSingleton,
            AnyNode::PatternMatchSequence(_) => NodeKind::PatternMatchSequence,
            AnyNode::PatternMatchMapping(_) => NodeKind::PatternMatchMapping,
            AnyNode::PatternMatchClass(_) => NodeKind::PatternMatchClass,
            AnyNode::PatternMatchStar(_) => NodeKind::PatternMatchStar,
            AnyNode::PatternMatchAs(_) => NodeKind::PatternMatchAs,
            AnyNode::PatternMatchOr(_) => NodeKind::PatternMatchOr,
            AnyNode::TypeParamTypeVar(_) => NodeKind::TypeParamTypeVar,
            AnyNode::TypeParamTypeVarTuple(_) => NodeKind::TypeParamTypeVarTuple,
            AnyNode::TypeParamParamSpec(_) => NodeKind::TypeParamParamSpec,
            AnyNode::FStringFormatSpec(_) => NodeKind::FStringFormatSpec,
            AnyNode::PatternArguments(_) => NodeKind::PatternArguments,
            AnyNode::PatternKeyword(_) => NodeKind::PatternKeyword,
            AnyNode::Comprehension(_) => NodeKind::Comprehension,
            AnyNode::Arguments(_) => NodeKind::Arguments,
            AnyNode::Parameters(_) => NodeKind::Parameters,
            AnyNode::Parameter(_) => NodeKind::Parameter,
            AnyNode::ParameterWithDefault(_) => NodeKind::ParameterWithDefault,
            AnyNode::Keyword(_) => NodeKind::Keyword,
            AnyNode::Alias(_) => NodeKind::Alias,
            AnyNode::WithItem(_) => NodeKind::WithItem,
            AnyNode::MatchCase(_) => NodeKind::MatchCase,
            AnyNode::Decorator(_) => NodeKind::Decorator,
            AnyNode::ElifElseClause(_) => NodeKind::ElifElseClause,
            AnyNode::TypeParams(_) => NodeKind::TypeParams,
            AnyNode::FString(_) => NodeKind::FString,
            AnyNode::StringLiteral(_) => NodeKind::StringLiteral,
            AnyNode::BytesLiteral(_) => NodeKind::BytesLiteral,
            AnyNode::Identifier(_) => NodeKind::Identifier,
        }
    }
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
