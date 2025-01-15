use crate::visitor::source_order::SourceOrderVisitor;
use crate::{
    self as ast, Alias, AnyNodeRef, AnyParameterRef, ArgOrKeyword, MatchCase, Node,
    PatternArguments, PatternKeyword,
};

impl<'a> Node<'a, &'a ast::ModModule> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ModModule { body, range: _ } = self.as_ref();
        visitor.visit_body(body);
    }
}

impl<'a> Node<'a, &'a ast::ModExpression> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ModExpression { body, range: _ } = self.as_ref();
        visitor.visit_expr(body);
    }
}

impl<'a> Node<'a, &'a ast::StmtFunctionDef> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtFunctionDef {
            parameters,
            body,
            decorator_list,
            returns,
            type_params,
            ..
        } = self.as_ref();

        for decorator in decorator_list {
            visitor.visit_decorator(decorator);
        }

        if let Some(type_params) = type_params {
            visitor.visit_type_params(type_params);
        }

        visitor.visit_parameters(parameters);

        if let Some(expr) = returns {
            visitor.visit_annotation(expr);
        }

        visitor.visit_body(body);
    }
}

impl<'a> Node<'a, &'a ast::StmtClassDef> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtClassDef {
            arguments,
            body,
            decorator_list,
            type_params,
            ..
        } = self.as_ref();

        for decorator in decorator_list {
            visitor.visit_decorator(decorator);
        }

        if let Some(type_params) = type_params {
            visitor.visit_type_params(type_params);
        }

        if let Some(arguments) = arguments {
            visitor.visit_arguments(arguments);
        }

        visitor.visit_body(body);
    }
}

impl<'a> Node<'a, &'a ast::StmtReturn> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtReturn { value, range: _ } = self.as_ref();
        if let Some(expr) = value {
            visitor.visit_expr(expr);
        }
    }
}

impl<'a> Node<'a, &'a ast::StmtDelete> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtDelete { targets, range: _ } = self.as_ref();
        for expr in targets {
            visitor.visit_expr(expr);
        }
    }
}

impl<'a> Node<'a, &'a ast::StmtTypeAlias> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtTypeAlias {
            range: _,
            name,
            type_params,
            value,
        } = self.as_ref();

        visitor.visit_expr(name);
        if let Some(type_params) = type_params {
            visitor.visit_type_params(type_params);
        }
        visitor.visit_expr(value);
    }
}

impl<'a> Node<'a, &'a ast::StmtAssign> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtAssign {
            targets,
            value,
            range: _,
        } = self.as_ref();

        for expr in targets {
            visitor.visit_expr(expr);
        }

        visitor.visit_expr(value);
    }
}

impl<'a> Node<'a, &'a ast::StmtAugAssign> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtAugAssign {
            target,
            op,
            value,
            range: _,
        } = self.as_ref();

        visitor.visit_expr(target);
        visitor.visit_operator(op);
        visitor.visit_expr(value);
    }
}

impl<'a> Node<'a, &'a ast::StmtAnnAssign> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtAnnAssign {
            target,
            annotation,
            value,
            range: _,
            simple: _,
        } = self.as_ref();

        visitor.visit_expr(target);
        visitor.visit_annotation(annotation);
        if let Some(expr) = value {
            visitor.visit_expr(expr);
        }
    }
}

impl<'a> Node<'a, &'a ast::StmtFor> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtFor {
            target,
            iter,
            body,
            orelse,
            ..
        } = self.as_ref();

        visitor.visit_expr(target);
        visitor.visit_expr(iter);
        visitor.visit_body(body);
        visitor.visit_body(orelse);
    }
}

impl<'a> Node<'a, &'a ast::StmtWhile> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtWhile {
            test,
            body,
            orelse,
            range: _,
        } = self.as_ref();

        visitor.visit_expr(test);
        visitor.visit_body(body);
        visitor.visit_body(orelse);
    }
}

impl<'a> Node<'a, &'a ast::StmtIf> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtIf {
            test,
            body,
            elif_else_clauses,
            range: _,
        } = self.as_ref();

        visitor.visit_expr(test);
        visitor.visit_body(body);
        for clause in elif_else_clauses {
            visitor.visit_elif_else_clause(clause);
        }
    }
}

impl<'a> Node<'a, &'a ast::ElifElseClause> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ElifElseClause {
            range: _,
            test,
            body,
        } = self.as_ref();
        if let Some(test) = test {
            visitor.visit_expr(test);
        }
        visitor.visit_body(body);
    }
}

impl<'a> Node<'a, &'a ast::StmtWith> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtWith {
            items,
            body,
            is_async: _,
            range: _,
        } = self.as_ref();

        for with_item in items {
            visitor.visit_with_item(with_item);
        }
        visitor.visit_body(body);
    }
}

impl<'a> Node<'a, &'a ast::StmtMatch> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtMatch {
            subject,
            cases,
            range: _,
        } = self.as_ref();

        visitor.visit_expr(subject);
        for match_case in cases {
            visitor.visit_match_case(match_case);
        }
    }
}

impl<'a> Node<'a, &'a ast::StmtRaise> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtRaise {
            exc,
            cause,
            range: _,
        } = self.as_ref();

        if let Some(expr) = exc {
            visitor.visit_expr(expr);
        };
        if let Some(expr) = cause {
            visitor.visit_expr(expr);
        };
    }
}

impl<'a> Node<'a, &'a ast::StmtTry> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
            is_star: _,
            range: _,
        } = self.as_ref();

        visitor.visit_body(body);
        for except_handler in handlers {
            visitor.visit_except_handler(except_handler);
        }
        visitor.visit_body(orelse);
        visitor.visit_body(finalbody);
    }
}

impl<'a> Node<'a, &'a ast::StmtAssert> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtAssert {
            test,
            msg,
            range: _,
        } = self.as_ref();
        visitor.visit_expr(test);
        if let Some(expr) = msg {
            visitor.visit_expr(expr);
        }
    }
}

impl<'a> Node<'a, &'a ast::StmtImport> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtImport { names, range: _ } = self.as_ref();

        for alias in names {
            visitor.visit_alias(alias);
        }
    }
}

impl<'a> Node<'a, &'a ast::StmtImportFrom> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtImportFrom {
            range: _,
            module: _,
            names,
            level: _,
        } = self.as_ref();

        for alias in names {
            visitor.visit_alias(alias);
        }
    }
}

impl<'a> Node<'a, &'a ast::StmtGlobal> {
    #[inline]
    pub(crate) fn visit_source_order<V>(self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtGlobal { range: _, names: _ } = self.as_ref();
    }
}

impl<'a> Node<'a, &'a ast::StmtNonlocal> {
    #[inline]
    pub(crate) fn visit_source_order<V>(self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtNonlocal { range: _, names: _ } = self.as_ref();
    }
}

impl<'a> Node<'a, &'a ast::StmtExpr> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtExpr { value, range: _ } = self.as_ref();
        visitor.visit_expr(value);
    }
}

impl<'a> Node<'a, &'a ast::StmtPass> {
    #[inline]
    pub(crate) fn visit_source_order<V>(self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtPass { range: _ } = self.as_ref();
    }
}

impl<'a> Node<'a, &'a ast::StmtBreak> {
    #[inline]
    pub(crate) fn visit_source_order<V>(self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtBreak { range: _ } = self.as_ref();
    }
}

impl<'a> Node<'a, &'a ast::StmtContinue> {
    #[inline]
    pub(crate) fn visit_source_order<V>(self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtContinue { range: _ } = self.as_ref();
    }
}

impl<'a> Node<'a, &'a ast::StmtIpyEscapeCommand> {
    #[inline]
    pub(crate) fn visit_source_order<V>(self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtIpyEscapeCommand {
            range: _,
            kind: _,
            value: _,
        } = self.as_ref();
    }
}

impl<'a> Node<'a, &'a ast::ExprBoolOp> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprBoolOp {
            op,
            values,
            range: _,
        } = self.as_ref();
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

impl<'a> Node<'a, &'a ast::ExprNamed> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprNamed {
            target,
            value,
            range: _,
        } = self.as_ref();
        visitor.visit_expr(target);
        visitor.visit_expr(value);
    }
}

impl<'a> Node<'a, &'a ast::ExprBinOp> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprBinOp {
            left,
            op,
            right,
            range: _,
        } = self.as_ref();
        visitor.visit_expr(left);
        visitor.visit_operator(op);
        visitor.visit_expr(right);
    }
}

impl<'a> Node<'a, &'a ast::ExprUnaryOp> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprUnaryOp {
            op,
            operand,
            range: _,
        } = self.as_ref();

        visitor.visit_unary_op(op);
        visitor.visit_expr(operand);
    }
}

impl<'a> Node<'a, &'a ast::ExprLambda> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprLambda {
            parameters,
            body,
            range: _,
        } = self.as_ref();

        if let Some(parameters) = parameters {
            visitor.visit_parameters(parameters);
        }
        visitor.visit_expr(body);
    }
}

impl<'a> Node<'a, &'a ast::ExprIf> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprIf {
            test,
            body,
            orelse,
            range: _,
        } = self.as_ref();

        // `body if test else orelse`
        visitor.visit_expr(body);
        visitor.visit_expr(test);
        visitor.visit_expr(orelse);
    }
}

impl<'a> Node<'a, &'a ast::ExprDict> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprDict { items, range: _ } = self.as_ref();

        for ast::DictItem { key, value } in items {
            if let Some(key) = key {
                visitor.visit_expr(key);
            }
            visitor.visit_expr(value);
        }
    }
}

impl<'a> Node<'a, &'a ast::ExprSet> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprSet { elts, range: _ } = self.as_ref();

        for expr in elts {
            visitor.visit_expr(expr);
        }
    }
}

impl<'a> Node<'a, &'a ast::ExprListComp> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprListComp {
            elt,
            generators,
            range: _,
        } = self.as_ref();

        visitor.visit_expr(elt);
        for comprehension in generators {
            visitor.visit_comprehension(comprehension);
        }
    }
}

impl<'a> Node<'a, &'a ast::ExprSetComp> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprSetComp {
            elt,
            generators,
            range: _,
        } = self.as_ref();

        visitor.visit_expr(elt);
        for comprehension in generators {
            visitor.visit_comprehension(comprehension);
        }
    }
}

impl<'a> Node<'a, &'a ast::ExprDictComp> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprDictComp {
            key,
            value,
            generators,
            range: _,
        } = self.as_ref();

        visitor.visit_expr(key);
        visitor.visit_expr(value);

        for comprehension in generators {
            visitor.visit_comprehension(comprehension);
        }
    }
}

impl<'a> Node<'a, &'a ast::ExprGenerator> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprGenerator {
            elt,
            generators,
            range: _,
            parenthesized: _,
        } = self.as_ref();
        visitor.visit_expr(elt);
        for comprehension in generators {
            visitor.visit_comprehension(comprehension);
        }
    }
}

impl<'a> Node<'a, &'a ast::ExprAwait> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprAwait { value, range: _ } = self.as_ref();
        visitor.visit_expr(value);
    }
}

impl<'a> Node<'a, &'a ast::ExprYield> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprYield { value, range: _ } = self.as_ref();
        if let Some(expr) = value {
            visitor.visit_expr(expr);
        }
    }
}

impl<'a> Node<'a, &'a ast::ExprYieldFrom> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprYieldFrom { value, range: _ } = self.as_ref();
        visitor.visit_expr(value);
    }
}

impl<'a> Node<'a, &'a ast::ExprCompare> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprCompare {
            left,
            ops,
            comparators,
            range: _,
        } = self.as_ref();

        visitor.visit_expr(left);

        for (op, comparator) in ops.iter().zip(comparators) {
            visitor.visit_cmp_op(op);
            visitor.visit_expr(comparator);
        }
    }
}

impl<'a> Node<'a, &'a ast::ExprCall> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprCall {
            func,
            arguments,
            range: _,
        } = self.as_ref();
        visitor.visit_expr(func);
        visitor.visit_arguments(arguments);
    }
}

impl<'a> Node<'a, &'a ast::FStringFormatSpec> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
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
        } = self.as_ref();
        visitor.visit_expr(expression);

        if let Some(format_spec) = format_spec {
            for spec_part in &format_spec.elements {
                visitor.visit_f_string_element(spec_part);
            }
        }
    }
}

impl<'a> Node<'a, &'a ast::FStringLiteralElement> {
    pub(crate) fn visit_source_order<V>(self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::FStringLiteralElement { range: _, value: _ } = self.as_ref();
    }
}

impl<'a> Node<'a, &'a ast::ExprFString> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprFString { value, range: _ } = self.as_ref();

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

impl<'a> Node<'a, &'a ast::ExprStringLiteral> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprStringLiteral { value, range: _ } = self.as_ref();

        for string_literal in value {
            visitor.visit_string_literal(string_literal);
        }
    }
}

impl<'a> Node<'a, &'a ast::ExprBytesLiteral> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprBytesLiteral { value, range: _ } = self.as_ref();

        for bytes_literal in value {
            visitor.visit_bytes_literal(bytes_literal);
        }
    }
}

impl<'a> Node<'a, &'a ast::ExprNumberLiteral> {
    #[inline]
    pub(crate) fn visit_source_order<V>(self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprNumberLiteral { range: _, value: _ } = self.as_ref();
    }
}

impl<'a> Node<'a, &'a ast::ExprBooleanLiteral> {
    #[inline]
    pub(crate) fn visit_source_order<V>(self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprBooleanLiteral { range: _, value: _ } = self.as_ref();
    }
}

impl<'a> Node<'a, &'a ast::ExprNoneLiteral> {
    #[inline]
    pub(crate) fn visit_source_order<V>(self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprNoneLiteral { range: _ } = self.as_ref();
    }
}

impl<'a> Node<'a, &'a ast::ExprEllipsisLiteral> {
    #[inline]
    pub(crate) fn visit_source_order<V>(self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprEllipsisLiteral { range: _ } = self.as_ref();
    }
}

impl<'a> Node<'a, &'a ast::ExprAttribute> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprAttribute {
            value,
            attr: _,
            ctx: _,
            range: _,
        } = self.as_ref();

        visitor.visit_expr(value);
    }
}

impl<'a> Node<'a, &'a ast::ExprSubscript> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprSubscript {
            value,
            slice,
            ctx: _,
            range: _,
        } = self.as_ref();
        visitor.visit_expr(value);
        visitor.visit_expr(slice);
    }
}

impl<'a> Node<'a, &'a ast::ExprStarred> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprStarred {
            value,
            ctx: _,
            range: _,
        } = self.as_ref();

        visitor.visit_expr(value);
    }
}

impl<'a> Node<'a, &'a ast::ExprName> {
    #[inline]
    pub(crate) fn visit_source_order<V>(self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprName {
            range: _,
            id: _,
            ctx: _,
        } = self.as_ref();
    }
}

impl<'a> Node<'a, &'a ast::ExprList> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprList {
            elts,
            ctx: _,
            range: _,
        } = self.as_ref();

        for expr in elts {
            visitor.visit_expr(expr);
        }
    }
}

impl<'a> Node<'a, &'a ast::ExprTuple> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprTuple {
            elts,
            ctx: _,
            range: _,
            parenthesized: _,
        } = self.as_ref();

        for expr in elts {
            visitor.visit_expr(expr);
        }
    }
}

impl<'a> Node<'a, &'a ast::ExprSlice> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprSlice {
            lower,
            upper,
            step,
            range: _,
        } = self.as_ref();

        if let Some(expr) = lower {
            visitor.visit_expr(expr);
        }
        if let Some(expr) = upper {
            visitor.visit_expr(expr);
        }
        if let Some(expr) = step {
            visitor.visit_expr(expr);
        }
    }
}

impl<'a> Node<'a, &'a ast::ExprIpyEscapeCommand> {
    #[inline]
    pub(crate) fn visit_source_order<V>(self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprIpyEscapeCommand {
            range: _,
            kind: _,
            value: _,
        } = self.as_ref();
    }
}

impl<'a> Node<'a, &'a ast::ExceptHandlerExceptHandler> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExceptHandlerExceptHandler {
            range: _,
            type_,
            name: _,
            body,
        } = self.as_ref();
        if let Some(expr) = type_ {
            visitor.visit_expr(expr);
        }
        visitor.visit_body(body);
    }
}

impl<'a> Node<'a, &'a ast::PatternMatchValue> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchValue { value, range: _ } = self.as_ref();
        visitor.visit_expr(value);
    }
}

impl<'a> Node<'a, &'a ast::PatternMatchSingleton> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchSingleton { value, range: _ } = self.as_ref();
        visitor.visit_singleton(value);
    }
}

impl<'a> Node<'a, &'a ast::PatternMatchSequence> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchSequence { patterns, range: _ } = self.as_ref();
        for pattern in patterns {
            visitor.visit_pattern(pattern);
        }
    }
}

impl<'a> Node<'a, &'a ast::PatternMatchMapping> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchMapping {
            keys,
            patterns,
            range: _,
            rest: _,
        } = self.as_ref();
        for (key, pattern) in keys.iter().zip(patterns) {
            visitor.visit_expr(key);
            visitor.visit_pattern(pattern);
        }
    }
}

impl<'a> Node<'a, &'a ast::PatternMatchClass> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchClass {
            cls,
            arguments: parameters,
            range: _,
        } = self.as_ref();
        visitor.visit_expr(cls);
        visitor.visit_pattern_arguments(parameters);
    }
}

impl<'a> Node<'a, &'a ast::PatternMatchStar> {
    #[inline]
    pub(crate) fn visit_source_order<V>(self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchStar { range: _, name: _ } = self.as_ref();
    }
}

impl<'a> Node<'a, &'a ast::PatternMatchAs> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchAs {
            pattern,
            range: _,
            name: _,
        } = self.as_ref();
        if let Some(pattern) = pattern {
            visitor.visit_pattern(pattern);
        }
    }
}

impl<'a> Node<'a, &'a ast::PatternMatchOr> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchOr { patterns, range: _ } = self.as_ref();
        for pattern in patterns {
            visitor.visit_pattern(pattern);
        }
    }
}

impl<'a> Node<'a, &'a ast::PatternArguments> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let PatternArguments {
            range: _,
            patterns,
            keywords,
        } = self.as_ref();

        for pattern in patterns {
            visitor.visit_pattern(pattern);
        }

        for keyword in keywords {
            visitor.visit_pattern_keyword(keyword);
        }
    }
}

impl<'a> Node<'a, &'a ast::PatternKeyword> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let PatternKeyword {
            range: _,
            attr: _,
            pattern,
        } = self.as_ref();

        visitor.visit_pattern(pattern);
    }
}

impl<'a> Node<'a, &'a ast::Comprehension> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::Comprehension {
            range: _,
            target,
            iter,
            ifs,
            is_async: _,
        } = self.as_ref();
        visitor.visit_expr(target);
        visitor.visit_expr(iter);

        for expr in ifs {
            visitor.visit_expr(expr);
        }
    }
}

impl<'a> Node<'a, &'a ast::Arguments> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
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

impl<'a> Node<'a, &'a ast::Parameters> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
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

impl<'a> Node<'a, &'a ast::Parameter> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::Parameter {
            range: _,
            name: _,
            annotation,
        } = self.as_ref();

        if let Some(expr) = annotation {
            visitor.visit_annotation(expr);
        }
    }
}

impl<'a> Node<'a, &'a ast::ParameterWithDefault> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ParameterWithDefault {
            range: _,
            parameter,
            default,
        } = self.as_ref();
        visitor.visit_parameter(parameter);
        if let Some(expr) = default {
            visitor.visit_expr(expr);
        }
    }
}

impl<'a> Node<'a, &'a ast::Keyword> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::Keyword {
            range: _,
            arg: _,
            value,
        } = self.as_ref();

        visitor.visit_expr(value);
    }
}

impl<'a> Node<'a, &'a Alias> {
    #[inline]
    pub(crate) fn visit_source_order<V>(self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::Alias {
            range: _,
            name: _,
            asname: _,
        } = self.as_ref();
    }
}

impl<'a> Node<'a, &'a ast::WithItem> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::WithItem {
            range: _,
            context_expr,
            optional_vars,
        } = self.as_ref();

        visitor.visit_expr(context_expr);

        if let Some(expr) = optional_vars {
            visitor.visit_expr(expr);
        }
    }
}

impl<'a> Node<'a, &'a ast::MatchCase> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::MatchCase {
            range: _,
            pattern,
            guard,
            body,
        } = self.as_ref();

        visitor.visit_pattern(pattern);
        if let Some(expr) = guard {
            visitor.visit_expr(expr);
        }
        visitor.visit_body(body);
    }
}

impl<'a> Node<'a, &'a ast::Decorator> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::Decorator {
            range: _,
            expression,
        } = self.as_ref();

        visitor.visit_expr(expression);
    }
}

impl<'a> Node<'a, &'a ast::TypeParams> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::TypeParams {
            range: _,
            type_params,
        } = self.as_ref();

        for type_param in type_params {
            visitor.visit_type_param(type_param);
        }
    }
}

impl<'a> Node<'a, &'a ast::TypeParamTypeVar> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::TypeParamTypeVar {
            bound,
            default,
            name: _,
            range: _,
        } = self.as_ref();

        if let Some(expr) = bound {
            visitor.visit_expr(expr);
        }
        if let Some(expr) = default {
            visitor.visit_expr(expr);
        }
    }
}

impl<'a> Node<'a, &'a ast::TypeParamTypeVarTuple> {
    #[inline]
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::TypeParamTypeVarTuple {
            range: _,
            name: _,
            default,
        } = self.as_ref();
        if let Some(expr) = default {
            visitor.visit_expr(expr);
        }
    }
}

impl<'a> Node<'a, &'a ast::TypeParamParamSpec> {
    #[inline]
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::TypeParamParamSpec {
            range: _,
            name: _,
            default,
        } = self.as_ref();
        if let Some(expr) = default {
            visitor.visit_expr(expr);
        }
    }
}

impl<'a> Node<'a, &'a ast::FString> {
    pub(crate) fn visit_source_order<V>(self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::FString {
            elements,
            range: _,
            flags: _,
        } = self.as_ref();

        for fstring_element in elements {
            visitor.visit_f_string_element(fstring_element);
        }
    }
}

impl<'a> Node<'a, &'a ast::StringLiteral> {
    #[inline]
    pub(crate) fn visit_source_order<V>(self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StringLiteral {
            range: _,
            value: _,
            flags: _,
        } = self.as_ref();
    }
}

impl<'a> Node<'a, &'a ast::BytesLiteral> {
    #[inline]
    pub(crate) fn visit_source_order<V>(self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::BytesLiteral {
            range: _,
            value: _,
            flags: _,
        } = self.as_ref();
    }
}

impl<'a> Node<'a, &'a ast::Identifier> {
    #[inline]
    pub(crate) fn visit_source_order<V>(self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::Identifier { range: _, id: _ } = self.as_ref();
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
