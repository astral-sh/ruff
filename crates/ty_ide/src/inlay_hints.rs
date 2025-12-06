use std::{fmt, vec};

use crate::{Db, HasNavigationTargets, NavigationTarget};
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::visitor::source_order::{self, SourceOrderVisitor, TraversalSignal};
use ruff_python_ast::{AnyNodeRef, ArgOrKeyword, Expr, ExprUnaryOp, Stmt, UnaryOp};
use ruff_text_size::{Ranged, TextRange, TextSize};
use ty_python_semantic::types::ide_support::inlay_hint_call_argument_details;
use ty_python_semantic::types::{Type, TypeDetail};
use ty_python_semantic::{HasType, SemanticModel};

#[derive(Debug, Clone)]
pub struct InlayHint {
    pub position: TextSize,
    pub kind: InlayHintKind,
    pub label: InlayHintLabel,
    pub text_edits: Vec<InlayHintTextEdit>,
}

impl InlayHint {
    fn variable_type(expr: &Expr, ty: Type, db: &dyn Db, allow_edits: bool) -> Self {
        let position = expr.range().end();
        // Render the type to a string, and get subspans for all the types that make it up
        let details = ty.display(db).to_string_parts();

        // Ok so the idea here is that we potentially have a random soup of spans here,
        // and each byte of the string can have at most one target associate with it.
        // Thankfully, they were generally pushed in print order, with the inner smaller types
        // appearing before the outer bigger ones.
        //
        // So we record where we are in the string, and every time we find a type, we
        // check if it's further along in the string. If it is, great, we give it the
        // span for its range, and then advance where we are.
        let mut offset = 0;
        let mut label_parts = vec![": ".into()];
        for (target, detail) in details.targets.iter().zip(&details.details) {
            match detail {
                TypeDetail::Type(ty) => {
                    let start = target.start().to_usize();
                    let end = target.end().to_usize();
                    // If we skipped over some bytes, push them with no target
                    if start > offset {
                        label_parts.push(details.label[offset..start].into());
                    }
                    // Ok, this is the first type that claimed these bytes, give it the target
                    if start >= offset {
                        let target = ty.navigation_targets(db).into_iter().next();
                        label_parts.push(
                            InlayHintLabelPart::new(&details.label[start..end]).with_target(target),
                        );
                        offset = end;
                    }
                }
                TypeDetail::SignatureStart
                | TypeDetail::SignatureEnd
                | TypeDetail::Parameter(_) => {
                    // Don't care about these
                }
            }
        }
        // "flush" the rest of the label without any target
        if offset < details.label.len() {
            label_parts.push(details.label[offset..details.label.len()].into());
        }

        let text_edits = if details.is_valid_syntax && allow_edits {
            vec![InlayHintTextEdit {
                range: TextRange::new(position, position),
                new_text: format!(": {}", details.label),
            }]
        } else {
            vec![]
        };

        Self {
            position,
            kind: InlayHintKind::Type,
            label: InlayHintLabel { parts: label_parts },
            text_edits,
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
            text_edits: vec![],
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

#[derive(Debug, Clone)]
pub struct InlayHintTextEdit {
    pub range: TextRange,
    pub new_text: String,
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
    in_no_edits_allowed: bool,
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
            in_no_edits_allowed: false,
        }
    }

    fn add_type_hint(&mut self, expr: &Expr, ty: Type<'db>, allow_edits: bool) {
        if !self.settings.variable_types {
            return;
        }

        let inlay_hint = InlayHint::variable_type(expr, ty, self.db, allow_edits);

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
                if !annotations_are_valid_syntax(assign) {
                    self.in_no_edits_allowed = true;
                }
                for target in &assign.targets {
                    self.visit_expr(target);
                }
                self.in_no_edits_allowed = false;
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
                        self.add_type_hint(expr, ty, !self.in_no_edits_allowed);
                    }
                }
                source_order::walk_expr(self, expr);
            }
            Expr::Attribute(attribute) => {
                if self.in_assignment {
                    if attribute.ctx.is_store() {
                        let ty = expr.inferred_type(&self.model);
                        self.add_type_hint(expr, ty, !self.in_no_edits_allowed);
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

fn annotations_are_valid_syntax(stmt_assign: &ruff_python_ast::StmtAssign) -> bool {
    if stmt_assign.targets.len() > 1 {
        return false;
    }

    if stmt_assign
        .targets
        .iter()
        .any(|target| matches!(target, Expr::Tuple(_)))
    {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::NavigationTarget;
    use crate::tests::IntoDiagnostic;
    use insta::{assert_snapshot, internals::SettingsBindDropGuard};
    use itertools::Itertools;
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

        let mut insta_settings = insta::Settings::clone_current();
        insta_settings.add_filter(r#"\\(\w\w|\.|")"#, "/$1");
        // Filter out TODO types because they are different between debug and release builds.
        insta_settings.add_filter(r"@Todo\(.+\)", "@Todo");

        let insta_settings_guard = insta_settings.bind_to_scope();

        InlayHintTest {
            db,
            file,
            range,
            _insta_settings_guard: insta_settings_guard,
        }
    }

    pub(super) struct InlayHintTest {
        pub(super) db: ty_project::TestDb,
        pub(super) file: File,
        pub(super) range: TextRange,
        _insta_settings_guard: SettingsBindDropGuard,
    }

    impl InlayHintTest {
        /// Returns the inlay hints for the given test case.
        ///
        /// All inlay hints are generated using the applicable settings. Use
        /// [`inlay_hints_with_settings`] to generate hints with custom settings.
        ///
        /// [`inlay_hints_with_settings`]: Self::inlay_hints_with_settings
        fn inlay_hints(&mut self) -> String {
            self.inlay_hints_with_settings(&InlayHintSettings {
                variable_types: true,
                call_argument_names: true,
            })
        }

        fn with_extra_file(&mut self, file_name: &str, content: &str) {
            self.db.write_file(file_name, content).unwrap();
        }

        /// Returns the inlay hints for the given test case with custom settings.
        fn inlay_hints_with_settings(&mut self, settings: &InlayHintSettings) -> String {
            let hints = inlay_hints(&self.db, self.file, self.range, settings);

            let mut inlay_hint_buf = source_text(&self.db, self.file).as_str().to_string();
            let mut text_edit_buf = inlay_hint_buf.clone();

            let mut tbd_diagnostics = Vec::new();

            let mut offset = 0;

            let mut edit_offset = 0;

            for hint in hints {
                let end_position = hint.position.to_usize() + offset;
                let mut hint_str = "[".to_string();

                for part in hint.label.parts() {
                    if let Some(target) = part.target().cloned() {
                        let part_position = u32::try_from(end_position + hint_str.len()).unwrap();
                        let part_len = u32::try_from(part.text().len()).unwrap();
                        let label_range =
                            TextRange::at(TextSize::new(part_position), TextSize::new(part_len));
                        tbd_diagnostics.push((label_range, target));
                    }
                    hint_str.push_str(part.text());
                }

                for edit in hint.text_edits {
                    let start = edit.range.start().to_usize() + edit_offset;
                    let end = edit.range.end().to_usize() + edit_offset;

                    text_edit_buf.replace_range(start..end, &edit.new_text);

                    if start == end {
                        edit_offset += edit.new_text.len();
                    } else {
                        edit_offset += edit.new_text.len() - edit.range.len().to_usize();
                    }
                }

                hint_str.push(']');
                offset += hint_str.len();

                inlay_hint_buf.insert_str(end_position, &hint_str);
            }

            self.db.write_file("main2.py", &inlay_hint_buf).unwrap();
            let inlayed_file =
                system_path_to_file(&self.db, "main2.py").expect("newly written file to existing");

            let location_diagnostics = tbd_diagnostics.into_iter().map(|(label_range, target)| {
                InlayHintLocationDiagnostic::new(FileRange::new(inlayed_file, label_range), &target)
            });

            let mut rendered_diagnostics = location_diagnostics
                .map(|diagnostic| self.render_diagnostic(diagnostic))
                .join("");

            if !rendered_diagnostics.is_empty() {
                rendered_diagnostics = format!(
                    "{}{}",
                    crate::MarkupKind::PlainText.horizontal_line(),
                    rendered_diagnostics
                        .strip_suffix("\n")
                        .unwrap_or(&rendered_diagnostics)
                );
            }

            let rendered_edit_diagnostic = if edit_offset != 0 {
                let edit_diagnostic = InlayHintEditDiagnostic::new(text_edit_buf);
                let text_edit_buf = self.render_diagnostic(edit_diagnostic);

                format!(
                    "{}{}",
                    crate::MarkupKind::PlainText.horizontal_line(),
                    text_edit_buf
                )
            } else {
                String::new()
            };

            format!("{inlay_hint_buf}{rendered_diagnostics}{rendered_edit_diagnostic}",)
        }

        fn render_diagnostic<D>(&self, diagnostic: D) -> String
        where
            D: IntoDiagnostic,
        {
            use std::fmt::Write;

            let mut buf = String::new();

            let config = DisplayDiagnosticConfig::default()
                .color(false)
                .format(DiagnosticFormat::Full);

            let diag = diagnostic.into_diagnostic();
            write!(buf, "{}", diag.display(&self.db, &config)).unwrap();

            buf
        }
    }

    #[test]
    fn test_assign_statement() {
        let mut test = inlay_hint_test(
            "
            def i(x: int, /) -> int:
                return x

            x = 1
            y = x
            z = i(1)
            w = z
            aa = b'foo'
            bb = aa
            ",
        );

        assert_snapshot!(test.inlay_hints(), @r#"
        def i(x: int, /) -> int:
            return x

        x = 1
        y[: Literal[1]] = x
        z[: int] = i(1)
        w[: int] = z
        aa = b'foo'
        bb[: Literal[b"foo"]] = aa

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:351:1
            |
        349 | Final: _SpecialForm
        350 |
        351 | Literal: _SpecialForm
            | ^^^^^^^
        352 | TypedDict: _SpecialForm
            |
        info: Source
         --> main2.py:6:5
          |
        5 | x = 1
        6 | y[: Literal[1]] = x
          |     ^^^^^^^
        7 | z[: int] = i(1)
        8 | w[: int] = z
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
         --> main2.py:6:13
          |
        5 | x = 1
        6 | y[: Literal[1]] = x
          |             ^
        7 | z[: int] = i(1)
        8 | w[: int] = z
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
         --> main2.py:7:5
          |
        5 | x = 1
        6 | y[: Literal[1]] = x
        7 | z[: int] = i(1)
          |     ^^^
        8 | w[: int] = z
        9 | aa = b'foo'
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
          --> main2.py:8:5
           |
         6 | y[: Literal[1]] = x
         7 | z[: int] = i(1)
         8 | w[: int] = z
           |     ^^^
         9 | aa = b'foo'
        10 | bb[: Literal[b"foo"]] = aa
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:351:1
            |
        349 | Final: _SpecialForm
        350 |
        351 | Literal: _SpecialForm
            | ^^^^^^^
        352 | TypedDict: _SpecialForm
            |
        info: Source
          --> main2.py:10:6
           |
         8 | w[: int] = z
         9 | aa = b'foo'
        10 | bb[: Literal[b"foo"]] = aa
           |      ^^^^^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:1448:7
             |
        1447 | @disjoint_base
        1448 | class bytes(Sequence[int]):
             |       ^^^^^
        1449 |     """bytes(iterable_of_ints) -> bytes
        1450 |     bytes(string, encoding[, errors]) -> bytes
             |
        info: Source
          --> main2.py:10:14
           |
         8 | w[: int] = z
         9 | aa = b'foo'
        10 | bb[: Literal[b"foo"]] = aa
           |              ^^^^^^
           |

        ---------------------------------------------
        info[inlay-hint-edit]: File after edits
        info: Source

        def i(x: int, /) -> int:
            return x

        x = 1
        y: Literal[1] = x
        z: int = i(1)
        w: int = z
        aa = b'foo'
        bb: Literal[b"foo"] = aa
        "#);
    }

    #[test]
    fn test_unpacked_tuple_assignment() {
        let mut test = inlay_hint_test(
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

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:351:1
            |
        349 | Final: _SpecialForm
        350 |
        351 | Literal: _SpecialForm
            | ^^^^^^^
        352 | TypedDict: _SpecialForm
            |
        info: Source
          --> main2.py:8:6
           |
         7 | x1, y1 = (1, 'abc')
         8 | x2[: Literal[1]], y2[: Literal["abc"]] = (x1, y1)
           |      ^^^^^^^
         9 | x3[: int], y3[: str] = (i(1), s('abc'))
        10 | x4[: int], y4[: str] = (x3, y3)
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
          --> main2.py:8:14
           |
         7 | x1, y1 = (1, 'abc')
         8 | x2[: Literal[1]], y2[: Literal["abc"]] = (x1, y1)
           |              ^
         9 | x3[: int], y3[: str] = (i(1), s('abc'))
        10 | x4[: int], y4[: str] = (x3, y3)
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:351:1
            |
        349 | Final: _SpecialForm
        350 |
        351 | Literal: _SpecialForm
            | ^^^^^^^
        352 | TypedDict: _SpecialForm
            |
        info: Source
          --> main2.py:8:24
           |
         7 | x1, y1 = (1, 'abc')
         8 | x2[: Literal[1]], y2[: Literal["abc"]] = (x1, y1)
           |                        ^^^^^^^
         9 | x3[: int], y3[: str] = (i(1), s('abc'))
        10 | x4[: int], y4[: str] = (x3, y3)
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ^^^
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
          --> main2.py:8:32
           |
         7 | x1, y1 = (1, 'abc')
         8 | x2[: Literal[1]], y2[: Literal["abc"]] = (x1, y1)
           |                                ^^^^^
         9 | x3[: int], y3[: str] = (i(1), s('abc'))
        10 | x4[: int], y4[: str] = (x3, y3)
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
          --> main2.py:9:6
           |
         7 | x1, y1 = (1, 'abc')
         8 | x2[: Literal[1]], y2[: Literal["abc"]] = (x1, y1)
         9 | x3[: int], y3[: str] = (i(1), s('abc'))
           |      ^^^
        10 | x4[: int], y4[: str] = (x3, y3)
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ^^^
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
          --> main2.py:9:17
           |
         7 | x1, y1 = (1, 'abc')
         8 | x2[: Literal[1]], y2[: Literal["abc"]] = (x1, y1)
         9 | x3[: int], y3[: str] = (i(1), s('abc'))
           |                 ^^^
        10 | x4[: int], y4[: str] = (x3, y3)
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
          --> main2.py:10:6
           |
         8 | x2[: Literal[1]], y2[: Literal["abc"]] = (x1, y1)
         9 | x3[: int], y3[: str] = (i(1), s('abc'))
        10 | x4[: int], y4[: str] = (x3, y3)
           |      ^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ^^^
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
          --> main2.py:10:17
           |
         8 | x2[: Literal[1]], y2[: Literal["abc"]] = (x1, y1)
         9 | x3[: int], y3[: str] = (i(1), s('abc'))
        10 | x4[: int], y4[: str] = (x3, y3)
           |                 ^^^
           |
        "#);
    }

    #[test]
    fn test_multiple_assignment() {
        let mut test = inlay_hint_test(
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

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:351:1
            |
        349 | Final: _SpecialForm
        350 |
        351 | Literal: _SpecialForm
            | ^^^^^^^
        352 | TypedDict: _SpecialForm
            |
        info: Source
          --> main2.py:8:6
           |
         7 | x1, y1 = 1, 'abc'
         8 | x2[: Literal[1]], y2[: Literal["abc"]] = x1, y1
           |      ^^^^^^^
         9 | x3[: int], y3[: str] = i(1), s('abc')
        10 | x4[: int], y4[: str] = x3, y3
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
          --> main2.py:8:14
           |
         7 | x1, y1 = 1, 'abc'
         8 | x2[: Literal[1]], y2[: Literal["abc"]] = x1, y1
           |              ^
         9 | x3[: int], y3[: str] = i(1), s('abc')
        10 | x4[: int], y4[: str] = x3, y3
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:351:1
            |
        349 | Final: _SpecialForm
        350 |
        351 | Literal: _SpecialForm
            | ^^^^^^^
        352 | TypedDict: _SpecialForm
            |
        info: Source
          --> main2.py:8:24
           |
         7 | x1, y1 = 1, 'abc'
         8 | x2[: Literal[1]], y2[: Literal["abc"]] = x1, y1
           |                        ^^^^^^^
         9 | x3[: int], y3[: str] = i(1), s('abc')
        10 | x4[: int], y4[: str] = x3, y3
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ^^^
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
          --> main2.py:8:32
           |
         7 | x1, y1 = 1, 'abc'
         8 | x2[: Literal[1]], y2[: Literal["abc"]] = x1, y1
           |                                ^^^^^
         9 | x3[: int], y3[: str] = i(1), s('abc')
        10 | x4[: int], y4[: str] = x3, y3
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
          --> main2.py:9:6
           |
         7 | x1, y1 = 1, 'abc'
         8 | x2[: Literal[1]], y2[: Literal["abc"]] = x1, y1
         9 | x3[: int], y3[: str] = i(1), s('abc')
           |      ^^^
        10 | x4[: int], y4[: str] = x3, y3
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ^^^
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
          --> main2.py:9:17
           |
         7 | x1, y1 = 1, 'abc'
         8 | x2[: Literal[1]], y2[: Literal["abc"]] = x1, y1
         9 | x3[: int], y3[: str] = i(1), s('abc')
           |                 ^^^
        10 | x4[: int], y4[: str] = x3, y3
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
          --> main2.py:10:6
           |
         8 | x2[: Literal[1]], y2[: Literal["abc"]] = x1, y1
         9 | x3[: int], y3[: str] = i(1), s('abc')
        10 | x4[: int], y4[: str] = x3, y3
           |      ^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ^^^
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
          --> main2.py:10:17
           |
         8 | x2[: Literal[1]], y2[: Literal["abc"]] = x1, y1
         9 | x3[: int], y3[: str] = i(1), s('abc')
        10 | x4[: int], y4[: str] = x3, y3
           |                 ^^^
           |
        "#);
    }

    #[test]
    fn test_tuple_assignment() {
        let mut test = inlay_hint_test(
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

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2695:7
             |
        2694 | @disjoint_base
        2695 | class tuple(Sequence[_T_co]):
             |       ^^^^^
        2696 |     """Built-in immutable sequence.
             |
        info: Source
          --> main2.py:8:5
           |
         7 | x = (1, 'abc')
         8 | y[: tuple[Literal[1], Literal["abc"]]] = x
           |     ^^^^^
         9 | z[: tuple[int, str]] = (i(1), s('abc'))
        10 | w[: tuple[int, str]] = z
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:351:1
            |
        349 | Final: _SpecialForm
        350 |
        351 | Literal: _SpecialForm
            | ^^^^^^^
        352 | TypedDict: _SpecialForm
            |
        info: Source
          --> main2.py:8:11
           |
         7 | x = (1, 'abc')
         8 | y[: tuple[Literal[1], Literal["abc"]]] = x
           |           ^^^^^^^
         9 | z[: tuple[int, str]] = (i(1), s('abc'))
        10 | w[: tuple[int, str]] = z
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
          --> main2.py:8:19
           |
         7 | x = (1, 'abc')
         8 | y[: tuple[Literal[1], Literal["abc"]]] = x
           |                   ^
         9 | z[: tuple[int, str]] = (i(1), s('abc'))
        10 | w[: tuple[int, str]] = z
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:351:1
            |
        349 | Final: _SpecialForm
        350 |
        351 | Literal: _SpecialForm
            | ^^^^^^^
        352 | TypedDict: _SpecialForm
            |
        info: Source
          --> main2.py:8:23
           |
         7 | x = (1, 'abc')
         8 | y[: tuple[Literal[1], Literal["abc"]]] = x
           |                       ^^^^^^^
         9 | z[: tuple[int, str]] = (i(1), s('abc'))
        10 | w[: tuple[int, str]] = z
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ^^^
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
          --> main2.py:8:31
           |
         7 | x = (1, 'abc')
         8 | y[: tuple[Literal[1], Literal["abc"]]] = x
           |                               ^^^^^
         9 | z[: tuple[int, str]] = (i(1), s('abc'))
        10 | w[: tuple[int, str]] = z
           |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2695:7
             |
        2694 | @disjoint_base
        2695 | class tuple(Sequence[_T_co]):
             |       ^^^^^
        2696 |     """Built-in immutable sequence.
             |
        info: Source
          --> main2.py:9:5
           |
         7 | x = (1, 'abc')
         8 | y[: tuple[Literal[1], Literal["abc"]]] = x
         9 | z[: tuple[int, str]] = (i(1), s('abc'))
           |     ^^^^^
        10 | w[: tuple[int, str]] = z
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
          --> main2.py:9:11
           |
         7 | x = (1, 'abc')
         8 | y[: tuple[Literal[1], Literal["abc"]]] = x
         9 | z[: tuple[int, str]] = (i(1), s('abc'))
           |           ^^^
        10 | w[: tuple[int, str]] = z
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ^^^
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
          --> main2.py:9:16
           |
         7 | x = (1, 'abc')
         8 | y[: tuple[Literal[1], Literal["abc"]]] = x
         9 | z[: tuple[int, str]] = (i(1), s('abc'))
           |                ^^^
        10 | w[: tuple[int, str]] = z
           |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2695:7
             |
        2694 | @disjoint_base
        2695 | class tuple(Sequence[_T_co]):
             |       ^^^^^
        2696 |     """Built-in immutable sequence.
             |
        info: Source
          --> main2.py:10:5
           |
         8 | y[: tuple[Literal[1], Literal["abc"]]] = x
         9 | z[: tuple[int, str]] = (i(1), s('abc'))
        10 | w[: tuple[int, str]] = z
           |     ^^^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
          --> main2.py:10:11
           |
         8 | y[: tuple[Literal[1], Literal["abc"]]] = x
         9 | z[: tuple[int, str]] = (i(1), s('abc'))
        10 | w[: tuple[int, str]] = z
           |           ^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ^^^
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
          --> main2.py:10:16
           |
         8 | y[: tuple[Literal[1], Literal["abc"]]] = x
         9 | z[: tuple[int, str]] = (i(1), s('abc'))
        10 | w[: tuple[int, str]] = z
           |                ^^^
           |

        ---------------------------------------------
        info[inlay-hint-edit]: File after edits
        info: Source

        def i(x: int, /) -> int:
            return x
        def s(x: str, /) -> str:
            return x

        x = (1, 'abc')
        y: tuple[Literal[1], Literal["abc"]] = x
        z: tuple[int, str] = (i(1), s('abc'))
        w: tuple[int, str] = z
        "#);
    }

    #[test]
    fn test_nested_tuple_assignment() {
        let mut test = inlay_hint_test(
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
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:351:1
            |
        349 | Final: _SpecialForm
        350 |
        351 | Literal: _SpecialForm
            | ^^^^^^^
        352 | TypedDict: _SpecialForm
            |
        info: Source
          --> main2.py:8:6
           |
         7 | x1, (y1, z1) = (1, ('abc', 2))
         8 | x2[: Literal[1]], (y2[: Literal["abc"]], z2[: Literal[2]]) = (x1, (y1, z1))
           |      ^^^^^^^
         9 | x3[: int], (y3[: str], z3[: int]) = (i(1), (s('abc'), i(2)))
        10 | x4[: int], (y4[: str], z4[: int]) = (x3, (y3, z3))
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
          --> main2.py:8:14
           |
         7 | x1, (y1, z1) = (1, ('abc', 2))
         8 | x2[: Literal[1]], (y2[: Literal["abc"]], z2[: Literal[2]]) = (x1, (y1, z1))
           |              ^
         9 | x3[: int], (y3[: str], z3[: int]) = (i(1), (s('abc'), i(2)))
        10 | x4[: int], (y4[: str], z4[: int]) = (x3, (y3, z3))
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:351:1
            |
        349 | Final: _SpecialForm
        350 |
        351 | Literal: _SpecialForm
            | ^^^^^^^
        352 | TypedDict: _SpecialForm
            |
        info: Source
          --> main2.py:8:25
           |
         7 | x1, (y1, z1) = (1, ('abc', 2))
         8 | x2[: Literal[1]], (y2[: Literal["abc"]], z2[: Literal[2]]) = (x1, (y1, z1))
           |                         ^^^^^^^
         9 | x3[: int], (y3[: str], z3[: int]) = (i(1), (s('abc'), i(2)))
        10 | x4[: int], (y4[: str], z4[: int]) = (x3, (y3, z3))
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ^^^
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
          --> main2.py:8:33
           |
         7 | x1, (y1, z1) = (1, ('abc', 2))
         8 | x2[: Literal[1]], (y2[: Literal["abc"]], z2[: Literal[2]]) = (x1, (y1, z1))
           |                                 ^^^^^
         9 | x3[: int], (y3[: str], z3[: int]) = (i(1), (s('abc'), i(2)))
        10 | x4[: int], (y4[: str], z4[: int]) = (x3, (y3, z3))
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:351:1
            |
        349 | Final: _SpecialForm
        350 |
        351 | Literal: _SpecialForm
            | ^^^^^^^
        352 | TypedDict: _SpecialForm
            |
        info: Source
          --> main2.py:8:47
           |
         7 | x1, (y1, z1) = (1, ('abc', 2))
         8 | x2[: Literal[1]], (y2[: Literal["abc"]], z2[: Literal[2]]) = (x1, (y1, z1))
           |                                               ^^^^^^^
         9 | x3[: int], (y3[: str], z3[: int]) = (i(1), (s('abc'), i(2)))
        10 | x4[: int], (y4[: str], z4[: int]) = (x3, (y3, z3))
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
          --> main2.py:8:55
           |
         7 | x1, (y1, z1) = (1, ('abc', 2))
         8 | x2[: Literal[1]], (y2[: Literal["abc"]], z2[: Literal[2]]) = (x1, (y1, z1))
           |                                                       ^
         9 | x3[: int], (y3[: str], z3[: int]) = (i(1), (s('abc'), i(2)))
        10 | x4[: int], (y4[: str], z4[: int]) = (x3, (y3, z3))
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
          --> main2.py:9:6
           |
         7 | x1, (y1, z1) = (1, ('abc', 2))
         8 | x2[: Literal[1]], (y2[: Literal["abc"]], z2[: Literal[2]]) = (x1, (y1, z1))
         9 | x3[: int], (y3[: str], z3[: int]) = (i(1), (s('abc'), i(2)))
           |      ^^^
        10 | x4[: int], (y4[: str], z4[: int]) = (x3, (y3, z3))
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ^^^
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
          --> main2.py:9:18
           |
         7 | x1, (y1, z1) = (1, ('abc', 2))
         8 | x2[: Literal[1]], (y2[: Literal["abc"]], z2[: Literal[2]]) = (x1, (y1, z1))
         9 | x3[: int], (y3[: str], z3[: int]) = (i(1), (s('abc'), i(2)))
           |                  ^^^
        10 | x4[: int], (y4[: str], z4[: int]) = (x3, (y3, z3))
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
          --> main2.py:9:29
           |
         7 | x1, (y1, z1) = (1, ('abc', 2))
         8 | x2[: Literal[1]], (y2[: Literal["abc"]], z2[: Literal[2]]) = (x1, (y1, z1))
         9 | x3[: int], (y3[: str], z3[: int]) = (i(1), (s('abc'), i(2)))
           |                             ^^^
        10 | x4[: int], (y4[: str], z4[: int]) = (x3, (y3, z3))
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
          --> main2.py:10:6
           |
         8 | x2[: Literal[1]], (y2[: Literal["abc"]], z2[: Literal[2]]) = (x1, (y1, z1))
         9 | x3[: int], (y3[: str], z3[: int]) = (i(1), (s('abc'), i(2)))
        10 | x4[: int], (y4[: str], z4[: int]) = (x3, (y3, z3))
           |      ^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ^^^
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
          --> main2.py:10:18
           |
         8 | x2[: Literal[1]], (y2[: Literal["abc"]], z2[: Literal[2]]) = (x1, (y1, z1))
         9 | x3[: int], (y3[: str], z3[: int]) = (i(1), (s('abc'), i(2)))
        10 | x4[: int], (y4[: str], z4[: int]) = (x3, (y3, z3))
           |                  ^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
          --> main2.py:10:29
           |
         8 | x2[: Literal[1]], (y2[: Literal["abc"]], z2[: Literal[2]]) = (x1, (y1, z1))
         9 | x3[: int], (y3[: str], z3[: int]) = (i(1), (s('abc'), i(2)))
        10 | x4[: int], (y4[: str], z4[: int]) = (x3, (y3, z3))
           |                             ^^^
           |
        "#);
    }

    #[test]
    fn test_assign_statement_with_type_annotation() {
        let mut test = inlay_hint_test(
            "
            def i(x: int, /) -> int:
                return x

            x: int = 1
            y = x
            z: int = i(1)
            w = z",
        );

        assert_snapshot!(test.inlay_hints(), @r#"
        def i(x: int, /) -> int:
            return x

        x: int = 1
        y[: Literal[1]] = x
        z: int = i(1)
        w[: int] = z
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:351:1
            |
        349 | Final: _SpecialForm
        350 |
        351 | Literal: _SpecialForm
            | ^^^^^^^
        352 | TypedDict: _SpecialForm
            |
        info: Source
         --> main2.py:6:5
          |
        5 | x: int = 1
        6 | y[: Literal[1]] = x
          |     ^^^^^^^
        7 | z: int = i(1)
        8 | w[: int] = z
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
         --> main2.py:6:13
          |
        5 | x: int = 1
        6 | y[: Literal[1]] = x
          |             ^
        7 | z: int = i(1)
        8 | w[: int] = z
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
         --> main2.py:8:5
          |
        6 | y[: Literal[1]] = x
        7 | z: int = i(1)
        8 | w[: int] = z
          |     ^^^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: File after edits
        info: Source

        def i(x: int, /) -> int:
            return x

        x: int = 1
        y: Literal[1] = x
        z: int = i(1)
        w: int = z
        "#);
    }

    #[test]
    fn test_assign_statement_out_of_range() {
        let mut test = inlay_hint_test(
            "
            def i(x: int, /) -> int:
                return x
            <START>x = i(1)<END>
            z = x",
        );

        assert_snapshot!(test.inlay_hints(), @r#"
        def i(x: int, /) -> int:
            return x
        x[: int] = i(1)
        z = x
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
         --> main2.py:4:5
          |
        2 | def i(x: int, /) -> int:
        3 |     return x
        4 | x[: int] = i(1)
          |     ^^^
        5 | z = x
          |

        ---------------------------------------------
        info[inlay-hint-edit]: File after edits
        info: Source

        def i(x: int, /) -> int:
            return x
        x: int = i(1)
        z = x
        "#);
    }

    #[test]
    fn test_assign_attribute_of_instance() {
        let mut test = inlay_hint_test(
            "
            class A:
                def __init__(self, y):
                    self.x = int(1)
                    self.y = y

            a = A(2)
            a.y = int(3)
            ",
        );

        assert_snapshot!(test.inlay_hints(), @r#"
        class A:
            def __init__(self, y):
                self.x[: int] = int(1)
                self.y[: Unknown] = y

        a[: A] = A([y=]2)
        a.y[: int] = int(3)

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
         --> main2.py:4:18
          |
        2 | class A:
        3 |     def __init__(self, y):
        4 |         self.x[: int] = int(1)
          |                  ^^^
        5 |         self.y[: Unknown] = y
          |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:20:1
           |
        19 | # Types
        20 | Unknown = object()
           | ^^^^^^^
        21 | AlwaysTruthy = object()
        22 | AlwaysFalsy = object()
           |
        info: Source
         --> main2.py:5:18
          |
        3 |     def __init__(self, y):
        4 |         self.x[: int] = int(1)
        5 |         self.y[: Unknown] = y
          |                  ^^^^^^^
        6 |
        7 | a[: A] = A([y=]2)
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class A:
          |       ^
        3 |     def __init__(self, y):
        4 |         self.x = int(1)
          |
        info: Source
         --> main2.py:7:5
          |
        5 |         self.y[: Unknown] = y
        6 |
        7 | a[: A] = A([y=]2)
          |     ^
        8 | a.y[: int] = int(3)
          |

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
         --> main2.py:7:13
          |
        5 |         self.y[: Unknown] = y
        6 |
        7 | a[: A] = A([y=]2)
          |             ^
        8 | a.y[: int] = int(3)
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
         --> main2.py:8:7
          |
        7 | a[: A] = A([y=]2)
        8 | a.y[: int] = int(3)
          |       ^^^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: File after edits
        info: Source

        class A:
            def __init__(self, y):
                self.x: int = int(1)
                self.y: Unknown = y

        a: A = A(2)
        a.y: int = int(3)
        "#);
    }

    #[test]
    fn test_match_name_binding() {
        let mut test = inlay_hint_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", ab]:
                        x = ab
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @r#"
        def my_func(command: str):
            match command.split():
                case ["get", ab]:
                    x[: @Todo] = ab
        "#);
    }

    #[test]
    fn test_match_rest_binding() {
        let mut test = inlay_hint_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", *ab]:
                        x = ab
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @r#"
        def my_func(command: str):
            match command.split():
                case ["get", *ab]:
                    x[: @Todo] = ab
        "#);
    }

    #[test]
    fn test_match_as_binding() {
        let mut test = inlay_hint_test(
            r#"
            def my_func(command: str):
                match command.split():
                    case ["get", ("a" | "b") as ab]:
                        x = ab
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @r#"
        def my_func(command: str):
            match command.split():
                case ["get", ("a" | "b") as ab]:
                    x[: @Todo] = ab
        "#);
    }

    #[test]
    fn test_match_keyword_binding() {
        let mut test = inlay_hint_test(
            r#"
            class Click:
                __match_args__ = ("position", "button")
                def __init__(self, pos, btn):
                    self.position: int = pos
                    self.button: str = btn

            def my_func(event: Click):
                match event:
                    case Click(x, button=ab):
                        x = ab
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @r#"
        class Click:
            __match_args__ = ("position", "button")
            def __init__(self, pos, btn):
                self.position: int = pos
                self.button: str = btn

        def my_func(event: Click):
            match event:
                case Click(x, button=ab):
                    x[: @Todo] = ab
        "#);
    }

    #[test]
    fn test_typevar_name_binding() {
        let mut test = inlay_hint_test(
            r#"
            type Alias1[AB: int = bool] = tuple[AB, list[AB]]
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @"type Alias1[AB: int = bool] = tuple[AB, list[AB]]");
    }

    #[test]
    fn test_typevar_spec_binding() {
        let mut test = inlay_hint_test(
            r#"
            from typing import Callable
            type Alias2[**AB = [int, str]] = Callable[AB, tuple[AB]]
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @r"
        from typing import Callable
        type Alias2[**AB = [int, str]] = Callable[AB, tuple[AB]]
        ");
    }

    #[test]
    fn test_typevar_tuple_binding() {
        let mut test = inlay_hint_test(
            r#"
            type Alias3[*AB = ()] = tuple[tuple[*AB], tuple[*AB]]
            "#,
        );

        assert_snapshot!(test.inlay_hints(), @"type Alias3[*AB = ()] = tuple[tuple[*AB], tuple[*AB]]");
    }

    #[test]
    fn test_many_literals() {
        let mut test = inlay_hint_test(
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
        i = b'/x00'
        j = +1
        k = -1.0
        "#);
    }

    #[test]
    fn test_many_literals_tuple() {
        let mut test = inlay_hint_test(
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
        i = (b'/x01', b'/x02')
        j = (+1, +2.0)
        k = (-1, -2.0)
        "#);
    }

    #[test]
    fn test_many_literals_unpacked_tuple() {
        let mut test = inlay_hint_test(
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
        i1, i2 = (b'/x01', b'/x02')
        j1, j2 = (+1, +2.0)
        k1, k2 = (-1, -2.0)
        "#);
    }

    #[test]
    fn test_many_literals_multiple() {
        let mut test = inlay_hint_test(
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
        i1, i2 = b'/x01', b'/x02'
        j1, j2 = +1, +2.0
        k1, k2 = -1, -2.0
        "#);
    }

    #[test]
    fn test_many_literals_list() {
        let mut test = inlay_hint_test(
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
        i[: list[Unknown | bytes]] = [b'/x01', b'/x02']
        j[: list[Unknown | int | float]] = [+1, +2.0]
        k[: list[Unknown | int | float]] = [-1, -2.0]

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2802:7
             |
        2801 | @disjoint_base
        2802 | class list(MutableSequence[_T]):
             |       ^^^^
        2803 |     """Built-in mutable sequence.
             |
        info: Source
         --> main2.py:2:5
          |
        2 | a[: list[Unknown | int]] = [1, 2]
          |     ^^^^
        3 | b[: list[Unknown | float]] = [1.0, 2.0]
        4 | c[: list[Unknown | bool]] = [True, False]
          |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:20:1
           |
        19 | # Types
        20 | Unknown = object()
           | ^^^^^^^
        21 | AlwaysTruthy = object()
        22 | AlwaysFalsy = object()
           |
        info: Source
         --> main2.py:2:10
          |
        2 | a[: list[Unknown | int]] = [1, 2]
          |          ^^^^^^^
        3 | b[: list[Unknown | float]] = [1.0, 2.0]
        4 | c[: list[Unknown | bool]] = [True, False]
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
         --> main2.py:2:20
          |
        2 | a[: list[Unknown | int]] = [1, 2]
          |                    ^^^
        3 | b[: list[Unknown | float]] = [1.0, 2.0]
        4 | c[: list[Unknown | bool]] = [True, False]
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2802:7
             |
        2801 | @disjoint_base
        2802 | class list(MutableSequence[_T]):
             |       ^^^^
        2803 |     """Built-in mutable sequence.
             |
        info: Source
         --> main2.py:3:5
          |
        2 | a[: list[Unknown | int]] = [1, 2]
        3 | b[: list[Unknown | float]] = [1.0, 2.0]
          |     ^^^^
        4 | c[: list[Unknown | bool]] = [True, False]
        5 | d[: list[Unknown | None]] = [None, None]
          |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:20:1
           |
        19 | # Types
        20 | Unknown = object()
           | ^^^^^^^
        21 | AlwaysTruthy = object()
        22 | AlwaysFalsy = object()
           |
        info: Source
         --> main2.py:3:10
          |
        2 | a[: list[Unknown | int]] = [1, 2]
        3 | b[: list[Unknown | float]] = [1.0, 2.0]
          |          ^^^^^^^
        4 | c[: list[Unknown | bool]] = [True, False]
        5 | d[: list[Unknown | None]] = [None, None]
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:661:7
            |
        660 | @disjoint_base
        661 | class float:
            |       ^^^^^
        662 |     """Convert a string or number to a floating-point number, if possible."""
            |
        info: Source
         --> main2.py:3:20
          |
        2 | a[: list[Unknown | int]] = [1, 2]
        3 | b[: list[Unknown | float]] = [1.0, 2.0]
          |                    ^^^^^
        4 | c[: list[Unknown | bool]] = [True, False]
        5 | d[: list[Unknown | None]] = [None, None]
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2802:7
             |
        2801 | @disjoint_base
        2802 | class list(MutableSequence[_T]):
             |       ^^^^
        2803 |     """Built-in mutable sequence.
             |
        info: Source
         --> main2.py:4:5
          |
        2 | a[: list[Unknown | int]] = [1, 2]
        3 | b[: list[Unknown | float]] = [1.0, 2.0]
        4 | c[: list[Unknown | bool]] = [True, False]
          |     ^^^^
        5 | d[: list[Unknown | None]] = [None, None]
        6 | e[: list[Unknown | str]] = ["hel", "lo"]
          |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:20:1
           |
        19 | # Types
        20 | Unknown = object()
           | ^^^^^^^
        21 | AlwaysTruthy = object()
        22 | AlwaysFalsy = object()
           |
        info: Source
         --> main2.py:4:10
          |
        2 | a[: list[Unknown | int]] = [1, 2]
        3 | b[: list[Unknown | float]] = [1.0, 2.0]
        4 | c[: list[Unknown | bool]] = [True, False]
          |          ^^^^^^^
        5 | d[: list[Unknown | None]] = [None, None]
        6 | e[: list[Unknown | str]] = ["hel", "lo"]
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2591:7
             |
        2590 | @final
        2591 | class bool(int):
             |       ^^^^
        2592 |     """Returns True when the argument is true, False otherwise.
        2593 |     The builtins True and False are the only two instances of the class bool.
             |
        info: Source
         --> main2.py:4:20
          |
        2 | a[: list[Unknown | int]] = [1, 2]
        3 | b[: list[Unknown | float]] = [1.0, 2.0]
        4 | c[: list[Unknown | bool]] = [True, False]
          |                    ^^^^
        5 | d[: list[Unknown | None]] = [None, None]
        6 | e[: list[Unknown | str]] = ["hel", "lo"]
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2802:7
             |
        2801 | @disjoint_base
        2802 | class list(MutableSequence[_T]):
             |       ^^^^
        2803 |     """Built-in mutable sequence.
             |
        info: Source
         --> main2.py:5:5
          |
        3 | b[: list[Unknown | float]] = [1.0, 2.0]
        4 | c[: list[Unknown | bool]] = [True, False]
        5 | d[: list[Unknown | None]] = [None, None]
          |     ^^^^
        6 | e[: list[Unknown | str]] = ["hel", "lo"]
        7 | f[: list[Unknown | str]] = ['the', 're']
          |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:20:1
           |
        19 | # Types
        20 | Unknown = object()
           | ^^^^^^^
        21 | AlwaysTruthy = object()
        22 | AlwaysFalsy = object()
           |
        info: Source
         --> main2.py:5:10
          |
        3 | b[: list[Unknown | float]] = [1.0, 2.0]
        4 | c[: list[Unknown | bool]] = [True, False]
        5 | d[: list[Unknown | None]] = [None, None]
          |          ^^^^^^^
        6 | e[: list[Unknown | str]] = ["hel", "lo"]
        7 | f[: list[Unknown | str]] = ['the', 're']
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/types.pyi:950:11
            |
        948 | if sys.version_info >= (3, 10):
        949 |     @final
        950 |     class NoneType:
            |           ^^^^^^^^
        951 |         """The type of the None singleton."""
            |
        info: Source
         --> main2.py:5:20
          |
        3 | b[: list[Unknown | float]] = [1.0, 2.0]
        4 | c[: list[Unknown | bool]] = [True, False]
        5 | d[: list[Unknown | None]] = [None, None]
          |                    ^^^^
        6 | e[: list[Unknown | str]] = ["hel", "lo"]
        7 | f[: list[Unknown | str]] = ['the', 're']
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2802:7
             |
        2801 | @disjoint_base
        2802 | class list(MutableSequence[_T]):
             |       ^^^^
        2803 |     """Built-in mutable sequence.
             |
        info: Source
         --> main2.py:6:5
          |
        4 | c[: list[Unknown | bool]] = [True, False]
        5 | d[: list[Unknown | None]] = [None, None]
        6 | e[: list[Unknown | str]] = ["hel", "lo"]
          |     ^^^^
        7 | f[: list[Unknown | str]] = ['the', 're']
        8 | g[: list[Unknown | str]] = [f"{ft}", f"{ft}"]
          |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:20:1
           |
        19 | # Types
        20 | Unknown = object()
           | ^^^^^^^
        21 | AlwaysTruthy = object()
        22 | AlwaysFalsy = object()
           |
        info: Source
         --> main2.py:6:10
          |
        4 | c[: list[Unknown | bool]] = [True, False]
        5 | d[: list[Unknown | None]] = [None, None]
        6 | e[: list[Unknown | str]] = ["hel", "lo"]
          |          ^^^^^^^
        7 | f[: list[Unknown | str]] = ['the', 're']
        8 | g[: list[Unknown | str]] = [f"{ft}", f"{ft}"]
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ^^^
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
         --> main2.py:6:20
          |
        4 | c[: list[Unknown | bool]] = [True, False]
        5 | d[: list[Unknown | None]] = [None, None]
        6 | e[: list[Unknown | str]] = ["hel", "lo"]
          |                    ^^^
        7 | f[: list[Unknown | str]] = ['the', 're']
        8 | g[: list[Unknown | str]] = [f"{ft}", f"{ft}"]
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2802:7
             |
        2801 | @disjoint_base
        2802 | class list(MutableSequence[_T]):
             |       ^^^^
        2803 |     """Built-in mutable sequence.
             |
        info: Source
         --> main2.py:7:5
          |
        5 | d[: list[Unknown | None]] = [None, None]
        6 | e[: list[Unknown | str]] = ["hel", "lo"]
        7 | f[: list[Unknown | str]] = ['the', 're']
          |     ^^^^
        8 | g[: list[Unknown | str]] = [f"{ft}", f"{ft}"]
        9 | h[: list[Unknown | Template]] = [t"wow %d", t"wow %d"]
          |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:20:1
           |
        19 | # Types
        20 | Unknown = object()
           | ^^^^^^^
        21 | AlwaysTruthy = object()
        22 | AlwaysFalsy = object()
           |
        info: Source
         --> main2.py:7:10
          |
        5 | d[: list[Unknown | None]] = [None, None]
        6 | e[: list[Unknown | str]] = ["hel", "lo"]
        7 | f[: list[Unknown | str]] = ['the', 're']
          |          ^^^^^^^
        8 | g[: list[Unknown | str]] = [f"{ft}", f"{ft}"]
        9 | h[: list[Unknown | Template]] = [t"wow %d", t"wow %d"]
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ^^^
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
         --> main2.py:7:20
          |
        5 | d[: list[Unknown | None]] = [None, None]
        6 | e[: list[Unknown | str]] = ["hel", "lo"]
        7 | f[: list[Unknown | str]] = ['the', 're']
          |                    ^^^
        8 | g[: list[Unknown | str]] = [f"{ft}", f"{ft}"]
        9 | h[: list[Unknown | Template]] = [t"wow %d", t"wow %d"]
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2802:7
             |
        2801 | @disjoint_base
        2802 | class list(MutableSequence[_T]):
             |       ^^^^
        2803 |     """Built-in mutable sequence.
             |
        info: Source
          --> main2.py:8:5
           |
         6 | e[: list[Unknown | str]] = ["hel", "lo"]
         7 | f[: list[Unknown | str]] = ['the', 're']
         8 | g[: list[Unknown | str]] = [f"{ft}", f"{ft}"]
           |     ^^^^
         9 | h[: list[Unknown | Template]] = [t"wow %d", t"wow %d"]
        10 | i[: list[Unknown | bytes]] = [b'/x01', b'/x02']
           |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:20:1
           |
        19 | # Types
        20 | Unknown = object()
           | ^^^^^^^
        21 | AlwaysTruthy = object()
        22 | AlwaysFalsy = object()
           |
        info: Source
          --> main2.py:8:10
           |
         6 | e[: list[Unknown | str]] = ["hel", "lo"]
         7 | f[: list[Unknown | str]] = ['the', 're']
         8 | g[: list[Unknown | str]] = [f"{ft}", f"{ft}"]
           |          ^^^^^^^
         9 | h[: list[Unknown | Template]] = [t"wow %d", t"wow %d"]
        10 | i[: list[Unknown | bytes]] = [b'/x01', b'/x02']
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ^^^
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
          --> main2.py:8:20
           |
         6 | e[: list[Unknown | str]] = ["hel", "lo"]
         7 | f[: list[Unknown | str]] = ['the', 're']
         8 | g[: list[Unknown | str]] = [f"{ft}", f"{ft}"]
           |                    ^^^
         9 | h[: list[Unknown | Template]] = [t"wow %d", t"wow %d"]
        10 | i[: list[Unknown | bytes]] = [b'/x01', b'/x02']
           |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2802:7
             |
        2801 | @disjoint_base
        2802 | class list(MutableSequence[_T]):
             |       ^^^^
        2803 |     """Built-in mutable sequence.
             |
        info: Source
          --> main2.py:9:5
           |
         7 | f[: list[Unknown | str]] = ['the', 're']
         8 | g[: list[Unknown | str]] = [f"{ft}", f"{ft}"]
         9 | h[: list[Unknown | Template]] = [t"wow %d", t"wow %d"]
           |     ^^^^
        10 | i[: list[Unknown | bytes]] = [b'/x01', b'/x02']
        11 | j[: list[Unknown | int | float]] = [+1, +2.0]
           |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:20:1
           |
        19 | # Types
        20 | Unknown = object()
           | ^^^^^^^
        21 | AlwaysTruthy = object()
        22 | AlwaysFalsy = object()
           |
        info: Source
          --> main2.py:9:10
           |
         7 | f[: list[Unknown | str]] = ['the', 're']
         8 | g[: list[Unknown | str]] = [f"{ft}", f"{ft}"]
         9 | h[: list[Unknown | Template]] = [t"wow %d", t"wow %d"]
           |          ^^^^^^^
        10 | i[: list[Unknown | bytes]] = [b'/x01', b'/x02']
        11 | j[: list[Unknown | int | float]] = [+1, +2.0]
           |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/string/templatelib.pyi:10:7
           |
         9 | @final
        10 | class Template:  # TODO: consider making `Template` generic on `TypeVarTuple`
           |       ^^^^^^^^
        11 |     """Template object"""
           |
        info: Source
          --> main2.py:9:20
           |
         7 | f[: list[Unknown | str]] = ['the', 're']
         8 | g[: list[Unknown | str]] = [f"{ft}", f"{ft}"]
         9 | h[: list[Unknown | Template]] = [t"wow %d", t"wow %d"]
           |                    ^^^^^^^^
        10 | i[: list[Unknown | bytes]] = [b'/x01', b'/x02']
        11 | j[: list[Unknown | int | float]] = [+1, +2.0]
           |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2802:7
             |
        2801 | @disjoint_base
        2802 | class list(MutableSequence[_T]):
             |       ^^^^
        2803 |     """Built-in mutable sequence.
             |
        info: Source
          --> main2.py:10:5
           |
         8 | g[: list[Unknown | str]] = [f"{ft}", f"{ft}"]
         9 | h[: list[Unknown | Template]] = [t"wow %d", t"wow %d"]
        10 | i[: list[Unknown | bytes]] = [b'/x01', b'/x02']
           |     ^^^^
        11 | j[: list[Unknown | int | float]] = [+1, +2.0]
        12 | k[: list[Unknown | int | float]] = [-1, -2.0]
           |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:20:1
           |
        19 | # Types
        20 | Unknown = object()
           | ^^^^^^^
        21 | AlwaysTruthy = object()
        22 | AlwaysFalsy = object()
           |
        info: Source
          --> main2.py:10:10
           |
         8 | g[: list[Unknown | str]] = [f"{ft}", f"{ft}"]
         9 | h[: list[Unknown | Template]] = [t"wow %d", t"wow %d"]
        10 | i[: list[Unknown | bytes]] = [b'/x01', b'/x02']
           |          ^^^^^^^
        11 | j[: list[Unknown | int | float]] = [+1, +2.0]
        12 | k[: list[Unknown | int | float]] = [-1, -2.0]
           |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:1448:7
             |
        1447 | @disjoint_base
        1448 | class bytes(Sequence[int]):
             |       ^^^^^
        1449 |     """bytes(iterable_of_ints) -> bytes
        1450 |     bytes(string, encoding[, errors]) -> bytes
             |
        info: Source
          --> main2.py:10:20
           |
         8 | g[: list[Unknown | str]] = [f"{ft}", f"{ft}"]
         9 | h[: list[Unknown | Template]] = [t"wow %d", t"wow %d"]
        10 | i[: list[Unknown | bytes]] = [b'/x01', b'/x02']
           |                    ^^^^^
        11 | j[: list[Unknown | int | float]] = [+1, +2.0]
        12 | k[: list[Unknown | int | float]] = [-1, -2.0]
           |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2802:7
             |
        2801 | @disjoint_base
        2802 | class list(MutableSequence[_T]):
             |       ^^^^
        2803 |     """Built-in mutable sequence.
             |
        info: Source
          --> main2.py:11:5
           |
         9 | h[: list[Unknown | Template]] = [t"wow %d", t"wow %d"]
        10 | i[: list[Unknown | bytes]] = [b'/x01', b'/x02']
        11 | j[: list[Unknown | int | float]] = [+1, +2.0]
           |     ^^^^
        12 | k[: list[Unknown | int | float]] = [-1, -2.0]
           |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:20:1
           |
        19 | # Types
        20 | Unknown = object()
           | ^^^^^^^
        21 | AlwaysTruthy = object()
        22 | AlwaysFalsy = object()
           |
        info: Source
          --> main2.py:11:10
           |
         9 | h[: list[Unknown | Template]] = [t"wow %d", t"wow %d"]
        10 | i[: list[Unknown | bytes]] = [b'/x01', b'/x02']
        11 | j[: list[Unknown | int | float]] = [+1, +2.0]
           |          ^^^^^^^
        12 | k[: list[Unknown | int | float]] = [-1, -2.0]
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
          --> main2.py:11:20
           |
         9 | h[: list[Unknown | Template]] = [t"wow %d", t"wow %d"]
        10 | i[: list[Unknown | bytes]] = [b'/x01', b'/x02']
        11 | j[: list[Unknown | int | float]] = [+1, +2.0]
           |                    ^^^
        12 | k[: list[Unknown | int | float]] = [-1, -2.0]
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:661:7
            |
        660 | @disjoint_base
        661 | class float:
            |       ^^^^^
        662 |     """Convert a string or number to a floating-point number, if possible."""
            |
        info: Source
          --> main2.py:11:26
           |
         9 | h[: list[Unknown | Template]] = [t"wow %d", t"wow %d"]
        10 | i[: list[Unknown | bytes]] = [b'/x01', b'/x02']
        11 | j[: list[Unknown | int | float]] = [+1, +2.0]
           |                          ^^^^^
        12 | k[: list[Unknown | int | float]] = [-1, -2.0]
           |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2802:7
             |
        2801 | @disjoint_base
        2802 | class list(MutableSequence[_T]):
             |       ^^^^
        2803 |     """Built-in mutable sequence.
             |
        info: Source
          --> main2.py:12:5
           |
        10 | i[: list[Unknown | bytes]] = [b'/x01', b'/x02']
        11 | j[: list[Unknown | int | float]] = [+1, +2.0]
        12 | k[: list[Unknown | int | float]] = [-1, -2.0]
           |     ^^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:20:1
           |
        19 | # Types
        20 | Unknown = object()
           | ^^^^^^^
        21 | AlwaysTruthy = object()
        22 | AlwaysFalsy = object()
           |
        info: Source
          --> main2.py:12:10
           |
        10 | i[: list[Unknown | bytes]] = [b'/x01', b'/x02']
        11 | j[: list[Unknown | int | float]] = [+1, +2.0]
        12 | k[: list[Unknown | int | float]] = [-1, -2.0]
           |          ^^^^^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
          --> main2.py:12:20
           |
        10 | i[: list[Unknown | bytes]] = [b'/x01', b'/x02']
        11 | j[: list[Unknown | int | float]] = [+1, +2.0]
        12 | k[: list[Unknown | int | float]] = [-1, -2.0]
           |                    ^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:661:7
            |
        660 | @disjoint_base
        661 | class float:
            |       ^^^^^
        662 |     """Convert a string or number to a floating-point number, if possible."""
            |
        info: Source
          --> main2.py:12:26
           |
        10 | i[: list[Unknown | bytes]] = [b'/x01', b'/x02']
        11 | j[: list[Unknown | int | float]] = [+1, +2.0]
        12 | k[: list[Unknown | int | float]] = [-1, -2.0]
           |                          ^^^^^
           |

        ---------------------------------------------
        info[inlay-hint-edit]: File after edits
        info: Source

        a: list[Unknown | int] = [1, 2]
        b: list[Unknown | float] = [1.0, 2.0]
        c: list[Unknown | bool] = [True, False]
        d: list[Unknown | None] = [None, None]
        e: list[Unknown | str] = ["hel", "lo"]
        f: list[Unknown | str] = ['the', 're']
        g: list[Unknown | str] = [f"{ft}", f"{ft}"]
        h: list[Unknown | Template] = [t"wow %d", t"wow %d"]
        i: list[Unknown | bytes] = [b'/x01', b'/x02']
        j: list[Unknown | int | float] = [+1, +2.0]
        k: list[Unknown | int | float] = [-1, -2.0]
        "#);
    }

    #[test]
    fn test_simple_init_call() {
        let mut test = inlay_hint_test(
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

        assert_snapshot!(test.inlay_hints(), @r#"
        class MyClass:
            def __init__(self):
                self.x: int = 1

        x[: MyClass] = MyClass()
        y[: tuple[MyClass, MyClass]] = (MyClass(), MyClass())
        a[: MyClass], b[: MyClass] = MyClass(), MyClass()
        c[: MyClass], d[: MyClass] = (MyClass(), MyClass())

        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class MyClass:
          |       ^^^^^^^
        3 |     def __init__(self):
        4 |         self.x: int = 1
          |
        info: Source
         --> main2.py:6:5
          |
        4 |         self.x: int = 1
        5 |
        6 | x[: MyClass] = MyClass()
          |     ^^^^^^^
        7 | y[: tuple[MyClass, MyClass]] = (MyClass(), MyClass())
        8 | a[: MyClass], b[: MyClass] = MyClass(), MyClass()
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2695:7
             |
        2694 | @disjoint_base
        2695 | class tuple(Sequence[_T_co]):
             |       ^^^^^
        2696 |     """Built-in immutable sequence.
             |
        info: Source
         --> main2.py:7:5
          |
        6 | x[: MyClass] = MyClass()
        7 | y[: tuple[MyClass, MyClass]] = (MyClass(), MyClass())
          |     ^^^^^
        8 | a[: MyClass], b[: MyClass] = MyClass(), MyClass()
        9 | c[: MyClass], d[: MyClass] = (MyClass(), MyClass())
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class MyClass:
          |       ^^^^^^^
        3 |     def __init__(self):
        4 |         self.x: int = 1
          |
        info: Source
         --> main2.py:7:11
          |
        6 | x[: MyClass] = MyClass()
        7 | y[: tuple[MyClass, MyClass]] = (MyClass(), MyClass())
          |           ^^^^^^^
        8 | a[: MyClass], b[: MyClass] = MyClass(), MyClass()
        9 | c[: MyClass], d[: MyClass] = (MyClass(), MyClass())
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class MyClass:
          |       ^^^^^^^
        3 |     def __init__(self):
        4 |         self.x: int = 1
          |
        info: Source
         --> main2.py:7:20
          |
        6 | x[: MyClass] = MyClass()
        7 | y[: tuple[MyClass, MyClass]] = (MyClass(), MyClass())
          |                    ^^^^^^^
        8 | a[: MyClass], b[: MyClass] = MyClass(), MyClass()
        9 | c[: MyClass], d[: MyClass] = (MyClass(), MyClass())
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class MyClass:
          |       ^^^^^^^
        3 |     def __init__(self):
        4 |         self.x: int = 1
          |
        info: Source
         --> main2.py:8:5
          |
        6 | x[: MyClass] = MyClass()
        7 | y[: tuple[MyClass, MyClass]] = (MyClass(), MyClass())
        8 | a[: MyClass], b[: MyClass] = MyClass(), MyClass()
          |     ^^^^^^^
        9 | c[: MyClass], d[: MyClass] = (MyClass(), MyClass())
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class MyClass:
          |       ^^^^^^^
        3 |     def __init__(self):
        4 |         self.x: int = 1
          |
        info: Source
         --> main2.py:8:19
          |
        6 | x[: MyClass] = MyClass()
        7 | y[: tuple[MyClass, MyClass]] = (MyClass(), MyClass())
        8 | a[: MyClass], b[: MyClass] = MyClass(), MyClass()
          |                   ^^^^^^^
        9 | c[: MyClass], d[: MyClass] = (MyClass(), MyClass())
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class MyClass:
          |       ^^^^^^^
        3 |     def __init__(self):
        4 |         self.x: int = 1
          |
        info: Source
         --> main2.py:9:5
          |
        7 | y[: tuple[MyClass, MyClass]] = (MyClass(), MyClass())
        8 | a[: MyClass], b[: MyClass] = MyClass(), MyClass()
        9 | c[: MyClass], d[: MyClass] = (MyClass(), MyClass())
          |     ^^^^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class MyClass:
          |       ^^^^^^^
        3 |     def __init__(self):
        4 |         self.x: int = 1
          |
        info: Source
         --> main2.py:9:19
          |
        7 | y[: tuple[MyClass, MyClass]] = (MyClass(), MyClass())
        8 | a[: MyClass], b[: MyClass] = MyClass(), MyClass()
        9 | c[: MyClass], d[: MyClass] = (MyClass(), MyClass())
          |                   ^^^^^^^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: File after edits
        info: Source

        class MyClass:
            def __init__(self):
                self.x: int = 1

        x: MyClass = MyClass()
        y: tuple[MyClass, MyClass] = (MyClass(), MyClass())
        a, b = MyClass(), MyClass()
        c, d = (MyClass(), MyClass())
        "#);
    }

    #[test]
    fn test_generic_init_call() {
        let mut test = inlay_hint_test(
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
            --> stdlib/builtins.pyi:2802:7
             |
        2801 | @disjoint_base
        2802 | class list(MutableSequence[_T]):
             |       ^^^^
        2803 |     """Built-in mutable sequence.
             |
        info: Source
         --> main2.py:4:18
          |
        2 | class MyClass[T, U]:
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
        4 |         self.x[: list[T@MyClass]] = x
          |                  ^^^^
        5 |         self.y[: tuple[U@MyClass, U@MyClass]] = y
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2695:7
             |
        2694 | @disjoint_base
        2695 | class tuple(Sequence[_T_co]):
             |       ^^^^^
        2696 |     """Built-in immutable sequence.
             |
        info: Source
         --> main2.py:5:18
          |
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
        4 |         self.x[: list[T@MyClass]] = x
        5 |         self.y[: tuple[U@MyClass, U@MyClass]] = y
          |                  ^^^^^
        6 |
        7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class MyClass[T, U]:
          |       ^^^^^^^
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
        4 |         self.x = x
          |
        info: Source
         --> main2.py:7:5
          |
        5 |         self.y[: tuple[U@MyClass, U@MyClass]] = y
        6 |
        7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
          |     ^^^^^^^
        8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
        9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
          |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:20:1
           |
        19 | # Types
        20 | Unknown = object()
           | ^^^^^^^
        21 | AlwaysTruthy = object()
        22 | AlwaysFalsy = object()
           |
        info: Source
         --> main2.py:7:13
          |
        5 |         self.y[: tuple[U@MyClass, U@MyClass]] = y
        6 |
        7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
          |             ^^^^^^^
        8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
        9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
         --> main2.py:7:23
          |
        5 |         self.y[: tuple[U@MyClass, U@MyClass]] = y
        6 |
        7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
          |                       ^^^
        8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
        9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ^^^
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
         --> main2.py:7:28
          |
        5 |         self.y[: tuple[U@MyClass, U@MyClass]] = y
        6 |
        7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
          |                            ^^^
        8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
        9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
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
         --> main2.py:7:45
          |
        5 |         self.y[: tuple[U@MyClass, U@MyClass]] = y
        6 |
        7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
          |                                             ^
        8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
        9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
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
         --> main2.py:7:55
          |
        5 |         self.y[: tuple[U@MyClass, U@MyClass]] = y
        6 |
        7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
          |                                                       ^
        8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
        9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2695:7
             |
        2694 | @disjoint_base
        2695 | class tuple(Sequence[_T_co]):
             |       ^^^^^
        2696 |     """Built-in immutable sequence.
             |
        info: Source
          --> main2.py:8:5
           |
         7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
           |     ^^^^^
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class MyClass[T, U]:
          |       ^^^^^^^
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
        4 |         self.x = x
          |
        info: Source
          --> main2.py:8:11
           |
         7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
           |           ^^^^^^^
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
           |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:20:1
           |
        19 | # Types
        20 | Unknown = object()
           | ^^^^^^^
        21 | AlwaysTruthy = object()
        22 | AlwaysFalsy = object()
           |
        info: Source
          --> main2.py:8:19
           |
         7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
           |                   ^^^^^^^
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
          --> main2.py:8:29
           |
         7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
           |                             ^^^
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ^^^
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
          --> main2.py:8:34
           |
         7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
           |                                  ^^^
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class MyClass[T, U]:
          |       ^^^^^^^
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
        4 |         self.x = x
          |
        info: Source
          --> main2.py:8:40
           |
         7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
           |                                        ^^^^^^^
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
           |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:20:1
           |
        19 | # Types
        20 | Unknown = object()
           | ^^^^^^^
        21 | AlwaysTruthy = object()
        22 | AlwaysFalsy = object()
           |
        info: Source
          --> main2.py:8:48
           |
         7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
           |                                                ^^^^^^^
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
          --> main2.py:8:58
           |
         7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
           |                                                          ^^^
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ^^^
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
          --> main2.py:8:63
           |
         7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
           |                                                               ^^^
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
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
          --> main2.py:8:82
           |
         7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
           |                                                                                  ^
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
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
          --> main2.py:8:92
           |
         7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
           |                                                                                            ^
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
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
          --> main2.py:8:117
           |
         7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
           |                                                                                                                     ^
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
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
          --> main2.py:8:127
           |
         7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
           |                                                                                                                               ^
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class MyClass[T, U]:
          |       ^^^^^^^
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
        4 |         self.x = x
          |
        info: Source
          --> main2.py:9:5
           |
         7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
           |     ^^^^^^^
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
           |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:20:1
           |
        19 | # Types
        20 | Unknown = object()
           | ^^^^^^^
        21 | AlwaysTruthy = object()
        22 | AlwaysFalsy = object()
           |
        info: Source
          --> main2.py:9:13
           |
         7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
           |             ^^^^^^^
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
          --> main2.py:9:23
           |
         7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
           |                       ^^^
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ^^^
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
          --> main2.py:9:28
           |
         7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
           |                            ^^^
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class MyClass[T, U]:
          |       ^^^^^^^
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
        4 |         self.x = x
          |
        info: Source
          --> main2.py:9:39
           |
         7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
           |                                       ^^^^^^^
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
           |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:20:1
           |
        19 | # Types
        20 | Unknown = object()
           | ^^^^^^^
        21 | AlwaysTruthy = object()
        22 | AlwaysFalsy = object()
           |
        info: Source
          --> main2.py:9:47
           |
         7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
           |                                               ^^^^^^^
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
          --> main2.py:9:57
           |
         7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
           |                                                         ^^^
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ^^^
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
          --> main2.py:9:62
           |
         7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
           |                                                              ^^^
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
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
          --> main2.py:9:79
           |
         7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
           |                                                                               ^
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
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
          --> main2.py:9:89
           |
         7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
           |                                                                                         ^
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
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
          --> main2.py:9:114
           |
         7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
           |                                                                                                                  ^
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
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
          --> main2.py:9:124
           |
         7 | x[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b"))
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
           |                                                                                                                            ^
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class MyClass[T, U]:
          |       ^^^^^^^
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
        4 |         self.x = x
          |
        info: Source
          --> main2.py:10:5
           |
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
           |     ^^^^^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:20:1
           |
        19 | # Types
        20 | Unknown = object()
           | ^^^^^^^
        21 | AlwaysTruthy = object()
        22 | AlwaysFalsy = object()
           |
        info: Source
          --> main2.py:10:13
           |
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
           |             ^^^^^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
          --> main2.py:10:23
           |
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
           |                       ^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ^^^
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
          --> main2.py:10:28
           |
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
           |                            ^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class MyClass[T, U]:
          |       ^^^^^^^
        3 |     def __init__(self, x: list[T], y: tuple[U, U]):
        4 |         self.x = x
          |
        info: Source
          --> main2.py:10:39
           |
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
           |                                       ^^^^^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:20:1
           |
        19 | # Types
        20 | Unknown = object()
           | ^^^^^^^
        21 | AlwaysTruthy = object()
        22 | AlwaysFalsy = object()
           |
        info: Source
          --> main2.py:10:47
           |
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
           |                                               ^^^^^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
          --> main2.py:10:57
           |
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
           |                                                         ^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ^^^
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
          --> main2.py:10:62
           |
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
           |                                                              ^^^
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
          --> main2.py:10:80
           |
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
           |                                                                                ^
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
          --> main2.py:10:90
           |
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
           |                                                                                          ^
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
          --> main2.py:10:115
           |
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
           |                                                                                                                   ^
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
          --> main2.py:10:125
           |
         8 | y[: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a",…
         9 | a[: MyClass[Unknown | int, str]], b[: MyClass[Unknown | int, str]] = MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "b…
        10 | c[: MyClass[Unknown | int, str]], d[: MyClass[Unknown | int, str]] = (MyClass([x=][42], [y=]("a", "b")), MyClass([x=][42], [y=]("a", "…
           |                                                                                                                             ^
           |

        ---------------------------------------------
        info[inlay-hint-edit]: File after edits
        info: Source

        class MyClass[T, U]:
            def __init__(self, x: list[T], y: tuple[U, U]):
                self.x = x
                self.y = y

        x: MyClass[Unknown | int, str] = MyClass([42], ("a", "b"))
        y: tuple[MyClass[Unknown | int, str], MyClass[Unknown | int, str]] = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
        a, b = MyClass([42], ("a", "b")), MyClass([42], ("a", "b"))
        c, d = (MyClass([42], ("a", "b")), MyClass([42], ("a", "b")))
        "#);
    }

    #[test]
    fn test_disabled_variable_types() {
        let mut test = inlay_hint_test(
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
        let mut test = inlay_hint_test(
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
         --> main2.py:3:6
          |
        2 | def foo(x: int): pass
        3 | foo([x=]1)
          |      ^
          |
        ");
    }

    #[test]
    fn test_function_call_with_positional_or_keyword_parameter_redundant_name() {
        let mut test = inlay_hint_test(
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
         --> main2.py:6:6
          |
        4 | y = 2
        5 | foo(x)
        6 | foo([x=]y)
          |      ^
          |
        ");
    }

    #[test]
    fn test_function_call_with_positional_or_keyword_parameter_redundant_attribute() {
        let mut test = inlay_hint_test(
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
         --> main.py:3:7
          |
        2 | def foo(x: int): pass
        3 | class MyClass:
          |       ^^^^^^^
        4 |     def __init__(self):
        5 |         self.x: int = 1
          |
        info: Source
         --> main2.py:7:7
          |
        5 |         self.x: int = 1
        6 |         self.y: int = 2
        7 | val[: MyClass] = MyClass()
          |       ^^^^^^^
        8 |
        9 | foo(val.x)
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int): pass
          |         ^
        3 | class MyClass:
        4 |     def __init__(self):
          |
        info: Source
          --> main2.py:10:6
           |
         9 | foo(val.x)
        10 | foo([x=]val.y)
           |      ^
           |

        ---------------------------------------------
        info[inlay-hint-edit]: File after edits
        info: Source

        def foo(x: int): pass
        class MyClass:
            def __init__(self):
                self.x: int = 1
                self.y: int = 2
        val: MyClass = MyClass()

        foo(val.x)
        foo(val.y)
        ");
    }

    #[test]
    fn test_function_call_with_positional_or_keyword_parameter_redundant_attribute_not() {
        // This one checks that we don't allow elide `x=` for `x.y`
        let mut test = inlay_hint_test(
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
         --> main.py:3:7
          |
        2 | def foo(x: int): pass
        3 | class MyClass:
          |       ^^^^^^^
        4 |     def __init__(self):
        5 |         self.x: int = 1
          |
        info: Source
         --> main2.py:7:5
          |
        5 |         self.x: int = 1
        6 |         self.y: int = 2
        7 | x[: MyClass] = MyClass()
          |     ^^^^^^^
        8 |
        9 | foo(x.x)
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int): pass
          |         ^
        3 | class MyClass:
        4 |     def __init__(self):
          |
        info: Source
          --> main2.py:10:6
           |
         9 | foo(x.x)
        10 | foo([x=]x.y)
           |      ^
           |

        ---------------------------------------------
        info[inlay-hint-edit]: File after edits
        info: Source

        def foo(x: int): pass
        class MyClass:
            def __init__(self):
                self.x: int = 1
                self.y: int = 2
        x: MyClass = MyClass()

        foo(x.x)
        foo(x.y)
        ");
    }

    #[test]
    fn test_function_call_with_positional_or_keyword_parameter_redundant_call() {
        let mut test = inlay_hint_test(
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
         --> main.py:3:7
          |
        2 | def foo(x: int): pass
        3 | class MyClass:
          |       ^^^^^^^
        4 |     def __init__(self):
        5 |     def x() -> int:
          |
        info: Source
          --> main2.py:9:7
           |
         7 |     def y() -> int:
         8 |         return 2
         9 | val[: MyClass] = MyClass()
           |       ^^^^^^^
        10 |
        11 | foo(val.x())
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int): pass
          |         ^
        3 | class MyClass:
        4 |     def __init__(self):
          |
        info: Source
          --> main2.py:12:6
           |
        11 | foo(val.x())
        12 | foo([x=]val.y())
           |      ^
           |

        ---------------------------------------------
        info[inlay-hint-edit]: File after edits
        info: Source

        def foo(x: int): pass
        class MyClass:
            def __init__(self):
            def x() -> int:
                return 1
            def y() -> int:
                return 2
        val: MyClass = MyClass()

        foo(val.x())
        foo(val.y())
        ");
    }

    #[test]
    fn test_function_call_with_positional_or_keyword_parameter_redundant_complex() {
        let mut test = inlay_hint_test(
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
         --> main.py:5:7
          |
        4 | def foo(x: int): pass
        5 | class MyClass:
          |       ^^^^^^^
        6 |     def __init__(self):
        7 |     def x() -> List[int]:
          |
        info: Source
          --> main2.py:11:7
           |
         9 |     def y() -> List[int]:
        10 |         return 2
        11 | val[: MyClass] = MyClass()
           |       ^^^^^^^
        12 |
        13 | foo(val.x()[0])
           |

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
          --> main2.py:14:6
           |
        13 | foo(val.x()[0])
        14 | foo([x=]val.y()[1])
           |      ^
           |

        ---------------------------------------------
        info[inlay-hint-edit]: File after edits
        info: Source

        from typing import List

        def foo(x: int): pass
        class MyClass:
            def __init__(self):
            def x() -> List[int]:
                return 1
            def y() -> List[int]:
                return 2
        val: MyClass = MyClass()

        foo(val.x()[0])
        foo(val.y()[1])
        ");
    }

    #[test]
    fn test_function_call_with_positional_or_keyword_parameter_redundant_subscript() {
        let mut test = inlay_hint_test(
            "
            def foo(x: int): pass
            x = [1]
            y = [2]

            foo(x[0])
            foo(y[0])",
        );

        assert_snapshot!(test.inlay_hints(), @r#"
        def foo(x: int): pass
        x[: list[Unknown | int]] = [1]
        y[: list[Unknown | int]] = [2]

        foo(x[0])
        foo([x=]y[0])
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2802:7
             |
        2801 | @disjoint_base
        2802 | class list(MutableSequence[_T]):
             |       ^^^^
        2803 |     """Built-in mutable sequence.
             |
        info: Source
         --> main2.py:3:5
          |
        2 | def foo(x: int): pass
        3 | x[: list[Unknown | int]] = [1]
          |     ^^^^
        4 | y[: list[Unknown | int]] = [2]
          |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:20:1
           |
        19 | # Types
        20 | Unknown = object()
           | ^^^^^^^
        21 | AlwaysTruthy = object()
        22 | AlwaysFalsy = object()
           |
        info: Source
         --> main2.py:3:10
          |
        2 | def foo(x: int): pass
        3 | x[: list[Unknown | int]] = [1]
          |          ^^^^^^^
        4 | y[: list[Unknown | int]] = [2]
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
         --> main2.py:3:20
          |
        2 | def foo(x: int): pass
        3 | x[: list[Unknown | int]] = [1]
          |                    ^^^
        4 | y[: list[Unknown | int]] = [2]
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2802:7
             |
        2801 | @disjoint_base
        2802 | class list(MutableSequence[_T]):
             |       ^^^^
        2803 |     """Built-in mutable sequence.
             |
        info: Source
         --> main2.py:4:5
          |
        2 | def foo(x: int): pass
        3 | x[: list[Unknown | int]] = [1]
        4 | y[: list[Unknown | int]] = [2]
          |     ^^^^
        5 |
        6 | foo(x[0])
          |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:20:1
           |
        19 | # Types
        20 | Unknown = object()
           | ^^^^^^^
        21 | AlwaysTruthy = object()
        22 | AlwaysFalsy = object()
           |
        info: Source
         --> main2.py:4:10
          |
        2 | def foo(x: int): pass
        3 | x[: list[Unknown | int]] = [1]
        4 | y[: list[Unknown | int]] = [2]
          |          ^^^^^^^
        5 |
        6 | foo(x[0])
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
         --> main2.py:4:20
          |
        2 | def foo(x: int): pass
        3 | x[: list[Unknown | int]] = [1]
        4 | y[: list[Unknown | int]] = [2]
          |                    ^^^
        5 |
        6 | foo(x[0])
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int): pass
          |         ^
        3 | x = [1]
        4 | y = [2]
          |
        info: Source
         --> main2.py:7:6
          |
        6 | foo(x[0])
        7 | foo([x=]y[0])
          |      ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: File after edits
        info: Source

        def foo(x: int): pass
        x: list[Unknown | int] = [1]
        y: list[Unknown | int] = [2]

        foo(x[0])
        foo(y[0])
        "#);
    }

    #[test]
    fn test_function_call_with_positional_only_parameter() {
        let mut test = inlay_hint_test(
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
        let mut test = inlay_hint_test(
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
        let mut test = inlay_hint_test(
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
        let mut test = inlay_hint_test(
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
        let mut test = inlay_hint_test(
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
         --> main2.py:3:9
          |
        2 | def foo(x: int, /, y: int): pass
        3 | foo(1, [y=]2)
          |         ^
          |
        ");
    }

    #[test]
    fn test_function_call_positional_only_and_variadic_parameters() {
        let mut test = inlay_hint_test(
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
        let mut test = inlay_hint_test(
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
        let mut test = inlay_hint_test(
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
         --> main2.py:4:6
          |
        2 | class Foo:
        3 |     def __init__(self, x: int): pass
        4 | Foo([x=]1)
          |      ^
        5 | f[: Foo] = Foo([x=]1)
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class Foo:
          |       ^^^
        3 |     def __init__(self, x: int): pass
        4 | Foo(1)
          |
        info: Source
         --> main2.py:5:5
          |
        3 |     def __init__(self, x: int): pass
        4 | Foo([x=]1)
        5 | f[: Foo] = Foo([x=]1)
          |     ^^^
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
         --> main2.py:5:17
          |
        3 |     def __init__(self, x: int): pass
        4 | Foo([x=]1)
        5 | f[: Foo] = Foo([x=]1)
          |                 ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: File after edits
        info: Source

        class Foo:
            def __init__(self, x: int): pass
        Foo(1)
        f: Foo = Foo(1)
        ");
    }

    #[test]
    fn test_class_constructor_call_new() {
        let mut test = inlay_hint_test(
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
         --> main2.py:4:6
          |
        2 | class Foo:
        3 |     def __new__(cls, x: int): pass
        4 | Foo([x=]1)
          |      ^
        5 | f[: Foo] = Foo([x=]1)
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class Foo:
          |       ^^^
        3 |     def __new__(cls, x: int): pass
        4 | Foo(1)
          |
        info: Source
         --> main2.py:5:5
          |
        3 |     def __new__(cls, x: int): pass
        4 | Foo([x=]1)
        5 | f[: Foo] = Foo([x=]1)
          |     ^^^
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
         --> main2.py:5:17
          |
        3 |     def __new__(cls, x: int): pass
        4 | Foo([x=]1)
        5 | f[: Foo] = Foo([x=]1)
          |                 ^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: File after edits
        info: Source

        class Foo:
            def __new__(cls, x: int): pass
        Foo(1)
        f: Foo = Foo(1)
        ");
    }

    #[test]
    fn test_class_constructor_call_meta_class_call() {
        let mut test = inlay_hint_test(
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
         --> main2.py:6:6
          |
        4 | class Foo(metaclass=MetaFoo):
        5 |     pass
        6 | Foo([x=]1)
          |      ^
          |
        ");
    }

    #[test]
    fn test_callable_call() {
        let mut test = inlay_hint_test(
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
        let mut test = inlay_hint_test(
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
         --> main2.py:4:12
          |
        2 | class Foo:
        3 |     def bar(self, y: int): pass
        4 | Foo().bar([y=]2)
          |            ^
          |
        ");
    }

    #[test]
    fn test_class_method_call() {
        let mut test = inlay_hint_test(
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
         --> main2.py:5:10
          |
        3 |     @classmethod
        4 |     def bar(cls, y: int): pass
        5 | Foo.bar([y=]2)
          |          ^
          |
        ");
    }

    #[test]
    fn test_static_method_call() {
        let mut test = inlay_hint_test(
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
         --> main2.py:5:10
          |
        3 |     @staticmethod
        4 |     def bar(y: int): pass
        5 | Foo.bar([y=]2)
          |          ^
          |
        ");
    }

    #[test]
    fn test_function_call_with_union_type() {
        let mut test = inlay_hint_test(
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
         --> main2.py:3:6
          |
        2 | def foo(x: int | str): pass
        3 | foo([x=]1)
          |      ^
        4 | foo([x=]'abc')
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
         --> main2.py:4:6
          |
        2 | def foo(x: int | str): pass
        3 | foo([x=]1)
        4 | foo([x=]'abc')
          |      ^
          |
        ");
    }

    #[test]
    fn test_function_call_multiple_positional_arguments() {
        let mut test = inlay_hint_test(
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
         --> main2.py:3:6
          |
        2 | def foo(x: int, y: str, z: bool): pass
        3 | foo([x=]1, [y=]'hello', [z=]True)
          |      ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:17
          |
        2 | def foo(x: int, y: str, z: bool): pass
          |                 ^
        3 | foo(1, 'hello', True)
          |
        info: Source
         --> main2.py:3:13
          |
        2 | def foo(x: int, y: str, z: bool): pass
        3 | foo([x=]1, [y=]'hello', [z=]True)
          |             ^
          |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:25
          |
        2 | def foo(x: int, y: str, z: bool): pass
          |                         ^
        3 | foo(1, 'hello', True)
          |
        info: Source
         --> main2.py:3:26
          |
        2 | def foo(x: int, y: str, z: bool): pass
        3 | foo([x=]1, [y=]'hello', [z=]True)
          |                          ^
          |
        ");
    }

    #[test]
    fn test_function_call_mixed_positional_and_keyword() {
        let mut test = inlay_hint_test(
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
         --> main2.py:3:6
          |
        2 | def foo(x: int, y: str, z: bool): pass
        3 | foo([x=]1, z=True, y='hello')
          |      ^
          |
        ");
    }

    #[test]
    fn test_function_call_with_default_parameters() {
        let mut test = inlay_hint_test(
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
         --> main2.py:3:6
          |
        2 | def foo(x: int, y: str = 'default', z: bool = False): pass
        3 | foo([x=]1)
          |      ^
        4 | foo([x=]1, [y=]'custom')
        5 | foo([x=]1, [y=]'custom', [z=]True)
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
         --> main2.py:4:6
          |
        2 | def foo(x: int, y: str = 'default', z: bool = False): pass
        3 | foo([x=]1)
        4 | foo([x=]1, [y=]'custom')
          |      ^
        5 | foo([x=]1, [y=]'custom', [z=]True)
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
         --> main2.py:4:13
          |
        2 | def foo(x: int, y: str = 'default', z: bool = False): pass
        3 | foo([x=]1)
        4 | foo([x=]1, [y=]'custom')
          |             ^
        5 | foo([x=]1, [y=]'custom', [z=]True)
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
         --> main2.py:5:6
          |
        3 | foo([x=]1)
        4 | foo([x=]1, [y=]'custom')
        5 | foo([x=]1, [y=]'custom', [z=]True)
          |      ^
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
         --> main2.py:5:13
          |
        3 | foo([x=]1)
        4 | foo([x=]1, [y=]'custom')
        5 | foo([x=]1, [y=]'custom', [z=]True)
          |             ^
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
         --> main2.py:5:27
          |
        3 | foo([x=]1)
        4 | foo([x=]1, [y=]'custom')
        5 | foo([x=]1, [y=]'custom', [z=]True)
          |                           ^
          |
        ");
    }

    #[test]
    fn test_nested_function_calls() {
        let mut test = inlay_hint_test(
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
          --> main2.py:10:6
           |
         8 | def baz(a: int, b: str, c: bool): pass
         9 |
        10 | baz([a=]foo([x=]5), [b=]bar([y=]bar([y=]'test')), [c=]True)
           |      ^
           |

        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:9
          |
        2 | def foo(x: int) -> int:
          |         ^
        3 |     return x * 2
          |
        info: Source
          --> main2.py:10:14
           |
         8 | def baz(a: int, b: str, c: bool): pass
         9 |
        10 | baz([a=]foo([x=]5), [b=]bar([y=]bar([y=]'test')), [c=]True)
           |              ^
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
          --> main2.py:10:22
           |
         8 | def baz(a: int, b: str, c: bool): pass
         9 |
        10 | baz([a=]foo([x=]5), [b=]bar([y=]bar([y=]'test')), [c=]True)
           |                      ^
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
          --> main2.py:10:30
           |
         8 | def baz(a: int, b: str, c: bool): pass
         9 |
        10 | baz([a=]foo([x=]5), [b=]bar([y=]bar([y=]'test')), [c=]True)
           |                              ^
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
          --> main2.py:10:38
           |
         8 | def baz(a: int, b: str, c: bool): pass
         9 |
        10 | baz([a=]foo([x=]5), [b=]bar([y=]bar([y=]'test')), [c=]True)
           |                                      ^
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
          --> main2.py:10:52
           |
         8 | def baz(a: int, b: str, c: bool): pass
         9 |
        10 | baz([a=]foo([x=]5), [b=]bar([y=]bar([y=]'test')), [c=]True)
           |                                                    ^
           |
        ");
    }

    #[test]
    fn test_method_chaining() {
        let mut test = inlay_hint_test(
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
         --> main2.py:8:10
          |
        6 |         return self
        7 |     def baz(self): pass
        8 | A().foo([value=]42).bar([name=]'test').baz()
          |          ^^^^^
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
         --> main2.py:8:26
          |
        6 |         return self
        7 |     def baz(self): pass
        8 | A().foo([value=]42).bar([name=]'test').baz()
          |                          ^^^^
          |
        ");
    }

    #[test]
    fn test_nexted_keyword_function_calls() {
        let mut test = inlay_hint_test(
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
         --> main2.py:5:12
          |
        3 |     return x
        4 | def bar(y: int): pass
        5 | bar(y=foo([x=]'test'))
          |            ^
          |
        ");
    }

    #[test]
    fn test_lambda_function_calls() {
        let mut test = inlay_hint_test(
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
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:20:1
           |
        19 | # Types
        20 | Unknown = object()
           | ^^^^^^^
        21 | AlwaysTruthy = object()
        22 | AlwaysFalsy = object()
           |
        info: Source
         --> main2.py:2:14
          |
        2 | foo[: (x) -> Unknown] = lambda x: x * 2
          |              ^^^^^^^
        3 | bar[: (a, b) -> Unknown] = lambda a, b: a + b
        4 | foo([x=]5)
          |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:20:1
           |
        19 | # Types
        20 | Unknown = object()
           | ^^^^^^^
        21 | AlwaysTruthy = object()
        22 | AlwaysFalsy = object()
           |
        info: Source
         --> main2.py:3:17
          |
        2 | foo[: (x) -> Unknown] = lambda x: x * 2
        3 | bar[: (a, b) -> Unknown] = lambda a, b: a + b
          |                 ^^^^^^^
        4 | foo([x=]5)
        5 | bar([a=]1, [b=]2)
          |
        ");
    }

    #[test]
    fn test_literal_string() {
        let mut test = inlay_hint_test(
            r#"
            from typing import LiteralString
            def my_func(x: LiteralString):
                y = x
            my_func(x="hello")"#,
        );

        assert_snapshot!(test.inlay_hints(), @r#"
        from typing import LiteralString
        def my_func(x: LiteralString):
            y[: LiteralString] = x
        my_func(x="hello")
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ^^^
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
         --> main2.py:4:9
          |
        2 | from typing import LiteralString
        3 | def my_func(x: LiteralString):
        4 |     y[: LiteralString] = x
          |         ^^^^^^^^^^^^^
        5 | my_func(x="hello")
          |

        ---------------------------------------------
        info[inlay-hint-edit]: File after edits
        info: Source

        from typing import LiteralString
        def my_func(x: LiteralString):
            y: LiteralString = x
        my_func(x="hello")
        "#);
    }

    #[test]
    fn test_literal_group() {
        let mut test = inlay_hint_test(
            r#"
            def branch(cond: int):
                if cond < 10:
                    x = 1
                elif cond < 20:
                    x = 2
                elif cond < 30:
                    x = 3
                elif cond < 40:
                    x = "hello"
                else:
                    x = None
                y = x"#,
        );

        assert_snapshot!(test.inlay_hints(), @r#"
        def branch(cond: int):
            if cond < 10:
                x = 1
            elif cond < 20:
                x = 2
            elif cond < 30:
                x = 3
            elif cond < 40:
                x = "hello"
            else:
                x = None
            y[: Literal[1, 2, 3, "hello"] | None] = x
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/typing.pyi:351:1
            |
        349 | Final: _SpecialForm
        350 |
        351 | Literal: _SpecialForm
            | ^^^^^^^
        352 | TypedDict: _SpecialForm
            |
        info: Source
          --> main2.py:13:9
           |
        11 |     else:
        12 |         x = None
        13 |     y[: Literal[1, 2, 3, "hello"] | None] = x
           |         ^^^^^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
          --> main2.py:13:17
           |
        11 |     else:
        12 |         x = None
        13 |     y[: Literal[1, 2, 3, "hello"] | None] = x
           |                 ^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
          --> main2.py:13:20
           |
        11 |     else:
        12 |         x = None
        13 |     y[: Literal[1, 2, 3, "hello"] | None] = x
           |                    ^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
          --> main2.py:13:23
           |
        11 |     else:
        12 |         x = None
        13 |     y[: Literal[1, 2, 3, "hello"] | None] = x
           |                       ^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ^^^
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
          --> main2.py:13:26
           |
        11 |     else:
        12 |         x = None
        13 |     y[: Literal[1, 2, 3, "hello"] | None] = x
           |                          ^^^^^^^
           |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/types.pyi:950:11
            |
        948 | if sys.version_info >= (3, 10):
        949 |     @final
        950 |     class NoneType:
            |           ^^^^^^^^
        951 |         """The type of the None singleton."""
            |
        info: Source
          --> main2.py:13:37
           |
        11 |     else:
        12 |         x = None
        13 |     y[: Literal[1, 2, 3, "hello"] | None] = x
           |                                     ^^^^
           |

        ---------------------------------------------
        info[inlay-hint-edit]: File after edits
        info: Source

        def branch(cond: int):
            if cond < 10:
                x = 1
            elif cond < 20:
                x = 2
            elif cond < 30:
                x = 3
            elif cond < 40:
                x = "hello"
            else:
                x = None
            y: Literal[1, 2, 3, "hello"] | None = x
        "#);
    }

    #[test]
    fn test_generic_alias() {
        let mut test = inlay_hint_test(
            r"
            class Foo[T]: ...

            a = Foo[int]",
        );

        assert_snapshot!(test.inlay_hints(), @r#"
        class Foo[T]: ...

        a[: <class 'Foo[int]'>] = Foo[int]
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:2:7
          |
        2 | class Foo[T]: ...
          |       ^^^
        3 |
        4 | a = Foo[int]
          |
        info: Source
         --> main2.py:4:13
          |
        2 | class Foo[T]: ...
        3 |
        4 | a[: <class 'Foo[int]'>] = Foo[int]
          |             ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
         --> main2.py:4:17
          |
        2 | class Foo[T]: ...
        3 |
        4 | a[: <class 'Foo[int]'>] = Foo[int]
          |                 ^^^
          |
        "#);
    }

    #[test]
    fn test_subclass_type() {
        let mut test = inlay_hint_test(
            r"
            def f(x: list[str]):
                y = type(x)",
        );

        assert_snapshot!(test.inlay_hints(), @r#"
        def f(x: list[str]):
            y[: type[list[str]]] = type(x)
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:247:7
            |
        246 | @disjoint_base
        247 | class type:
            |       ^^^^
        248 |     """type(object) -> the object's type
        249 |     type(name, bases, dict, **kwds) -> a new type
            |
        info: Source
         --> main2.py:3:9
          |
        2 | def f(x: list[str]):
        3 |     y[: type[list[str]]] = type(x)
          |         ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2802:7
             |
        2801 | @disjoint_base
        2802 | class list(MutableSequence[_T]):
             |       ^^^^
        2803 |     """Built-in mutable sequence.
             |
        info: Source
         --> main2.py:3:14
          |
        2 | def f(x: list[str]):
        3 |     y[: type[list[str]]] = type(x)
          |              ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ^^^
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
         --> main2.py:3:19
          |
        2 | def f(x: list[str]):
        3 |     y[: type[list[str]]] = type(x)
          |                   ^^^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: File after edits
        info: Source

        def f(x: list[str]):
            y: type[list[str]] = type(x)
        "#);
    }

    #[test]
    fn test_property_literal_type() {
        let mut test = inlay_hint_test(
            r"
            class F:
                @property
                def whatever(self): ...

            ab = F.whatever",
        );

        assert_snapshot!(test.inlay_hints(), @r"
        class F:
            @property
            def whatever(self): ...

        ab[: property] = F.whatever
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> main.py:4:9
          |
        2 | class F:
        3 |     @property
        4 |     def whatever(self): ...
          |         ^^^^^^^^
        5 |
        6 | ab = F.whatever
          |
        info: Source
         --> main2.py:6:6
          |
        4 |     def whatever(self): ...
        5 |
        6 | ab[: property] = F.whatever
          |      ^^^^^^^^
          |

        ---------------------------------------------
        info[inlay-hint-edit]: File after edits
        info: Source

        class F:
            @property
            def whatever(self): ...

        ab: property = F.whatever
        ");
    }

    #[test]
    fn test_complex_parameter_combinations() {
        let mut test = inlay_hint_test(
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
         --> main2.py:3:16
          |
        2 | def foo(a: int, b: str, /, c: float, d: bool = True, *, e: int, f: str = 'default'): pass
        3 | foo(1, 'pos', [c=]3.14, [d=]False, e=42)
          |                ^
        4 | foo(1, 'pos', [c=]3.14, e=42, f='custom')
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
         --> main2.py:3:26
          |
        2 | def foo(a: int, b: str, /, c: float, d: bool = True, *, e: int, f: str = 'default'): pass
        3 | foo(1, 'pos', [c=]3.14, [d=]False, e=42)
          |                          ^
        4 | foo(1, 'pos', [c=]3.14, e=42, f='custom')
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
         --> main2.py:4:16
          |
        2 | def foo(a: int, b: str, /, c: float, d: bool = True, *, e: int, f: str = 'default'): pass
        3 | foo(1, 'pos', [c=]3.14, [d=]False, e=42)
        4 | foo(1, 'pos', [c=]3.14, e=42, f='custom')
          |                ^
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
         --> main2.py:4:6
          |
        2 | from foo import bar
        3 |
        4 | bar([x=]1)
          |      ^
          |
        ");
    }

    #[test]
    fn test_overloaded_function_calls() {
        let mut test = inlay_hint_test(
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
          --> main2.py:11:6
           |
         9 |     return x
        10 |
        11 | foo([x=]42)
           |      ^
        12 | foo([x=]'hello')
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
          --> main2.py:12:6
           |
        11 | foo([x=]42)
        12 | foo([x=]'hello')
           |      ^
           |
        ");
    }

    #[test]
    fn test_disabled_function_argument_names() {
        let mut test = inlay_hint_test(
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
        let mut test = inlay_hint_test(
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
         --> main2.py:4:6
          |
        2 | def foo(x: int): pass
        3 | def bar(y: int): pass
        4 | foo([x=]1)
          |      ^
        5 | bar(2)
          |
        ");
    }

    #[test]
    fn test_function_call_with_argument_name_starting_with_underscore() {
        let mut test = inlay_hint_test(
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
         --> main2.py:3:9
          |
        2 | def foo(_x: int, y: int): pass
        3 | foo(1, [y=]2)
          |         ^
          |
        ");
    }

    #[test]
    fn test_function_call_different_formatting() {
        let mut test = inlay_hint_test(
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
         --> main2.py:7:6
          |
        5 | ): ...
        6 |
        7 | foo([x=]1, [y=]2)
          |      ^
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
         --> main2.py:7:13
          |
        5 | ): ...
        6 |
        7 | foo([x=]1, [y=]2)
          |             ^
          |
        ");
    }

    #[test]
    fn test_function_signature_inlay_hint() {
        let mut test = inlay_hint_test(
            "
                  def foo(x: int, *y: bool, z: str | int | list[str]): ...

                  a = foo",
        );

        assert_snapshot!(test.inlay_hints(), @r#"
        def foo(x: int, *y: bool, z: str | int | list[str]): ...

        a[: def foo(x: int, *y: bool, *, z: str | int | list[str]) -> Unknown] = foo
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
         --> main2.py:4:16
          |
        2 | def foo(x: int, *y: bool, z: str | int | list[str]): ...
        3 |
        4 | a[: def foo(x: int, *y: bool, *, z: str | int | list[str]) -> Unknown] = foo
          |                ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2591:7
             |
        2590 | @final
        2591 | class bool(int):
             |       ^^^^
        2592 |     """Returns True when the argument is true, False otherwise.
        2593 |     The builtins True and False are the only two instances of the class bool.
             |
        info: Source
         --> main2.py:4:25
          |
        2 | def foo(x: int, *y: bool, z: str | int | list[str]): ...
        3 |
        4 | a[: def foo(x: int, *y: bool, *, z: str | int | list[str]) -> Unknown] = foo
          |                         ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ^^^
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
         --> main2.py:4:37
          |
        2 | def foo(x: int, *y: bool, z: str | int | list[str]): ...
        3 |
        4 | a[: def foo(x: int, *y: bool, *, z: str | int | list[str]) -> Unknown] = foo
          |                                     ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:348:7
            |
        347 | @disjoint_base
        348 | class int:
            |       ^^^
        349 |     """int([x]) -> integer
        350 |     int(x, base=10) -> integer
            |
        info: Source
         --> main2.py:4:43
          |
        2 | def foo(x: int, *y: bool, z: str | int | list[str]): ...
        3 |
        4 | a[: def foo(x: int, *y: bool, *, z: str | int | list[str]) -> Unknown] = foo
          |                                           ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
            --> stdlib/builtins.pyi:2802:7
             |
        2801 | @disjoint_base
        2802 | class list(MutableSequence[_T]):
             |       ^^^^
        2803 |     """Built-in mutable sequence.
             |
        info: Source
         --> main2.py:4:49
          |
        2 | def foo(x: int, *y: bool, z: str | int | list[str]): ...
        3 |
        4 | a[: def foo(x: int, *y: bool, *, z: str | int | list[str]) -> Unknown] = foo
          |                                                 ^^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
           --> stdlib/builtins.pyi:915:7
            |
        914 | @disjoint_base
        915 | class str(Sequence[str]):
            |       ^^^
        916 |     """str(object='') -> str
        917 |     str(bytes_or_buffer[, encoding[, errors]]) -> str
            |
        info: Source
         --> main2.py:4:54
          |
        2 | def foo(x: int, *y: bool, z: str | int | list[str]): ...
        3 |
        4 | a[: def foo(x: int, *y: bool, *, z: str | int | list[str]) -> Unknown] = foo
          |                                                      ^^^
          |

        info[inlay-hint-location]: Inlay Hint Target
          --> stdlib/ty_extensions.pyi:20:1
           |
        19 | # Types
        20 | Unknown = object()
           | ^^^^^^^
        21 | AlwaysTruthy = object()
        22 | AlwaysFalsy = object()
           |
        info: Source
         --> main2.py:4:63
          |
        2 | def foo(x: int, *y: bool, z: str | int | list[str]): ...
        3 |
        4 | a[: def foo(x: int, *y: bool, *, z: str | int | list[str]) -> Unknown] = foo
          |                                                               ^^^^^^^
          |
        "#);
    }

    #[test]
    fn test_module_inlay_hint() {
        let mut test = inlay_hint_test(
            "
                      import foo

                      a = foo",
        );

        test.with_extra_file("foo.py", "'''Foo module'''");

        assert_snapshot!(test.inlay_hints(), @r"
        import foo

        a[: <module 'foo'>] = foo
        ---------------------------------------------
        info[inlay-hint-location]: Inlay Hint Target
         --> foo.py:1:1
          |
        1 | '''Foo module'''
          | ^^^^^^^^^^^^^^^^
          |
        info: Source
         --> main2.py:4:5
          |
        2 | import foo
        3 |
        4 | a[: <module 'foo'>] = foo
          |     ^^^^^^^^^^^^^^
          |
        ");
    }

    #[test]
    fn test_literal_type_alias_inlay_hint() {
        let mut test = inlay_hint_test(
            "
                        from typing import Literal

                        a = Literal['a', 'b', 'c']",
        );

        assert_snapshot!(test.inlay_hints(), @r#"
        from typing import Literal

        a[: <special form 'Literal["a", "b", "c"]'>] = Literal['a', 'b', 'c']
        "#);
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

    struct InlayHintEditDiagnostic {
        file_content: String,
    }

    impl InlayHintEditDiagnostic {
        fn new(file_content: String) -> Self {
            Self { file_content }
        }
    }

    impl IntoDiagnostic for InlayHintEditDiagnostic {
        fn into_diagnostic(self) -> Diagnostic {
            let mut main = Diagnostic::new(
                DiagnosticId::Lint(LintName::of("inlay-hint-edit")),
                Severity::Info,
                "File after edits".to_string(),
            );

            main.sub(SubDiagnostic::new(
                SubDiagnosticSeverity::Info,
                format!("{}\n{}", "Source", self.file_content),
            ));

            main
        }
    }
}
