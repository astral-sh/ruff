use ruff_text_size::Ranged;

use crate::visitor::source_order::SourceOrderVisitor;
use crate::{
    self as ast, Alias, AnyNodeRef, AnyParameterRef, ArgOrKeyword, MatchCase, PatternArguments,
    PatternKeyword,
};

impl ast::ElifElseClause {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ElifElseClause {
            range: _,
            test,
            body,
        } = self;
        if let Some(test) = test {
            visitor.visit_expr(test);
        }
        visitor.visit_body(body);
    }
}

impl ast::ExprDict {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprDict { items, range: _ } = self;

        for ast::DictItem { key, value } in items {
            if let Some(key) = key {
                visitor.visit_expr(key);
            }
            visitor.visit_expr(value);
        }
    }
}

impl ast::ExprBoolOp {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprBoolOp {
            op,
            values,
            range: _,
        } = self;
        match values.as_slice() {
            [left, rest @ ..] => {
                visitor.visit_expr(left);
                visitor.visit_bool_op(op);
                for expr in rest {
                    visitor.visit_expr(expr);
                }
            }
            [] => {
                visitor.visit_bool_op(op);
            }
        }
    }
}

impl ast::ExprCompare {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprCompare {
            left,
            ops,
            comparators,
            range: _,
        } = self;

        visitor.visit_expr(left);

        for (op, comparator) in ops.iter().zip(comparators) {
            visitor.visit_cmp_op(op);
            visitor.visit_expr(comparator);
        }
    }
}

impl ast::FStringFormatSpec {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        for element in &self.elements {
            visitor.visit_f_string_element(element);
        }
    }
}

impl ast::FStringExpressionElement {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::FStringExpressionElement {
            expression,
            format_spec,
            ..
        } = self;
        visitor.visit_expr(expression);

        if let Some(format_spec) = format_spec {
            for spec_part in &format_spec.elements {
                visitor.visit_f_string_element(spec_part);
            }
        }
    }
}

impl ast::FStringLiteralElement {
    pub(crate) fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::FStringLiteralElement { range: _, value: _ } = self;
    }
}

impl ast::ExprFString {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprFString { value, range: _ } = self;

        for f_string_part in value {
            match f_string_part {
                ast::FStringPart::Literal(string_literal) => {
                    visitor.visit_string_literal(string_literal);
                }
                ast::FStringPart::FString(f_string) => {
                    visitor.visit_f_string(f_string);
                }
            }
        }
    }
}

impl ast::ExprStringLiteral {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprStringLiteral { value, range: _ } = self;

        for string_literal in value {
            visitor.visit_string_literal(string_literal);
        }
    }
}

impl ast::ExprBytesLiteral {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprBytesLiteral { value, range: _ } = self;

        for bytes_literal in value {
            visitor.visit_bytes_literal(bytes_literal);
        }
    }
}

impl ast::ExceptHandlerExceptHandler {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExceptHandlerExceptHandler {
            range: _,
            type_,
            name,
            body,
        } = self;
        if let Some(expr) = type_ {
            visitor.visit_expr(expr);
        }

        if let Some(name) = name {
            visitor.visit_identifier(name);
        }

        visitor.visit_body(body);
    }
}

impl ast::PatternMatchValue {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchValue { value, range: _ } = self;
        visitor.visit_expr(value);
    }
}

impl ast::PatternMatchSingleton {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchSingleton { value, range: _ } = self;
        visitor.visit_singleton(value);
    }
}

impl ast::PatternMatchSequence {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchSequence { patterns, range: _ } = self;
        for pattern in patterns {
            visitor.visit_pattern(pattern);
        }
    }
}

impl ast::PatternMatchMapping {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchMapping {
            keys,
            patterns,
            rest,
            range: _,
        } = self;

        let mut rest = rest.as_ref();

        for (key, pattern) in keys.iter().zip(patterns) {
            if let Some(rest_identifier) = rest {
                if rest_identifier.start() < key.start() {
                    visitor.visit_identifier(rest_identifier);
                    rest = None;
                }
            }
            visitor.visit_expr(key);
            visitor.visit_pattern(pattern);
        }

        if let Some(rest) = rest {
            visitor.visit_identifier(rest);
        }
    }
}

impl ast::PatternMatchClass {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchClass {
            cls,
            arguments: parameters,
            range: _,
        } = self;
        visitor.visit_expr(cls);
        visitor.visit_pattern_arguments(parameters);
    }
}

impl ast::PatternMatchStar {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchStar { range: _, name } = self;

        if let Some(name) = name {
            visitor.visit_identifier(name);
        }
    }
}

impl ast::PatternMatchAs {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchAs {
            pattern,
            range: _,
            name,
        } = self;
        if let Some(pattern) = pattern {
            visitor.visit_pattern(pattern);
        }

        if let Some(name) = name {
            visitor.visit_identifier(name);
        }
    }
}

impl ast::PatternMatchOr {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchOr { patterns, range: _ } = self;
        for pattern in patterns {
            visitor.visit_pattern(pattern);
        }
    }
}

impl ast::PatternArguments {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let PatternArguments {
            range: _,
            patterns,
            keywords,
        } = self;

        for pattern in patterns {
            visitor.visit_pattern(pattern);
        }

        for keyword in keywords {
            visitor.visit_pattern_keyword(keyword);
        }
    }
}

impl ast::PatternKeyword {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let PatternKeyword {
            range: _,
            attr,
            pattern,
        } = self;

        visitor.visit_identifier(attr);
        visitor.visit_pattern(pattern);
    }
}

impl ast::Comprehension {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::Comprehension {
            range: _,
            target,
            iter,
            ifs,
            is_async: _,
        } = self;
        visitor.visit_expr(target);
        visitor.visit_expr(iter);

        for expr in ifs {
            visitor.visit_expr(expr);
        }
    }
}

impl ast::Arguments {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        for arg_or_keyword in self.arguments_source_order() {
            match arg_or_keyword {
                ArgOrKeyword::Arg(arg) => visitor.visit_expr(arg),
                ArgOrKeyword::Keyword(keyword) => visitor.visit_keyword(keyword),
            }
        }
    }
}

impl ast::Parameters {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        for parameter in self {
            match parameter {
                AnyParameterRef::NonVariadic(parameter_with_default) => {
                    visitor.visit_parameter_with_default(parameter_with_default);
                }
                AnyParameterRef::Variadic(parameter) => visitor.visit_parameter(parameter),
            }
        }
    }
}

impl ast::Parameter {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::Parameter {
            range: _,
            name,
            annotation,
        } = self;

        visitor.visit_identifier(name);
        if let Some(expr) = annotation {
            visitor.visit_annotation(expr);
        }
    }
}

impl ast::ParameterWithDefault {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ParameterWithDefault {
            range: _,
            parameter,
            default,
        } = self;
        visitor.visit_parameter(parameter);
        if let Some(expr) = default {
            visitor.visit_expr(expr);
        }
    }
}

impl ast::Keyword {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::Keyword {
            range: _,
            arg,
            value,
        } = self;

        if let Some(arg) = arg {
            visitor.visit_identifier(arg);
        }
        visitor.visit_expr(value);
    }
}

impl Alias {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::Alias {
            range: _,
            name,
            asname,
        } = self;

        visitor.visit_identifier(name);
        if let Some(asname) = asname {
            visitor.visit_identifier(asname);
        }
    }
}

impl ast::WithItem {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::WithItem {
            range: _,
            context_expr,
            optional_vars,
        } = self;

        visitor.visit_expr(context_expr);

        if let Some(expr) = optional_vars {
            visitor.visit_expr(expr);
        }
    }
}

impl ast::MatchCase {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::MatchCase {
            range: _,
            pattern,
            guard,
            body,
        } = self;

        visitor.visit_pattern(pattern);
        if let Some(expr) = guard {
            visitor.visit_expr(expr);
        }
        visitor.visit_body(body);
    }
}

impl ast::Decorator {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::Decorator {
            range: _,
            expression,
        } = self;

        visitor.visit_expr(expression);
    }
}

impl ast::TypeParams {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::TypeParams {
            range: _,
            type_params,
        } = self;

        for type_param in type_params {
            visitor.visit_type_param(type_param);
        }
    }
}

impl ast::TypeParamTypeVar {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::TypeParamTypeVar {
            bound,
            default,
            name,
            range: _,
        } = self;

        visitor.visit_identifier(name);
        if let Some(expr) = bound {
            visitor.visit_expr(expr);
        }
        if let Some(expr) = default {
            visitor.visit_expr(expr);
        }
    }
}

impl ast::TypeParamTypeVarTuple {
    #[inline]
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::TypeParamTypeVarTuple {
            range: _,
            name,
            default,
        } = self;
        visitor.visit_identifier(name);
        if let Some(expr) = default {
            visitor.visit_expr(expr);
        }
    }
}

impl ast::TypeParamParamSpec {
    #[inline]
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::TypeParamParamSpec {
            range: _,
            name,
            default,
        } = self;
        visitor.visit_identifier(name);
        if let Some(expr) = default {
            visitor.visit_expr(expr);
        }
    }
}

impl ast::FString {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::FString {
            elements,
            range: _,
            flags: _,
        } = self;

        for fstring_element in elements {
            visitor.visit_f_string_element(fstring_element);
        }
    }
}

impl ast::StringLiteral {
    #[inline]
    pub(crate) fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StringLiteral {
            range: _,
            value: _,
            flags: _,
        } = self;
    }
}

impl ast::BytesLiteral {
    #[inline]
    pub(crate) fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::BytesLiteral {
            range: _,
            value: _,
            flags: _,
        } = self;
    }
}

impl ast::Identifier {
    #[inline]
    pub(crate) fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::Identifier { range: _, id: _ } = self;
    }
}

impl<'a> AnyNodeRef<'a> {
    /// Compares two any node refs by their pointers (referential equality).
    pub fn ptr_eq(self, other: AnyNodeRef) -> bool {
        self.as_ptr().eq(&other.as_ptr()) && self.kind() == other.kind()
    }

    /// In our AST, only some alternative branches are represented as a node. This has historical
    /// reasons, e.g. we added a node for elif/else in if statements which was not originally
    /// present in the parser.
    pub const fn is_alternative_branch_with_node(self) -> bool {
        matches!(
            self,
            AnyNodeRef::ExceptHandlerExceptHandler(_) | AnyNodeRef::ElifElseClause(_)
        )
    }

    /// The last child of the last branch, if the node has multiple branches.
    pub fn last_child_in_body(&self) -> Option<AnyNodeRef<'a>> {
        let body = match self {
            AnyNodeRef::StmtFunctionDef(ast::StmtFunctionDef { body, .. })
            | AnyNodeRef::StmtClassDef(ast::StmtClassDef { body, .. })
            | AnyNodeRef::StmtWith(ast::StmtWith { body, .. })
            | AnyNodeRef::MatchCase(MatchCase { body, .. })
            | AnyNodeRef::ExceptHandlerExceptHandler(ast::ExceptHandlerExceptHandler {
                body,
                ..
            })
            | AnyNodeRef::ElifElseClause(ast::ElifElseClause { body, .. }) => body,
            AnyNodeRef::StmtIf(ast::StmtIf {
                body,
                elif_else_clauses,
                ..
            }) => elif_else_clauses.last().map_or(body, |clause| &clause.body),

            AnyNodeRef::StmtFor(ast::StmtFor { body, orelse, .. })
            | AnyNodeRef::StmtWhile(ast::StmtWhile { body, orelse, .. }) => {
                if orelse.is_empty() {
                    body
                } else {
                    orelse
                }
            }

            AnyNodeRef::StmtMatch(ast::StmtMatch { cases, .. }) => {
                return cases.last().map(AnyNodeRef::from);
            }

            AnyNodeRef::StmtTry(ast::StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                ..
            }) => {
                if finalbody.is_empty() {
                    if orelse.is_empty() {
                        if handlers.is_empty() {
                            body
                        } else {
                            return handlers.last().map(AnyNodeRef::from);
                        }
                    } else {
                        orelse
                    }
                } else {
                    finalbody
                }
            }

            // Not a node that contains an indented child node.
            _ => return None,
        };

        body.last().map(AnyNodeRef::from)
    }

    /// Check if the given statement is the first statement after the colon of a branch, be it in if
    /// statements, for statements, after each part of a try-except-else-finally or function/class
    /// definitions.
    ///
    ///
    /// ```python
    /// if True:    <- has body
    ///     a       <- first statement
    ///     b
    /// elif b:     <- has body
    ///     c       <- first statement
    ///     d
    /// else:       <- has body
    ///     e       <- first statement
    ///     f
    ///
    /// class:      <- has body
    ///     a: int  <- first statement
    ///     b: int
    ///
    /// ```
    ///
    /// For nodes with multiple bodies, we check all bodies that don't have their own node. For
    /// try-except-else-finally, each except branch has it's own node, so for the `StmtTry`, we check
    /// the `try:`, `else:` and `finally:`, bodies, while `ExceptHandlerExceptHandler` has it's own
    /// check. For for-else and while-else, we check both branches for the whole statement.
    ///
    /// ```python
    /// try:        <- has body (a)
    ///     6/8     <- first statement (a)
    ///     1/0
    /// except:     <- has body (b)
    ///     a       <- first statement (b)
    ///     b
    /// else:
    ///     c       <- first statement (a)
    ///     d
    /// finally:
    ///     e       <- first statement (a)
    ///     f
    /// ```
    pub fn is_first_statement_in_body(&self, body: AnyNodeRef) -> bool {
        match body {
            AnyNodeRef::StmtFor(ast::StmtFor { body, orelse, .. })
            | AnyNodeRef::StmtWhile(ast::StmtWhile { body, orelse, .. }) => {
                are_same_optional(*self, body.first()) || are_same_optional(*self, orelse.first())
            }

            AnyNodeRef::StmtTry(ast::StmtTry {
                body,
                orelse,
                finalbody,
                ..
            }) => {
                are_same_optional(*self, body.first())
                    || are_same_optional(*self, orelse.first())
                    || are_same_optional(*self, finalbody.first())
            }

            AnyNodeRef::StmtIf(ast::StmtIf { body, .. })
            | AnyNodeRef::ElifElseClause(ast::ElifElseClause { body, .. })
            | AnyNodeRef::StmtWith(ast::StmtWith { body, .. })
            | AnyNodeRef::ExceptHandlerExceptHandler(ast::ExceptHandlerExceptHandler {
                body,
                ..
            })
            | AnyNodeRef::MatchCase(MatchCase { body, .. })
            | AnyNodeRef::StmtFunctionDef(ast::StmtFunctionDef { body, .. })
            | AnyNodeRef::StmtClassDef(ast::StmtClassDef { body, .. }) => {
                are_same_optional(*self, body.first())
            }

            AnyNodeRef::StmtMatch(ast::StmtMatch { cases, .. }) => {
                are_same_optional(*self, cases.first())
            }

            _ => false,
        }
    }

    /// Returns `true` if `statement` is the first statement in an alternate `body` (e.g. the else of an if statement)
    pub fn is_first_statement_in_alternate_body(&self, body: AnyNodeRef) -> bool {
        match body {
            AnyNodeRef::StmtFor(ast::StmtFor { orelse, .. })
            | AnyNodeRef::StmtWhile(ast::StmtWhile { orelse, .. }) => {
                are_same_optional(*self, orelse.first())
            }

            AnyNodeRef::StmtTry(ast::StmtTry {
                handlers,
                orelse,
                finalbody,
                ..
            }) => {
                are_same_optional(*self, handlers.first())
                    || are_same_optional(*self, orelse.first())
                    || are_same_optional(*self, finalbody.first())
            }

            AnyNodeRef::StmtIf(ast::StmtIf {
                elif_else_clauses, ..
            }) => are_same_optional(*self, elif_else_clauses.first()),
            _ => false,
        }
    }
}

/// Returns `true` if `right` is `Some` and `left` and `right` are referentially equal.
fn are_same_optional<'a, T>(left: AnyNodeRef, right: Option<T>) -> bool
where
    T: Into<AnyNodeRef<'a>>,
{
    right.is_some_and(|right| left.ptr_eq(right.into()))
}
