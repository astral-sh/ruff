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
from enum import Enum
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
    "BytesLiteralValue",
    "StringLiteralValue",
    "FStringValue",
    "TStringValue",
    "Arguments",
    "CmpOp",
    "Comprehension",
    "DictItem",
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
    owned_enum_ty: str

    add_suffix_to_is_methods: bool
    anynode_is_label: str
    doc: str | None

    def __init__(self, group_name: str, group: dict[str, Any]) -> None:
        self.name = group_name
        self.owned_enum_ty = group_name
        self.add_suffix_to_is_methods = group.get("add_suffix_to_is_methods", False)
        self.anynode_is_label = group.get("anynode_is_label", to_snake_case(group_name))
        self.doc = group.get("doc")
        self.nodes = [
            Node(self, node_name, node) for node_name, node in group["nodes"].items()
        ]


@dataclass
class Node:
    name: str
    variant: str
    ty: str
    doc: str | None
    fields: list[Field] | None
    flat_fields: list[Field]
    derives: list[str]
    custom_debug: bool
    custom_source_order: bool
    custom_range: bool
    source_order: list[str] | None

    def __init__(self, group: Group, node_name: str, node: dict[str, Any]) -> None:
        self.name = node_name
        self.variant = node.get("variant", node_name.removeprefix(group.name))
        self.ty = f"crate::{node_name}"
        self.fields = None
        fields = node.get("fields")
        if fields is not None:
            self.fields = [Field(f) for f in fields]
        self.flat_fields = (
            self.fields
            if self.fields is not None
            else [Field(f) for f in node["flat_fields"]]
        )
        self.custom_debug = node.get("custom_debug", False)
        self.custom_source_order = node.get("custom_source_order", False)
        self.custom_range = node.get("custom_range", False)
        self.derives = node.get("derives", [])
        self.doc = node.get("doc")
        self.source_order = node.get("source_order")

    def fields_in_source_order(self) -> list[Field]:
        if self.fields is None:
            return []
        if self.source_order is None:
            return list(filter(lambda x: not x.skip_source_order(), self.fields))

        fields = []
        for field_name in self.source_order:
            for field in self.fields:
                if field.skip_source_order():
                    continue
                if field.name == field_name:
                    fields.append(field)
                    break
            else:
                raise ValueError(
                    f"{self.name}: no source-order field named {field_name!r}"
                )
        return fields


@dataclass
class Field:
    name: str
    ty: str
    _skip_visit: bool
    is_annotation: bool
    parsed_ty: FieldType

    def __init__(self, field: dict[str, Any]) -> None:
        self.name = field["name"]
        self.ty = field["type"]
        self.parsed_ty = FieldType(self.ty)
        self._skip_visit = field.get("skip_visit", False)
        self.is_annotation = field.get("is_annotation", False)

    def skip_source_order(self) -> bool:
        return self._skip_visit or self.parsed_ty.inner in [
            "str",
            "ExprContext",
            "Name",
            "u32",
            "bool",
            "Number",
            "IpyEscapeKind",
        ]


# Extracts the type argument from a Rust type used in AST field syntax.
# Box<str> -> str
# Box<Expr> -> Expr
# If the type does not have a type argument, it will return the string.
# Does not support nested types
def extract_type_argument(rust_type_str: str) -> str:
    open_bracket_index = rust_type_str.find("<")
    if open_bracket_index == -1:
        return rust_type_str
    close_bracket_index = rust_type_str.rfind(">")
    if close_bracket_index == -1 or close_bracket_index <= open_bracket_index:
        raise ValueError(f"Brackets are not balanced for type {rust_type_str}")
    inner_type = rust_type_str[open_bracket_index + 1 : close_bracket_index].strip()
    inner_type = inner_type.replace("crate::", "")
    return inner_type


class SequenceKind(Enum):
    VEC = "vec"
    BOXED_SLICE = "boxed_slice"
    THIN_VEC = "thin_vec"


def split_sequence_type(rule: str) -> tuple[SequenceKind | None, str]:
    if "&" in rule:
        raise ValueError(f"`&T*` is unsupported; use `Box<[T]>`: {rule}")

    if "*" in rule:
        if rule.endswith("*") and rule.count("*") == 1:
            return SequenceKind.VEC, rule[:-1]
        raise ValueError(f"`*` must be at the end: {rule}")

    for prefix, suffix, sequence_kind in (
        ("Vec<", ">", SequenceKind.VEC),
        ("ThinVec<", ">", SequenceKind.THIN_VEC),
        ("Box<[", "]>", SequenceKind.BOXED_SLICE),
    ):
        if rule.startswith(prefix):
            if not rule.endswith(suffix):
                raise ValueError(f"Unclosed collection type: {rule}")
            return sequence_kind, rule[len(prefix) : -len(suffix)]

    return None, rule


@dataclass
class FieldType:
    rule: str
    name: str
    inner: str
    sequence_kind: SequenceKind | None = None
    optional: bool = False

    def __init__(self, rule: str) -> None:
        self.rule = rule
        self.optional = False
        if "?" in rule:
            if not rule.endswith("?") or rule.count("?") != 1:
                raise ValueError(f"`?` must be at the end: {rule}")
            self.optional = True
            rule = rule[:-1]

        self.sequence_kind, self.name = split_sequence_type(rule)
        if self.optional and self.sequence_kind is not None:
            raise ValueError(f"optional field cannot be sequence or slice: {self.rule}")
        if self.sequence_kind is not None and (
            not self.name or any(ch in self.name for ch in "?*&[]<>")
        ):
            raise ValueError(f"Invalid collection element type: {rule}")

        self.inner = extract_type_argument(self.name)


# ------------------------------------------------------------------------------
# Preamble


def write_preamble(out: list[str]) -> None:
    out.append("""
    // This is a generated file. Don't modify it by hand!
    // Run `crates/ruff_python_ast/generate.py` to re-generate the file.

    use crate::name::Name;
    use crate::visitor::source_order::SourceOrderVisitor;
    """)


# ------------------------------------------------------------------------------
# Owned enum


def write_owned_enum(out: list[str], ast: Ast) -> None:
    """
    Create an enum for each group that contains an owned copy of a syntax node.

    ```rust
    pub enum TypeParam {
        TypeVar(TypeParamTypeVar),
        TypeVarTuple(TypeParamTypeVarTuple),
        ...
    }
    ```

    Also creates:
    - `impl Ranged for TypeParam`
    - `impl HasNodeIndex for TypeParam`
    - `TypeParam::visit_source_order`
    - `impl From<TypeParamTypeVar> for TypeParam`
    - `impl Ranged for TypeParamTypeVar`
    - `impl HasNodeIndex for TypeParamTypeVar`
    - `fn TypeParam::is_type_var() -> bool`

    If the `add_suffix_to_is_methods` group option is true, then the
    `is_type_var` method will be named `is_type_var_type_param`.
    """

    for group in ast.groups:
        out.append("")
        if group.doc is not None:
            write_rustdoc(out, group.doc)
        out.append("#[derive(Clone, Debug, PartialEq)]")
        out.append('#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]')
        out.append(f"pub enum {group.owned_enum_ty} {{")
        for node in group.nodes:
            out.append(f"{node.variant}({node.ty}),")
        out.append("}")

        for node in group.nodes:
            out.append(f"""
            impl From<{node.ty}> for {group.owned_enum_ty} {{
                fn from(node: {node.ty}) -> Self {{
                    Self::{node.variant}(node)
                }}
            }}
            """)

        out.append(f"""
        impl ruff_text_size::Ranged for {group.owned_enum_ty} {{
            fn range(&self) -> ruff_text_size::TextRange {{
                match self {{
        """)
        for node in group.nodes:
            out.append(f"Self::{node.variant}(node) => node.range(),")
        out.append("""
                }
            }
        }
        """)

        out.append(f"""
        impl crate::HasNodeIndex for {group.owned_enum_ty} {{
            fn node_index(&self) -> crate::NodeIndex {{
                match self {{
        """)
        for node in group.nodes:
            out.append(f"Self::{node.variant}(node) => node.node_index(),")
        out.append("""
                }
            }
        }
        """)

        out.append(
            "#[allow(dead_code, clippy::match_wildcard_for_single_variants)]"
        )  # Not all is_methods are used
        out.append(f"impl {group.name} {{")
        for node in group.nodes:
            is_name = to_snake_case(node.variant)
            variant_name = node.variant
            match_arm = f"Self::{variant_name}"
            if group.add_suffix_to_is_methods:
                is_name = to_snake_case(node.variant + group.name)
            if len(group.nodes) > 1:
                out.append(f"""
                    #[inline]
                    pub const fn is_{is_name}(&self) -> bool {{
                        matches!(self, {match_arm}(_))
                    }}

                    #[inline]
                    pub fn {is_name}(self) -> Option<{node.ty}> {{
                        match self {{
                            {match_arm}(val) => Some(val),
                            _ => None,
                        }}
                    }}

                    #[inline]
                    pub fn expect_{is_name}(self) -> {node.ty} {{
                        match self {{
                            {match_arm}(val) => val,
                            _ => panic!("called expect on {{self:?}}"),
                        }}
                    }}

                    #[inline]
                    pub fn as_{is_name}_mut(&mut self) -> Option<&mut {node.ty}> {{
                        match self {{
                            {match_arm}(val) => Some(val),
                            _ => None,
                        }}
                    }}

                    #[inline]
                    pub fn as_{is_name}(&self) -> Option<&{node.ty}> {{
                        match self {{
                            {match_arm}(val) => Some(val),
                            _ => None,
                        }}
                    }}
                           """)
            elif len(group.nodes) == 1:
                out.append(f"""
                    #[inline]
                    pub const fn is_{is_name}(&self) -> bool {{
                        matches!(self, {match_arm}(_))
                    }}

                    #[inline]
                    pub fn {is_name}(self) -> Option<{node.ty}> {{
                        match self {{
                            {match_arm}(val) => Some(val),
                        }}
                    }}

                    #[inline]
                    pub fn expect_{is_name}(self) -> {node.ty} {{
                        match self {{
                            {match_arm}(val) => val,
                        }}
                    }}

                    #[inline]
                    pub fn as_{is_name}_mut(&mut self) -> Option<&mut {node.ty}> {{
                        match self {{
                            {match_arm}(val) => Some(val),
                        }}
                    }}

                    #[inline]
                    pub fn as_{is_name}(&self) -> Option<&{node.ty}> {{
                        match self {{
                            {match_arm}(val) => Some(val),
                        }}
                    }}
                           """)

        out.append("}")

    for node in ast.all_nodes:
        if node.custom_range:
            continue
        out.append(f"""
            impl ruff_text_size::Ranged for {node.ty} {{
                fn range(&self) -> ruff_text_size::TextRange {{
                    self.range
                }}
            }}
        """)

    for node in ast.all_nodes:
        out.append(f"""
            impl crate::HasNodeIndex for {node.ty} {{
                fn node_index(&self) -> crate::NodeIndex {{
                    self.node_index
                }}
            }}
        """)

    for group in ast.groups:
        out.append(f"""
            impl {group.owned_enum_ty} {{
                #[allow(unused)]
                pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
                where
                    V: crate::visitor::source_order::SourceOrderVisitor<'a> + ?Sized,
                {{
                    match self {{
        """)
        for node in group.nodes:
            out.append(
                f"{group.owned_enum_ty}::{node.variant}(node) => node.visit_source_order(visitor),"
            )
        out.append("""
                    }
                }
            }
        """)


# ------------------------------------------------------------------------------
# AnyNodeRef


def write_anynoderef(out: list[str], ast: Ast) -> None:
    """
    Create the AnyNodeRef type.

    ```rust
    pub enum AnyNodeRef<'a> {
        ...
        TypeParamTypeVar(&'a TypeParamTypeVar),
        TypeParamTypeVarTuple(&'a TypeParamTypeVarTuple),
        ...
    }
    ```

    Also creates:
    - `impl<'a> From<&'a TypeParam> for AnyNodeRef<'a>`
    - `impl<'a> From<&'a TypeParamTypeVarTuple> for AnyNodeRef<'a>`
    - `impl Ranged for AnyNodeRef<'_>`
    - `impl HasNodeIndex for AnyNodeRef<'_>`
    - `fn AnyNodeRef::as_ptr(&self) -> std::ptr::NonNull<()>`
    - `fn AnyNodeRef::visit_source_order(self, visitor &mut impl SourceOrderVisitor)`
    """

    out.append("""
    /// A flattened enumeration of all AST nodes.
    #[derive(Copy, Clone, Debug, is_macro::Is, PartialEq)]
    #[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
    pub enum AnyNodeRef<'a> {
    """)
    for node in ast.all_nodes:
        out.append(f"""{node.name}(&'a {node.ty}),""")
    out.append("""
    }
    """)

    for group in ast.groups:
        out.append(f"""
            impl<'a> From<&'a {group.owned_enum_ty}> for AnyNodeRef<'a> {{
                fn from(node: &'a {group.owned_enum_ty}) -> AnyNodeRef<'a> {{
                    match node {{
        """)
        for node in group.nodes:
            out.append(
                f"{group.owned_enum_ty}::{node.variant}(node) => AnyNodeRef::{node.name}(node),"
            )
        out.append("""
                    }
                }
            }
        """)

    for node in ast.all_nodes:
        out.append(f"""
            impl<'a> From<&'a {node.ty}> for AnyNodeRef<'a> {{
                fn from(node: &'a {node.ty}) -> AnyNodeRef<'a> {{
                    AnyNodeRef::{node.name}(node)
                }}
            }}
        """)

    out.append("""
        impl ruff_text_size::Ranged for AnyNodeRef<'_> {
            fn range(&self) -> ruff_text_size::TextRange {
                match self {
    """)
    for node in ast.all_nodes:
        out.append(f"""AnyNodeRef::{node.name}(node) => node.range(),""")
    out.append("""
                }
            }
        }
    """)

    out.append("""
        impl crate::HasNodeIndex for AnyNodeRef<'_> {
            fn node_index(&self) -> crate::NodeIndex {
                match self {
    """)
    for node in ast.all_nodes:
        out.append(f"""AnyNodeRef::{node.name}(node) => node.node_index(),""")
    out.append("""
                }
            }
        }
    """)

    out.append("""
        impl AnyNodeRef<'_> {
            pub fn as_ptr(&self) -> std::ptr::NonNull<()> {
                match self {
    """)
    for node in ast.all_nodes:
        out.append(
            f"AnyNodeRef::{node.name}(node) => std::ptr::NonNull::from(*node).cast(),"
        )
    out.append("""
                }
            }
        }
    """)

    out.append("""
        impl<'a> AnyNodeRef<'a> {
            pub fn visit_source_order<'b, V>(self, visitor: &mut V)
            where
                V: crate::visitor::source_order::SourceOrderVisitor<'b> + ?Sized,
                'a: 'b,
            {
                match self {
    """)
    for node in ast.all_nodes:
        out.append(
            f"AnyNodeRef::{node.name}(node) => node.visit_source_order(visitor),"
        )
    out.append("""
                }
            }
        }
    """)

    for group in ast.groups:
        out.append(f"""
        impl AnyNodeRef<'_> {{
            pub const fn is_{group.anynode_is_label}(self) -> bool {{
                matches!(self,
        """)
        for i, node in enumerate(group.nodes):
            if i > 0:
                out.append("|")
            out.append(f"""AnyNodeRef::{node.name}(_)""")
        out.append("""
                )
            }
        }
        """)


# ------------------------------------------------------------------------------
# NodeKind


def write_nodekind(out: list[str], ast: Ast) -> None:
    """
    Create the NodeKind type.

    ```rust
    pub enum NodeKind {
        ...
        TypeParamTypeVar,
        TypeParamTypeVarTuple,
        ...
    }

    Also creates:
    - `fn AnyNodeRef::kind(self) -> NodeKind`
    ```
    """

    out.append("""
    #[repr(u8)]
    #[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
    pub enum NodeKind {
    """)
    for node in ast.all_nodes:
        out.append(f"""{node.name},""")
    out.append("""
    }
    """)

    out.append("""
    impl AnyNodeRef<'_> {
        pub const fn kind(self) -> NodeKind {
            match self {
    """)
    for node in ast.all_nodes:
        out.append(f"""AnyNodeRef::{node.name}(_) => NodeKind::{node.name},""")
    out.append("""
            }
        }
    }
    """)


# ------------------------------------------------------------------------------
# Node structs


def write_node(out: list[str], ast: Ast) -> None:
    group_names = [group.name for group in ast.groups]
    for group in ast.groups:
        for node in group.nodes:
            if node.fields is None:
                continue
            if node.doc is not None:
                write_rustdoc(out, node.doc)
            out.append(
                "#[derive(Clone"
                + ("" if node.custom_debug else ", Debug")
                + ", PartialEq"
                + "".join(f", {derive}" for derive in node.derives)
                + ")]"
            )
            out.append('#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]')
            name = node.name
            out.append(f"pub struct {name} {{")
            out.append("pub node_index: crate::NodeIndex,")
            if not node.custom_range:
                out.append("pub range: ruff_text_size::TextRange,")
            for field in node.fields:
                field_str = f"pub {field.name}: "
                ty = field.parsed_ty

                rust_ty = f"{field.parsed_ty.name}"
                if ty.name in types_requiring_crate_prefix:
                    rust_ty = f"crate::{rust_ty}"

                if ty.sequence_kind is SequenceKind.VEC:
                    rust_ty = f"Vec<{rust_ty}>"
                elif ty.sequence_kind is SequenceKind.BOXED_SLICE:
                    rust_ty = f"Box<[{rust_ty}]>"
                elif ty.sequence_kind is SequenceKind.THIN_VEC:
                    rust_ty = f"thin_vec::ThinVec<{rust_ty}>"
                else:
                    if ty.name in group_names:
                        rust_ty = f"Box<{rust_ty}>"
                if ty.optional:
                    rust_ty = f"Option<{rust_ty}>"

                field_str += rust_ty + ","
                out.append(field_str)
            out.append("}")
            out.append("")


# ------------------------------------------------------------------------------
# Source order visitor


def write_source_order(out: list[str], ast: Ast) -> None:
    for group in ast.groups:
        for node in group.nodes:
            if node.fields is None or node.custom_source_order:
                continue
            name = node.name
            fields_list = ""
            body = ""

            for field in node.fields:
                if field.skip_source_order():
                    fields_list += f"{field.name}: _,\n"
                else:
                    fields_list += f"{field.name},\n"
            if not node.custom_range:
                fields_list += "range: _,\n"
            fields_list += "node_index: _,\n"

            for field in node.fields_in_source_order():
                visitor_name = (
                    type_to_visitor_function.get(
                        field.parsed_ty.inner, VisitorInfo("")
                    ).name
                    or f"visit_{to_snake_case(field.parsed_ty.inner)}"
                )
                visits_sequence = type_to_visitor_function.get(
                    field.parsed_ty.inner, VisitorInfo("")
                ).accepts_sequence

                if field.is_annotation:
                    visitor_name = "visit_annotation"

                if field.parsed_ty.optional:
                    body += f"""
                            if let Some({field.name}) = {field.name} {{
                                visitor.{visitor_name}({field.name});
                            }}\n
                      """
                elif not visits_sequence and field.parsed_ty.sequence_kind is not None:
                    body += f"""
                            for elm in {field.name} {{
                                visitor.{visitor_name}(elm);
                            }}
                     """
                else:
                    body += f"visitor.{visitor_name}({field.name});\n"

            visitor_arg_name = "visitor"
            if len(node.fields_in_source_order()) == 0:
                visitor_arg_name = "_"

            out.append(f"""
impl {name} {{
    pub(crate) fn visit_source_order<'a, V>(&'a self, {visitor_arg_name}: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {{
        let {name} {{
            {fields_list}
        }} = self;
        {body}
    }}
}}
        """)


# ------------------------------------------------------------------------------
# Flat syntax storage


PARTS = {
    "StringLiteralValue": "StringLiteral",
    "BytesLiteralValue": "BytesLiteral",
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
    lower: str
    read: str


def lower_node(ty: str, value: str) -> str:
    return f"u64::from(crate::{ty}::lower({value}, builder).slot)"


def field_codec(field: Field, nodes: set[str]) -> WireField:
    name = field.name
    ty = field.parsed_ty
    inner = ty.inner
    src = f"self.{name}"
    if ty.sequence_kind is not None or inner in PARTS:
        item = PARTS.get(inner, inner)
        if item in nodes:
            element = f"Handle<crate::{item}>"
            lower = lower_node(item, "item")
        elif item == "DictItem":
            element = "crate::DictItem"
            lower = f"""{{
                let key = item.key.as_ref().map_or(0, |key| {lower_node("Expr", "key")});
                key | ({lower_node("Expr", "&item.value")} << 32)
            }}"""
        elif item == "FStringPart":
            element = "crate::FStringPart"
            lower = f"""match item {{
                crate::FStringPartRef::Literal(value) => {lower_node("StringLiteral", "value")},
                crate::FStringPartRef::FString(value) => {lower_node("FString", "value")},
            }}"""
        else:
            assert item == "CmpOp", item
            element = "crate::CmpOp"
            lower = "item.encode()"
        param = "_" if item == "CmpOp" else "builder"
        lower = (
            f"builder.collect::<{element}, _>({src}.iter(), |{param}, item| {lower})"
        )
        if inner == "StringLiteralValue":
            return WireField(
                name,
                8,
                "crate::flat::StringValue<'a>",
                lower,
                "crate::flat::StringValue::new(List::new(self.ast, word), payload.read::<8, 6>())",
            )
        if inner in PARTS:
            value = {
                "FStringValue": "FStringValue",
                "TStringValue": "TStringValue",
                "BytesLiteralValue": "BytesValue",
            }[inner]
            return WireField(
                name,
                8,
                f"crate::flat::{value}<'a>",
                lower,
                f"crate::flat::{value}::new(List::new(self.ast, word))",
            )
        return WireField(
            name,
            8,
            f"NodeList<'a, crate::{item}>" if item in nodes else f"List<'a, {element}>",
            lower,
            "List::new(self.ast, word)",
        )
    if inner in nodes:
        result = f"View<'a, crate::{inner}>"
        if ty.optional:
            return WireField(
                name,
                4,
                f"Option<{result}>",
                f"{src}.as_ref().map_or(0, |item| {lower_node(inner, 'item')})",
                "self.ast.optional_child(word)",
            )
        return WireField(
            name, 4, result, lower_node(inner, f"&{src}"), "self.ast.child(word)"
        )
    if inner == "Name":
        return WireField(
            name,
            4,
            "&'a crate::name::Name",
            f"u64::from(builder.intern_name({src}.as_str()).0)",
            "&self.ast.names[split_words(word)[0] as usize]",
        )
    if inner in {"str", "LiteralText"}:
        return WireField(
            name, 8, "&'a str", f"builder.store_string(&{src})", "self.ast.string(word)"
        )
    if inner == "LiteralBytes":
        return WireField(
            name, 8, "&'a [u8]", f"builder.store_bytes(&{src})", "self.ast.bytes(word)"
        )
    if inner in TAGS:
        return WireField(
            name, 1, f"crate::{inner}", f"{src}.encode()", "Word::decode(word)"
        )
    if inner in FLAGS:
        return WireField(
            name,
            1,
            f"crate::{inner}",
            f"u64::from({src}.bits())",
            f"crate::{inner}::from_bits(word.to_le_bytes()[0])",
        )
    if inner == "bool":
        return WireField(name, 1, "bool", f"u64::from({src})", "word != 0")
    if inner == "u32":
        return WireField(name, 4, "u32", f"u64::from({src})", "split_words(word)[0]")
    if inner == "Number":
        return WireField(
            name,
            4,
            "&'a crate::Number",
            f"{{ let slot = offset(builder.ast.numbers.len()); builder.ast.numbers.push({src}.clone()); u64::from(slot) }}",
            "&self.ast.numbers[split_words(word)[0] as usize]",
        )
    if inner == "DebugText":
        assert ty.optional
        return WireField(
            name,
            4,
            "Option<&'a crate::DebugText>",
            f"{src}.as_ref().map_or(u64::from(ABSENT), |text| {{ let slot = offset(builder.ast.debug_text.len()); builder.ast.debug_text.push(text.clone()); u64::from(slot) }})",
            "(split_words(word)[0] != ABSENT).then(|| &self.ast.debug_text[split_words(word)[0] as usize])",
        )
    raise ValueError(f"Missing flat field codec for {field.name}: {field.ty}")


def layout(
    fields: list[WireField], preferred: str | None = None
) -> tuple[dict[str, int], dict[str, int]]:
    """Reserve one byte for the kind and, for nodes with spilled fields, four for the extra offset."""
    inline = fields.copy()
    if sum(f.width for f in fields) > 15:
        inline = []
        for field in sorted(fields, key=lambda f: (f.name != preferred, f.width)):
            if sum(f.width for f in inline) + field.width <= 11:
                inline.append(field)
    offset = 4 if len(inline) < len(fields) else 0
    offsets = {}
    for field in sorted(inline, key=lambda f: -f.width):
        offsets[field.name] = offset
        offset += field.width
    spills = {}
    word = 0
    for field in fields:
        if field not in inline:
            spills[field.name] = word
            word += (field.width + 3) // 4
    return offsets, spills


def rebuild_field(field: Field, nodes: set[str], implicit_boxes: set[str]) -> str:
    ty = field.parsed_ty
    inner = ty.inner
    get = f"view.{field.name}()"
    if inner in PARTS:
        return f"{get}.to_literal()"
    if ty.sequence_kind is not None:
        items = f"{get}.iter()"
        if inner == "DictItem":
            items += ".map(super::list::DictItemView::to_owned_item)"
        elif inner != "CmpOp":
            items += ".map(View::to_owned_node)"
        return f"{items}.collect()"
    if inner in nodes:
        if ty.name.startswith("Box<") or ty.name in implicit_boxes:
            if ty.optional:
                return f"{get}.map(|item| Box::new(item.to_owned_node()))"
            return f"Box::new({get}.to_owned_node())"
        if ty.optional:
            return f"{get}.map(View::to_owned_node)"
        return f"{get}.to_owned_node()"
    if inner in {"Number", "DebugText", "Name"}:
        return f"{get}.cloned()" if ty.optional else f"{get}.clone()"
    if inner in {"str", "LiteralText", "LiteralBytes", "Name"}:
        return f"{get}.into()"
    return get


def source_order_fields(node: Node) -> list[Field]:
    fields = node.flat_fields
    if node.source_order is None:
        return fields
    positions = {name: index for index, name in enumerate(node.source_order)}
    return sorted(fields, key=lambda field: positions.get(field.name, len(fields)))


def push_children(node: Node, nodes: set[str], words: dict[str, str]) -> str:
    """Push children in reverse source order for the nonrecursive preorder iterator."""
    ordered = "self.at(row).push_children_reversed(stack);"
    if node.name in {"Arguments", "PatternArguments"}:
        positional = "args" if node.name == "Arguments" else "patterns"
        return (
            f"if node.keywords().is_empty() {{ node.{positional}().push_reversed(stack); }} "
            f"else if node.{positional}().is_empty() {{ node.keywords().push_reversed(stack); }} else {{"
            + ordered
            + "}"
        )
    if node.name == "Parameters":
        # Error recovery can place variadic parameters before ordinary ones.
        return (
            "if node.vararg().is_none() && node.kwarg().is_none() {"
            "node.kwonlyargs().push_reversed(stack); node.args().push_reversed(stack); "
            "node.posonlyargs().push_reversed(stack); } else {" + ordered + "}"
        )
    if node.name in {"PatternMatchMapping", "InterpolatedElement"}:
        return ordered

    result = []
    for field in reversed(source_order_fields(node)):
        if field._skip_visit:
            continue
        ty = field.parsed_ty
        inner = ty.inner
        get = f"node.{field.name}()"
        if inner == "DictItem":
            result.append(
                f"for item in {get}.iter().rev() {{ stack.push(item.value.erase().handle()); if let Some(key) = item.key {{ stack.push(key.erase().handle()); }} }}"
            )
        elif inner == "FStringValue":
            return ordered
        elif inner in PARTS:
            result.append(
                f"for child in {get}.iter().rev() {{ stack.push(child.erase().handle()); }}"
            )
        elif inner in nodes and ty.sequence_kind:
            result.append(f"{get}.push_reversed(stack);")
        elif inner in nodes and ty.optional:
            result.append(
                f"let child = split_words({words[field.name]})[0]; if child != 0 {{ stack.push(Handle::from_word(child)); }}"
            )
        elif inner in nodes:
            result.append(
                f"stack.push(Handle::from_word(split_words({words[field.name]})[0]));"
            )
    return "".join(result)


def next_descendant(node: Node, nodes: set[str], words: dict[str, str]) -> str:
    """Advance to the first child, keeping later siblings on the stack."""
    push = push_children(node, nodes, words)
    if not push:
        return "stack.pop()"

    fields = [
        field
        for field in source_order_fields(node)
        if not field._skip_visit
        and (
            field.parsed_ty.inner in nodes
            or field.parsed_ty.inner in PARTS
            or field.parsed_ty.inner == "DictItem"
        )
    ]
    if len(fields) == 1:
        field = fields[0]
        ty = field.parsed_ty
        raw = words[field.name].replace("self.ast.extra_word", "self.extra_word")
        if (ty.inner in nodes and ty.sequence_kind) or ty.inner in {
            "StringLiteralValue",
            "BytesLiteralValue",
            "TStringValue",
        }:
            return f"List::<Handle<Any>>::new(self, {raw}).first_descendant(stack)"
        if ty.inner in nodes and ty.optional:
            return f"let id = split_words({raw})[0]; if id == 0 {{ stack.pop() }} else {{ Some(Handle::from_word(id)) }}"

    if (
        node.name != "InterpolatedElement"
        and 1 <= len(fields) <= 2
        and all(
            field.parsed_ty.inner in nodes
            and not field.parsed_ty.sequence_kind
            and not field.parsed_ty.optional
            and "extra_word" not in words[field.name]
            for field in fields
        )
    ):
        handles = [
            f"Handle::from_word(split_words({words[field.name]})[0])"
            for field in fields
        ]
        second = f"stack.push({handles[1]}); " if len(handles) == 2 else ""
        return second + f"Some({handles[0]})"

    push = push.replace("self.ast.extra_word", "self.extra_word")
    view = (
        f"let node = self.at(row.cast::<crate::{node.name}>());"
        if "node." in push
        else ""
    )
    return f"{view} {push} stack.pop()"


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
    if node.name == "PatternArguments":
        return "super::visit_merged(node.patterns().iter().map(View::erase), node.keywords().iter().map(View::erase), &mut |child| child.dispatch(visitor));"
    if node.name == "Parameters":
        return """
        if node.vararg().is_none() && node.kwarg().is_none() {
            for parameter in node.posonlyargs().iter().chain(node.args()).chain(node.kwonlyargs()) {
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
    if node.name == "PatternMatchMapping":
        return """
        let mut rest = node.rest();
        for (key, pattern) in node.keys().iter().zip(node.patterns()) {
            if let Some(identifier) = rest && identifier.start() < key.start() {
                visitor.visit_identifier(identifier);
                rest = None;
            }
            visitor.visit_expr(key);
            visitor.visit_pattern(pattern);
        }
        if let Some(identifier) = rest { visitor.visit_identifier(identifier); }
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
        ty = field.parsed_ty
        inner = ty.inner
        get = f"node.{field.name}()"
        if field._skip_visit:
            continue
        method = f"visit_{to_snake_case(PARTS.get(inner, inner))}"
        if field.is_annotation or (node.name, field.name) == (
            "Parameter",
            "annotation",
        ):
            method = "visit_annotation"
        if (node.name, field.name) == ("InterpolatedElement", "format_spec"):
            statements.append(
                f"if let Some(spec) = {get} {{ spec.erase().visit_source_order(visitor); }}"
            )
        elif inner == "DictItem":
            statements.append(
                f"for item in {get} {{ if let Some(key) = item.key {{ visitor.visit_expr(key); }} visitor.visit_expr(item.value); }}"
            )
        elif inner == "FStringValue":
            statements.append(
                f"for part in {get} {{ match part {{ super::FStringPartView::Literal(v) => visitor.visit_string_literal(v), super::FStringPartView::FString(v) => visitor.visit_f_string(v) }} }}"
            )
        elif inner == "Stmt" and ty.sequence_kind:
            statements.append(f"visitor.visit_body({get});")
        elif inner in PARTS or (inner in nodes and ty.sequence_kind):
            statements.append(f"for item in {get} {{ visitor.{method}(item); }}")
        elif inner in nodes and ty.optional:
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


def construction_field(
    field: Field, wire: WireField, nodes: set[str]
) -> tuple[str, str]:
    name = field.name
    ty = field.parsed_ty
    inner = ty.inner
    value = f"self.{name}"
    if ty.sequence_kind is not None or inner in PARTS:
        item = PARTS.get(inner, inner)
        element = f"Handle<crate::{item}>" if item in nodes else f"crate::{item}"
        return f"super::super::ListId<{element}>", f"{value}.word()"
    if inner in nodes:
        handle = f"BuildId<crate::{inner}>"
        if ty.optional:
            return (
                f"Option<{handle}>",
                f"{value}.map_or(0, |id| u64::from(id.slot))",
            )
        return handle, f"u64::from({value}.slot)"
    if inner == "Name":
        return "super::super::NameId", f"u64::from({value}.0)"
    if inner in {"LiteralText", "LiteralBytes"}:
        rust = "&'a str" if inner == "LiteralText" else "&'a [u8]"
        return rust, wire.lower.replace(f"&{value}", value)
    if inner == "str":
        return "Box<str>", wire.lower
    if inner == "Number":
        return "crate::Number", wire.lower.replace(f"{value}.clone()", value)
    if inner == "DebugText":
        return "Option<crate::DebugText>", wire.lower.replace(
            ".as_ref().map_or", ".map_or"
        ).replace("text.clone()", "text")
    return wire.result, wire.lower


def generate_flat_patterns(ast: Ast, nodes: set[str]) -> list[str]:
    """Generate field projections for destructuring a node in a match."""
    groups = ", ".join(f"{group.name}Kind" for group in ast.groups)
    out = [
        f"pub mod fields {{ use super::{{View, List, NodeList, Ranged, NodeKind, AnyKind, {groups}}};"
    ]
    for node in ast.all_nodes:
        ty = node.name
        fields = [
            field_codec(field, nodes)
            for field in node.flat_fields
            if (ty, field.name) != ("ExprCall", "range_start")
        ]
        out.append(f"#[derive(Clone, Copy)] pub struct {ty}<'a> {{")
        out.append(
            f"pub node: View<'a, crate::{ty}>, pub range: ruff_text_size::TextRange,"
        )
        for field in fields:
            out.append(f"pub {field.name}: {field.result},")
        out.append("}")
        out.append(
            f"impl<'a> View<'a, crate::{ty}> {{ #[inline] pub fn fields(self) -> {ty}<'a> {{"
        )
        out.append(f"{ty} {{ node: self, range: self.range(),")
        for field in fields:
            out.append(f"{field.name}: self.{field.name}(),")
        out.append("} } }")
    for group in ast.groups:
        ty = group.name
        out.append(f"#[derive(Clone, Copy)] pub enum {ty}<'a> {{")
        for node in group.nodes:
            out.append(f"{node.variant}({node.name}<'a>),")
        out.append("}")
        out.append(
            f"impl<'a> {ty}<'a> {{ pub fn view(self) -> View<'a, crate::{ty}> {{ match self {{"
        )
        for node in group.nodes:
            out.append(f"Self::{node.variant}(fields) => fields.node.into(),")
        out.append("} } }")
        out.append(
            f"impl<'a> View<'a, crate::{ty}> {{ #[inline] pub fn fields(self) -> {ty}<'a> {{ match self.classify() {{"
        )
        for node in group.nodes:
            out.append(
                f"{ty}Kind::{node.variant}(node) => {ty}::{node.variant}(node.fields()),"
            )
        out.append("} } }")
        out.append(f"impl<'a> View<'a, crate::{ty}> {{")
        for node in group.nodes:
            method = to_snake_case(
                node.variant + (group.name if group.add_suffix_to_is_methods else "")
            )
            out.append(
                f"#[inline] pub fn is_{method}(self) -> bool {{ self.node_kind() == NodeKind::{node.name} }}"
            )
            out.append(
                f"#[inline] pub fn as_{method}(self) -> Option<View<'a, crate::{node.name}>> {{ self.cast() }}"
            )
        out.append("}")
        out.append(
            f"impl<'a> {ty}Kind<'a> {{ #[inline] pub fn view(self) -> View<'a, crate::{ty}> {{ match self {{"
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
            out.append(
                f"impl<'a> From<View<'a, crate::{node.name}>> for View<'a, crate::{ty}> {{"
            )
            out.append(
                f"fn from(node: View<'a, crate::{node.name}>) -> Self {{ node.retype() }} }}"
            )
    out.append("#[derive(Clone, Copy)] pub enum AnyFields<'a> {")
    for node in ast.all_nodes:
        out.append(f"{node.name}({node.name}<'a>),")
    out.append("}")
    out.append(
        "impl<'a> View<'a, super::super::Any> { #[inline] pub fn fields(self) -> AnyFields<'a> { match self.classify() {"
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
        out.append(f"{node.name}(View<'a, crate::{node.name}>),")
    out.append("}")
    out.append(
        "impl<'a> View<'a, Any> { #[inline] pub fn classify(self) -> AnyKind<'a> { match self.node_kind() {"
    )
    for node in ast.all_nodes:
        out.append(f"NodeKind::{node.name} => AnyKind::{node.name}(self.retype()),")
    out.append("} } }")
    out.append("impl<'a> View<'a, Any> {")
    for node in ast.all_nodes:
        out.append(
            f"pub fn as_{to_snake_case(node.name)}(self) -> Option<View<'a, crate::{node.name}>> {{ self.cast() }}"
        )
        out.append(
            f"pub fn is_{to_snake_case(node.name)}(self) -> bool {{ self.node_kind() == NodeKind::{node.name} }}"
        )
    out.append("}")
    out.append(
        "impl<'a> AnyKind<'a> { #[inline] pub fn view(self) -> View<'a, Any> { match self {"
    )
    for node in ast.all_nodes:
        out.append(f"Self::{node.name}(node) => node.erase(),")
    out.append("} } }")
    out.append(
        "pub(super) fn same_fields(left: View<'_, Any>, right: View<'_, Any>) -> bool { match left.classify() {"
    )
    for node in ast.all_nodes:
        fields = [
            f
            for f in node.flat_fields
            if (node.name, f.name) != ("ExprCall", "range_start")
        ]
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
            f"impl<'a> From<View<'a, crate::{ty}>> for View<'a, Any> {{ fn from(node: View<'a, crate::{ty}>) -> Self {{ node.erase() }} }}"
        )
    for group in ast.groups:
        ty = group.name
        out.append(
            f"impl<'a> From<{ty}Kind<'a>> for View<'a, Any> {{ fn from(node: {ty}Kind<'a>) -> Self {{ node.view().erase() }} }}"
        )
    return out


def generate_flat(ast: Ast) -> str:
    groups = {group.name for group in ast.groups}
    nodes = groups | {node.name for node in ast.all_nodes}
    words_by_node: dict[str, dict[str, str]] = {}
    out = [
        "// Generated by crates/ruff_python_ast/generate.py from ast.toml.",
        "use ruff_text_size::Ranged;",
        "use super::{ABSENT, Any, BuildId, Builder, Handle, List, Lower, Node, NodeList, Payload, Rebuild, View, offset, split_words};",
        "use super::scalar::Word;",
        "use crate::NodeKind;",
    ]
    initializers = [
        "pub mod build { use super::{BuildId, Handle, Builder, ABSENT, Word, offset}; use super::super::{Emit, EmittedNode};"
    ]
    for group in ast.groups:
        name = group.name
        kinds = " | ".join(f"NodeKind::{n.name}" for n in group.nodes)
        debug_arms = " ".join(
            f'{name}Kind::{n.variant}(v) => f.debug_tuple("{n.variant}").field(&v).finish(),'
            for n in group.nodes
        )
        out.append(f"""impl Node for crate::{name} {{
            fn debug(view: View<'_, Self>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {{
                match view.classify() {{ {debug_arms} }}
            }}
            fn matches(kind: NodeKind) -> bool {{ matches!(kind, {kinds}) }}
        }}
        impl Lower for crate::{name} {{
            fn lower(&self, builder: &mut Builder) -> BuildId<Self> {{ match self {{""")
        for n in group.nodes:
            out.append(f"Self::{n.variant}(value) => value.lower(builder).cast(),")
            for id_type in ("Handle", "BuildId"):
                initializers.append(
                    f"impl From<{id_type}<crate::{n.name}>> for {id_type}<crate::{name}> {{ "
                    f"fn from(value: {id_type}<crate::{n.name}>) -> Self {{ value.cast() }} }}"
                )
        out.append("} } }")
        out.append(
            f"impl Rebuild for crate::{name} {{ fn rebuild(view: View<'_, Self>) -> Self {{ match view.classify() {{"
        )
        for n in group.nodes:
            out.append(
                f"{name}Kind::{n.variant}(value) => Self::{n.variant}(value.to_owned_node()),"
            )
        out.append("} } }")
        # Classifying a view can reuse the tag from the arena row.
        out.append(
            f"#[repr(u8)] #[derive(Debug, Copy, Clone)] pub enum {name}Kind<'a> {{"
        )
        for n in group.nodes:
            out.append(
                f"{n.variant}(View<'a, crate::{n.name}>) = NodeKind::{n.name} as u8,"
            )
        out.append("}")
        out.append(
            f"impl<'a> View<'a, crate::{name}> {{ #[inline] pub fn classify(self) -> {name}Kind<'a> {{ match self.node_kind() {{"
        )
        for n in group.nodes:
            out.append(f"NodeKind::{n.name} => {name}Kind::{n.variant}(self.retype()),")
        out.append('_ => unreachable!("typed node belongs to this group"), } } }')
    for n in ast.all_nodes:
        fields = [
            field_codec(f, nodes)
            for f in n.flat_fields
            if (n.name, f.name) != ("ExprCall", "range_start")
        ]
        paired_lists = n.name in {"Arguments", "PatternArguments", "ExprCompare"}
        if paired_lists:
            first, second = fields
            fields = [replace(first, width=4), second]
        offsets, spills = layout(fields, "args" if n.name == "Parameters" else None)
        words_by_node[n.name] = {}
        debug_fields = ["node_index", "range"]
        if n.fields is None:
            # The hand-written structs put their range before their index.
            debug_fields.reverse()
        if n.name == "Identifier":
            debug_fields.insert(0, "id")
        else:
            debug_fields.extend(field.name for field in fields)
        debug_fields = "".join(
            f'.field("{name}", &v.{name}())' for name in debug_fields
        )
        out.append(f"""impl Node for crate::{n.name} {{
             const KIND: Option<NodeKind> = Some(NodeKind::{n.name});
             fn debug(v: View<'_, Self>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {{
                 f.debug_struct("{n.name}"){debug_fields}.finish()
             }}
             #[inline]
             fn matches(kind: NodeKind) -> bool {{ kind == NodeKind::{n.name} }}
        }}
        impl Lower for crate::{n.name} {{
             fn lower(&self, builder: &mut Builder) -> BuildId<Self> {{""")
        for f in fields:
            out.append(f"let {f.name} = {f.lower};")
        method = f"push_{to_snake_case(n.name)}"
        parameters = ", ".join(f.name for f in fields)
        out.append(f"builder.{method}(self.range(), [{parameters}]) }} }}")
        out.append(
            f"impl Builder {{ #[inline] fn {method}(&mut self, range: ruff_text_size::TextRange, [{parameters}]: [u64; {len(fields)}]) -> BuildId<crate::{n.name}> {{"
        )
        default = ", Default" if n.name == "Parameters" else ""
        borrows_literal = any(
            field.parsed_ty.inner in {"LiteralText", "LiteralBytes"}
            for field in n.flat_fields
        )
        lifetime = "<'a>" if borrows_literal else ""
        constructor = n.name + ("<'_>" if borrows_literal else "")
        initializers.append(
            f"#[derive(Debug{default})] pub struct {n.name}{lifetime} {{pub range: ruff_text_size::TextRange,"
        )
        encode = []
        for logical, wire in zip(
            (
                f
                for f in n.flat_fields
                if (n.name, f.name) != ("ExprCall", "range_start")
            ),
            fields,
            strict=True,
        ):
            rust, expression = construction_field(logical, wire, nodes)
            initializers.append(f"pub {wire.name}: {rust},")
            encode.append(f"let {wire.name} = {expression};")
        initializers.append(
            f"}} impl Emit for {constructor} {{ type Output = crate::{n.name}; #[inline] fn emit(self, builder: &mut Builder) -> EmittedNode<Self::Output> {{"
        )
        initializers.extend(encode)
        initializers.append(
            f"(builder.{method}(self.range, [{parameters}]), self.range, crate::NodeKind::{n.name}) }} }}"
        )
        mutable = "mut " if fields else ""
        out.append(f"let {mutable}payload = Payload::new(NodeKind::{n.name});")
        if n.name == "ExprCompare":
            # Zero marks a chain or an incomplete comparison.
            assert offsets == {"operands": 0, "ops": 8}
            out.append(
                "let [start, len] = split_words(ops);"
                "if len == 1 && split_words(operands)[1] == 2 {"
                "payload.write::<12, 1>(u64::from(self.ast.extra[start as usize]) + 1); }"
            )
        if paired_lists:
            out.append(
                f"let [{parameters}] = super::list::Interval::pack_pair(&mut self.ast.extra, [{parameters}]);"
            )
        for f in fields:
            if f.name in offsets:
                out.append(f"payload.write::<{offsets[f.name]}, {f.width}>({f.name});")
        if n.name == "ExprStringLiteral":
            out.append("payload.write::<8, 6>(self.string_literal_text(value));")
        if n.name == "StmtFunctionDef":
            # The parser records the number of rows in the body in the last two bytes.
            assert all(
                offsets[f.name] + f.width <= 13 for f in fields if f.name in offsets
            )
        if spills:
            if n.name == "Parameters":
                # Ordinary positional-or-keyword parameters fit in the row.
                out.append(
                    "if split_words(posonlyargs)[1] == 0 && split_words(kwonlyargs)[1] == 0 "
                    "&& vararg == 0 && kwarg == 0 { "
                    "payload.write::<0, 4>(u64::from(ABSENT)); } else {"
                )
            out.append(
                "payload.write::<0, 4>(u64::from(offset(self.ast.extra.len())));"
            )
            words = []
            for field in fields:
                if field.name in spills:
                    words.append(f"split_words({field.name})[0]")
                    if field.width == 8:
                        words.append(f"split_words({field.name})[1]")
            out.append("self.ast.extra.extend([" + ", ".join(words) + "]);")
            if n.name == "Parameters":
                out.append("}")
        if n.name in {"ExprYield", "ExprYieldFrom"}:
            out.append(
                "let node = self.push(range, payload);"
                "self.ast.yield_rows_end = node.slot + 1; node } }"
            )
        else:
            out.append("self.push(range, payload) } }")
        out.append(
            f"impl Rebuild for crate::{n.name} {{ fn rebuild(view: View<'_, Self>) -> Self {{ Self {{"
        )
        out.append("node_index: view.node_index(),")
        if not n.custom_range:
            out.append("range: view.range(),")
        for field in n.flat_fields:
            value = (
                "view.range().start()"
                if (n.name, field.name) == ("ExprCall", "range_start")
                else rebuild_field(
                    field, nodes, groups if n.fields is not None else set()
                )
            )
            out.append(f"{field.name}: {value},")
        out.append("} } }")
        if "ctx" in offsets:
            out.append(
                f"impl Builder {{ pub(super) fn {to_snake_case(n.name)}_context(&mut self, node: BuildId<crate::Expr>, ctx: crate::ExprContext) {{ self.ast.rows[node.slot()].payload.write::<{offsets['ctx']}, 1>(ctx.encode()); }} }}"
            )
        if fields:
            generics, lifetime = (
                ("<'a>", "'a") if any("'a" in f.result for f in fields) else ("", "'_")
            )
            out.append(f"impl{generics} View<{lifetime}, crate::{n.name}> {{")
            if n.name == "ExprCompare":
                out.append(
                    f"""
                    /// Returns the operands and operator of a single comparison.
                    #[inline]
                    pub fn as_single(self) -> Option<(View<'a, crate::Expr>, crate::CmpOp, View<'a, crate::Expr>)> {{
                        let payload = &self.row.payload;
                        let operator = payload.read::<12, 1>().checked_sub(1)?;
                        let start = payload.offset::<{offsets["operands"]}>();
                        let &[left, right] = self.ast.extra.get(start..)?.first_chunk()?;
                        Some((
                            self.ast.child(u64::from(left)),
                            crate::CmpOp::decode(operator),
                            self.ast.child(u64::from(right)),
                        ))
                    }}
                    """
                )
            if n.name == "Parameters":
                out.append(
                    "pub(super) fn has_only_regular_args(self) -> bool { "
                    "self.row.payload.offset::<0>() == ABSENT as usize }"
                )
            for f in fields:
                if f.name in offsets:
                    raw = f"payload.read::<{offsets[f.name]}, {f.width}>()"
                    if paired_lists and f is fields[0]:
                        boundary = offsets[fields[1].name]
                        raw = f"super::list::Interval::preceding(payload.read::<{boundary}, 4>(), {raw})"
                else:
                    index = "payload.offset::<0>()"
                    if spills[f.name]:
                        index += f" + {spills[f.name]}"
                    raw = f"self.ast.extra_word::<{str(f.width == 8).lower()}>({index})"
                    if n.name == "Parameters":
                        raw = f"if payload.offset::<0>() == ABSENT as usize {{ 0 }} else {{ {raw} }}"
                if f.result == "&'a crate::name::Name":
                    out.append(
                        f"#[inline] pub fn name_id(self) -> super::NameId {{ let payload = &self.row.payload; super::NameId(split_words({raw})[0]) }}"
                    )
                words_by_node[n.name][f.name] = raw
                if (n.name, f.name) == ("StringLiteral", "value"):
                    out.append(
                        f"pub(super) fn text_range(self) -> super::Interval {{ let payload = &self.row.payload; super::Interval::decode({raw}) }}"
                    )
                if f.name == "iter":
                    out.append(
                        '#[expect(clippy::iter_not_returning_iterator, reason = "Python names the iterable expression in a for loop or comprehension `iter`.")]'
                    )
                out.append(f"""#[inline] pub fn {f.name}(self) -> {f.result} {{
                    let payload = &self.row.payload;
                    let word = {raw};
                    {f.read}
                }}""")
            out.append("}")
    steps: dict[str, list[str]] = {}
    for node in ast.all_nodes:
        step = next_descendant(node, nodes, words_by_node[node.name])
        steps.setdefault(step, []).append(f"NodeKind::{node.name}")
    out.append("""
        impl super::Ast {
            #[inline]
            pub(super) fn next_descendant(&self, row: Handle<Any>, stack: &mut Vec<Handle<Any>>) -> Option<Handle<Any>> {
                let payload = &self.rows[row.slot()].payload;
                match payload.kind {
    """)
    for step, kinds in steps.items():
        out.append(f"{' | '.join(kinds)} => {{ {step} }}")
    out.append("} } }")
    out.append(
        "impl<'a> View<'a, super::Any> { pub fn visit_source_order(self, visitor: &mut (impl super::visitor::source_order::SourceOrderVisitor<'a> + ?Sized)) { match self.node_kind() {"
    )
    for node in ast.all_nodes:
        walk = walk_fields(node, nodes)
        out.append(f"NodeKind::{node.name} => {{")
        if walk:
            out.append(f"let node = self.retype::<crate::{node.name}>(); {walk}")
        out.append("}")
    out.append("} } }")
    out.append(
        "impl<'a> View<'a, super::Any> { fn dispatch(self, visitor: &mut (impl super::visitor::source_order::SourceOrderVisitor<'a> + ?Sized)) { match self.node_kind() {"
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
        for field in node.flat_fields
        if field.parsed_ty.inner in PARTS
    }
    assert len(part_lists) == 1, (
        "string-like expressions must share the parts list layout"
    )
    part_list = part_lists.pop()
    out.append(f"""impl<'a> super::StringLike<'a> {{
        #[inline] pub(super) fn part_nodes(self) -> NodeList<'a, Any> {{
            let node: View<'a, Any> = self.into();
            let payload = &node.row.payload;
            List::new(node.ast, {part_list})
        }}
    }}""")
    initializers.append("}")
    out.extend(initializers)
    out.extend(generate_flat_patterns(ast, nodes))
    out.extend(generate_flat_any(ast))
    return "\n".join(out)


# ------------------------------------------------------------------------------
# Format and write output


def generate(ast: Ast) -> list[str]:
    out = []
    write_preamble(out)
    write_owned_enum(out, ast)
    write_anynoderef(out, ast)
    write_nodekind(out, ast)
    write_node(out, ast)
    write_source_order(out, ast)
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
    flat.write_text(rustfmt(generate_flat(ast)))


if __name__ == "__main__":
    main()
