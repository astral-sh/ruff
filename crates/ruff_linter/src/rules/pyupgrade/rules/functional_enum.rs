use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::rules::pyupgrade::rules::create_class_def_stmt;
use itertools::Itertools;
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::name::Name;
use ruff_python_ast::{
    Arguments, DictItem, Expr, ExprCall, ExprContext, ExprList, ExprName, ExprStringLiteral,
    ExprTuple, Stmt, StmtAssign, StmtPass,
};
use ruff_python_semantic::SemanticModel;
use ruff_python_stdlib::identifiers::is_identifier;
use ruff_text_size::{Ranged, TextRange, TextSize};
use smallvec::{smallvec, SmallVec};

/// ## What it does
/// Checks for enums declared using functional syntax
/// that can be rewritten to use the class syntax.
///
/// ## Why is this bad?
/// `Enum`s can be defined using either a functional syntax (`E = Enum(...)`)
/// or a class syntax (`class E(Enum): ...`).
///
/// The class syntax is more readable and generally preferred over the functional syntax.
///
/// ## Example
///
/// ```python
/// from enum import Enum
///
///
/// E = Enum("E", ["A", "B", "C"])
/// ```
///
/// Use instead:
///
/// ```python
/// from enum import Enum
///
///
/// class E(Enum):
///     A = auto()
///     B = auto()
///     C = auto()
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct FunctionalEnum;

impl Violation for FunctionalEnum {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Enum declared using functional syntax".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with class syntax".to_string())
    }
}

/// UP048
pub(crate) fn functional_enum(checker: &mut Checker, stmt: &StmtAssign) {
    let Expr::Call(call) = stmt.value.as_ref() else {
        return;
    };

    let [Expr::Name(ExprName { id: name, .. })] = &stmt.targets[..] else {
        return;
    };

    let Some(members) = enum_members(call, checker.semantic()) else {
        return;
    };

    let mut diagnostic = Diagnostic::new(FunctionalEnum, stmt.range);

    let (name, enum_func, range) = (name.as_str(), &call.func, stmt.range());
    if let Some(fix) = convert_to_class_syntax(name, enum_func, &members, range, checker) {
        diagnostic.set_fix(fix);
    }

    checker.diagnostics.push(diagnostic);
}

#[derive(Debug)]
enum EnumMember<'a> {
    NameOnly(String),
    NameValue(String, &'a Expr),
}

impl<'a> EnumMember<'a> {
    fn from(name: String, value: Option<&'a Expr>) -> Option<Self> {
        if !is_identifier(&name) {
            return None;
        }

        if matches!(value, Some(Expr::Starred(_))) {
            return None;
        }

        match value {
            Some(value) => Some(Self::NameValue(name, value)),
            None => Some(Self::NameOnly(name)),
        }
    }

    fn from_exprs(name_expr: &Expr, value: Option<&'a Expr>) -> Option<Self> {
        let Expr::StringLiteral(string) = name_expr else {
            return None;
        };

        let name = string.value.to_string();

        Self::from(name, value)
    }
}

impl EnumMember<'_> {
    fn name(&self) -> &str {
        match self {
            EnumMember::NameOnly(name) => name,
            EnumMember::NameValue(name, _) => name,
        }
    }

    fn value(&self) -> Option<&Expr> {
        match self {
            EnumMember::NameOnly(_) => None,
            EnumMember::NameValue(_, value) => Some(value),
        }
    }

    fn create_assignment(&self, auto: String) -> Stmt {
        let member_name = ExprName {
            id: Name::new(self.name()),
            ctx: ExprContext::Store,
            range: TextRange::default(),
        };

        let target = Expr::Name(member_name);

        let value = match self.value() {
            Some(expr) => expr.clone(),

            None => Expr::Call(ExprCall {
                func: Box::new(Expr::Name(ExprName {
                    id: Name::new(auto),
                    ctx: ExprContext::Load,
                    range: TextRange::default(),
                })),
                arguments: Arguments {
                    args: Box::from([]),
                    keywords: Box::from([]),
                    range: TextRange::default(),
                },
                range: TextRange::default(),
            }),
        };

        Stmt::Assign(StmtAssign {
            targets: vec![target],
            value: Box::new(value),
            range: TextRange::default(),
        })
    }
}

fn enum_members<'a>(call: &'a ExprCall, semantic: &SemanticModel) -> Option<Vec<EnumMember<'a>>> {
    if !is_enum_func(&call.func, semantic) {
        return None;
    }

    if call.arguments.len() != 2 {
        return None;
    }

    // https://github.com/python/cpython/blob/3.13/Lib/enum.py#L838
    match call.arguments.find_positional(1)? {
        Expr::StringLiteral(string) => enum_members_from_string(string),

        Expr::List(ExprList { elts, .. }) | Expr::Tuple(ExprTuple { elts, .. }) => {
            match elts.first() {
                None => Some(vec![]),
                Some(Expr::StringLiteral(_)) => enum_members_from_iterable_of_strings(elts),
                Some(Expr::List(_) | Expr::Tuple(_)) => enum_members_from_iterable_of_pairs(elts),
                _ => None,
            }
        }

        Expr::Dict(dict) => enum_members_from_dict_items(&dict.items),

        _ => None,
    }
}

/// See also [`ruff_python_semantic::analyze::class::is_enumeration`].
fn is_enum_func(func: &Expr, semantic: &SemanticModel) -> bool {
    let Some(qualified_name) = semantic.resolve_qualified_name(func) else {
        return false;
    };

    matches!(
        qualified_name.segments(),
        [
            "enum",
            "Enum" | "Flag" | "IntEnum" | "IntFlag" | "StrEnum" | "ReprEnum" | "CheckEnum"
        ]
    )
}

/// ```python
/// E = Enum("E", "A B C")
/// ```
fn enum_members_from_string(string: &ExprStringLiteral) -> Option<Vec<EnumMember>> {
    let mut members = vec![];

    let normalized = string.value.to_string().replace(',', " ");

    for name in normalized.split_whitespace() {
        let Some(member) = EnumMember::from(name.to_string(), None) else {
            return None;
        };

        members.push(member);
    }

    Some(members)
}

/// ```python
/// E = Enum("E", ["A", "B", "C"])
/// E = Enum("E", ("A", "B", "C"))
/// ```
fn enum_members_from_iterable_of_strings(elements: &[Expr]) -> Option<Vec<EnumMember>> {
    let mut members = vec![];

    for element in elements {
        let member = EnumMember::from_exprs(element, None)?;
        members.push(member);
    }

    Some(members)
}

/// ```python
/// E = Enum("E", [("A", 1), ("B", 2), ("C", 3)])
/// E = Enum("E", (("A", 1), ("B", 2), ("C", 3)))
/// ```
fn enum_members_from_iterable_of_pairs(elements: &[Expr]) -> Option<Vec<EnumMember>> {
    let mut members = vec![];

    for element in elements {
        let elts = match element {
            Expr::List(ExprList { elts, .. }) => elts,
            Expr::Tuple(ExprTuple { elts, .. }) => elts,
            _ => return None,
        };

        let [name, value] = &elts[..] else {
            return None;
        };
        let member = EnumMember::from_exprs(name, Some(value))?;

        members.push(member);
    }

    Some(members)
}

/// ```python
/// E = Enum("E", {"A": 1, "B": 2, "C": 3})
/// ```
fn enum_members_from_dict_items(items: &[DictItem]) -> Option<Vec<EnumMember>> {
    let mut members = vec![];

    for item in items {
        let Some(name_expr) = &item.key else {
            return None;
        };

        let member = EnumMember::from_exprs(name_expr, Some(&item.value))?;
        members.push(member);
    }

    Some(members)
}

fn convert_to_class_syntax(
    name: &str,
    enum_func: &Expr,
    members: &[EnumMember],
    range: TextRange,
    checker: &Checker,
) -> Option<Fix> {
    let name = name.to_string();

    let mut other_edits: SmallVec<[Edit; 1]> = smallvec![];

    let body = if members.is_empty() {
        vec![Stmt::Pass(StmtPass {
            range: TextRange::default(),
        })]
    } else {
        let auto = if matches!(members.first(), Some(EnumMember::NameOnly(_))) {
            let (edit, binding) = import_enum_auto(range.start(), checker)?;
            other_edits.push(edit);
            binding
        } else {
            "auto".to_string()
        };

        members
            .iter()
            .map(|member| member.create_assignment(auto.clone()))
            .collect_vec()
    };

    let class = create_class_def_stmt(&name, body, None, enum_func);
    let new_content = checker.generator().stmt(&class);
    let replace_with_class = Edit::range_replacement(new_content, range);

    Some(Fix::unsafe_edits(replace_with_class, other_edits))
}

#[inline]
fn import_enum_auto(offset: TextSize, checker: &Checker) -> Option<(Edit, String)> {
    let (importer, semantic) = (checker.importer(), checker.semantic());
    let import_request = ImportRequest::import_from("enum", "auto");

    importer
        .get_or_import_symbol(&import_request, offset, semantic)
        .ok()
}
