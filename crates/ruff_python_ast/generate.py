#!/usr/bin/python
# /// script
# requires-python = ">=3.11"
# dependencies = []
#
# [tool.ty.analysis]
# return-type-inference = "always"
#
# [tool.ty.rules]
# truthiness-test-of-none-union = "warn"
# blanket-ignore-comment = "warn"
# missing-type-argument = "warn"
# possibly-unresolved-reference = "warn"
# unsound-return-statement = "warn"
# unsound-yield = "warn"
# unsupported-dynamic-base = "warn"
# division-by-zero = "warn"
# dynamic-function-decorator-return = "warn"
# unsound-assignment = "warn"
# redundant-condition-strict = "warn"
# disjoint-cast = "warn"
# missing-direct-dependency = "warn"
#
# [tool.uv]
# exclude-newer = "P7D"
# require-build-hashes = true
# preview-features = ["build-dependency-hashes", "missing-exclude-newer-package-lock"]
# ///

from __future__ import annotations

import re
from dataclasses import dataclass, replace
from pathlib import Path
from subprocess import check_output
from typing import Any

import tomllib

# Types that require `crate::`. We can slowly remove these types as we move them to generate scripts.
types_requiring_crate_prefix = {
    "IpyEscapeKind",
    "ExprContext",
    "Identifier",
    "Number",
    "BytesValue",
    "StringValue",
    "FStringValue",
    "TStringValue",
    "Arguments",
    "CmpOp",
    "Comprehension",
    "MappingPatternItems",
    "UnaryOp",
    "BoolOp",
    "Operator",
    "Decorator",
    "TypeParams",
    "Parameters",
    "ElifElseClause",
    "WithItem",
    "MatchCase",
    "Alias",
    "Singleton",
    "PatternArguments",
}


@dataclass
class VisitorInfo:
    name: str
    accepts_sequence: bool = False


# Map of AST node types to their corresponding visitor information.
# Only visitors that are different from the default `visit_*` method are included.
# These visitors either have a different name or accept a sequence of items.
type_to_visitor_function: dict[str, VisitorInfo] = {
    "TypeParams": VisitorInfo("visit_type_params", True),
    "Parameters": VisitorInfo("visit_parameters", True),
    "Stmt": VisitorInfo("visit_body", True),
    "Arguments": VisitorInfo("visit_arguments", True),
}


def rustfmt(code: str) -> str:
    return check_output(["rustfmt", "--emit=stdout"], input=code, text=True)


def to_snake_case(node: str) -> str:
    """Converts CamelCase to snake_case"""
    return re.sub(r"([A-Z])", r"_\1", node).lower().lstrip("_")


def write_rustdoc(out: list[str], doc: str) -> None:
    for line in doc.split("\n"):
        out.append(f"/// {line}")


# ------------------------------------------------------------------------------
# Read AST description


def load_ast(root: Path) -> Ast:
    ast_path = root.joinpath("crates", "ruff_python_ast", "ast.toml")
    with ast_path.open("rb") as ast_file:
        ast = tomllib.load(ast_file)
    return Ast(ast)


# ------------------------------------------------------------------------------
# Preprocess


@dataclass
class Ast:
    """
    The parsed representation of the `ast.toml` file. Defines all of the Python
    AST syntax nodes, and which groups (`Stmt`, `Expr`, etc.) they belong to.
    """

    groups: list[Group]
    all_nodes: list[Node]

    def __init__(self, ast: dict[str, Any]) -> None:
        self.groups = []
        self.all_nodes = []
        for group_name, group in ast.items():
            group = Group(group_name, group)
            self.all_nodes.extend(group.nodes)
            if group_name != "ungrouped":
                self.groups.append(group)


@dataclass
class Group:
    name: str
    nodes: list[Node]

    add_suffix_to_is_methods: bool
    doc: str | None

    def __init__(self, group_name: str, group: dict[str, Any]) -> None:
        self.name = group_name
        self.add_suffix_to_is_methods = group.get("add_suffix_to_is_methods", False)
        self.doc = group.get("doc")
        self.nodes = [
            Node(self, node_name, node) for node_name, node in group["nodes"].items()
        ]


@dataclass
class Node:
    name: str
    variant: str
    doc: str | None
    fields: list[Field]
    source_order: list[str] | None
    debug_range_first: bool

    def __init__(self, group: Group, node_name: str, node: dict[str, Any]) -> None:
        self.name = node_name
        self.variant = node.get("variant", node_name.removeprefix(group.name))
        self.doc = node.get("doc")
        self.fields = [Field(field) for field in node["fields"]]
        self.source_order = node.get("source_order")
        # Auxiliary nodes print their source range first in parser snapshots.
        self.debug_range_first = group.name in {
            "ungrouped",
            "ExceptHandler",
            "InterpolatedStringElement",
        }


@dataclass
class Field:
    name: str
    ty: str
    optional: bool
    is_list: bool
    is_annotation: bool

    def __init__(self, field: dict[str, Any]) -> None:
        self.name = field["name"]
        spec = field["type"]
        self.optional = spec.endswith("?")
        self.is_list = spec.endswith("*")
        self.ty = spec[:-1] if self.optional or self.is_list else spec
        if not self.ty.isidentifier():
            raise ValueError(f"Invalid type for field {self.name}: {spec}")
        self.is_annotation = field.get("is_annotation", False)


# ------------------------------------------------------------------------------
# Flat syntax storage


PARTS = {
    "StringValue": "StringLiteral",
    "BytesValue": "BytesLiteral",
    "FStringValue": "FStringPart",
    "TStringValue": "TString",
}
FLAGS = {"StringLiteralFlags", "BytesLiteralFlags", "FStringFlags", "TStringFlags"}
TAGS = {
    "ExprContext",
    "Operator",
    "CmpOp",
    "UnaryOp",
    "BoolOp",
    "Singleton",
    "IpyEscapeKind",
    "ConversionFlag",
}


@dataclass(frozen=True)
class WireField:
    name: str
    width: int
    result: str
    build_type: str
    encode: str
    read: str


def field_codec(field: Field, nodes: set[str]) -> WireField:
    name = field.name
    inner = field.ty
    value = f"self.{name}"

    def codec(
        width: int,
        result: str,
        encode: str,
        read: str,
        *,
        build_type: str | None = None,
    ) -> WireField:
        return WireField(name, width, result, build_type or result, encode, read)

    if inner == "MappingPatternItems":
        return codec(
            8,
            "crate::flat::MappingPatternItems<'a>",
            f"{value}.word()",
            "super::MappingPatternItems(List::new(self.syntax.ast, word))",
            build_type="super::super::MappingPatternItemsId",
        )
    if field.is_list or inner in PARTS:
        item = PARTS.get(inner, inner)
        element = (
            f"AstNodeId<crate::kind::{item}>" if item in nodes else f"crate::{item}"
        )
        read = "List::new(self.syntax.ast, word)"
        if inner == "StringValue":
            result = "crate::flat::StringValue<'a>"
            read = (
                "crate::flat::StringValue::new(List::new(self.syntax.ast, word), "
                "u64::from(self.syntax.row.expr_string_literal_text_offset()) | "
                "(u64::from(self.syntax.row.expr_string_literal_text_len()) << 32))"
            )
        elif inner in PARTS:
            wrapper = {
                "FStringValue": "FStringValue",
                "TStringValue": "TStringValue",
                "BytesValue": "BytesValue",
            }[inner]
            result = f"crate::flat::{wrapper}<'a>"
            read = f"crate::flat::{wrapper}::new({read})"
        else:
            result = (
                f"NodeList<'a, crate::kind::{item}>"
                if item in nodes
                else f"List<'a, {element}>"
            )
        return codec(
            8,
            result,
            f"{value}.word()",
            read,
            build_type=f"super::super::ListId<{element}>",
        )
    if inner in nodes:
        view = f"crate::{inner}<'a>"
        handle = f"BuildId<crate::kind::{inner}>"
        if field.optional:
            return codec(
                4,
                f"Option<{view}>",
                f"{value}.map_or(0, |id| u64::from(id.slot))",
                "self.syntax.ast.optional_child(word)",
                build_type=f"Option<{handle}>",
            )
        return codec(
            4,
            view,
            f"u64::from({value}.slot)",
            "self.syntax.ast.child(word)",
            build_type=handle,
        )
    if inner == "Name":
        return codec(
            4,
            "&'a crate::name::Name",
            f"u64::from({value}.0)",
            "&self.syntax.ast.names[split_words(word)[0] as usize]",
            build_type="super::super::NameId",
        )
    if inner == "str":
        return codec(
            8,
            "&'a str",
            f"builder.store_string(&{value})",
            "self.syntax.ast.string(word)",
            build_type="Box<str>",
        )
    if inner == "LiteralText":
        return codec(
            8,
            "&'a str",
            f"builder.store_string({value})",
            "self.syntax.ast.string(word)",
        )
    if inner == "LiteralBytes":
        return codec(
            8,
            "&'a [u8]",
            f"builder.store_bytes({value})",
            "self.syntax.ast.bytes(word)",
        )
    if inner in TAGS:
        return codec(1, f"crate::{inner}", f"{value}.encode()", "Word::decode(word)")
    if inner in FLAGS:
        return codec(
            1,
            f"crate::{inner}",
            f"u64::from({value}.bits())",
            f"crate::{inner}::from_bits(word.to_le_bytes()[0])",
        )
    if inner == "bool":
        return codec(1, "bool", f"u64::from({value})", "word != 0")
    if inner == "u32":
        return codec(4, "u32", f"u64::from({value})", "split_words(word)[0]")
    if inner == "Number":
        return codec(
            4,
            "&'a crate::Number",
            f"{{ let slot = offset(builder.ast.numbers.len()); builder.ast.numbers.push({value}); u64::from(slot) }}",
            "&self.syntax.ast.numbers[split_words(word)[0] as usize]",
            build_type="crate::Number",
        )
    if inner == "DebugText":
        assert field.optional
        return codec(
            4,
            "Option<&'a crate::DebugText>",
            f"{value}.map_or(u64::from(ABSENT), |text| {{ let slot = offset(builder.ast.debug_text.len()); builder.ast.debug_text.push(text); u64::from(slot) }})",
            "(split_words(word)[0] != ABSENT).then(|| &self.syntax.ast.debug_text[split_words(word)[0] as usize])",
            build_type="Option<crate::DebugText>",
        )
    raise ValueError(f"Missing flat field codec for {field.name}: {field.ty}")


@dataclass(frozen=True)
class InlineField:
    name: str
    offset: int
    width: int


# These derived values are read without following the node's children.
CACHED_FIELDS = {
    "ExprCompare": [InlineField("single_op", 12, 1)],
    "ExprStringLiteral": [
        InlineField("text_offset", 8, 4),
        InlineField("text_len", 12, 2),
    ],
    "StmtFunctionDef": [InlineField("body_row_count", 12, 2)],
}


def layout(
    fields: list[WireField], node: str
) -> tuple[dict[str, int], dict[str, int], int | None]:
    """Place naturally aligned fields in fifteen bytes, spilling fields into extra if needed."""
    reserved = {
        i
        for field in CACHED_FIELDS.get(node, [])
        for i in range(field.offset, field.offset + field.width)
    }

    def place(inline: list[WireField], extra: int | None) -> dict[str, int] | None:
        used = reserved.copy()
        if extra is not None:
            span = range(extra, extra + 4)
            if not used.isdisjoint(span):
                return None
            used.update(span)
        positions = {}
        for field in sorted(inline, key=lambda field: -field.width):
            for start in range(0, 16 - field.width, field.width):
                span = range(start, start + field.width)
                if used.isdisjoint(span):
                    positions[field.name] = start
                    used.update(span)
                    break
            else:
                return None
        return positions

    def with_extra(inline: list[WireField]) -> tuple[dict[str, int], int] | None:
        # Moving the spill offset can leave an aligned eight-byte slot for a list.
        for extra in (0, 4, 8):
            if (positions := place(inline, extra)) is not None:
                return positions, extra
        return None

    if (complete := place(fields, None)) is not None:
        return complete, {}, None
    kept: list[WireField] = []
    for field in sorted(fields, key=lambda field: field.width):
        if with_extra([*kept, field]) is not None:
            kept.append(field)
    placed = with_extra(kept)
    assert placed is not None
    offsets, extra = placed
    spills = {}
    words = 0
    for f in fields:
        if f not in kept:
            spills[f.name] = words
            words += (f.width + 3) // 4
    return offsets, spills, extra


def source_order_fields(node: Node) -> list[Field]:
    fields = node.fields
    if node.source_order is None:
        return fields
    positions = {name: index for index, name in enumerate(node.source_order)}
    return sorted(fields, key=lambda field: positions.get(field.name, len(fields)))


def child_fields(node: Node, nodes: set[str]) -> list[Field]:
    return [
        field
        for field in source_order_fields(node)
        if (field.ty in nodes or field.ty in PARTS or field.ty == "MappingPatternItems")
    ]


def child_sources(
    node: Node, fields: list[Field], words: dict[str, str], view: str
) -> list[tuple[str, str]]:
    """The source-ordered fields, each containing a handle or a list."""
    if not fields:
        return []
    if node.name in {"Arguments", "PatternArguments", "Parameters"}:
        return [("list", f"{view}.ordered_items().words()")]
    result = []
    for field in fields:
        raw = words[field.name].replace("self.syntax.ast.extra_word", "self.extra_word")
        if (node.name, field.name) == ("InterpolatedElement", "format_spec"):
            # The source-order visitor exposes the elements of the auxiliary spec.
            result.append(
                (
                    "list",
                    f"{view}.format_spec().map_or(&[][..], |spec| spec.elements().words())",
                )
            )
        elif field.is_list or field.ty in PARTS or field.ty == "MappingPatternItems":
            result.append(
                (
                    "list",
                    f"List::<AstNodeId<crate::kind::Any>>::new(self, {raw}).words()",
                )
            )
        elif field.optional:
            result.append(("optional", f"split_words({raw})[0]"))
        else:
            result.append(("node", f"split_words({raw})[0]"))
    return result


def field_value(kind: str, value: str) -> str:
    if kind in {"node", "optional"}:
        return f"Field::One({child_handle(kind, value)})"
    assert kind == "list"
    return f"Field::Rows({value})"


def child_handle(kind: str, value: str) -> str:
    if kind == "node":
        return f"Some(AstNodeId::from_word({value}))"
    assert kind == "optional"
    return f"{{ let word = {value}; (word != 0).then(|| AstNodeId::from_word(word)) }}"


def simple_children(sources: list[tuple[str, str]]) -> str | None:
    kinds = tuple("node" if kind == "optional" else kind for kind, _ in sources)
    if not sources:
        return "Children::empty(self)"
    if kinds == ("node",):
        return f"Children::one(self, {child_handle(*sources[0])})"
    if kinds == ("node", "node"):
        return f"Children::two(self, {child_handle(*sources[0])}, {child_handle(*sources[1])})"
    if kinds == ("list",):
        return f"Children::rows(self, {sources[0][1]})"
    if kinds in (("node", "list"), ("list", "node"), ("node", "list", "node")):
        first = child_handle(*sources[0]) if kinds[0] == "node" else "None"
        last = child_handle(*sources[-1]) if kinds[-1] == "node" else "None"
        return f"Children::sequence(self, {first}, {sources[kinds.index('list')][1]}, {last})"
    return None


def child_field(
    node: Node, nodes: set[str], words: dict[str, str], _schema: list[InlineField]
) -> str:
    sources = child_sources(
        node, child_fields(node, nodes), words, f"node.retype::<crate::{node.name}>()"
    )
    if simple_children(sources) is not None:
        return "Field::One(None)"
    arms = [f"{i} => {field_value(*source)}," for i, source in enumerate(sources)]
    return "match field {" + "".join(arms) + "_ => Field::One(None) }"


def direct_children(
    node: Node, nodes: set[str], words: dict[str, str], _schema: list[InlineField]
) -> str:
    view = f"node.retype::<crate::{node.name}>()"
    sources = child_sources(node, child_fields(node, nodes), words, view)
    return (
        simple_children(sources)
        or f"Children::fields(node, {len(sources)})"
    )


def next_descendant(
    node: Node, nodes: set[str], words: dict[str, str], _schema: list[InlineField]
) -> str:
    sources = child_sources(
        node, child_fields(node, nodes), words, f"node.retype::<crate::{node.name}>()"
    )
    resume = "stack.resume()"
    steps = []
    for i in reversed(range(len(sources))):
        kind, value = sources[i]
        if kind == "node":
            if i == 0:
                return "".join(steps) + value
            steps.append(f"stack.push_node({value});")
        elif kind == "list" and i == 0:
            return "".join(steps) + f"stack.descend_list({value})"
        elif kind == "optional":
            if i == 0:
                return (
                    "".join(steps)
                    + f"let word = {value}; if word == 0 {{ {resume} }} else {{ word }}"
                )
            steps.append(
                f"let word = {value}; if word != 0 {{ stack.push_node(word); }}"
            )
        else:
            assert kind == "list"
            steps.append(f"stack.push_list({value});")
    return "".join(steps) + resume


def statement_subtrees(ast: Ast, statements: set[str]) -> set[str]:
    """Node and group types whose subtrees can contain a statement."""
    types = statements | {"Stmt"}
    while True:
        containers = {
            node.name
            for node in ast.all_nodes
            if any(field.ty in types for field in node.fields)
        }
        for group in ast.groups:
            if any(
                node.name in types or node.name in containers for node in group.nodes
            ):
                containers.add(group.name)
        if containers <= types:
            return types
        types.update(containers)


def walk_fields(node: Node, nodes: set[str]) -> str:
    """Visit the fields of a projected node in Python source order."""
    if node.name == "Arguments":
        return """
        if node.keywords().is_empty() {
            for arg in node.args() { visitor.visit_expr(arg); }
        } else if node.args().is_empty() {
            for keyword in node.keywords() { visitor.visit_keyword(keyword); }
        } else {
            for argument in node.iter_source_order() {
                match argument {
                    super::ArgOrKeyword::Arg(expr) => visitor.visit_expr(expr),
                    super::ArgOrKeyword::Keyword(keyword) => visitor.visit_keyword(keyword),
                }
            }
        }
        """
    if node.name in {"PatternArguments", "PatternMatchMapping"}:
        return "for child in node.children() { child.dispatch(visitor); }"
    if node.name == "Parameters":
        return """
        if node.has_only_regular_args() {
            for parameter in node.ordered_items() {
                let parameter = parameter.retype::<crate::ParameterWithDefault>();
                visitor.visit_parameter_with_default(parameter);
            }
        } else {
            for parameter in node.iter_source_order() {
                match parameter {
                    super::AnyParameter::NonVariadic(node) => { visitor.visit_parameter_with_default(node); }
                    super::AnyParameter::Variadic(node) => visitor.visit_parameter(node),
                }
            }
        }
        """
    if node.name == "ExprBoolOp":
        return """let mut values = node.values().iter();
        if let Some(left) = values.next() { visitor.visit_expr(left); }
        visitor.visit_bool_op(node.op());
        for value in values { visitor.visit_expr(value); }"""
    if node.name == "ExprCompare":
        return """visitor.visit_expr(node.first_operand());
        for (_, op, right) in node.iter() { visitor.visit_cmp_op(op); visitor.visit_expr(right); }"""
    statements = []
    for field in source_order_fields(node):
        inner = field.ty
        get = f"node.{field.name}()"
        method = f"visit_{to_snake_case(PARTS.get(inner, inner))}"
        if field.is_annotation:
            method = "visit_annotation"
        if (node.name, field.name) == ("InterpolatedElement", "format_spec"):
            statements.append(
                f"if let Some(spec) = {get} {{ spec.syntax_node().visit_source_order(visitor); }}"
            )
        elif inner == "FStringValue":
            statements.append(
                f"for part in {get} {{ match part {{ super::FStringPartView::Literal(v) => visitor.visit_string_literal(v), super::FStringPartView::FString(v) => visitor.visit_f_string(v) }} }}"
            )
        elif inner == "Stmt" and field.is_list:
            statements.append(f"visitor.visit_body({get});")
        elif inner in PARTS or (inner in nodes and field.is_list):
            statements.append(f"for item in {get} {{ visitor.{method}(item); }}")
        elif inner in nodes and field.optional:
            statements.append(
                f"if let Some(item) = {get} {{ visitor.{method}(item); }}"
            )
        elif inner in nodes or inner in {
            "Singleton",
            "Operator",
            "BoolOp",
            "UnaryOp",
            "CmpOp",
        }:
            statements.append(f"visitor.{method}({get});")
    return "".join(statements)


def copy_field(field: Field, nodes: set[str]) -> str:
    """Build a field from a view into a different arena."""
    inner = field.ty
    value = f"node.{field.name}()"
    if inner == "MappingPatternItems":
        return f"{value}.copy_to(self, rewriter)"
    if field.is_list or inner in PARTS:
        item = PARTS.get(inner, inner)
        if item in nodes:
            return f"self.map_list({value}, |b, child| b.copy_with(child, rewriter))"
        if item == "FStringPart":
            return f"""self.map_list({value}, |b, part| match part {{
                super::FStringPartView::Literal(v) => super::FStringPartId::Literal(b.copy_with(v, rewriter)),
                super::FStringPartView::FString(v) => super::FStringPartId::FString(b.copy_with(v, rewriter)),
            }})"""
        assert item == "CmpOp", item
        return f"self.list({value})"
    if inner in nodes:
        return (
            f"{value}.map(|child| self.copy_with(child, rewriter))"
            if field.optional
            else f"self.copy_with({value}, rewriter)"
        )
    if inner == "Name":
        return f"self.intern_name({value}.as_str())"
    if inner == "Number":
        return f"{value}.clone()"
    if inner == "DebugText":
        return f"{value}.cloned()"
    if inner == "str":
        return f"{value}.into()"
    return value


def replacement_hook(ty: str) -> str:
    method = {
        "Stmt": "replace_stmt",
        "Expr": "replace_expr",
        "StringLiteral": "replace_string_literal",
        "InterpolatedStringElement": "replace_interpolated_string_element",
    }.get(ty)
    if method is None:
        return ""
    return f"""fn replacement<R: super::Rewriter + ?Sized>(
        node: Self::Borrowed<'_>, builder: &mut Builder, rewriter: &mut R,
    ) -> Option<BuildId<Self>> {{ rewriter.{method}(builder, node) }}
"""


def generate_flat_patterns(ast: Ast, nodes: set[str]) -> list[str]:
    """Generate field projections for destructuring a node in a match."""
    groups = ", ".join(f"{group.name}Kind" for group in ast.groups)
    out = [
        f"pub(crate) mod fields {{ use super::{{AstNode, SyntaxNode, List, NodeList, Ranged, NodeKind, AnyKind, {groups}}};"
    ]
    for node in ast.all_nodes:
        ty = node.name
        fields = [field_codec(field, nodes) for field in node.fields]
        out.append(f"#[derive(Clone, Copy)] pub struct {ty}Fields<'a> {{")
        out.append(f"pub node: crate::{ty}<'a>, pub range: ruff_text_size::TextRange,")
        for field in fields:
            out.append(f"pub {field.name}: {field.result},")
        out.append("}")
        out.append(
            f"impl<'a> crate::{ty}<'a> {{ #[inline] pub fn fields(self) -> {ty}Fields<'a> {{"
        )
        out.append(f"{ty}Fields {{ node: self, range: self.range(),")
        for field in fields:
            out.append(f"{field.name}: self.{field.name}(),")
        out.append("} } }")
    for group in ast.groups:
        ty = group.name
        out.append(f"#[derive(Clone, Copy)] pub enum {ty}Fields<'a> {{")
        for node in group.nodes:
            out.append(f"{node.variant}({node.name}Fields<'a>),")
        out.append("}")
        out.append(
            f"impl<'a> {ty}Fields<'a> {{ pub fn view(self) -> crate::{ty}<'a> {{ match self {{"
        )
        for node in group.nodes:
            out.append(f"Self::{node.variant}(fields) => fields.node.into(),")
        out.append("} } }")
        out.append(
            f"impl<'a> crate::{ty}<'a> {{ #[inline] pub fn fields(self) -> {ty}Fields<'a> {{ match self.classify() {{"
        )
        for node in group.nodes:
            out.append(
                f"{ty}Kind::{node.variant}(node) => {ty}Fields::{node.variant}(node.fields()),"
            )
        out.append("} } }")
        out.append(f"impl<'a> crate::{ty}<'a> {{")
        for node in group.nodes:
            method = to_snake_case(
                node.variant + (group.name if group.add_suffix_to_is_methods else "")
            )
            out.append(
                f"#[inline] pub fn is_{method}(self) -> bool {{ self.node_kind() == NodeKind::{node.name} }}"
            )
            out.append(
                f"#[inline] pub fn as_{method}(self) -> Option<crate::{node.name}<'a>> {{ self.cast() }}"
            )
        out.append("}")
        out.append(
            f"impl<'a> {ty}Kind<'a> {{ #[inline] pub fn view(self) -> crate::{ty}<'a> {{ match self {{"
        )
        for node in group.nodes:
            out.append(f"Self::{node.variant}(node) => node.into(),")
        out.append("} } }")
        out.append(
            f"impl ruff_text_size::Ranged for {ty}Kind<'_> {{ fn range(&self) -> ruff_text_size::TextRange {{ self.view().range() }} }}"
        )
        out.append(
            f"impl crate::HasNodeIndex for {ty}Kind<'_> {{ fn node_index(&self) -> crate::NodeIndex {{ self.view().node_index() }} }}"
        )
        for node in group.nodes:
            out.append(f"impl<'a> From<crate::{node.name}<'a>> for crate::{ty}<'a> {{")
            out.append(
                f"fn from(node: crate::{node.name}<'a>) -> Self {{ node.syntax_node().retype() }} }}"
            )
    out.append("#[derive(Clone, Copy)] pub enum AnyFields<'a> {")
    for node in ast.all_nodes:
        out.append(f"{node.name}({node.name}Fields<'a>),")
    out.append("}")
    out.append(
        "impl<'a> SyntaxNode<'a> { #[inline] pub fn fields(self) -> AnyFields<'a> { match self.classify() {"
    )
    for node in ast.all_nodes:
        out.append(
            f"AnyKind::{node.name}(node) => AnyFields::{node.name}(node.fields()),"
        )
    out.append("} } }")
    out.append("}")
    return out


def generate_flat_any(ast: Ast) -> list[str]:
    """Classify an untyped view without decoding its payload."""
    out = ["#[derive(Clone, Copy, Debug)] pub enum AnyKind<'a> {"]
    for node in ast.all_nodes:
        out.append(f"{node.name}(crate::{node.name}<'a>),")
    out.append("}")
    out.append(
        "impl<'a> SyntaxNode<'a> { #[inline] pub fn classify(self) -> AnyKind<'a> { match self.node_kind() {"
    )
    for node in ast.all_nodes:
        out.append(f"NodeKind::{node.name} => AnyKind::{node.name}(self.retype()),")
    out.append("} } }")
    out.append(
        "impl SyntaxNode<'_> { pub(super) fn fmt_fields(self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { match self.kind() {"
    )
    for node in ast.all_nodes:
        out.append(
            f"NodeKind::{node.name} => std::fmt::Debug::fmt(&self.retype::<crate::{node.name}>(), f),"
        )
    out.append("} } }")
    out.append("impl<'a> SyntaxNode<'a> {")
    for node in ast.all_nodes:
        out.append(
            f"pub fn as_{to_snake_case(node.name)}(self) -> Option<crate::{node.name}<'a>> {{ self.cast() }}"
        )
        out.append(
            f"pub fn is_{to_snake_case(node.name)}(self) -> bool {{ self.node_kind() == NodeKind::{node.name} }}"
        )
    out.append("}")
    out.append(
        "impl<'a> AnyKind<'a> { #[inline] pub fn view(self) -> SyntaxNode<'a> { match self {"
    )
    for node in ast.all_nodes:
        out.append(f"Self::{node.name}(node) => node.syntax_node(),")
    out.append("} } }")
    out.append(
        "pub(super) fn same_fields(left: SyntaxNode<'_>, right: SyntaxNode<'_>) -> bool { match left.classify() {"
    )
    for node in ast.all_nodes:
        fields = node.fields
        if fields:
            comparisons = " && ".join(
                f"left.{f.name}() == right.{f.name}()" for f in fields
            )
            out.append(
                f"AnyKind::{node.name}(left) => {{ let right = right.retype::<crate::{node.name}>(); {comparisons} }},"
            )
        else:
            out.append(f"AnyKind::{node.name}(_) => true,")
    out.append("} }")
    for ty in [n.name for n in ast.all_nodes] + [g.name for g in ast.groups]:
        out.append(
            f"impl<'a> From<crate::{ty}<'a>> for SyntaxNode<'a> {{ fn from(node: crate::{ty}<'a>) -> Self {{ node.syntax_node() }} }}"
        )
    for group in ast.groups:
        ty = group.name
        out.append(
            f"impl<'a> From<{ty}Kind<'a>> for SyntaxNode<'a> {{ fn from(node: {ty}Kind<'a>) -> Self {{ node.view().syntax_node() }} }}"
        )
    return out


def generate_row_fields(schemas: dict[str, list[InlineField]]) -> str:
    out = [
        "// Generated by crates/ruff_python_ast/generate.py from ast.toml.",
        "node_payloads! {",
    ]
    for node, fields in schemas.items():
        if not fields:
            continue
        arm = to_snake_case(node)
        out.append(f"    {node}({arm}) {{")
        end = 0
        for field in sorted(fields, key=lambda field: field.offset):
            assert end <= field.offset and field.offset + field.width <= 15
            padding = (
                f"[_pad_{end}; {field.offset - end}] " if end < field.offset else ""
            )
            getter = f"{arm}_{field.name}"
            out.append(
                f"        {padding}{field.name}: u{8 * field.width} at {field.offset} => {getter}, set_{getter};"
            )
            end = field.offset + field.width
        out.append("    }")
    return "\n".join([*out, "}", ""])


def generate_flat(ast: Ast) -> tuple[str, str]:
    groups = {group.name for group in ast.groups}
    nodes = groups | {node.name for node in ast.all_nodes}
    schemas: dict[str, list[InlineField]] = {}
    words_by_node: dict[str, dict[str, str]] = {}
    out = [
        "// Generated by crates/ruff_python_ast/generate.py from ast.toml.",
        "use ruff_text_size::Ranged;",
        "use super::{ABSENT, AstNode, AstNodeId, BuildId, Builder, Emit, List, NodeType, NodeList, Row, SyntaxNode, offset, split_words};",
        "use super::scalar::Word;",
        "use crate::NodeKind;",
    ]
    initializers = [
        "pub mod build { use super::{BuildId, AstNodeId, Builder, ABSENT, Word, offset}; use super::super::{Emit, EmittedNode};"
    ]
    for group in ast.groups:
        name = group.name
        kinds = " | ".join(f"NodeKind::{n.name}" for n in group.nodes)
        debug_arms = " ".join(
            f'{name}Kind::{n.variant}(v) => f.debug_tuple("{n.variant}").field(&v).finish(),'
            for n in group.nodes
        )
        out.append(f"""impl NodeType for crate::kind::{name} {{
            type Borrowed<'ast> = crate::{name}<'ast>;
            {replacement_hook(name)}
            fn matches(kind: NodeKind) -> bool {{ matches!(kind, {kinds}) }}
        }}
        impl std::fmt::Debug for crate::{name}<'_> {{
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {{
                match self.classify() {{ {debug_arms} }}
            }}
        }}""")
        for n in group.nodes:
            for id_type in ("AstNodeId", "BuildId"):
                initializers.append(
                    f"impl From<{id_type}<crate::kind::{n.name}>> for {id_type}<crate::kind::{name}> {{ "
                    f"fn from(value: {id_type}<crate::kind::{n.name}>) -> Self {{ value.cast() }} }}"
                )
        # Classifying a view can reuse the tag from the arena row.
        out.append(
            f"#[repr(u8)] #[derive(Debug, Copy, Clone)] pub enum {name}Kind<'a> {{"
        )
        for n in group.nodes:
            out.append(f"{n.variant}(crate::{n.name}<'a>) = NodeKind::{n.name} as u8,")
        out.append("}")
        out.append(
            f"impl<'a> crate::{name}<'a> {{ #[inline] pub fn classify(self) -> {name}Kind<'a> {{ match self.node_kind() {{"
        )
        for n in group.nodes:
            out.append(
                f"NodeKind::{n.name} => {name}Kind::{n.variant}(self.syntax.retype()),"
            )
        out.append('_ => unreachable!("typed node belongs to this group"), } } }')
    for n in ast.all_nodes:
        fields = [field_codec(f, nodes) for f in n.fields]
        paired_lists = n.name == "ExprCompare"
        if paired_lists:
            first, second = fields
            fields = [replace(first, width=4), second]
        offsets, spills, extra_offset = layout(fields, n.name)
        arm = to_snake_case(n.name)
        schemas[n.name] = [
            *(
                InlineField(f.name, offsets[f.name], f.width)
                for f in fields
                if f.name in offsets
            ),
            *CACHED_FIELDS.get(n.name, []),
        ]
        if extra_offset is not None:
            schemas[n.name].append(InlineField("extra_offset", extra_offset, 4))
        words_by_node[n.name] = {}
        debug_fields = ["node_index", "range"]
        if n.debug_range_first:
            debug_fields.reverse()
        if n.name == "Identifier":
            debug_fields.insert(0, "id")
        else:
            debug_fields.extend(field.name for field in fields)
        debug_fields = "".join(
            f'.field("{name}", &v.{name}())' for name in debug_fields
        )
        out.append(f"""impl NodeType for crate::kind::{n.name} {{
             type Borrowed<'ast> = crate::{n.name}<'ast>;
             const KIND: Option<NodeKind> = Some(NodeKind::{n.name});
             {replacement_hook(n.name)}
             #[inline]
             fn matches(kind: NodeKind) -> bool {{ kind == NodeKind::{n.name} }}
        }}
        impl std::fmt::Debug for crate::{n.name}<'_> {{
             fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {{
                 let v = *self;
                 f.debug_struct("{n.name}"){debug_fields}.finish()
             }}
        }}""")
        method = f"push_{to_snake_case(n.name)}"
        parameters = ", ".join(f.name for f in fields)
        out.append(
            f"impl Builder {{ #[inline] fn {method}(&mut self, range: ruff_text_size::TextRange, [{parameters}]: [u64; {len(fields)}]) -> BuildId<crate::kind::{n.name}> {{"
        )
        default = ", Default" if n.name == "Parameters" else ""
        borrows_literal = any(
            field.ty in {"LiteralText", "LiteralBytes"} for field in n.fields
        )
        lifetime = "<'a>" if borrows_literal else ""
        constructor = n.name + ("<'_>" if borrows_literal else "")
        initializers.append(
            f"#[derive(Debug{default})] pub struct {n.name}{lifetime} {{pub range: ruff_text_size::TextRange,"
        )
        for field in fields:
            initializers.append(f"pub {field.name}: {field.build_type},")
        initializers.append(
            f"}} impl Emit for {constructor} {{ type Output = crate::kind::{n.name}; #[inline] fn emit(self, builder: &mut Builder) -> EmittedNode<Self::Output> {{"
        )
        initializers.extend(f"let {field.name} = {field.encode};" for field in fields)
        initializers.append(
            f"(builder.{method}(self.range, [{parameters}]), self.range, crate::NodeKind::{n.name}) }} }}"
        )
        if n.name == "Parameters":
            schemas[n.name] = [InlineField("items", 0, 8), InlineField("layout", 8, 4)]
            out.append(f"self.parameters_from_groups(range, [{parameters}]) }} }}")
            out.append("impl<'a> crate::Parameters<'a> {")
            out.append(
                "#[inline] pub(super) fn ordered_items(self) -> NodeList<'a, crate::kind::Any> { List::new(self.syntax.ast, self.syntax.row.parameters_items()) }"
            )
            for f, read in zip(
                fields,
                [
                    "parameter_group::<0>",
                    "parameter_group::<1>",
                    "variadic_parameter::<false>",
                    "parameter_group::<2>",
                    "variadic_parameter::<true>",
                ],
            ):
                out.append(
                    f"#[inline] pub fn {f.name}(self) -> {f.result} {{ let word = self.{read}(); {f.read} }}"
                )
            out.append("}")
            continue
        if n.name in {"Arguments", "PatternArguments"}:
            schemas[n.name] = [
                InlineField("keyword_group", 0, 8),
                InlineField("positional_len", 8, 4),
                InlineField("interleaved", 12, 1),
            ]
            first, second = fields
            out.append(f"""
                let groups = self.ordered_argument_lists({parameters});
                self.{method}_parts(range, groups)
                }}
                #[inline] fn {method}_parts(&mut self, range: ruff_text_size::TextRange, (keywords, positional, interleaved): (u64, u32, bool)) -> BuildId<crate::kind::{n.name}> {{
                let mut row = Row::new(NodeKind::{n.name}, range);
                row.set_{arm}_keyword_group(keywords);
                row.set_{arm}_positional_len(positional);
                row.set_{arm}_interleaved(u8::from(interleaved));
                self.push(row) }} }}
                impl<'a> crate::{n.name}<'a> {{
                #[inline] pub(super) fn ordered_items(self) -> NodeList<'a, crate::kind::Any> {{
                    List::new(self.syntax.ast, super::ordered::argument_items(self.syntax.row.{arm}_keyword_group(), self.syntax.row.{arm}_positional_len(), self.syntax.row.{arm}_interleaved()))
                }}""")
            for f, raw in [
                (
                    first,
                    f"super::list::Interval::preceding(self.syntax.row.{arm}_keyword_group(), u64::from(self.syntax.row.{arm}_positional_len()))",
                ),
                (second, f"self.syntax.row.{arm}_keyword_group()"),
            ]:
                out.append(f"""#[inline] pub fn {f.name}(self) -> {f.result} {{
                    let word = {raw};
                    {f.read}
                }}""")
            out.append("}")
            continue
        mutable = "mut " if fields else ""
        out.append(f"let {mutable}row = Row::new(NodeKind::{n.name}, range);")
        if n.name == "ExprCompare":
            # Zero marks a chain or an incomplete comparison.
            out.append(
                "let [start, len] = split_words(ops);"
                "if len == 1 && split_words(operands)[1] == 2 {"
                "row.set_expr_compare_single_op(self.ast.extra[start as usize].to_le_bytes()[0] + 1); }"
            )
        if paired_lists:
            out.append(
                f"let [{parameters}] = super::list::Interval::pack_pair(&mut self.ast.extra, [{parameters}]);"
            )
        for f in fields:
            if f.name in offsets:
                value = {
                    1: f"{f.name}.to_le_bytes()[0]",
                    4: f"split_words({f.name})[0]",
                    8: f.name,
                }[f.width]
                out.append(f"row.set_{arm}_{f.name}({value});")
        if n.name == "ExprStringLiteral":
            out.append(
                "let (start, len) = self.string_literal_text(value);"
                "row.set_expr_string_literal_text_offset(start); row.set_expr_string_literal_text_len(len);"
            )
        if spills:
            out.append(f"row.set_{arm}_extra_offset(offset(self.ast.extra.len()));")
            words = []
            for field in fields:
                if field.name in spills:
                    words.append(f"split_words({field.name})[0]")
                    if field.width == 8:
                        words.append(f"split_words({field.name})[1]")
            out.append("self.ast.extra.extend([" + ", ".join(words) + "]);")
        if n.name in {"ExprYield", "ExprYieldFrom"}:
            out.append(
                "let node = self.push(row);"
                "self.ast.yield_rows_end = node.slot + 1; node } }"
            )
        else:
            out.append("self.push(row) } }")
        if fields:
            generics, lifetime = (
                ("<'a>", "'a") if any("'a" in f.result for f in fields) else ("", "'_")
            )
            out.append(f"impl{generics} crate::{n.name}<{lifetime}> {{")
            if n.name == "ExprCompare":
                out.append(
                    """
                    /// Returns the operands and operator of a single comparison.
                    #[inline]
                    pub fn as_single(self) -> Option<(crate::Expr<'a>, crate::CmpOp, crate::Expr<'a>)> {
                        let operator = self.syntax.row.expr_compare_single_op().checked_sub(1)?;
                        let start = split_words(self.syntax.row.expr_compare_operands())[0] as usize;
                        let &[left, right] = self.syntax.ast.extra.get(start..)?.first_chunk()?;
                        Some((
                            self.syntax.ast.child(u64::from(left)),
                            crate::CmpOp::decode(u64::from(operator)),
                            self.syntax.ast.child(u64::from(right)),
                        ))
                    }
                    """
                )
            for f in fields:
                if f.name in offsets:
                    raw = f"self.syntax.row.{arm}_{f.name}()"
                    if f.width < 8:
                        raw = f"u64::from({raw})"
                    shared = f"record.inline_word::<{offsets[f.name]}, {f.width}>()"
                    if paired_lists and f is fields[0]:
                        second = fields[1].name
                        raw = f"super::list::Interval::preceding(self.syntax.row.{arm}_{second}(), {raw})"
                        shared = f"super::list::Interval::preceding(record.inline_word::<{offsets[second]}, 4>(), {shared})"
                else:
                    base = f"self.syntax.row.{arm}_extra_offset() as usize"
                    shared_base = f"split_words(record.inline_word::<{extra_offset}, 4>())[0] as usize"
                    index = "base" if not spills[f.name] else f"base + {spills[f.name]}"
                    read = f"self.syntax.ast.extra_word::<{str(f.width == 8).lower()}>({index})"
                    raw = f"{{ let base = {base}; {read} }}"
                    shared = f"{{ let base = {shared_base}; {read} }}"
                if f.result == "&'a crate::name::Name":
                    out.append(
                        f"#[inline] pub fn name_id(self) -> super::NameId {{ super::NameId(split_words({raw})[0]) }}"
                    )
                words_by_node[n.name][f.name] = shared
                if (n.name, f.name) == ("StringLiteral", "value"):
                    out.append(
                        f"pub(super) fn text_range(self) -> super::Interval {{ super::Interval::decode({raw}) }}"
                    )
                if f.name == "iter":
                    out.append(
                        '#[expect(clippy::iter_not_returning_iterator, reason = "Python names the iterable expression in a for loop or comprehension `iter`.")]'
                    )
                out.append(f"""#[inline] pub fn {f.name}(self) -> {f.result} {{
                    let word = {raw};
                    {f.read}
                }}""")
            out.append("}")
    row_code = [generate_row_fields(schemas), "impl super::Ast {"]
    statements = {
        node.name
        for group in ast.groups
        if group.name == "Stmt"
        for node in group.nodes
    }
    statement_types = statement_subtrees(ast, statements)

    def enter_statement_node(
        node: Node, _nodes: set[str], words: dict[str, str], _schema: list[InlineField]
    ) -> str:
        fields = [
            field for field in source_order_fields(node) if field.ty in statement_types
        ]
        sources = child_sources(
            node, fields, words, f"node.retype::<crate::{node.name}>()"
        )
        assert all(kind == "list" for kind, _ in sources), (
            "Statement-bearing fields must be lists"
        )
        push = "".join(f"stack.push_list({value});" for _, value in reversed(sources))
        result = "Some(node.retype())" if node.name in statements else "None"
        return push + result

    for generate_step, signature in [
        (
            direct_children,
            "fn direct_children<'a>(&'a self, node: SyntaxNode<'a>) -> Children<'a>",
        ),
        (
            child_field,
            "fn child_field<'a>(&'a self, node: SyntaxNode<'a>, field: u8) -> Field<'a>",
        ),
        (
            next_descendant,
            "fn next_descendant<'a>(&'a self, node: SyntaxNode<'a>, stack: &mut Pending) -> u32",
        ),
        (
            enter_statement_node,
            "fn enter_statement_node<'a>(&'a self, node: SyntaxNode<'a>, stack: &mut StatementLists<'a>) -> Option<crate::Stmt<'a>>",
        ),
    ]:
        steps: dict[str, list[str]] = {}
        for node in ast.all_nodes:
            step = generate_step(
                node, nodes, words_by_node[node.name], schemas[node.name]
            )
            steps.setdefault(step, []).append(f"NodeKind::{node.name}")
        if generate_step == child_field:
            del steps["Field::One(None)"]
        elif generate_step == enter_statement_node:
            del steps["None"]
        if generate_step != child_field:
            row_code.append("#[inline]")
        row_code.append(
            f"pub(super) {signature} {{ let record = node.row; match record.kind() {{"
        )
        for step, kinds in steps.items():
            row_code.append(f"{' | '.join(kinds)} => {{ {step} }}")
        if generate_step == child_field:
            row_code.append("_ => Field::One(None),")
        elif generate_step == enter_statement_node:
            row_code.append("_ => None,")
        row_code.append("} }")
    row_code.append("}")
    out.append(
        "impl Builder { pub(super) fn copy_any<R: super::Rewriter + ?Sized>(&mut self, node: super::SyntaxNode<'_>, rewriter: &mut R) -> BuildId<crate::kind::Any> { match node.classify() {"
    )
    for node in ast.all_nodes:
        if node.name in {"Arguments", "PatternArguments"}:
            arm = to_snake_case(node.name)
            first, second = node.fields
            keyword_kind = "Keyword" if node.name == "Arguments" else "PatternKeyword"
            out.append(f"""AnyKind::{node.name}(node) => {{
                let first = {copy_field(first, nodes)};
                let second = {copy_field(second, nodes)};
                let template = (node.syntax.row.{arm}_interleaved() != 0).then(|| node.ordered_items());
                let groups = self.copy_argument_order(first.word(), second.word(), template, NodeKind::{keyword_kind});
                self.push_{arm}_parts(node.range(), groups).cast()
            }},""")
            continue
        fields = [f"{field.name}: {copy_field(field, nodes)}," for field in node.fields]
        out.append(
            f"AnyKind::{node.name}(node) => build::{node.name} {{ range: node.range(), "
            + "".join(fields)
            + "}.emit(self).0.cast(),"
        )
    out.append("} } }")
    out.append(
        "impl<'a> SyntaxNode<'a> { pub fn visit_source_order(self, visitor: &mut (impl super::visitor::source_order::SourceOrderVisitor<'a> + ?Sized)) { match self.node_kind() {"
    )
    for node in ast.all_nodes:
        walk = walk_fields(node, nodes)
        out.append(f"NodeKind::{node.name} => {{")
        if walk:
            out.append(f"let node = self.retype::<crate::{node.name}>(); {walk}")
        out.append("}")
    out.append("} } }")
    out.append(
        "impl<'a> SyntaxNode<'a> { fn dispatch(self, visitor: &mut (impl super::visitor::source_order::SourceOrderVisitor<'a> + ?Sized)) { match self.node_kind() {"
    )
    membership = {node.name: group.name for group in ast.groups for node in group.nodes}
    for node in ast.all_nodes:
        ty = membership.get(node.name, node.name)
        if ty == "InterpolatedStringFormatSpec":
            out.append(f"NodeKind::{node.name} => self.visit_source_order(visitor),")
        else:
            out.append(
                f"NodeKind::{node.name} => {{ visitor.visit_{to_snake_case(ty)}(self.retype()); }}"
            )
    out.append("} } }")
    # Each string-like expression stores a list of one-word part handles.
    part_lists = {
        words_by_node[node.name][field.name]
        for node in ast.all_nodes
        for field in node.fields
        if field.ty in PARTS
    }
    assert len(part_lists) == 1, (
        "string-like expressions must share the parts list layout"
    )
    part_list = part_lists.pop()
    row_code.append(f"""impl<'a> super::StringLike<'a> {{
        #[inline] pub(super) fn part_nodes(self) -> NodeList<'a, crate::kind::Any> {{
            let node: SyntaxNode<'a> = self.into();
            let record = node.row;
            List::new(node.ast, {part_list})
        }}
    }}""")
    initializers.append("}")
    out.extend(initializers)
    out.extend(generate_flat_patterns(ast, nodes))
    out.extend(generate_flat_any(ast))
    return "\n".join(out), "\n".join(row_code)


# ------------------------------------------------------------------------------
# Format and write output


def generate(ast: Ast) -> list[str]:
    out = [
        "// Generated by crates/ruff_python_ast/generate.py from ast.toml.",
        "//! Borrowed Python syntax nodes and their lifetime-free ID markers.",
        "#[repr(u8)] #[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)] pub enum NodeKind {",
        *(f"{node.name}," for node in ast.all_nodes),
        "}",
    ]
    types = [*ast.groups, *ast.all_nodes]
    out.append(
        "/// Lifetime-free marker types used by AST IDs and the parser.\npub mod kind {"
    )
    for name in ["Any", *(node.name for node in types)]:
        out.append(
            f"#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)] pub enum {name} {{}}"
        )
    out.append("}")
    for node in types:
        if node.doc is not None:
            write_rustdoc(out, node.doc)
        out.append(
            f"#[repr(transparent)] #[derive(Copy, Clone, PartialEq)] pub struct {node.name}<'ast> {{ pub(crate) syntax: crate::SyntaxNode<'ast> }}"
        )
        out.append(f"crate::flat::impl_ast_node!({node.name});")
    return out


def write_output(root: Path, out: list[str]) -> None:
    out_path = root.joinpath("crates", "ruff_python_ast", "src", "generated.rs")
    out_path.write_text(rustfmt("\n".join(out)))


# ------------------------------------------------------------------------------
# Main


def main() -> None:
    root = Path(
        check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip()
    )
    ast = load_ast(root)
    out = generate(ast)
    write_output(root, out)
    flat = root / "crates" / "ruff_python_ast" / "src" / "flat" / "generated.rs"
    views, rows = generate_flat(ast)
    flat.write_text(rustfmt(views))
    flat.with_name("row").joinpath("generated.rs").write_text(rustfmt(rows))


if __name__ == "__main__":
    main()
