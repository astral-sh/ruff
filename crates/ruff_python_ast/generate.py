#!/usr/bin/python
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///

from __future__ import annotations

import re
from dataclasses import dataclass
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
    return re.sub("([A-Z])", r"_\1", node).lower().lstrip("_")


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
    ungrouped_nodes: list[Node]
    all_nodes: list[Node]

    def __init__(self, ast: dict[str, Any]) -> None:
        self.groups = []
        self.ungrouped_nodes = []
        self.all_nodes = []
        for group_name, group in ast.items():
            group = Group(group_name, group)
            self.all_nodes.extend(group.nodes)
            if group_name == "ungrouped":
                self.ungrouped_nodes = group.nodes
            else:
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
        self.ref_enum_ty = group_name + "Ref"
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
    derives: list[str]
    custom_source_order: bool
    source_order: list[str] | None

    def __init__(self, group: Group, node_name: str, node: dict[str, Any]) -> None:
        self.name = node_name
        self.variant = node.get("variant", node_name.removeprefix(group.name))
        self.ty = f"crate::{node_name}"
        self.fields = None
        fields = node.get("fields")
        if fields is not None:
            self.fields = [Field(f) for f in fields]
        self.custom_source_order = node.get("custom_source_order", False)
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
            field = None
            for field in self.fields:
                if field.skip_source_order():
                    continue
                if field.name == field_name:
                    field = field
                    break
            fields.append(field)
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
            fn node_index(&self) -> &crate::AtomicNodeIndex {{
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
                fn node_index(&self) -> &crate::AtomicNodeIndex {{
                    &self.node_index
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
# Ref enum


def write_ref_enum(out: list[str], ast: Ast) -> None:
    """
    Create an enum for each group that contains a reference to a syntax node.

    ```rust
    pub enum TypeParamRef<'a> {
        TypeVar(&'a TypeParamTypeVar),
        TypeVarTuple(&'a TypeParamTypeVarTuple),
        ...
    }
    ```

    Also creates:
    - `impl<'a> From<&'a TypeParam> for TypeParamRef<'a>`
    - `impl<'a> From<&'a TypeParamTypeVar> for TypeParamRef<'a>`
    - `impl Ranged for TypeParamRef<'_>`
    - `impl HasNodeIndex for TypeParamRef<'_>`
    - `fn TypeParamRef::is_type_var() -> bool`

    The name of each variant can be customized via the `variant` node option. If
    the `add_suffix_to_is_methods` group option is true, then the `is_type_var`
    method will be named `is_type_var_type_param`.
    """

    for group in ast.groups:
        out.append("")
        if group.doc is not None:
            write_rustdoc(out, group.doc)
        out.append("""#[derive(Clone, Copy, Debug, PartialEq, is_macro::Is)]""")
        out.append('#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]')
        out.append(f"""pub enum {group.ref_enum_ty}<'a> {{""")
        for node in group.nodes:
            if group.add_suffix_to_is_methods:
                is_name = to_snake_case(node.variant + group.name)
                out.append(f'#[is(name = "{is_name}")]')
            out.append(f"""{node.variant}(&'a {node.ty}),""")
        out.append("}")

        out.append(f"""
            impl<'a> From<&'a {group.owned_enum_ty}> for {group.ref_enum_ty}<'a> {{
                fn from(node: &'a {group.owned_enum_ty}) -> Self {{
                    match node {{
        """)
        for node in group.nodes:
            out.append(
                f"{group.owned_enum_ty}::{node.variant}(node) => {group.ref_enum_ty}::{node.variant}(node),"
            )
        out.append("""
                    }
                }
            }
        """)

        for node in group.nodes:
            out.append(f"""
            impl<'a> From<&'a {node.ty}> for {group.ref_enum_ty}<'a> {{
                fn from(node: &'a {node.ty}) -> Self {{
                    Self::{node.variant}(node)
                }}
            }}
            """)

        out.append(f"""
        impl ruff_text_size::Ranged for {group.ref_enum_ty}<'_> {{
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
        impl crate::HasNodeIndex for {group.ref_enum_ty}<'_> {{
            fn node_index(&self) -> &crate::AtomicNodeIndex {{
                match self {{
        """)
        for node in group.nodes:
            out.append(f"Self::{node.variant}(node) => node.node_index(),")
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
    - `impl<'a> From<TypeParamRef<'a>> for AnyNodeRef<'a>`
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

        out.append(f"""
            impl<'a> From<{group.ref_enum_ty}<'a>> for AnyNodeRef<'a> {{
                fn from(node: {group.ref_enum_ty}<'a>) -> AnyNodeRef<'a> {{
                    match node {{
        """)
        for node in group.nodes:
            out.append(
                f"{group.ref_enum_ty}::{node.variant}(node) => AnyNodeRef::{node.name}(node),"
            )
        out.append("""
                    }
                }
            }
        """)

        # `as_*` methods to convert from `AnyNodeRef` to e.g. `ExprRef`
        out.append(f"""
            impl<'a> AnyNodeRef<'a> {{
                pub fn as_{to_snake_case(group.ref_enum_ty)}(self) -> Option<{group.ref_enum_ty}<'a>> {{
                    match self {{
        """)
        for node in group.nodes:
            out.append(
                f"Self::{node.name}(node) => Some({group.ref_enum_ty}::{node.variant}(node)),"
            )
        out.append("""
                        _ => None,
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
            fn node_index(&self) -> &crate::AtomicNodeIndex {
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
# AnyRootNodeRef


def write_root_anynoderef(out: list[str], ast: Ast) -> None:
    """
    Create the AnyRootNodeRef type.

    ```rust
    pub enum AnyRootNodeRef<'a> {
        ...
        TypeParam(&'a TypeParam),
        ...
    }
    ```

    Also creates:
    - `impl<'a> From<&'a TypeParam> for AnyRootNodeRef<'a>`
    - `impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a TypeParam`
    - `impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a TypeParamVarTuple`
    - `impl Ranged for AnyRootNodeRef<'_>`
    - `impl HasNodeIndex for AnyRootNodeRef<'_>`
    - `fn AnyRootNodeRef::visit_source_order(self, visitor &mut impl SourceOrderVisitor)`
    """

    root_nodes = [(group.name, group.owned_enum_ty) for group in ast.groups]
    root_nodes.extend((node.name, node.ty) for node in ast.ungrouped_nodes)

    out.append("""
    /// An enumeration of all AST nodes.
    ///
    /// Unlike `AnyNodeRef`, this type does not flatten nested enums, so its variants only
    /// consist of the "root" AST node types. This is useful as it exposes references to the
    /// original enums, not just references to their inner values.
    ///
    /// For example, `AnyRootNodeRef::Mod` contains a reference to the `Mod` enum, while
    /// `AnyNodeRef` has top-level `AnyNodeRef::ModModule` and `AnyNodeRef::ModExpression`
    /// variants.
    #[derive(Copy, Clone, Debug, PartialEq)]
    #[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
    pub enum AnyRootNodeRef<'a> {
    """)
    for name, ty in root_nodes:
        out.append(f"""{name}(&'a {ty}),""")
    out.append("""
    }
    """)

    out.append("""
    /// The unflattened enum or struct type stored by an [`AnyRootNodeRef`].
    ///
    /// Unlike [`NodeKind`], this does not distinguish variants of root enums such as [`Stmt`]
    /// and [`Expr`].
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    #[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
    #[repr(u8)]
    pub enum RootNodeKind {
    """)
    for name, _ in root_nodes:
        out.append(f"""{name},""")
    out.append("""
    }

    impl RootNodeKind {
        /// All root node kinds in discriminant order.
        pub const ALL: &'static [Self] = &[
    """)
    for name, _ in root_nodes:
        out.append(f"""Self::{name},""")
    out.append("""
        ];

        /// Returns the root node kind with the given discriminant.
        #[inline]
        pub fn from_u8(value: u8) -> Option<Self> {
            match value {
    """)
    for index, (name, _) in enumerate(root_nodes):
        out.append(f"""{index} => Some(Self::{name}),""")
    out.append("""
                _ => None,
            }
        }
    }
    """)

    for group in ast.groups:
        out.append(f"""
            impl<'a> From<&'a {group.owned_enum_ty}> for AnyRootNodeRef<'a> {{
                #[inline]
                fn from(node: &'a {group.owned_enum_ty}) -> AnyRootNodeRef<'a> {{
                        AnyRootNodeRef::{group.name}(node)
                }}
            }}
        """)

        out.append(f"""
            impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a {group.owned_enum_ty} {{
                type Error = ();
                fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a {group.owned_enum_ty}, ()> {{
                    match node {{
                        AnyRootNodeRef::{group.name}(node) => Ok(node),
                        _ => Err(())
                    }}
                }}
            }}
        """)

        for node in group.nodes:
            out.append(f"""
                impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a {node.ty} {{
                    type Error = ();
                    fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a {node.ty}, ()> {{
                        match node {{
                            AnyRootNodeRef::{group.name}({group.owned_enum_ty}::{node.variant}(node)) => Ok(node),
                            _ => Err(())
                        }}
                    }}
                }}
            """)

    for node in ast.ungrouped_nodes:
        out.append(f"""
            impl<'a> From<&'a {node.ty}> for AnyRootNodeRef<'a> {{
                #[inline]
                fn from(node: &'a {node.ty}) -> AnyRootNodeRef<'a> {{
                    AnyRootNodeRef::{node.name}(node)
                }}
            }}
        """)

        out.append(f"""
            impl<'a> TryFrom<AnyRootNodeRef<'a>> for &'a {node.ty} {{
                type Error = ();
                fn try_from(node: AnyRootNodeRef<'a>) -> Result<&'a {node.ty}, ()> {{
                    match node {{
                        AnyRootNodeRef::{node.name}(node) => Ok(node),
                        _ => Err(())
                    }}
                }}
            }}
        """)

    out.append("""
        impl ruff_text_size::Ranged for AnyRootNodeRef<'_> {
            fn range(&self) -> ruff_text_size::TextRange {
                match self {
    """)
    for name, _ in root_nodes:
        out.append(f"""AnyRootNodeRef::{name}(node) => node.range(),""")
    out.append("""
                }
            }
        }
    """)

    out.append("""
        impl crate::HasNodeIndex for AnyRootNodeRef<'_> {
            fn node_index(&self) -> &crate::AtomicNodeIndex {
                match self {
    """)
    for name, _ in root_nodes:
        out.append(f"""AnyRootNodeRef::{name}(node) => node.node_index(),""")
    out.append("""
                }
            }
        }
    """)

    out.append("""
        impl<'a> AnyRootNodeRef<'a> {
            /// Decomposes this reference into its root node kind and a type-erased pointer.
            #[inline]
            pub fn into_raw_parts(self) -> (RootNodeKind, std::ptr::NonNull<()>) {
                match self {
    """)
    for name, _ in root_nodes:
        out.append(
            f"""AnyRootNodeRef::{name}(node) => (RootNodeKind::{name}, std::ptr::NonNull::from(node).cast()),"""
        )
    out.append("""
                }
            }

            /// Reconstructs an AST reference from its root node kind and type-erased pointer.
            ///
            /// # Safety
            ///
            /// - `pointer` must be properly aligned for and point to the exact root node type
            ///   represented by `kind`.
            /// - The pointer's provenance must permit reads of a complete, initialized, and valid
            ///   value of that type.
            /// - The pointed-to value must not be moved, dropped, or accessed mutably for `'a`.
            #[inline]
            #[expect(unsafe_code, reason = "reconstructs a type-erased AST reference")]
            pub unsafe fn from_raw_parts(kind: RootNodeKind, pointer: std::ptr::NonNull<()>) -> Self {
                let pointer = pointer.as_ptr();
                // SAFETY: The caller guarantees that `pointer` is readable as the exact root node
                // type selected by `kind` and remains valid and immutable for `'a`.
                unsafe { match kind {
    """)
    for name, ty in root_nodes:
        out.append(
            f"""RootNodeKind::{name} => AnyRootNodeRef::{name}(&*pointer.cast::<{ty}>()),"""
        )
    out.append("""
                }}
            }

            pub fn visit_source_order<'b, V>(self, visitor: &mut V)
            where
                V: crate::visitor::source_order::SourceOrderVisitor<'b> + ?Sized,
                'a: 'b,
            {
                match self {
    """)
    for name, _ in root_nodes:
        out.append(
            f"""AnyRootNodeRef::{name}(node) => node.visit_source_order(visitor),"""
        )
    out.append("""
                }
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
                "#[derive(Clone, Debug, PartialEq"
                + "".join(f", {derive}" for derive in node.derives)
                + ")]"
            )
            out.append('#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]')
            name = node.name
            out.append(f"pub struct {name} {{")
            out.append("pub node_index: crate::AtomicNodeIndex,")
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
# Format and write output


def generate(ast: Ast) -> list[str]:
    out = []
    write_preamble(out)
    write_owned_enum(out, ast)
    write_ref_enum(out, ast)
    write_anynoderef(out, ast)
    write_root_anynoderef(out, ast)
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


if __name__ == "__main__":
    main()
