use std::{fmt, vec};

use crate::{Db, NavigationTarget};
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::visitor::source_order::{self, SourceOrderVisitor, TraversalSignal};
use ruff_python_ast::{AnyNodeRef, ArgOrKeyword, Expr, ExprUnaryOp, Stmt, UnaryOp};
use ruff_text_size::{Ranged, TextRange, TextSize};
use ty_python_semantic::types::Type;
use ty_python_semantic::types::ide_support::inlay_hint_call_argument_details;
use ty_python_semantic::{HasType, SemanticModel};

#[derive(Debug, Clone)]
pub struct InlayHint {
    pub position: TextSize,
    pub kind: InlayHintKind,
    pub label: InlayHintLabel,
}

impl InlayHint {
    fn variable_type(position: TextSize, ty: Type, db: &dyn Db) -> Self {
        let label_parts = vec![
            ": ".into(),
            InlayHintLabelPart::new(ty.display(db).to_string()),
        ];

        Self {
            position,
            kind: InlayHintKind::Type,
            label: InlayHintLabel { parts: label_parts },
        }
    }

    fn call_argument_name(
        position: TextSize,
        name: &str,
        navigation_target: Option<NavigationTarget>,
    ) -> Self {
        let label_parts = vec![
            InlayHintLabelPart::new(name).with_target(navigation_target),
            "=".into(),
        ];

        Self {
            position,
            kind: InlayHintKind::CallArgumentName,
            label: InlayHintLabel { parts: label_parts },
        }
    }

    pub fn display(&self) -> InlayHintDisplay<'_> {
        InlayHintDisplay { inlay_hint: self }
    }
}

#[derive(Debug, Clone)]
pub enum InlayHintKind {
    Type,
    CallArgumentName,
}

#[derive(Debug, Clone)]
pub struct InlayHintLabel {
    parts: Vec<InlayHintLabelPart>,
}

impl InlayHintLabel {
    pub fn parts(&self) -> &[InlayHintLabelPart] {
        &self.parts
    }

    pub fn into_parts(self) -> Vec<InlayHintLabelPart> {
        self.parts
    }
}

pub struct InlayHintDisplay<'a> {
    inlay_hint: &'a InlayHint,
}

impl fmt::Display for InlayHintDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        for part in &self.inlay_hint.label.parts {
            write!(f, "{}", part.text)?;
        }
        Ok(())
    }
}

#[derive(Default, Debug, Clone)]
pub struct InlayHintLabelPart {
    text: String,

    target: Option<NavigationTarget>,
}

impl InlayHintLabelPart {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            target: None,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn into_text(self) -> String {
        self.text
    }

    pub fn target(&self) -> Option<&NavigationTarget> {
        self.target.as_ref()
    }

    pub fn with_target(self, target: Option<NavigationTarget>) -> Self {
        Self { target, ..self }
    }
}

impl From<String> for InlayHintLabelPart {
    fn from(s: String) -> Self {
        Self {
            text: s,
            target: None,
        }
    }
}

impl From<&str> for InlayHintLabelPart {
    fn from(s: &str) -> Self {
        Self {
            text: s.to_string(),
            target: None,
        }
    }
}

pub fn inlay_hints(
    db: &dyn Db,
    file: File,
    range: TextRange,
    settings: &InlayHintSettings,
) -> Vec<InlayHint> {
    let mut visitor = InlayHintVisitor::new(db, file, range, settings);

    let ast = parsed_module(db, file).load(db);

    visitor.visit_body(ast.suite());

    visitor.hints
}

/// Settings to control the behavior of inlay hints.
#[derive(Clone, Debug)]
pub struct InlayHintSettings {
    /// Whether to show variable type hints.
    ///
    /// For example, this would enable / disable hints like the ones quoted below:
    /// ```python
    /// x": Literal[1]" = 1
    /// ```
    pub variable_types: bool,

    /// Whether to show call argument names.
    ///
    /// For example, this would enable / disable hints like the ones quoted below:
    /// ```python
    /// def foo(x: int): pass
    /// foo("x="1)
    /// ```
    pub call_argument_names: bool,
    // Add any new setting that enables additional inlays to `any_enabled`.
}

impl InlayHintSettings {
    pub fn any_enabled(&self) -> bool {
        self.variable_types || self.call_argument_names
    }
}

impl Default for InlayHintSettings {
    fn default() -> Self {
        Self {
            variable_types: true,
            call_argument_names: true,
        }
    }
}

struct InlayHintVisitor<'a, 'db> {
    db: &'db dyn Db,
    model: SemanticModel<'db>,
    hints: Vec<InlayHint>,
    in_assignment: bool,
    range: TextRange,
    settings: &'a InlayHintSettings,
}

impl<'a, 'db> InlayHintVisitor<'a, 'db> {
    fn new(db: &'db dyn Db, file: File, range: TextRange, settings: &'a InlayHintSettings) -> Self {
        Self {
            db,
            model: SemanticModel::new(db, file),
            hints: Vec::new(),
            in_assignment: false,
            range,
            settings,
        }
    }

    fn add_type_hint(&mut self, position: TextSize, ty: Type<'db>) {
        if !self.settings.variable_types {
            return;
        }

        let inlay_hint = InlayHint::variable_type(position, ty, self.db);

        self.hints.push(inlay_hint);
    }

    fn add_call_argument_name(
        &mut self,
        position: TextSize,
        name: &str,
        navigation_target: Option<NavigationTarget>,
    ) {
        if !self.settings.call_argument_names {
            return;
        }

        if name.starts_with('_') {
            return;
        }

        let inlay_hint = InlayHint::call_argument_name(position, name, navigation_target);

        self.hints.push(inlay_hint);
    }
}

impl SourceOrderVisitor<'_> for InlayHintVisitor<'_, '_> {
    fn enter_node(&mut self, node: AnyNodeRef<'_>) -> TraversalSignal {
        if self.range.intersect(node.range()).is_some() {
            TraversalSignal::Traverse
        } else {
            TraversalSignal::Skip
        }
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        let node = AnyNodeRef::from(stmt);

        if !self.enter_node(node).is_traverse() {
            return;
        }

        match stmt {
            Stmt::Assign(assign) => {
                self.in_assignment = !type_hint_is_excessive_for_expr(&assign.value);
                for target in &assign.targets {
                    self.visit_expr(target);
                }
                self.in_assignment = false;

                self.visit_expr(&assign.value);

                return;
            }
            Stmt::Expr(expr) => {
                self.visit_expr(&expr.value);
                return;
            }
            // TODO
            Stmt::FunctionDef(_) => {}
            Stmt::For(_) => {}
            _ => {}
        }

        source_order::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'_ Expr) {
        match expr {
            Expr::Name(name) => {
                if self.in_assignment {
                    if name.ctx.is_store() {
                        let ty = expr.inferred_type(&self.model);
                        self.add_type_hint(expr.range().end(), ty);
                    }
                }
                source_order::walk_expr(self, expr);
            }
            Expr::Attribute(attribute) => {
                if self.in_assignment {
                    if attribute.ctx.is_store() {
                        let ty = expr.inferred_type(&self.model);
                        self.add_type_hint(expr.range().end(), ty);
                    }
                }
                source_order::walk_expr(self, expr);
            }
            Expr::Call(call) => {
                let details = inlay_hint_call_argument_details(self.db, &self.model, call)
                    .unwrap_or_default();

                self.visit_expr(&call.func);

                for (index, arg_or_keyword) in call.arguments.arguments_source_order().enumerate() {
                    if let Some((name, parameter_label_offset)) = details.argument_names.get(&index)
                        && !arg_matches_name(&arg_or_keyword, name)
                    {
                        self.add_call_argument_name(
                            arg_or_keyword.range().start(),
                            name,
                            parameter_label_offset.map(NavigationTarget::from),
                        );
                    }
                    self.visit_expr(arg_or_keyword.value());
                }
            }
            _ => {
                source_order::walk_expr(self, expr);
            }
        }
    }
}

/// Given a positional argument, check if the expression is the "same name"
/// as the function argument itself.
///
/// This allows us to filter out reptitive inlay hints like `x=x`, `x=y.x`, etc.
fn arg_matches_name(arg_or_keyword: &ArgOrKeyword, name: &str) -> bool {
    // Only care about positional args
    let ArgOrKeyword::Arg(arg) = arg_or_keyword else {
        return false;
    };

    let mut expr = *arg;
    loop {
        match expr {
            // `x=x(1, 2)` counts as a match, recurse for it
            Expr::Call(expr_call) => expr = &expr_call.func,
            // `x=x[0]` is a match, recurse for it
            Expr::Subscript(expr_subscript) => expr = &expr_subscript.value,
            // `x=x` is a match
            Expr::Name(expr_name) => return expr_name.id.as_str() == name,
            // `x=y.x` is a match
            Expr::Attribute(expr_attribute) => return expr_attribute.attr.as_str() == name,
            _ => return false,
        }
    }
}

/// Given an expression that's the RHS of an assignment, would it be excessive to
/// emit an inlay type hint for the variable assigned to it?
///
/// This is used to suppress inlay hints for things like `x = 1`, `x, y = (1, 2)`, etc.
fn type_hint_is_excessive_for_expr(expr: &Expr) -> bool {
    match expr {
        // A tuple of all literals is excessive to typehint
        Expr::Tuple(expr_tuple) => expr_tuple.elts.iter().all(type_hint_is_excessive_for_expr),

        // Various Literal[...] types which are always excessive to hint
        | Expr::BytesLiteral(_)
        | Expr::NumberLiteral(_)
        | Expr::BooleanLiteral(_)
        | Expr::StringLiteral(_)
        // `None` isn't terribly verbose, but still redundant
        | Expr::NoneLiteral(_)
        // This one expands to `str` which isn't verbose but is redundant
        | Expr::FString(_)
        // This one expands to `Template` which isn't verbose but is redundant
        | Expr::TString(_)=> true,

        // You too `+1 and `-1`, get back here
        Expr::UnaryOp(ExprUnaryOp { op: UnaryOp::UAdd | UnaryOp::USub, operand, .. }) => matches!(**operand, Expr::NumberLiteral(_)),

        // Everything else is reasonable
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::NavigationTarget;
    use crate::tests::IntoDiagnostic;
    use insta::assert_snapshot;
    use ruff_db::{
        diagnostic::{
            Annotation, Diagnostic, DiagnosticFormat, DiagnosticId, DisplayDiagnosticConfig,
            LintName, Severity, Span, SubDiagnostic, SubDiagnosticSeverity,
        },
        files::{File, FileRange, system_path_to_file},
        source::source_text,
    };
    use ruff_python_trivia::textwrap::dedent;
    use ruff_text_size::TextSize;

    use ruff_db::system::{DbWithWritableSystem, SystemPathBuf};
    use ty_project::ProjectMetadata;

    pub(super) fn inlay_hint_test(source: &str) -> InlayHintTest {
        const START: &str = "<START>";
        const END: &str = "<END>";

        let mut db = ty_project::TestDb::new(ProjectMetadata::new(
            "test".into(),
            SystemPathBuf::from("/"),
        ));

        db.init_program().unwrap();

        let source = dedent(source);

        let start = source.find(START);
        let end = source
            .find(END)
            .map(|x| if start.is_some() { x - START.len() } else { x })
            .unwrap_or(source.len());

        let range = TextRange::new(
            TextSize::try_from(start.unwrap_or_default()).unwrap(),
            TextSize::try_from(end).unwrap(),
        );

        let source = source.replace(START, "");
        let source = source.replace(END, "");

        db.write_file("main.py", source)
            .expect("write to memory file system to be successful");

        let file = system_path_to_file(&db, "main.py").expect("newly written file to existing");

        InlayHintTest { db, file, range }
    }

    pub(super) struct InlayHintTest {
        pub(super) db: ty_project::TestDb,
        pub(super) file: File,
        pub(super) range: TextRange,
    }

    impl InlayHintTest {
        /// Returns the inlay hints for the given test case.
        ///
        /// All inlay hints are generated using the applicable settings. Use
        /// [`inlay_hints_with_settings`] to generate hints with custom settings.
        ///
        /// [`inlay_hints_with_settings`]: Self::inlay_hints_with_settings
        fn inlay_hints(&self) -> String {
            self.inlay_hints_with_settings(&InlayHintSettings {
                variable_types: true,
                call_argument_names: true,
            })
        }

        fn with_extra_file(&mut self, file_name: &str, content: &str) {
            self.db.write_file(file_name, content).unwrap();
        }

        /// Returns the inlay hints for the given test case with custom settings.
        fn inlay_hints_with_settings(&self, settings: &InlayHintSettings) -> String {
            let hints = inlay_hints(&self.db, self.file, self.range, settings);

            let mut buf = source_text(&self.db, self.file).as_str().to_string();

            let mut diagnostics = Vec::new();

            let mut offset = 0;

            for hint in hints {
                let mut hint_str = "[".to_string();

                let end_position = (hint.position.to_u32() as usize) + offset;

                for part in hint.label.parts() {
                    hint_str.push_str(part.text());

                    if let Some(target) = part.target() {
                        let label_range = TextRange::at(hint.position, TextSize::ZERO);

                        let label_file_range = FileRange::new(self.file, label_range);

                        diagnostics
                            .push(InlayHintLocationDiagnostic::new(label_file_range, target));
                    }
                }

                hint_str.push(']');

                offset += hint_str.len();

                buf.insert_str(end_position, &hint_str);
            }

            let mut rendered_diagnostics = self.render_diagnostics(diagnostics);

            if !rendered_diagnostics.is_empty() {
                rendered_diagnostics = format!(
                    "{}{}",
                    crate::MarkupKind::PlainText.horizontal_line(),
                    rendered_diagnostics
                );
            }

            format!("{buf}{rendered_diagnostics}",)
        }

        fn render_diagnostics<I, D>(&self, diagnostics: I) -> String
        where
            I: IntoIterator<Item = D>,
            D: IntoDiagnostic,
        {
            use std::fmt::Write;

            let mut buf = String::new();

            let config = DisplayDiagnosticConfig::default()
                .color(false)
                .format(DiagnosticFormat::Full);

            for diagnostic in diagnostics {
                let diag = diagnostic.into_diagnostic();
                write!(buf, "{}", diag.display(&self.db, &config)).unwrap();
            }

            buf
        }
    }

    #[test]
    fn test_assign_statement() {
        let test = inlay_hint_test(
            "
            def i(x: int, /) -> int:
                return x

            x = 1
            y = x
            z = i(1)
            w = z
            ",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def i(x: int, /) -> int:
            return x

        x = 1
        y[: Literal[1]] = x
        z[: int] = i(1)
        w[: int] = z
        ");
    }

    #[test]
    fn test_unpacked_tuple_assignment() {
        let test = inlay_hint_test(
            "
            def i(x: int, /) -> int:
                return x
            def s(x: str, /) -> str:
                return x

            x1, y1 = (1, 'abc')
            x2, y2 = (x1, y1)
            x3, y3 = (i(1), s('abc'))
            x4, y4 = (x3, y3)
            ",
        );

        assert_snapshot!(test.inlay_hints(), @r#"
        def i(x: int, /) -> int:
            return x
        def s(x: str, /) -> str:
            return x

        x1, y1 = (1, 'abc')
        x2[: Literal[1]], y2[: Literal["abc"]] = (x1, y1)
        x3[: int], y3[: str] = (i(1), s('abc'))
        x4[: int], y4[: str] = (x3, y3)
        "#);
    }

    #[test]
    fn test_multiple_assignment() {
        let test = inlay_hint_test(
            "
            def i(x: int, /) -> int:
                return x
            def s(x: str, /) -> str:
                return x

            x1, y1 = 1, 'abc'
            x2, y2 = x1, y1
            x3, y3 = i(1), s('abc')
            x4, y4 = x3, y3
            ",
        );

        assert_snapshot!(test.inlay_hints(), @r#"
        def i(x: int, /) -> int:
            return x
        def s(x: str, /) -> str:
            return x

        x1, y1 = 1, 'abc'
        x2[: Literal[1]], y2[: Literal["abc"]] = x1, y1
        x3[: int], y3[: str] = i(1), s('abc')
        x4[: int], y4[: str] = x3, y3
        "#);
    }

    #[test]
    fn test_tuple_assignment() {
        let test = inlay_hint_test(
            "
            def i(x: int, /) -> int:
                return x
            def s(x: str, /) -> str:
                return x

            x = (1, 'abc')
            y = x
            z = (i(1), s('abc'))
            w = z
            ",
        );

        assert_snapshot!(test.inlay_hints(), @r#"
        def i(x: int, /) -> int:
            return x
        def s(x: str, /) -> str:
            return x

        x = (1, 'abc')
        y[: tuple[Literal[1], Literal["abc"]]] = x
        z[: tuple[int, str]] = (i(1), s('abc'))
        w[: tuple[int, str]] = z
        "#);
    }

    #[test]
    fn test_nested_tuple_assignment() {
        let test = inlay_hint_test(
            "
            def i(x: int, /) -> int:
                return x
            def s(x: str, /) -> str:
                return x

            x1, (y1, z1) = (1, ('abc', 2))
            x2, (y2, z2) = (x1, (y1, z1))
            x3, (y3, z3) = (i(1), (s('abc'), i(2)))
            x4, (y4, z4) = (x3, (y3, z3))",
        );

        assert_snapshot!(test.inlay_hints(), @r#"
        def i(x: int, /) -> int:
            return x
        def s(x: str, /) -> str:
            return x

        x1, (y1, z1) = (1, ('abc', 2))
        x2[: Literal[1]], (y2[: Literal["abc"]], z2[: Literal[2]]) = (x1, (y1, z1))
        x3[: int], (y3[: str], z3[: int]) = (i(1), (s('abc'), i(2)))
        x4[: int], (y4[: str], z4[: int]) = (x3, (y3, z3))
        "#);
    }

    #[test]
    fn test_assign_statement_with_type_annotation() {
        let test = inlay_hint_test(
            "
            def i(x: int, /) -> int:
                return x

            x: int = 1
            y = x
            z: int = i(1)
            w = z",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def i(x: int, /) -> int:
            return x

        x: int = 1
        y[: Literal[1]] = x
        z: int = i(1)
        w[: int] = z
        ");
    }

    #[test]
    fn test_assign_statement_out_of_range() {
        let test = inlay_hint_test(
            "
            def i(x: int, /) -> int:
                return x
            <START>x = i(1)<END>
            z = x",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def i(x: int, /) -> int:
            return x
        x[: int] = i(1)
        z = x
        ");
    }

    #[test]
    fn test_assign_attribute_of_instance() {
        let test = inlay_hint_test(
            "
            class A:
                def __init__(self, y):
                    self.x = int(1)
                    self.y = y

            a = A(2)
            a.y = int(3)
            ",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        class A:
            def __init__(self, y):
                self.x[: int] = int(1)
                self.y[: Unknown] = y

        a[: A] = A([y=]2)
        a.y[: int] = int(3)

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:24
          |
        2 | class A:
        3 |     def __init__(self, y):
          |                        ^
        4 |         self.x = int(1)
        5 |         self.y = y
          |
        info: Source
         --> main.py:7:7
          |
        5 |         self.y = y
        6 |
        7 | a = A(2)
          |       ^
        8 | a.y = int(3)
          |
        ");
    }

    #[test]
    fn test_many_literals() {
        let test = inlay_hint_test(
            r#"
            a = 1
            b = 1.0
            c = True
            d = None
            e = "hello"
            f = 'there'
            g = f"{e} {f}"
            h = t"wow %d"
            i = b'\x00'
            j = +1
            k = -1.0
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @r#"
        a = 1
        b = 1.0
        c = True
        d = None
        e = "hello"
        f = 'there'
        g = f"{e} {f}"
        h = t"wow %d"
        i = b'\x00'
        j = +1
        k = -1.0
        "#);
    }

    #[test]
    fn test_many_literals_tuple() {
        let test = inlay_hint_test(
            r#"
            a = (1, 2)
            b = (1.0, 2.0)
            c = (True, False)
            d = (None, None)
            e = ("hel", "lo")
            f = ('the', 're')
            g = (f"{ft}", f"{ft}")
            h = (t"wow %d", t"wow %d")
            i = (b'\x01', b'\x02')
            j = (+1, +2.0)
            k = (-1, -2.0)
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @r#"
        a = (1, 2)
        b = (1.0, 2.0)
        c = (True, False)
        d = (None, None)
        e = ("hel", "lo")
        f = ('the', 're')
        g = (f"{ft}", f"{ft}")
        h = (t"wow %d", t"wow %d")
        i = (b'\x01', b'\x02')
        j = (+1, +2.0)
        k = (-1, -2.0)
        "#);
    }

    #[test]
    fn test_many_literals_unpacked_tuple() {
        let test = inlay_hint_test(
            r#"
            a1, a2 = (1, 2)
            b1, b2 = (1.0, 2.0)
            c1, c2 = (True, False)
            d1, d2 = (None, None)
            e1, e2 = ("hel", "lo")
            f1, f2 = ('the', 're')
            g1, g2 = (f"{ft}", f"{ft}")
            h1, h2 = (t"wow %d", t"wow %d")
            i1, i2 = (b'\x01', b'\x02')
            j1, j2 = (+1, +2.0)
            k1, k2 = (-1, -2.0)
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @r#"
        a1, a2 = (1, 2)
        b1, b2 = (1.0, 2.0)
        c1, c2 = (True, False)
        d1, d2 = (None, None)
        e1, e2 = ("hel", "lo")
        f1, f2 = ('the', 're')
        g1, g2 = (f"{ft}", f"{ft}")
        h1, h2 = (t"wow %d", t"wow %d")
        i1, i2 = (b'\x01', b'\x02')
        j1, j2 = (+1, +2.0)
        k1, k2 = (-1, -2.0)
        "#);
    }

    #[test]
    fn test_many_literals_multiple() {
        let test = inlay_hint_test(
            r#"
            a1, a2 = 1, 2
            b1, b2 = 1.0, 2.0
            c1, c2 = True, False
            d1, d2 = None, None
            e1, e2 = "hel", "lo"
            f1, f2 = 'the', 're'
            g1, g2 = f"{ft}", f"{ft}"
            h1, h2 = t"wow %d", t"wow %d"
            i1, i2 = b'\x01', b'\x02'
            j1, j2 = +1, +2.0
            k1, k2 = -1, -2.0
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @r#"
        a1, a2 = 1, 2
        b1, b2 = 1.0, 2.0
        c1, c2 = True, False
        d1, d2 = None, None
        e1, e2 = "hel", "lo"
        f1, f2 = 'the', 're'
        g1, g2 = f"{ft}", f"{ft}"
        h1, h2 = t"wow %d", t"wow %d"
        i1, i2 = b'\x01', b'\x02'
        j1, j2 = +1, +2.0
        k1, k2 = -1, -2.0
        "#);
    }

    #[test]
    fn test_many_literals_list() {
        let test = inlay_hint_test(
            r#"
            a = [1, 2]
            b = [1.0, 2.0]
            c = [True, False]
            d = [None, None]
            e = ["hel", "lo"]
            f = ['the', 're']
            g = [f"{ft}", f"{ft}"]
            h = [t"wow %d", t"wow %d"]
            i = [b'\x01', b'\x02']
            j = [+1, +2.0]
            k = [-1, -2.0]
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @r#"
        a[: list[Unknown | int]] = [1, 2]
        b[: list[Unknown | float]] = [1.0, 2.0]
        c[: list[Unknown | bool]] = [True, False]
        d[: list[Unknown | None]] = [None, None]
        e[: list[Unknown | str]] = ["hel", "lo"]
        f[: list[Unknown | str]] = ['the', 're']
        g[: list[Unknown | str]] = [f"{ft}", f"{ft}"]
        h[: list[Unknown | Template]] = [t"wow %d", t"wow %d"]
        i[: list[Unknown | bytes]] = [b'\x01', b'\x02']
        j[: list[Unknown | int | float]] = [+1, +2.0]
        k[: list[Unknown | int | float]] = [-1, -2.0]
        "#);
    }

    #[test]
    fn test_simple_init_call() {
        let test = inlay_hint_test(
            r#"
            class MyClass:
                def __init__(self):
                    self.x: int = 1

            x = MyClass()
            y = (MyClass(), MyClass())
            a, b = MyClass(), MyClass()
            c, d = (MyClass(), MyClass())
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @r"
        class MyClass:
            def __init__(self):
                self.x: int = 1

        x[: MyClass] = MyClass()
        y[: tuple[MyClass, MyClass]] = (MyClass(), MyClass())
        a[: MyClass], b[: MyClass] = MyClass(), MyClass()
        c[: MyClass], d[: MyClass] = (MyClass(), MyClass())
        ");
    }

    #[test]
    fn test_generic_init_call() {
        let test = inlay_hint_test(
            r#"
            class MyClass[T, U]:
                def __init__(self, x: list[T], y: tuple[U, U]):
                    self.x = x
                    self.y = y

            x = MyClass([42], ("a", "b"))
            y = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
            a, b = MyClass([42], ("a", "b")), MyClass([42], ("a", "b"))
            c, d = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @r#"
        class MyClass[T, U]:
            def __init__(self, x: list[T], y: tuple[U, U]):
                self.x[: list[T@MyClass]] = x
                self.y[: tuple[U@MyClass, U@MyClass]] = y

        x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
        y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b")))
        a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b"))
        c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b")))

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:24
          |
        2 | class MyClass[T, U]:
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
          |                        ^
        4 |         self.x = x
        5 |         self.y = y
          |
        info: Source
         --> main.py:7:13
          |
        5 |         self.y = y
        6 |
        7 | x = MyClass([42], ("a", "b"))
          |             ^
        8 | y = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
        9 | a, b = MyClass([42], ("a", "b")), MyClass([42], ("a", "b"))
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:36
          |
        2 | class MyClass[T, U]:
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
          |                                    ^
        4 |         self.x = x
        5 |         self.y = y
          |
        info: Source
         --> main.py:7:19
          |
        5 |         self.y = y
        6 |
        7 | x = MyClass([42], ("a", "b"))
          |                   ^
        8 | y = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
        9 | a, b = MyClass([42], ("a", "b")), MyClass([42], ("a", "b"))
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:24
          |
        2 | class MyClass[T, U]:
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
          |                        ^
        4 |         self.x = x
        5 |         self.y = y
          |
        info: Source
          --> main.py:8:14
           |
         7 | x = MyClass([42], ("a", "b"))
         8 | y = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
           |              ^
         9 | a, b = MyClass([42], ("a", "b")), MyClass([42], ("a", "b"))
        10 | c, d = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:36
          |
        2 | class MyClass[T, U]:
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
          |                                    ^
        4 |         self.x = x
        5 |         self.y = y
          |
        info: Source
          --> main.py:8:20
           |
         7 | x = MyClass([42], ("a", "b"))
         8 | y = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
           |                    ^
         9 | a, b = MyClass([42], ("a", "b")), MyClass([42], ("a", "b"))
        10 | c, d = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:24
          |
        2 | class MyClass[T, U]:
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
          |                        ^
        4 |         self.x = x
        5 |         self.y = y
          |
        info: Source
          --> main.py:8:41
           |
         7 | x = MyClass([42], ("a", "b"))
         8 | y = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
           |                                         ^
         9 | a, b = MyClass([42], ("a", "b")), MyClass([42], ("a", "b"))
        10 | c, d = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:36
          |
        2 | class MyClass[T, U]:
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
          |                                    ^
        4 |         self.x = x
        5 |         self.y = y
          |
        info: Source
          --> main.py:8:47
           |
         7 | x = MyClass([42], ("a", "b"))
         8 | y = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
           |                                               ^
         9 | a, b = MyClass([42], ("a", "b")), MyClass([42], ("a", "b"))
        10 | c, d = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:24
          |
        2 | class MyClass[T, U]:
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
          |                        ^
        4 |         self.x = x
        5 |         self.y = y
          |
        info: Source
          --> main.py:9:16
           |
         7 | x = MyClass([42], ("a", "b"))
         8 | y = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
         9 | a, b = MyClass([42], ("a", "b")), MyClass([42], ("a", "b"))
           |                ^
        10 | c, d = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:36
          |
        2 | class MyClass[T, U]:
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
          |                                    ^
        4 |         self.x = x
        5 |         self.y = y
          |
        info: Source
          --> main.py:9:22
           |
         7 | x = MyClass([42], ("a", "b"))
         8 | y = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
         9 | a, b = MyClass([42], ("a", "b")), MyClass([42], ("a", "b"))
           |                      ^
        10 | c, d = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:24
          |
        2 | class MyClass[T, U]:
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
          |                        ^
        4 |         self.x = x
        5 |         self.y = y
          |
        info: Source
          --> main.py:9:43
           |
         7 | x = MyClass([42], ("a", "b"))
         8 | y = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
         9 | a, b = MyClass([42], ("a", "b")), MyClass([42], ("a", "b"))
           |                                           ^
        10 | c, d = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:36
          |
        2 | class MyClass[T, U]:
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
          |                                    ^
        4 |         self.x = x
        5 |         self.y = y
          |
        info: Source
          --> main.py:9:49
           |
         7 | x = MyClass([42], ("a", "b"))
         8 | y = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
         9 | a, b = MyClass([42], ("a", "b")), MyClass([42], ("a", "b"))
           |                                                 ^
        10 | c, d = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:24
          |
        2 | class MyClass[T, U]:
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
          |                        ^
        4 |         self.x = x
        5 |         self.y = y
          |
        info: Source
          --> main.py:10:17
           |
         8 | y = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
         9 | a, b = MyClass([42], ("a", "b")), MyClass([42], ("a", "b"))
        10 | c, d = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
           |                 ^
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:36
          |
        2 | class MyClass[T, U]:
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
          |                                    ^
        4 |         self.x = x
        5 |         self.y = y
          |
        info: Source
          --> main.py:10:23
           |
         8 | y = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
         9 | a, b = MyClass([42], ("a", "b")), MyClass([42], ("a", "b"))
        10 | c, d = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
           |                       ^
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:24
          |
        2 | class MyClass[T, U]:
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
          |                        ^
        4 |         self.x = x
        5 |         self.y = y
          |
        info: Source
          --> main.py:10:44
           |
         8 | y = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
         9 | a, b = MyClass([42], ("a", "b")), MyClass([42], ("a", "b"))
        10 | c, d = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
           |                                            ^
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:36
          |
        2 | class MyClass[T, U]:
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
          |                                    ^
        4 |         self.x = x
        5 |         self.y = y
          |
        info: Source
          --> main.py:10:50
           |
         8 | y = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
         9 | a, b = MyClass([42], ("a", "b")), MyClass([42], ("a", "b"))
        10 | c, d = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
           |                                                  ^
           |
        "#);
    }

    #[test]
    fn test_disabled_variable_types() {
        let test = inlay_hint_test(
            "
            def i(x: int, /) -> int:
                return x

            x = i(1)
            ",
        );

        assert_snapshot!(
            test.inlay_hints_with_settings(&InlayHintSettings {
                variable_types: false,
                ..Default::default()
            }),
            @r"
        def i(x: int, /) -> int:
            return x

        x = i(1)
        "
        );
    }

    #[test]
    fn test_function_call_with_positional_or_keyword_parameter() {
        let test = inlay_hint_test(
            "
            def foo(x: int): pass
            foo(1)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(x: int): pass
        foo([x=]1)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int): pass
          |         ^
        3 | foo(1)
          |
        info: Source
         --> main.py:3:5
          |
        2 | def foo(x: int): pass
        3 | foo(1)
          |     ^
          |
        ");
    }

    #[test]
    fn test_function_call_with_positional_or_keyword_parameter_redundant_name() {
        let test = inlay_hint_test(
            "
            def foo(x: int): pass
            x = 1
            y = 2
            foo(x)
            foo(y)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(x: int): pass
        x = 1
        y = 2
        foo(x)
        foo([x=]y)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int): pass
          |         ^
        3 | x = 1
        4 | y = 2
          |
        info: Source
         --> main.py:6:5
          |
        4 | y = 2
        5 | foo(x)
        6 | foo(y)
          |     ^
          |
        ");
    }

    #[test]
    fn test_function_call_with_positional_or_keyword_parameter_redundant_attribute() {
        let test = inlay_hint_test(
            "
            def foo(x: int): pass
            class MyClass:
                def __init__(self):
                    self.x: int = 1
                    self.y: int = 2
            val = MyClass()

            foo(val.x)
            foo(val.y)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(x: int): pass
        class MyClass:
            def __init__(self):
                self.x: int = 1
                self.y: int = 2
        val[: MyClass] = MyClass()

        foo(val.x)
        foo([x=]val.y)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int): pass
          |         ^
        3 | class MyClass:
        4 |     def __init__(self):
          |
        info: Source
          --> main.py:10:5
           |
         9 | foo(val.x)
        10 | foo(val.y)
           |     ^
           |
        ");
    }

    #[test]
    fn test_function_call_with_positional_or_keyword_parameter_redundant_attribute_not() {
        // This one checks that we don't allow elide `x=` for `x.y`
        let test = inlay_hint_test(
            "
            def foo(x: int): pass
            class MyClass:
                def __init__(self):
                    self.x: int = 1
                    self.y: int = 2
            x = MyClass()

            foo(x.x)
            foo(x.y)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(x: int): pass
        class MyClass:
            def __init__(self):
                self.x: int = 1
                self.y: int = 2
        x[: MyClass] = MyClass()

        foo(x.x)
        foo([x=]x.y)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int): pass
          |         ^
        3 | class MyClass:
        4 |     def __init__(self):
          |
        info: Source
          --> main.py:10:5
           |
         9 | foo(x.x)
        10 | foo(x.y)
           |     ^
           |
        ");
    }

    #[test]
    fn test_function_call_with_positional_or_keyword_parameter_redundant_call() {
        let test = inlay_hint_test(
            "
            def foo(x: int): pass
            class MyClass:
                def __init__(self):
                def x() -> int:
                    return 1
                def y() -> int:
                    return 2
            val = MyClass()

            foo(val.x())
            foo(val.y())",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(x: int): pass
        class MyClass:
            def __init__(self):
            def x() -> int:
                return 1
            def y() -> int:
                return 2
        val[: MyClass] = MyClass()

        foo(val.x())
        foo([x=]val.y())
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int): pass
          |         ^
        3 | class MyClass:
        4 |     def __init__(self):
          |
        info: Source
          --> main.py:12:5
           |
        11 | foo(val.x())
        12 | foo(val.y())
           |     ^
           |
        ");
    }

    #[test]
    fn test_function_call_with_positional_or_keyword_parameter_redundant_complex() {
        let test = inlay_hint_test(
            "
            from typing import List

            def foo(x: int): pass
            class MyClass:
                def __init__(self):
                def x() -> List[int]:
                    return 1
                def y() -> List[int]:
                    return 2
            val = MyClass()

            foo(val.x()[0])
            foo(val.y()[1])",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        from typing import List

        def foo(x: int): pass
        class MyClass:
            def __init__(self):
            def x() -> List[int]:
                return 1
            def y() -> List[int]:
                return 2
        val[: MyClass] = MyClass()

        foo(val.x()[0])
        foo([x=]val.y()[1])
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:4:9
          |
        2 | from typing import List
        3 |
        4 | def foo(x: int): pass
          |         ^
        5 | class MyClass:
        6 |     def __init__(self):
          |
        info: Source
          --> main.py:14:5
           |
        13 | foo(val.x()[0])
        14 | foo(val.y()[1])
           |     ^
           |
        ");
    }

    #[test]
    fn test_function_call_with_positional_or_keyword_parameter_redundant_subscript() {
        let test = inlay_hint_test(
            "
            def foo(x: int): pass
            x = [1]
            y = [2]

            foo(x[0])
            foo(y[0])",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(x: int): pass
        x[: list[Unknown | int]] = [1]
        y[: list[Unknown | int]] = [2]

        foo(x[0])
        foo([x=]y[0])
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int): pass
          |         ^
        3 | x = [1]
        4 | y = [2]
          |
        info: Source
         --> main.py:7:5
          |
        6 | foo(x[0])
        7 | foo(y[0])
          |     ^
          |
        ");
    }

    #[test]
    fn test_function_call_with_positional_only_parameter() {
        let test = inlay_hint_test(
            "
            def foo(x: int, /): pass
            foo(1)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(x: int, /): pass
        foo(1)
        ");
    }

    #[test]
    fn test_function_call_with_variadic_parameter() {
        let test = inlay_hint_test(
            "
            def foo(*args: int): pass
            foo(1)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(*args: int): pass
        foo(1)
        ");
    }

    #[test]
    fn test_function_call_with_keyword_variadic_parameter() {
        let test = inlay_hint_test(
            "
            def foo(**kwargs: int): pass
            foo(x=1)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(**kwargs: int): pass
        foo(x=1)
        ");
    }

    #[test]
    fn test_function_call_with_keyword_only_parameter() {
        let test = inlay_hint_test(
            "
            def foo(*, x: int): pass
            foo(x=1)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(*, x: int): pass
        foo(x=1)
        ");
    }

    #[test]
    fn test_function_call_positional_only_and_positional_or_keyword_parameters() {
        let test = inlay_hint_test(
            "
            def foo(x: int, /, y: int): pass
            foo(1, 2)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(x: int, /, y: int): pass
        foo(1, [y=]2)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:20
          |
        2 | def foo(x: int, /, y: int): pass
          |                    ^
        3 | foo(1, 2)
          |
        info: Source
         --> main.py:3:8
          |
        2 | def foo(x: int, /, y: int): pass
        3 | foo(1, 2)
          |        ^
          |
        ");
    }

    #[test]
    fn test_function_call_positional_only_and_variadic_parameters() {
        let test = inlay_hint_test(
            "
            def foo(x: int, /, *args: int): pass
            foo(1, 2, 3)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(x: int, /, *args: int): pass
        foo(1, 2, 3)
        ");
    }

    #[test]
    fn test_function_call_positional_only_and_keyword_variadic_parameters() {
        let test = inlay_hint_test(
            "
            def foo(x: int, /, **kwargs: int): pass
            foo(1, x=2)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(x: int, /, **kwargs: int): pass
        foo(1, x=2)
        ");
    }

    #[test]
    fn test_class_constructor_call_init() {
        let test = inlay_hint_test(
            "
            class Foo:
                def __init__(self, x: int): pass
            Foo(1)
            f = Foo(1)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        class Foo:
            def __init__(self, x: int): pass
        Foo([x=]1)
        f[: Foo] = Foo([x=]1)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:24
          |
        2 | class Foo:
        3 |     def __init__(self, x: int): pass
          |                        ^
        4 | Foo(1)
        5 | f = Foo(1)
          |
        info: Source
         --> main.py:4:5
          |
        2 | class Foo:
        3 |     def __init__(self, x: int): pass
        4 | Foo(1)
          |     ^
        5 | f = Foo(1)
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:24
          |
        2 | class Foo:
        3 |     def __init__(self, x: int): pass
          |                        ^
        4 | Foo(1)
        5 | f = Foo(1)
          |
        info: Source
         --> main.py:5:9
          |
        3 |     def __init__(self, x: int): pass
        4 | Foo(1)
        5 | f = Foo(1)
          |         ^
          |
        ");
    }

    #[test]
    fn test_class_constructor_call_new() {
        let test = inlay_hint_test(
            "
            class Foo:
                def __new__(cls, x: int): pass
            Foo(1)
            f = Foo(1)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        class Foo:
            def __new__(cls, x: int): pass
        Foo([x=]1)
        f[: Foo] = Foo([x=]1)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:22
          |
        2 | class Foo:
        3 |     def __new__(cls, x: int): pass
          |                      ^
        4 | Foo(1)
        5 | f = Foo(1)
          |
        info: Source
         --> main.py:4:5
          |
        2 | class Foo:
        3 |     def __new__(cls, x: int): pass
        4 | Foo(1)
          |     ^
        5 | f = Foo(1)
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:22
          |
        2 | class Foo:
        3 |     def __new__(cls, x: int): pass
          |                      ^
        4 | Foo(1)
        5 | f = Foo(1)
          |
        info: Source
         --> main.py:5:9
          |
        3 |     def __new__(cls, x: int): pass
        4 | Foo(1)
        5 | f = Foo(1)
          |         ^
          |
        ");
    }

    #[test]
    fn test_class_constructor_call_meta_class_call() {
        let test = inlay_hint_test(
            "
            class MetaFoo:
                def __call__(self, x: int): pass
            class Foo(metaclass=MetaFoo):
                pass
            Foo(1)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        class MetaFoo:
            def __call__(self, x: int): pass
        class Foo(metaclass=MetaFoo):
            pass
        Foo([x=]1)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:24
          |
        2 | class MetaFoo:
        3 |     def __call__(self, x: int): pass
          |                        ^
        4 | class Foo(metaclass=MetaFoo):
        5 |     pass
          |
        info: Source
         --> main.py:6:5
          |
        4 | class Foo(metaclass=MetaFoo):
        5 |     pass
        6 | Foo(1)
          |     ^
          |
        ");
    }

    #[test]
    fn test_callable_call() {
        let test = inlay_hint_test(
            "
            from typing import Callable
            def foo(x: Callable[[int], int]):
                x(1)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        from typing import Callable
        def foo(x: Callable[[int], int]):
            x(1)
        ");
    }

    #[test]
    fn test_instance_method_call() {
        let test = inlay_hint_test(
            "
            class Foo:
                def bar(self, y: int): pass
            Foo().bar(2)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        class Foo:
            def bar(self, y: int): pass
        Foo().bar([y=]2)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:19
          |
        2 | class Foo:
        3 |     def bar(self, y: int): pass
          |                   ^
        4 | Foo().bar(2)
          |
        info: Source
         --> main.py:4:11
          |
        2 | class Foo:
        3 |     def bar(self, y: int): pass
        4 | Foo().bar(2)
          |           ^
          |
        ");
    }

    #[test]
    fn test_class_method_call() {
        let test = inlay_hint_test(
            "
            class Foo:
                @classmethod
                def bar(cls, y: int): pass
            Foo.bar(2)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        class Foo:
            @classmethod
            def bar(cls, y: int): pass
        Foo.bar([y=]2)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:4:18
          |
        2 | class Foo:
        3 |     @classmethod
        4 |     def bar(cls, y: int): pass
          |                  ^
        5 | Foo.bar(2)
          |
        info: Source
         --> main.py:5:9
          |
        3 |     @classmethod
        4 |     def bar(cls, y: int): pass
        5 | Foo.bar(2)
          |         ^
          |
        ");
    }

    #[test]
    fn test_static_method_call() {
        let test = inlay_hint_test(
            "
            class Foo:
                @staticmethod
                def bar(y: int): pass
            Foo.bar(2)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        class Foo:
            @staticmethod
            def bar(y: int): pass
        Foo.bar([y=]2)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:4:13
          |
        2 | class Foo:
        3 |     @staticmethod
        4 |     def bar(y: int): pass
          |             ^
        5 | Foo.bar(2)
          |
        info: Source
         --> main.py:5:9
          |
        3 |     @staticmethod
        4 |     def bar(y: int): pass
        5 | Foo.bar(2)
          |         ^
          |
        ");
    }

    #[test]
    fn test_function_call_with_union_type() {
        let test = inlay_hint_test(
            "
            def foo(x: int | str): pass
            foo(1)
            foo('abc')",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(x: int | str): pass
        foo([x=]1)
        foo([x=]'abc')
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int | str): pass
          |         ^
        3 | foo(1)
        4 | foo('abc')
          |
        info: Source
         --> main.py:3:5
          |
        2 | def foo(x: int | str): pass
        3 | foo(1)
          |     ^
        4 | foo('abc')
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int | str): pass
          |         ^
        3 | foo(1)
        4 | foo('abc')
          |
        info: Source
         --> main.py:4:5
          |
        2 | def foo(x: int | str): pass
        3 | foo(1)
        4 | foo('abc')
          |     ^
          |
        ");
    }

    #[test]
    fn test_function_call_multiple_positional_arguments() {
        let test = inlay_hint_test(
            "
            def foo(x: int, y: str, z: bool): pass
            foo(1, 'hello', True)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(x: int, y: str, z: bool): pass
        foo([x=]1, [y=]'hello', [z=]True)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int, y: str, z: bool): pass
          |         ^
        3 | foo(1, 'hello', True)
          |
        info: Source
         --> main.py:3:5
          |
        2 | def foo(x: int, y: str, z: bool): pass
        3 | foo(1, 'hello', True)
          |     ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:17
          |
        2 | def foo(x: int, y: str, z: bool): pass
          |                 ^
        3 | foo(1, 'hello', True)
          |
        info: Source
         --> main.py:3:8
          |
        2 | def foo(x: int, y: str, z: bool): pass
        3 | foo(1, 'hello', True)
          |        ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:25
          |
        2 | def foo(x: int, y: str, z: bool): pass
          |                         ^
        3 | foo(1, 'hello', True)
          |
        info: Source
         --> main.py:3:17
          |
        2 | def foo(x: int, y: str, z: bool): pass
        3 | foo(1, 'hello', True)
          |                 ^
          |
        ");
    }

    #[test]
    fn test_function_call_mixed_positional_and_keyword() {
        let test = inlay_hint_test(
            "
            def foo(x: int, y: str, z: bool): pass
            foo(1, z=True, y='hello')",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(x: int, y: str, z: bool): pass
        foo([x=]1, z=True, y='hello')
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int, y: str, z: bool): pass
          |         ^
        3 | foo(1, z=True, y='hello')
          |
        info: Source
         --> main.py:3:5
          |
        2 | def foo(x: int, y: str, z: bool): pass
        3 | foo(1, z=True, y='hello')
          |     ^
          |
        ");
    }

    #[test]
    fn test_function_call_with_default_parameters() {
        let test = inlay_hint_test(
            "
            def foo(x: int, y: str = 'default', z: bool = False): pass
            foo(1)
            foo(1, 'custom')
            foo(1, 'custom', True)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(x: int, y: str = 'default', z: bool = False): pass
        foo([x=]1)
        foo([x=]1, [y=]'custom')
        foo([x=]1, [y=]'custom', [z=]True)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int, y: str = 'default', z: bool = False): pass
          |         ^
        3 | foo(1)
        4 | foo(1, 'custom')
          |
        info: Source
         --> main.py:3:5
          |
        2 | def foo(x: int, y: str = 'default', z: bool = False): pass
        3 | foo(1)
          |     ^
        4 | foo(1, 'custom')
        5 | foo(1, 'custom', True)
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int, y: str = 'default', z: bool = False): pass
          |         ^
        3 | foo(1)
        4 | foo(1, 'custom')
          |
        info: Source
         --> main.py:4:5
          |
        2 | def foo(x: int, y: str = 'default', z: bool = False): pass
        3 | foo(1)
        4 | foo(1, 'custom')
          |     ^
        5 | foo(1, 'custom', True)
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:17
          |
        2 | def foo(x: int, y: str = 'default', z: bool = False): pass
          |                 ^
        3 | foo(1)
        4 | foo(1, 'custom')
          |
        info: Source
         --> main.py:4:8
          |
        2 | def foo(x: int, y: str = 'default', z: bool = False): pass
        3 | foo(1)
        4 | foo(1, 'custom')
          |        ^
        5 | foo(1, 'custom', True)
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int, y: str = 'default', z: bool = False): pass
          |         ^
        3 | foo(1)
        4 | foo(1, 'custom')
          |
        info: Source
         --> main.py:5:5
          |
        3 | foo(1)
        4 | foo(1, 'custom')
        5 | foo(1, 'custom', True)
          |     ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:17
          |
        2 | def foo(x: int, y: str = 'default', z: bool = False): pass
          |                 ^
        3 | foo(1)
        4 | foo(1, 'custom')
          |
        info: Source
         --> main.py:5:8
          |
        3 | foo(1)
        4 | foo(1, 'custom')
        5 | foo(1, 'custom', True)
          |        ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:37
          |
        2 | def foo(x: int, y: str = 'default', z: bool = False): pass
          |                                     ^
        3 | foo(1)
        4 | foo(1, 'custom')
          |
        info: Source
         --> main.py:5:18
          |
        3 | foo(1)
        4 | foo(1, 'custom')
        5 | foo(1, 'custom', True)
          |                  ^
          |
        ");
    }

    #[test]
    fn test_nested_function_calls() {
        let test = inlay_hint_test(
            "
            def foo(x: int) -> int:
                return x * 2

            def bar(y: str) -> str:
                return y

            def baz(a: int, b: str, c: bool): pass

            baz(foo(5), bar(bar('test')), True)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(x: int) -> int:
            return x * 2

        def bar(y: str) -> str:
            return y

        def baz(a: int, b: str, c: bool): pass

        baz([a=]foo([x=]5), [b=]bar([y=]bar([y=]'test')), [c=]True)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
          --> main.py:8:9
           |
         6 |     return y
         7 |
         8 | def baz(a: int, b: str, c: bool): pass
           |         ^
         9 |
        10 | baz(foo(5), bar(bar('test')), True)
           |
        info: Source
          --> main.py:10:5
           |
         8 | def baz(a: int, b: str, c: bool): pass
         9 |
        10 | baz(foo(5), bar(bar('test')), True)
           |     ^
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int) -> int:
          |         ^
        3 |     return x * 2
          |
        info: Source
          --> main.py:10:9
           |
         8 | def baz(a: int, b: str, c: bool): pass
         9 |
        10 | baz(foo(5), bar(bar('test')), True)
           |         ^
           |

        info[inlay-hint-location]: Inlay Hint Target
          --> main.py:8:17
           |
         6 |     return y
         7 |
         8 | def baz(a: int, b: str, c: bool): pass
           |                 ^
         9 |
        10 | baz(foo(5), bar(bar('test')), True)
           |
        info: Source
          --> main.py:10:13
           |
         8 | def baz(a: int, b: str, c: bool): pass
         9 |
        10 | baz(foo(5), bar(bar('test')), True)
           |             ^
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:5:9
          |
        3 |     return x * 2
        4 |
        5 | def bar(y: str) -> str:
          |         ^
        6 |     return y
          |
        info: Source
          --> main.py:10:17
           |
         8 | def baz(a: int, b: str, c: bool): pass
         9 |
        10 | baz(foo(5), bar(bar('test')), True)
           |                 ^
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:5:9
          |
        3 |     return x * 2
        4 |
        5 | def bar(y: str) -> str:
          |         ^
        6 |     return y
          |
        info: Source
          --> main.py:10:21
           |
         8 | def baz(a: int, b: str, c: bool): pass
         9 |
        10 | baz(foo(5), bar(bar('test')), True)
           |                     ^
           |

        info[inlay-hint-location]: Inlay Hint Target
          --> main.py:8:25
           |
         6 |     return y
         7 |
         8 | def baz(a: int, b: str, c: bool): pass
           |                         ^
         9 |
        10 | baz(foo(5), bar(bar('test')), True)
           |
        info: Source
          --> main.py:10:31
           |
         8 | def baz(a: int, b: str, c: bool): pass
         9 |
        10 | baz(foo(5), bar(bar('test')), True)
           |                               ^
           |
        ");
    }

    #[test]
    fn test_method_chaining() {
        let test = inlay_hint_test(
            "
            class A:
                def foo(self, value: int) -> 'A':
                    return self
                def bar(self, name: str) -> 'A':
                    return self
                def baz(self): pass
            A().foo(42).bar('test').baz()",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        class A:
            def foo(self, value: int) -> 'A':
                return self
            def bar(self, name: str) -> 'A':
                return self
            def baz(self): pass
        A().foo([value=]42).bar([name=]'test').baz()
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:19
          |
        2 | class A:
        3 |     def foo(self, value: int) -> 'A':
          |                   ^^^^^
        4 |         return self
        5 |     def bar(self, name: str) -> 'A':
          |
        info: Source
         --> main.py:8:9
          |
        6 |         return self
        7 |     def baz(self): pass
        8 | A().foo(42).bar('test').baz()
          |         ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:5:19
          |
        3 |     def foo(self, value: int) -> 'A':
        4 |         return self
        5 |     def bar(self, name: str) -> 'A':
          |                   ^^^^
        6 |         return self
        7 |     def baz(self): pass
          |
        info: Source
         --> main.py:8:17
          |
        6 |         return self
        7 |     def baz(self): pass
        8 | A().foo(42).bar('test').baz()
          |                 ^
          |
        ");
    }

    #[test]
    fn test_nexted_keyword_function_calls() {
        let test = inlay_hint_test(
            "
            def foo(x: str) -> str:
                return x
            def bar(y: int): pass
            bar(y=foo('test'))
            ",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(x: str) -> str:
            return x
        def bar(y: int): pass
        bar(y=foo([x=]'test'))

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: str) -> str:
          |         ^
        3 |     return x
        4 | def bar(y: int): pass
          |
        info: Source
         --> main.py:5:11
          |
        3 |     return x
        4 | def bar(y: int): pass
        5 | bar(y=foo('test'))
          |           ^
          |
        ");
    }

    #[test]
    fn test_lambda_function_calls() {
        let test = inlay_hint_test(
            "
            foo = lambda x: x * 2
            bar = lambda a, b: a + b
            foo(5)
            bar(1, 2)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        foo[: (x) -> Unknown] = lambda x: x * 2
        bar[: (a, b) -> Unknown] = lambda a, b: a + b
        foo([x=]5)
        bar([a=]1, [b=]2)
        ");
    }

    #[test]
    fn test_complex_parameter_combinations() {
        let test = inlay_hint_test(
            "
            def foo(a: int, b: str, /, c: float, d: bool = True, *, e: int, f: str = 'default'): pass
            foo(1, 'pos', 3.14, False, e=42)
            foo(1, 'pos', 3.14, e=42, f='custom')",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(a: int, b: str, /, c: float, d: bool = True, *, e: int, f: str = 'default'): pass
        foo(1, 'pos', [c=]3.14, [d=]False, e=42)
        foo(1, 'pos', [c=]3.14, e=42, f='custom')
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:28
          |
        2 | def foo(a: int, b: str, /, c: float, d: bool = True, *, e: int, f: str = 'default'): pass
          |                            ^
        3 | foo(1, 'pos', 3.14, False, e=42)
        4 | foo(1, 'pos', 3.14, e=42, f='custom')
          |
        info: Source
         --> main.py:3:15
          |
        2 | def foo(a: int, b: str, /, c: float, d: bool = True, *, e: int, f: str = 'default'): pass
        3 | foo(1, 'pos', 3.14, False, e=42)
          |               ^
        4 | foo(1, 'pos', 3.14, e=42, f='custom')
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:38
          |
        2 | def foo(a: int, b: str, /, c: float, d: bool = True, *, e: int, f: str = 'default'): pass
          |                                      ^
        3 | foo(1, 'pos', 3.14, False, e=42)
        4 | foo(1, 'pos', 3.14, e=42, f='custom')
          |
        info: Source
         --> main.py:3:21
          |
        2 | def foo(a: int, b: str, /, c: float, d: bool = True, *, e: int, f: str = 'default'): pass
        3 | foo(1, 'pos', 3.14, False, e=42)
          |                     ^
        4 | foo(1, 'pos', 3.14, e=42, f='custom')
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:28
          |
        2 | def foo(a: int, b: str, /, c: float, d: bool = True, *, e: int, f: str = 'default'): pass
          |                            ^
        3 | foo(1, 'pos', 3.14, False, e=42)
        4 | foo(1, 'pos', 3.14, e=42, f='custom')
          |
        info: Source
         --> main.py:4:15
          |
        2 | def foo(a: int, b: str, /, c: float, d: bool = True, *, e: int, f: str = 'default'): pass
        3 | foo(1, 'pos', 3.14, False, e=42)
        4 | foo(1, 'pos', 3.14, e=42, f='custom')
          |               ^
          |
        ");
    }

    #[test]
    fn test_function_calls_different_file() {
        let mut test = inlay_hint_test(
            "
            from foo import bar

            bar(1)",
        );

        test.with_extra_file(
            "foo.py",
            "
        def bar(x: int | str):
            pass",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        from foo import bar

        bar([x=]1)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> foo.py:2:17
          |
        2 |         def bar(x: int | str):
          |                 ^
        3 |             pass
          |
        info: Source
         --> main.py:4:5
          |
        2 | from foo import bar
        3 |
        4 | bar(1)
          |     ^
          |
        ");
    }

    #[test]
    fn test_overloaded_function_calls() {
        let test = inlay_hint_test(
            "
            from typing import overload

            @overload
            def foo(x: int) -> str: ...
            @overload
            def foo(x: str) -> int: ...
            def foo(x):
                return x

            foo(42)
            foo('hello')",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        from typing import overload

        @overload
        def foo(x: int) -> str: ...
        @overload
        def foo(x: str) -> int: ...
        def foo(x):
            return x

        foo([x=]42)
        foo([x=]'hello')
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:5:9
          |
        4 | @overload
        5 | def foo(x: int) -> str: ...
          |         ^
        6 | @overload
        7 | def foo(x: str) -> int: ...
          |
        info: Source
          --> main.py:11:5
           |
         9 |     return x
        10 |
        11 | foo(42)
           |     ^
        12 | foo('hello')
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:5:9
          |
        4 | @overload
        5 | def foo(x: int) -> str: ...
          |         ^
        6 | @overload
        7 | def foo(x: str) -> int: ...
          |
        info: Source
          --> main.py:12:5
           |
        11 | foo(42)
        12 | foo('hello')
           |     ^
           |
        ");
    }

    #[test]
    fn test_disabled_function_argument_names() {
        let test = inlay_hint_test(
            "
        def foo(x: int): pass
        foo(1)",
        );

        assert_snapshot!(test.inlay_hints_with_settings(&InlayHintSettings {
            call_argument_names: false,
            ..Default::default()
        }), @r"
        def foo(x: int): pass
        foo(1)
        ");
    }

    #[test]
    fn test_function_call_out_of_range() {
        let test = inlay_hint_test(
            "
            <START>def foo(x: int): pass
            def bar(y: int): pass
            foo(1)<END>
            bar(2)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(x: int): pass
        def bar(y: int): pass
        foo([x=]1)
        bar(2)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int): pass
          |         ^
        3 | def bar(y: int): pass
        4 | foo(1)
          |
        info: Source
         --> main.py:4:5
          |
        2 | def foo(x: int): pass
        3 | def bar(y: int): pass
        4 | foo(1)
          |     ^
        5 | bar(2)
          |
        ");
    }

    #[test]
    fn test_function_call_with_argument_name_starting_with_underscore() {
        let test = inlay_hint_test(
            "
            def foo(_x: int, y: int): pass
            foo(1, 2)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(_x: int, y: int): pass
        foo(1, [y=]2)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:18
          |
        2 | def foo(_x: int, y: int): pass
          |                  ^
        3 | foo(1, 2)
          |
        info: Source
         --> main.py:3:8
          |
        2 | def foo(_x: int, y: int): pass
        3 | foo(1, 2)
          |        ^
          |
        ");
    }

    #[test]
    fn test_function_call_different_formatting() {
        let test = inlay_hint_test(
            "
            def foo(
                x: int,
                y: int
            ): ...

            foo(1, 2)",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        def foo(
            x: int,
            y: int
        ): ...

        foo([x=]1, [y=]2)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:3:5
          |
        2 | def foo(
        3 |     x: int,
          |     ^
        4 |     y: int
        5 | ): ...
          |
        info: Source
         --> main.py:7:5
          |
        5 | ): ...
        6 |
        7 | foo(1, 2)
          |     ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:4:5
          |
        2 | def foo(
        3 |     x: int,
        4 |     y: int
          |     ^
        5 | ): ...
          |
        info: Source
         --> main.py:7:8
          |
        5 | ): ...
        6 |
        7 | foo(1, 2)
          |        ^
          |
        ");
    }

    struct InlayHintLocationDiagnostic {
        source: FileRange,
        target: FileRange,
    }

    impl InlayHintLocationDiagnostic {
        fn new(source: FileRange, target: &NavigationTarget) -> Self {
            Self {
                source,
                target: FileRange::new(target.file(), target.focus_range()),
            }
        }
    }

    impl IntoDiagnostic for InlayHintLocationDiagnostic {
        fn into_diagnostic(self) -> Diagnostic {
            let mut source = SubDiagnostic::new(SubDiagnosticSeverity::Info, "Source");

            source.annotate(Annotation::primary(
                Span::from(self.source.file()).with_range(self.source.range()),
            ));

            let mut main = Diagnostic::new(
                DiagnosticId::Lint(LintName::of("inlay-hint-location")),
                Severity::Info,
                "Inlay Hint Target".to_string(),
            );

            main.annotate(Annotation::primary(
                Span::from(self.target.file()).with_range(self.target.range()),
            ));

            main.sub(source);

            main
        }
    }
}
