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

impl crate::AstNode for crate::ModModule {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ModModule(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ModModule(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ModModule)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ModExpression {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ModExpression(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ModExpression(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ModExpression)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::StmtFunctionDef {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtFunctionDef(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtFunctionDef(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::StmtFunctionDef)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::StmtClassDef {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtClassDef(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtClassDef(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::StmtClassDef)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::StmtReturn {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtReturn(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtReturn(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::StmtReturn)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::StmtDelete {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtDelete(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtDelete(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::StmtDelete)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::StmtTypeAlias {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtTypeAlias(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtTypeAlias(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::StmtTypeAlias)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::StmtAssign {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtAssign(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtAssign(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::StmtAssign)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::StmtAugAssign {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtAugAssign(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtAugAssign(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::StmtAugAssign)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::StmtAnnAssign {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtAnnAssign(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtAnnAssign(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::StmtAnnAssign)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::StmtFor {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtFor(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtFor(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::StmtFor)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::StmtWhile {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtWhile(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtWhile(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::StmtWhile)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::StmtIf {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtIf(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtIf(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::StmtIf)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::StmtWith {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtWith(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtWith(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::StmtWith)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::StmtMatch {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtMatch(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtMatch(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::StmtMatch)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::StmtRaise {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtRaise(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtRaise(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::StmtRaise)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::StmtTry {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtTry(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtTry(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::StmtTry)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::StmtAssert {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtAssert(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtAssert(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::StmtAssert)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::StmtImport {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtImport(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtImport(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::StmtImport)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::StmtImportFrom {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtImportFrom(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtImportFrom(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::StmtImportFrom)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::StmtGlobal {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtGlobal(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtGlobal(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::StmtGlobal)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::StmtNonlocal {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtNonlocal(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtNonlocal(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::StmtNonlocal)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::StmtExpr {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtExpr(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtExpr(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::StmtExpr)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::StmtPass {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtPass(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtPass(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::StmtPass)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::StmtBreak {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtBreak(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtBreak(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::StmtBreak)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::StmtContinue {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtContinue(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtContinue(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::StmtContinue)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::StmtIpyEscapeCommand {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StmtIpyEscapeCommand(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StmtIpyEscapeCommand(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::StmtIpyEscapeCommand)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprBoolOp {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprBoolOp(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprBoolOp(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprBoolOp)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprNamed {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprNamed(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprNamed(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprNamed)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprBinOp {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprBinOp(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprBinOp(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprBinOp)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprUnaryOp {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprUnaryOp(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprUnaryOp(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprUnaryOp)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprLambda {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprLambda(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprLambda(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprLambda)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprIf {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprIf(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprIf(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprIf)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprDict {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprDict(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprDict(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprDict)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprSet {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprSet(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprSet(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprSet)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprListComp {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprListComp(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprListComp(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprListComp)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprSetComp {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprSetComp(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprSetComp(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprSetComp)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprDictComp {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprDictComp(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprDictComp(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprDictComp)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprGenerator {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprGenerator(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprGenerator(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprGenerator)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprAwait {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprAwait(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprAwait(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprAwait)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprYield {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprYield(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprYield(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprYield)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprYieldFrom {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprYieldFrom(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprYieldFrom(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprYieldFrom)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprCompare {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprCompare(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprCompare(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprCompare)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprCall {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprCall(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprCall(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprCall)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprFString {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprFString(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprFString(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprFString)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprStringLiteral {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprStringLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprStringLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprStringLiteral)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprBytesLiteral {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprBytesLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprBytesLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprBytesLiteral)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprNumberLiteral {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprNumberLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprNumberLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprNumberLiteral)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprBooleanLiteral {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprBooleanLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprBooleanLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprBooleanLiteral)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprNoneLiteral {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprNoneLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprNoneLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprNoneLiteral)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprEllipsisLiteral {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprEllipsisLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprEllipsisLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprEllipsisLiteral)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprAttribute {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprAttribute(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprAttribute(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprAttribute)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprSubscript {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprSubscript(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprSubscript(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprSubscript)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprStarred {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprStarred(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprStarred(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprStarred)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprName {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprName(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprName(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprName)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprList {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprList(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprList(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprList)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprTuple {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprTuple(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprTuple(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprTuple)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprSlice {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprSlice(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprSlice(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprSlice)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExprIpyEscapeCommand {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExprIpyEscapeCommand(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExprIpyEscapeCommand(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExprIpyEscapeCommand)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ExceptHandlerExceptHandler {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ExceptHandlerExceptHandler(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ExceptHandlerExceptHandler(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ExceptHandlerExceptHandler)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::FStringExpressionElement {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::FStringExpressionElement(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::FStringExpressionElement(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::FStringExpressionElement)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::FStringLiteralElement {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::FStringLiteralElement(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::FStringLiteralElement(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::FStringLiteralElement)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::PatternMatchValue {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::PatternMatchValue(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::PatternMatchValue(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::PatternMatchValue)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::PatternMatchSingleton {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::PatternMatchSingleton(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::PatternMatchSingleton(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::PatternMatchSingleton)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::PatternMatchSequence {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::PatternMatchSequence(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::PatternMatchSequence(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::PatternMatchSequence)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::PatternMatchMapping {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::PatternMatchMapping(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::PatternMatchMapping(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::PatternMatchMapping)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::PatternMatchClass {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::PatternMatchClass(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::PatternMatchClass(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::PatternMatchClass)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::PatternMatchStar {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::PatternMatchStar(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::PatternMatchStar(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::PatternMatchStar)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::PatternMatchAs {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::PatternMatchAs(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::PatternMatchAs(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::PatternMatchAs)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::PatternMatchOr {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::PatternMatchOr(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::PatternMatchOr(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::PatternMatchOr)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::TypeParamTypeVar {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::TypeParamTypeVar(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::TypeParamTypeVar(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::TypeParamTypeVar)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::TypeParamTypeVarTuple {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::TypeParamTypeVarTuple(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::TypeParamTypeVarTuple(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::TypeParamTypeVarTuple)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::TypeParamParamSpec {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::TypeParamParamSpec(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::TypeParamParamSpec(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::TypeParamParamSpec)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::FStringFormatSpec {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::FStringFormatSpec(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::FStringFormatSpec(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::FStringFormatSpec)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::PatternArguments {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::PatternArguments(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::PatternArguments(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::PatternArguments)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::PatternKeyword {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::PatternKeyword(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::PatternKeyword(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::PatternKeyword)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::Comprehension {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::Comprehension(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::Comprehension(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::Comprehension)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::Arguments {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::Arguments(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::Arguments(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::Arguments)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::Parameters {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::Parameters(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::Parameters(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::Parameters)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::Parameter {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::Parameter(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::Parameter(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::Parameter)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ParameterWithDefault {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ParameterWithDefault(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ParameterWithDefault(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ParameterWithDefault)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::Keyword {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::Keyword(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::Keyword(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::Keyword)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::Alias {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::Alias(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::Alias(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::Alias)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::WithItem {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::WithItem(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::WithItem(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::WithItem)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::MatchCase {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::MatchCase(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::MatchCase(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::MatchCase)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::Decorator {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::Decorator(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::Decorator(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::Decorator)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::ElifElseClause {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::ElifElseClause(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::ElifElseClause(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::ElifElseClause)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::TypeParams {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::TypeParams(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::TypeParams(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::TypeParams)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::FString {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::FString(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::FString(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::FString)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::StringLiteral {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::StringLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::StringLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::StringLiteral)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::BytesLiteral {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::BytesLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::BytesLiteral(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::BytesLiteral)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}

impl crate::AstNode for crate::Identifier {
    type Ref<'a> = &'a Self;

    fn cast(kind: AnyNode) -> Option<Self>
    where
        Self: Sized,
    {
        if let AnyNode::Identifier(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {
        if let AnyNodeRef::Identifier(node) = kind {
            Some(node)
        } else {
            None
        }
    }

    fn can_cast(kind: crate::NodeKind) -> bool {
        matches!(kind, crate::NodeKind::Identifier)
    }

    fn as_any_node_ref(&self) -> AnyNodeRef {
        AnyNodeRef::from(self)
    }

    fn into_any_node(self) -> AnyNode {
        AnyNode::from(self)
    }
}
