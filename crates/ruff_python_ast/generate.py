#!/usr/bin/python
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///

from __future__ import annotations

import re
from dataclasses import dataclass
from pathlib import Path
from subprocess import check_output
from textwrap import dedent, indent
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
    group: Group
    doc: str | None
    fields: list[Field] | None
    derives: list[str]
    custom_source_order: bool
    source_order: list[str] | None
    python_projection: PythonProjection | None

    def __init__(self, group: Group, node_name: str, node: dict[str, Any]) -> None:
        self.name = node_name
        self.variant = node.get("variant", node_name.removeprefix(group.name))
        self.ty = f"crate::{node_name}"
        self.group = group
        self.fields = None
        fields = node.get("fields")
        if fields is not None:
            self.fields = [Field(f) for f in fields]
        self.custom_source_order = node.get("custom_source_order", False)
        self.derives = node.get("derives", [])
        self.doc = node.get("doc")
        self.source_order = node.get("source_order")
        projection = node.get("python_projection")
        self.python_projection = None
        if projection is not None:
            field_names = []
            if self.fields is not None:
                field_names = [field.name for field in self.fields]
            self.python_projection = PythonProjection(
                self.name, field_names, projection
            )

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
    behavior: FieldBehavior
    parsed_ty: FieldType

    def __init__(self, field: dict[str, Any]) -> None:
        self.name = field["name"]
        self.ty = field["type"]
        self.parsed_ty = FieldType(self.ty)
        self._skip_visit = field.get("skip_visit", False)
        self.is_annotation = field.get("is_annotation", False)
        self.behavior = describe_field(self)

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


# Extracts the type argument from the given rust type with AST field type syntax.
# Box<str> -> str
# Box<Expr?> -> Expr
# If the type does not have a type argument, it will return the string.
# Does not support nested types
def extract_type_argument(rust_type_str: str) -> str:
    rust_type_str = rust_type_str.replace("*", "")
    rust_type_str = rust_type_str.replace("?", "")
    rust_type_str = rust_type_str.replace("&", "")

    open_bracket_index = rust_type_str.find("<")
    if open_bracket_index == -1:
        return rust_type_str
    close_bracket_index = rust_type_str.rfind(">")
    if close_bracket_index == -1 or close_bracket_index <= open_bracket_index:
        raise ValueError(f"Brackets are not balanced for type {rust_type_str}")
    inner_type = rust_type_str[open_bracket_index + 1 : close_bracket_index].strip()
    inner_type = inner_type.replace("crate::", "")
    return inner_type


@dataclass
class FieldType:
    rule: str
    name: str
    inner: str
    seq: bool = False
    optional: bool = False
    slice_: bool = False

    def __init__(self, rule: str) -> None:
        self.rule = rule
        self.name = ""
        self.inner = extract_type_argument(rule)

        # Some special casing that isn't currently defined in ast.toml
        last_pos = len(rule) - 1
        for i, ch in enumerate(rule):
            if ch == "?":
                if i == last_pos:
                    self.optional = True
                else:
                    raise ValueError(f"`?` must be at the end: {rule}")
            elif ch == "*":
                if self.slice_:  # The * after & is a slice
                    continue
                if i == last_pos:
                    self.seq = True
                else:
                    raise ValueError(f"`*` must be at the end: {rule}")
            elif ch == "&":
                if i == 0 and rule.endswith("*"):
                    self.slice_ = True
                else:
                    raise ValueError(
                        f"`&` must be at the start and end with `*`: {rule}"
                    )
            else:
                self.name += ch

        if self.optional and (self.seq or self.slice_):
            raise ValueError(f"optional field cannot be sequence or slice: {rule}")


@dataclass(frozen=True)
class FieldBehavior:
    stub_base: str
    is_optional: bool
    is_sequence: bool
    is_slice: bool
    is_expr: bool = False
    is_stmt: bool = False
    is_arguments: bool = False
    is_keyword: bool = False
    is_identifier: bool = False
    is_parameter_like: bool = False
    is_dict_item: bool = False
    is_expr_context: bool = False
    is_string_enum: bool = False
    is_bool: bool = False
    is_str: bool = False
    is_int: bool = False
    is_generic_node: bool = False

    def uses_expr_to_python(self) -> bool:
        return self.is_expr

    def uses_stmt_to_python(self) -> bool:
        return self.is_stmt

    def uses_node_to_python(self) -> bool:
        return self.is_arguments or self.is_keyword or self.is_parameter_like

    def uses_generic_node_to_python(self) -> bool:
        return self.is_generic_node

    def uses_py_string(self) -> bool:
        return (
            (self.is_identifier and not self.is_optional)
            or self.is_string_enum
            or self.is_expr_context
            or self.is_str
        )

    def needs_locator(self) -> bool:
        return (
            self.is_expr
            or self.is_stmt
            or self.uses_node_to_python()
            or self.is_dict_item
            or self.is_generic_node
        )


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
    for group in ast.groups:
        out.append(f"""{group.name}(&'a {group.owned_enum_ty}),""")
    for node in ast.ungrouped_nodes:
        out.append(f"""{node.name}(&'a {node.ty}),""")
    out.append("""
    }
    """)

    for group in ast.groups:
        out.append(f"""
            impl<'a> From<&'a {group.owned_enum_ty}> for AnyRootNodeRef<'a> {{
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
    for group in ast.groups:
        out.append(f"""AnyRootNodeRef::{group.name}(node) => node.range(),""")
    for node in ast.ungrouped_nodes:
        out.append(f"""AnyRootNodeRef::{node.name}(node) => node.range(),""")
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
    for group in ast.groups:
        out.append(f"""AnyRootNodeRef::{group.name}(node) => node.node_index(),""")
    for node in ast.ungrouped_nodes:
        out.append(f"""AnyRootNodeRef::{node.name}(node) => node.node_index(),""")
    out.append("""
                }
            }
        }
    """)

    out.append("""
        impl<'a> AnyRootNodeRef<'a> {
            pub fn visit_source_order<'b, V>(self, visitor: &mut V)
            where
                V: crate::visitor::source_order::SourceOrderVisitor<'b> + ?Sized,
                'a: 'b,
            {
                match self {
    """)
    for group in ast.groups:
        out.append(
            f"""AnyRootNodeRef::{group.name}(node) => node.visit_source_order(visitor),"""
        )
    for node in ast.ungrouped_nodes:
        out.append(
            f"""AnyRootNodeRef::{node.name}(node) => node.visit_source_order(visitor),"""
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
                if ty.slice_:
                    rust_ty = f"[{rust_ty}]"
                if (ty.name in group_names or ty.slice_) and ty.seq is False:
                    rust_ty = f"Box<{rust_ty}>"

                if ty.seq:
                    rust_ty = f"Vec<{rust_ty}>"
                elif ty.optional:
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
                elif not visits_sequence and field.parsed_ty.seq:
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
# Python projection outputs for external linters.
#
# Generates three outputs: Rust-side PyO3 class definitions, projection and
# lazy-loading implementations, and the Python-side interface classes.


def append_block(out: list[str], block: str) -> None:
    out.append(dedent(block).strip("\n"))


def node_var_name(node: Node) -> str:
    if node.group.name == "Stmt":
        return "stmt"
    if node.group.name == "Expr":
        return "expr"
    return "node"


def kind_enum_name(node: Node) -> str:
    if node.group.name == "Stmt":
        return "StmtKind"
    if node.group.name == "Expr":
        return "ExprKind"
    return ""


def any_node_variant(node: Node) -> str:
    return node.name


def type_path(node: Node) -> str:
    return f"ruff_python_ast::{node.name}"


def projection_metadata(node: Node) -> list[ProjectionMetadata]:
    return node.python_projection.metadata_fields  # type: ignore[union-attr]


def projection_special_cases(
    node: Node, node_var: str, kind_enum: str
) -> tuple[list[str], dict[str, str], dict[str, str]]:
    """
    Returns (setup_lines, metadata_values, precomputed_eager_fields)
    - metadata_values maps metadata field names to the local variable that holds the value
    - precomputed_eager_fields maps eager field names to local variable names
    """
    setup_lines: list[str] = []
    metadata_values: dict[str, str] = {}
    precomputed_eager_fields: dict[str, str] = {}

    if node.name == "ExprCall":
        setup_lines.extend(
            [
                f"        let callee = extract_callee(locator, range, {node_var});",
                f"        let function_text = Some(locator.slice({node_var}.func.range()).trim().to_string());",
                f"        let function_kind = Some({kind_enum}::from({node_var}.func.as_ref()).as_str().to_owned());",
                f"        let arguments = node_to_python(py, locator, AnyNodeRef::from(&{node_var}.arguments), types)?;",
            ]
        )
        metadata_values = {
            "callee": "callee",
            "function_text": "function_text",
            "function_kind": "function_kind",
        }
        precomputed_eager_fields["arguments"] = "arguments"
    elif projection_metadata(node):
        raise ValueError(
            f"metadata_fields not supported for node {node.name}; add handling in projection_special_cases"
        )

    return setup_lines, metadata_values, precomputed_eager_fields


STRINGY_ENUM_TYPES: set[str] = {
    "ExprContext",
    "BoolOp",
    "Operator",
    "UnaryOp",
    "CmpOp",
    "IpyEscapeKind",
    "Singleton",
}


@dataclass
class PythonProjection:
    class_name: str
    eager_fields: list[str]
    _eager_fields: set[str]
    metadata_fields: list[ProjectionMetadata]

    def __init__(
        self, node_name: str, field_names: list[str], projection: dict[str, Any]
    ) -> None:
        self.class_name = projection["class"]
        eager_fields = projection.get("eager_fields", [])
        unknown_fields = set(eager_fields) - set(field_names)
        if unknown_fields:
            raise ValueError(
                f"Unknown eager fields for {node_name}: {', '.join(sorted(unknown_fields))}"
            )
        self.eager_fields = eager_fields
        self._eager_fields = set(eager_fields)
        self.metadata_fields = [
            ProjectionMetadata(field) for field in projection.get("metadata_fields", [])
        ]

    def is_eager(self, field_name: str) -> bool:
        return field_name in self._eager_fields


@dataclass
class ProjectionMetadata:
    name: str
    rust_type: str
    stub_type: str

    def __init__(self, field: dict[str, Any]) -> None:
        self.name = field["name"]
        self.rust_type = field["rust_type"]
        self.stub_type = field["stub_type"]


@dataclass
class ProjectionModel:
    nodes: list[Node]

    @classmethod
    def from_ast(cls, ast: Ast) -> ProjectionModel:
        return cls([node for node in ast.all_nodes if node.python_projection])

    def fields(self) -> list[tuple[Node, Field]]:
        return [(node, field) for node in self.nodes for field in node.fields or []]

    def eager_fields(self) -> list[tuple[Node, Field]]:
        return [
            (node, field)
            for node, field in self.fields()
            if node.python_projection.is_eager(field.name)
        ]

    def lazy_fields(self) -> list[tuple[Node, Field]]:
        return [
            (node, field)
            for node, field in self.fields()
            if not node.python_projection.is_eager(field.name)
        ]


def describe_field(field: Field) -> FieldBehavior:
    parsed = field.parsed_ty
    inner = parsed.inner
    stub_base = {
        "Identifier": "str",
        "Name": "str",
        "ExprContext": "str",
        "bool": "bool",
        "str": "str",
        "u32": "int",
        "Number": "float",
        "FStringValue": "str",
        "TStringValue": "str",
        "StringLiteralValue": "str",
        "BytesLiteralValue": "str",
        "Arguments": "Arguments",
        "Keyword": "Keyword",
        "Alias": "Alias",
        "WithItem": "WithItem",
        "MatchCase": "MatchCase",
        "Decorator": "Decorator",
        "ElifElseClause": "ElifElseClause",
        "TypeParams": "TypeParams",
        "Comprehension": "Comprehension",
        "PatternArguments": "PatternArguments",
        "PatternKeyword": "PatternKeyword",
        "Parameter": "Parameter",
        "ParameterWithDefault": "ParameterWithDefault",
        "Parameters": "Parameters",
    }.get(inner, "Node")
    if stub_base == "Node" and inner in STRINGY_ENUM_TYPES:
        stub_base = "str"

    string_enum = inner in STRINGY_ENUM_TYPES
    expr_context = inner == "ExprContext"
    is_str = inner in {
        "str",
        "Name",
        "Identifier",
        "FStringValue",
        "TStringValue",
        "StringLiteralValue",
        "BytesLiteralValue",
    }
    is_int = inner == "u32"
    known_node_kinds = {
        "Expr",
        "Stmt",
        "Arguments",
        "Keyword",
        "Identifier",
        "Parameters",
        "Parameter",
        "ParameterWithDefault",
        "DictItem",
    }
    known_primitives = {
        "bool",
        "str",
        "Name",
        "u32",
        "Number",
        "ExprContext",
        "FStringValue",
        "TStringValue",
        "StringLiteralValue",
        "BytesLiteralValue",
    }
    return FieldBehavior(
        stub_base=stub_base,
        is_optional=parsed.optional,
        is_sequence=parsed.seq,
        is_slice=parsed.slice_,
        is_expr=inner == "Expr",
        is_stmt=inner == "Stmt",
        is_arguments=inner == "Arguments",
        is_keyword=inner == "Keyword",
        is_identifier=inner == "Identifier",
        is_parameter_like=inner
        in {
            "Parameters",
            "Parameter",
            "ParameterWithDefault",
        },
        is_dict_item=inner == "DictItem",
        is_expr_context=expr_context,
        is_string_enum=string_enum,
        is_bool=inner == "bool",
        is_str=is_str,
        is_int=is_int,
        is_generic_node=inner not in known_node_kinds
        and inner not in known_primitives
        and not string_enum
        and not expr_context,
    )


# Python stub generation


def python_field_type(field: Field) -> tuple[str, set[str]]:
    behavior = field.behavior

    ty = behavior.stub_base
    typing_imports: set[str] = set()
    if behavior.is_slice or behavior.is_sequence:
        ty = f"Sequence[{ty}]"
        typing_imports.add("Sequence")
    if behavior.is_optional:
        ty = f"Optional[{ty}]"
        typing_imports.add("Optional")
    return ty, typing_imports


def write_python_stub(out: list[str], ast: Ast) -> None:
    projection = ProjectionModel.from_ast(ast)
    projected_nodes = projection.nodes
    typing_imports: set[str] = set()
    for node in projected_nodes:
        for field in node.fields or []:
            _, imports = python_field_type(field)
            typing_imports.update(imports)
        for metadata in node.python_projection.metadata_fields:  # type: ignore[union-attr]
            if metadata.stub_type.startswith("Optional["):
                typing_imports.add("Optional")
    typing_line = (
        f"from typing import {', '.join(sorted(typing_imports))}\n\n"
        if typing_imports
        else ""
    )

    class_names = [node.python_projection.class_name for node in projected_nodes]

    header = dedent(
        """\
        # This file is auto-generated by crates/ruff_python_ast/generate.py
        from __future__ import annotations

        {typing_line}from . import Node

        __all__ = {class_names}

        """
    ).format(typing_line=typing_line, class_names=class_names)
    out.append(header)

    for node in projected_nodes:
        assert node.python_projection is not None
        class_name = node.python_projection.class_name
        class_lines = [f"class {class_name}(Node):"]
        if not node.fields:
            class_lines.append("    pass")
        else:
            for field in node.fields:
                ty, _ = python_field_type(field)
                class_lines.append(f"    {field.name}: {ty}")
        for metadata in node.python_projection.metadata_fields:
            class_lines.append(f"    {metadata.name}: {metadata.stub_type}")
        out.append("\n".join(class_lines) + "\n\n")

    # Ensure a trailing blank line so tools like isort keep the file unchanged.
    out[-1] = out[-1].rstrip("\n") + "\n\n"


def generate_python_stub(ast: Ast) -> list[str]:
    out = []
    write_python_stub(out, ast)
    return out


def write_python_stub_output(root: Path, out: list[str]) -> None:
    out_path = root.joinpath(
        "crates", "ruff_linter", "resources", "ruff_external", "nodes.pyi"
    )
    out_path.write_text("".join(out))


@dataclass(frozen=True)
class BaseArg:
    name: str
    ty: str
    expr: str
    optional: bool = False


BASE_ARGS: list[BaseArg] = [
    BaseArg("kind", "String", "kind"),
    BaseArg("span", "PyObject", "span_tuple(py, range)?"),
    BaseArg("text", "String", "text"),
    BaseArg("repr_value", "String", "repr_value"),
    BaseArg("node_id", "u32", "node_id"),
    BaseArg("store", "AstStoreHandle", "store"),
]
BASE_ARG_NAMES: set[str] = {arg.name for arg in BASE_ARGS}


def write_projection_bindings(out: list[str], projection: ProjectionModel) -> None:
    projected_nodes = projection.nodes
    lazy_fields = projection.lazy_fields()
    uses_expr_fields = any(field.behavior.is_expr for _, field in lazy_fields)
    uses_stmt_fields = any(field.behavior.is_stmt for _, field in lazy_fields)
    uses_arguments_fields = any(field.behavior.is_arguments for _, field in lazy_fields)
    uses_keyword_fields = any(field.behavior.is_keyword for _, field in lazy_fields)
    uses_generic_node_fields = any(
        field.behavior.is_generic_node for _, field in lazy_fields
    )
    uses_string_fields = any(field.behavior.is_str for _, field in lazy_fields)
    uses_int_fields = any(field.behavior.is_int for _, field in lazy_fields)
    uses_identifier_fields = any(
        field.behavior.is_identifier and not field.behavior.is_optional
        for _, field in lazy_fields
    )
    uses_optional_identifier_fields = any(
        field.behavior.is_identifier and field.behavior.is_optional
        for _, field in lazy_fields
    )
    needs_call_metadata = any(node.name == "ExprCall" for node in projected_nodes)

    super_imports: list[str] = ["py_none"]
    if uses_expr_fields:
        super_imports.append("expr_to_python")
    if uses_stmt_fields:
        super_imports.append("stmt_to_python")
    if uses_arguments_fields or uses_keyword_fields or uses_generic_node_fields:
        super_imports.append("node_to_python")
    if uses_identifier_fields or uses_string_fields:
        super_imports.append("py_string")
    if uses_int_fields:
        super_imports.append("py_int")
    if uses_optional_identifier_fields or needs_call_metadata:
        super_imports.append("optional_str")
    append_block(
        out,
        f"""
        // This file is auto-generated by crates/ruff_python_ast/generate.py

        #![allow(clippy::unnecessary_wraps)]

        use pyo3::exceptions::PyRuntimeError;

        use pyo3::prelude::*;
        use pyo3::PyClassInitializer;
        use pyo3::types::{{PyModule, PyTuple}};

        use ruff_python_ast::AnyNodeRef;

        use super::bindings;
        use super::source::SourceFileHandle;
        use super::store::AstStoreHandle;
        use super::ProjectionTypesRef;
        use super::{{{", ".join(super_imports)}}};
        """,
    )
    append_block(
        out,
        """
        fn cache_value(
            slot: &mut Option<PyObject>,
            py: Python<'_>,
            value: PyObject,
        ) -> PyResult<PyObject> {
            *slot = Some(value.clone_ref(py));
            Ok(value)
        }

        fn map_option<T>(
            py: Python<'_>,
            value: Option<T>,
            mapper: impl FnOnce(T) -> PyResult<PyObject>,
        ) -> PyResult<PyObject> {
            match value {
                Some(value) => mapper(value),
                None => Ok(py_none(py)),
            }
        }

        fn map_tuple<I, F>(py: Python<'_>, iter: I, mut mapper: F) -> PyResult<PyObject>
        where
            I: IntoIterator,
            F: FnMut(I::Item) -> PyResult<PyObject>,
        {
            let mut values = Vec::new();
            for value in iter {
                values.push(mapper(value)?);
            }
            Ok(PyTuple::new(py, values)?.into_any().unbind())
        }

        fn get_ast<T>(store: &AstStoreHandle, id: u32) -> PyResult<&T>
        where
            T: 'static,
        {
            store.get::<T>(id).map_err(|err| PyRuntimeError::new_err(err.to_string()))
        }
        """,
    )
    if projected_nodes:
        exports = ", ".join(
            f'"{node.python_projection.class_name}"'  # type: ignore[union-attr]
            for node in projected_nodes
        )
        out.append(f"pub(crate) const GENERATED_EXPORTS: &[&str] = &[{exports}];")
    else:
        out.append("pub(crate) const GENERATED_EXPORTS: &[&str] = &[];")

    registrations = "\n".join(
        f"    module.add_class::<{node.python_projection.class_name}>()?;"
        for node in projected_nodes
    )
    registration_block = indent(registrations, "    ") if registrations else ""
    append_block(
        out,
        f"""
        pub(crate) fn add_generated_classes(module: &Bound<'_, PyModule>) -> PyResult<()> {{
{registration_block}
            Ok(())
        }}
        """,
    )

    for node in projected_nodes:
        assert node.python_projection is not None
        class_name = node.python_projection.class_name
        fields = node.fields or []
        metadata_fields = [
            (metadata.name, metadata.rust_type)
            for metadata in node.python_projection.metadata_fields
        ]
        field_lines = [f"{field.name}: Option<PyObject>," for field in fields]
        field_lines.extend(
            [
                "#[allow(dead_code)]",
                "locator: SourceFileHandle,",
                "#[allow(dead_code)]",
                "projection: ProjectionTypesRef,",
            ]
        )
        for name, ty in metadata_fields:
            field_lines.append(f"{name}: {ty},")
        struct_fields = indent("\n".join(field_lines), "    ")
        append_block(
            out,
            f"""
            #[pyclass(module = "ruff_external", extends = bindings::Node, unsendable)]
            pub(crate) struct {class_name} {{
{struct_fields}
            }}
            """,
        )

        eager_fields = {
            field.name
            for field in fields
            if node.python_projection.is_eager(field.name)
        }
        required_base_args = [arg for arg in BASE_ARGS if not arg.optional]
        optional_base_args = [arg for arg in BASE_ARGS if arg.optional]
        arg_entries = [f"{arg.name}: {arg.ty}" for arg in required_base_args]
        arg_entries.extend(
            f"{eager_field_var_name(field)}: PyObject"
            for field in fields
            if field.name in eager_fields
        )
        arg_entries.extend(f"{name}: {ty}" for name, ty in metadata_fields)
        arg_entries.extend(
            ["locator: SourceFileHandle", "projection: ProjectionTypesRef"]
        )
        helper_arg_entries = list(arg_entries)
        helper_arg_entries.extend(f"{arg.name}: {arg.ty}" for arg in optional_base_args)
        arg_entries = ["py: Python<'_>"]
        arg_entries.extend(helper_arg_entries)
        helper_arg_list = ",\n        ".join(helper_arg_entries)

        constructor_arg_names = (
            [arg.name for arg in required_base_args]
            + [
                eager_field_var_name(field)
                for field in fields
                if field.name in eager_fields
            ]
            + [name for name, _ in metadata_fields]
            + ["locator", "projection"]
            + [arg.name for arg in optional_base_args]
        )
        append_block(
            out,
            f"""
            #[pymethods]
            impl {class_name} {{
            """,
        )
        field_inits = []
        for field in fields:
            if field.name in eager_fields:
                field_inits.append(
                    f"{field.name}: Some({eager_field_var_name(field)}),"
                )
            else:
                field_inits.append(f"{field.name}: None,")
        field_inits.extend(["locator,", "projection,"])
        for name, _ in metadata_fields:
            field_inits.append(f"{name},")
        init_block = indent("\n".join(field_inits), "                ")
        node_inner_args = [arg.name for arg in BASE_ARGS]
        node_inner_list = ",\n                    ".join(node_inner_args)
        helper_call_args = ",\n            ".join(constructor_arg_names)

        if fields:
            for field in fields:
                iter_attr = (
                    "#[allow(clippy::iter_not_returning_iterator)]\n"
                    if field.name == "iter"
                    else ""
                )
                if field.name in eager_fields:
                    append_block(
                        out,
                        f"""
                            {iter_attr}#[getter]
                            fn {field.name}(&self, py: Python<'_>) -> PyResult<PyObject> {{
                                Ok(self.{field.name}.as_ref().unwrap().clone_ref(py))
                            }}
                        """,
                    )
                else:
                    append_block(
                        out,
                        f"""
                            {iter_attr}#[getter]
                            fn {field.name}(this: PyRefMut<'_, Self>) -> PyResult<PyObject> {{
                                if let Some(value) = &this.{field.name} {{
                                    return Ok(value.clone_ref(this.py()));
                                }}
                                Self::load_{field.name}(this)
                            }}
                        """,
                    )
            for field in fields:
                if field.name not in eager_fields:
                    append_block(out, lazy_field_loader(node, field))
        if metadata_fields:
            for name, _ in metadata_fields:
                append_block(
                    out,
                    f"""
                            #[getter]
                            fn {name}(&self, py: Python<'_>) -> PyResult<PyObject> {{
                                Ok(optional_str(py, self.{name}.as_deref()))
                            }}
                    """,
                )

        append_block(
            out,
            """
            }
            """,
        )
        append_block(
            out,
            f"""
            impl {class_name} {{
                #[allow(clippy::too_many_arguments)]
                fn init_parts(
                    py: Python<'_>,
{helper_arg_list},
                ) -> (Self, bindings::Node) {{
                    (
                        Self {{
{init_block}
                        }},
                        bindings::Node::new_inner(
                            py,
            {node_inner_list},
                        ),
                    )
                }}

                #[allow(clippy::too_many_arguments)]
                pub(crate) fn new_instance(
                    py: Python<'_>,
{helper_arg_list},
                ) -> PyResult<PyObject> {{
                    let (this, node) = Self::init_parts(
                        py,
            {helper_call_args}
                    );
                    let initializer = PyClassInitializer::from(node).add_subclass(this);
                    Ok(Py::new(py, initializer)?.into_any())
                }}
            }}
            """,
        )


def eager_field_var_name(field: Field) -> str:
    if field.name in BASE_ARG_NAMES:
        return f"{field.name}_field"
    return field.name


def eager_field_assignment(node: Node, field: Field) -> tuple[list[str], str]:
    node_var = node_var_name(node)
    access = f"{node_var}.{field.name}"
    parsed = field.parsed_ty
    behavior = field.behavior
    name = eager_field_var_name(field)

    if behavior.is_sequence or behavior.is_slice:
        if behavior.is_string_enum and parsed.inner == "CmpOp":
            lines = [
                f"    let mut {name}_values = Vec::with_capacity({access}.len());",
                f"    for value in &{access} {{",
                f"        {name}_values.push(py_string(py, value.as_str()));",
                "    }",
                f"    let {name} = PyTuple::new(py, {name}_values)?.into_any().unbind();",
            ]
            return lines, name
        raise ValueError(
            f"Sequenced eager fields are not supported yet ({node.name}.{field.name})"
        )

    if behavior.is_expr:
        if behavior.is_optional:
            lines = [
                f"    let {name} = if let Some(value) = {access}.as_ref() {{",
                "        expr_to_python(py, locator, value, types)?",
                "    } else {",
                "        py_none(py)",
                "    };",
            ]
        else:
            lines = [
                f"    let {name} = expr_to_python(py, locator, &{access}, types)?;"
            ]
        return lines, name

    if behavior.is_stmt:
        if behavior.is_optional:
            lines = [
                f"    let {name} = if let Some(value) = {access}.as_ref() {{",
                "        stmt_to_python(py, locator, value, types)?",
                "    } else {",
                "        py_none(py)",
                "    };",
            ]
        else:
            lines = [
                f"    let {name} = stmt_to_python(py, locator, &{access}, types)?;"
            ]
        return lines, name

    if behavior.is_identifier:
        if behavior.is_optional:
            lines = [
                f"    let {name} = optional_str(py, {access}.as_ref().map(ruff_python_ast::Identifier::as_str));"
            ]
        else:
            lines = [f"    let {name} = py_string(py, {access}.as_str());"]
        return lines, name

    if behavior.is_arguments:
        if behavior.is_optional:
            lines = [
                f"    let {name} = if let Some(value) = {access}.as_ref() {{",
                "        node_to_python(py, locator, AnyNodeRef::from(value.as_ref()), types)?",
                "    } else {",
                "        py_none(py)",
                "    };",
            ]
        else:
            lines = [
                f"    let {name} = node_to_python(py, locator, AnyNodeRef::from(&{access}), types)?;"
            ]
        return lines, name

    if behavior.is_keyword:
        if behavior.is_optional:
            lines = [
                f"    let {name} = if let Some(value) = {access}.as_ref() {{",
                "        node_to_python(py, locator, AnyNodeRef::from(value), types)?",
                "    } else {",
                "        py_none(py)",
                "    };",
            ]
        else:
            lines = [
                f"    let {name} = node_to_python(py, locator, AnyNodeRef::from(&{access}), types)?;"
            ]
        return lines, name

    if behavior.is_expr_context:
        lines = [
            f'    let {name}_value = format!("{{access:?}}", access = {access});',
            f"    let {name} = py_string(py, &{name}_value);",
        ]
        return lines, name

    if behavior.is_string_enum:
        lines = [f"    let {name} = py_string(py, {access}.as_str());"]
        return lines, name

    if behavior.is_bool:
        lines = [f"    let {name} = py_bool(py, {access});"]
        return lines, name

    if behavior.is_str:
        if parsed.inner == "Name":
            if behavior.is_optional:
                lines = [
                    f"    let {name} = if let Some(value) = {access}.as_ref() {{",
                    "        py_string(py, value.as_str())",
                    "    } else {",
                    "        py_none(py)",
                    "    };",
                ]
            else:
                lines = [f"    let {name} = py_string(py, {access}.as_str());"]
        elif parsed.inner == "StringLiteralValue":
            if behavior.is_optional:
                lines = [
                    f"    let {name} = if let Some(value) = {access}.as_ref() {{",
                    "        py_string(py, value.to_str())",
                    "    } else {",
                    "        py_none(py)",
                    "    };",
                ]
            else:
                lines = [f"    let {name} = py_string(py, {access}.to_str());"]
        elif parsed.rule.startswith("Box<") and parsed.inner == "str":
            if behavior.is_optional:
                lines = [
                    f"    let {name} = if let Some(value) = {access}.as_ref() {{",
                    "        py_string(py, value.as_ref())",
                    "    } else {",
                    "        py_none(py)",
                    "    };",
                ]
            else:
                lines = [f"    let {name} = py_string(py, {access}.as_ref());"]
        elif behavior.is_optional:
            lines = [
                f"    let {name} = if let Some(value) = {access}.as_ref() {{",
                '        py_string(py, &format!("{value:?}"))',
                "    } else {",
                "        py_none(py)",
                "    };",
            ]
        else:
            lines = [
                f'    let {name} = py_string(py, &format!("{{value:?}}", value = {access}));'
            ]
        return lines, name

    if behavior.is_int:
        if behavior.is_optional:
            lines = [
                f"    let {name} = if let Some(value) = {access} {{",
                "        py_int(py, value)",
                "    } else {",
                "        py_none(py)",
                "    };",
            ]
        else:
            lines = [f"    let {name} = py_int(py, {access});"]
        return lines, name

    if behavior.is_generic_node:
        if behavior.is_optional:
            if parsed.rule.startswith("Box<"):
                lines = [
                    f"    let {name} = if let Some(value) = {access}.as_ref() {{",
                    "        node_to_python(py, locator, AnyNodeRef::from(value.as_ref()), types)?",
                    "    } else {",
                    "        py_none(py)",
                    "    };",
                ]
            else:
                lines = [
                    f"    let {name} = if let Some(value) = {access}.as_ref() {{",
                    "        node_to_python(py, locator, AnyNodeRef::from(value), types)?",
                    "    } else {",
                    "        py_none(py)",
                    "    };",
                ]
        elif behavior.is_sequence or behavior.is_slice:
            raise ValueError(
                f"Sequenced eager fields are not supported yet ({node.name}.{field.name})"
            )
        else:
            lines = [
                f"    let {name} = node_to_python(py, locator, AnyNodeRef::from(&{access}), types)?;"
            ]
        return lines, name

    raise ValueError(
        f"Unsupported eager field type for {node.name}.{field.name}: {parsed.inner}"
    )


def lazy_field_loader(node: Node, field: Field) -> str:
    field_name = field.name
    accessor = f"ast.{field.name}"
    parsed = field.parsed_ty
    behavior = field.behavior
    needs_locator = False
    needs_ast = False
    conversion = "py_none(py)"

    def set_locator() -> None:
        nonlocal needs_locator
        needs_locator = True

    def ensure_ast() -> None:
        nonlocal needs_ast
        needs_ast = True

    if behavior.is_expr:
        ensure_ast()
        set_locator()
        if behavior.is_optional:
            conversion = (
                f"map_option(py, {accessor}.as_ref(), |value| "
                "expr_to_python(py, &locator, value, this.projection))?"
            )
        elif behavior.is_sequence or behavior.is_slice:
            conversion = (
                f"map_tuple(py, {accessor}.iter(), |value| "
                "expr_to_python(py, &locator, value, this.projection))?"
            )
        else:
            conversion = f"expr_to_python(py, &locator, &{accessor}, this.projection)?"
    elif behavior.is_stmt:
        ensure_ast()
        set_locator()
        if behavior.is_optional:
            conversion = (
                f"map_option(py, {accessor}.as_ref(), |value| "
                "stmt_to_python(py, &locator, value, this.projection))?"
            )
        elif behavior.is_sequence or behavior.is_slice:
            conversion = (
                f"map_tuple(py, {accessor}.iter(), |value| "
                "stmt_to_python(py, &locator, value, this.projection))?"
            )
        else:
            conversion = f"stmt_to_python(py, &locator, &{accessor}, this.projection)?"
    elif behavior.is_arguments:
        ensure_ast()
        set_locator()
        if behavior.is_optional:
            conversion = (
                f"map_option(py, {accessor}.as_ref(), |value| "
                "node_to_python(py, &locator, AnyNodeRef::from(value.as_ref()), this.projection))?"
            )
        elif behavior.is_sequence or behavior.is_slice:
            conversion = (
                f"map_tuple(py, {accessor}.iter(), |value| "
                "node_to_python(py, &locator, AnyNodeRef::from(value), this.projection))?"
            )
        else:
            conversion = f"node_to_python(py, &locator, AnyNodeRef::from(&{accessor}), this.projection)?"
    elif behavior.is_keyword:
        ensure_ast()
        set_locator()
        if behavior.is_optional:
            conversion = (
                f"map_option(py, {accessor}.as_ref(), |value| "
                "node_to_python(py, &locator, AnyNodeRef::from(value), this.projection))?"
            )
        elif behavior.is_sequence or behavior.is_slice:
            conversion = (
                f"map_tuple(py, {accessor}.iter(), |value| "
                "node_to_python(py, &locator, AnyNodeRef::from(value), this.projection))?"
            )
        else:
            conversion = f"node_to_python(py, &locator, AnyNodeRef::from(&{accessor}), this.projection)?"
    elif behavior.is_identifier:
        ensure_ast()
        if behavior.is_sequence or behavior.is_slice:
            conversion = (
                f"map_tuple(py, {accessor}.iter(), |value| "
                "Ok(py_string(py, value.as_str())))?"
            )
        elif behavior.is_optional:
            conversion = f"optional_str(py, {accessor}.as_ref().map(ruff_python_ast::Identifier::as_str))"
        else:
            conversion = f"py_string(py, {accessor}.as_str())"
    elif behavior.is_str:
        ensure_ast()
        if parsed.inner == "Name":
            if behavior.is_sequence or behavior.is_slice:
                conversion = (
                    f"map_tuple(py, {accessor}.iter(), |value| "
                    "Ok(py_string(py, value.as_str())))?"
                )
            elif behavior.is_optional:
                conversion = (
                    f"optional_str(py, {accessor}.as_ref().map(|value| value.as_str()))"
                )
            else:
                conversion = f"py_string(py, {accessor}.as_str())"
        elif parsed.inner == "StringLiteralValue":
            if behavior.is_sequence or behavior.is_slice:
                conversion = (
                    f"map_tuple(py, {accessor}.iter(), |value| "
                    "Ok(py_string(py, value.to_str())))?"
                )
            elif behavior.is_optional:
                conversion = (
                    f"optional_str(py, {accessor}.as_ref().map(|value| value.to_str()))"
                )
            else:
                conversion = f"py_string(py, {accessor}.to_str())"
        elif parsed.rule.startswith("Box<") and parsed.inner == "str":
            if behavior.is_sequence or behavior.is_slice:
                conversion = (
                    f"map_tuple(py, {accessor}.iter(), |value| "
                    "Ok(py_string(py, value.as_ref())))?"
                )
            elif behavior.is_optional:
                conversion = (
                    f"optional_str(py, {accessor}.as_ref().map(|value| value.as_ref()))"
                )
            else:
                conversion = f"py_string(py, {accessor}.as_ref())"
        elif behavior.is_sequence or behavior.is_slice:
            conversion = f'map_tuple(py, {accessor}.iter(), |value| Ok(py_string(py, &format!("{{value:?}}"))))?'
        elif behavior.is_optional:
            conversion = f'map_option(py, {accessor}.as_ref(), |value| Ok(py_string(py, &format!("{{value:?}}"))))?'
        else:
            conversion = f'py_string(py, &format!("{{value:?}}", value = {accessor}))'
    elif behavior.is_int:
        ensure_ast()
        if behavior.is_sequence or behavior.is_slice:
            conversion = (
                f"map_tuple(py, {accessor}.iter(), |value| Ok(py_int(py, *value)))?"
            )
        elif behavior.is_optional:
            conversion = (
                f"map_option(py, {accessor}.as_ref(), |value| Ok(py_int(py, *value)))?"
            )
        else:
            conversion = f"py_int(py, {accessor})"
    elif behavior.is_parameter_like:
        ensure_ast()
        set_locator()
        if behavior.is_optional:
            if parsed.rule.startswith("Box<"):
                conversion = (
                    f"map_option(py, {accessor}.as_ref(), |value| "
                    "node_to_python(py, &locator, AnyNodeRef::from(value.as_ref()), this.projection))?"
                )
            else:
                conversion = (
                    f"map_option(py, {accessor}.as_ref(), |value| "
                    "node_to_python(py, &locator, AnyNodeRef::from(value), this.projection))?"
                )
        elif behavior.is_sequence or behavior.is_slice:
            conversion = (
                f"map_tuple(py, {accessor}.iter(), |value| "
                "node_to_python(py, &locator, AnyNodeRef::from(value), this.projection))?"
            )
        else:
            reference = (
                f"{accessor}.as_ref()"
                if parsed.rule.startswith("Box<")
                else f"&{accessor}"
            )
            conversion = f"node_to_python(py, &locator, AnyNodeRef::from({reference}), this.projection)?"
    elif behavior.is_dict_item:
        ensure_ast()
        set_locator()
        conversion = (
            f"map_tuple(py, {accessor}.iter(), |item| {{\n"
            "            let key = map_option(py, item.key.as_ref(), |key| expr_to_python(py, &locator, key, this.projection))?;\n"
            "            let value = expr_to_python(py, &locator, &item.value, this.projection)?;\n"
            "            PyTuple::new(py, [key, value]).map(|tuple| tuple.into_any().unbind())\n"
            "        })?"
        )
    elif behavior.is_generic_node:
        ensure_ast()
        set_locator()
        if behavior.is_optional:
            if parsed.rule.startswith("Box<"):
                conversion = (
                    f"map_option(py, {accessor}.as_ref(), |value| "
                    "node_to_python(py, &locator, AnyNodeRef::from(value.as_ref()), this.projection))?"
                )
            else:
                conversion = (
                    f"map_option(py, {accessor}.as_ref(), |value| "
                    "node_to_python(py, &locator, AnyNodeRef::from(value), this.projection))?"
                )
        elif behavior.is_sequence or behavior.is_slice:
            conversion = (
                f"map_tuple(py, {accessor}.iter(), |value| "
                "node_to_python(py, &locator, AnyNodeRef::from(value), this.projection))?"
            )
        else:
            conversion = f"node_to_python(py, &locator, AnyNodeRef::from(&{accessor}), this.projection)?"

    setup_lines = ["        let py = this.py();"]

    if needs_ast:
        setup_lines.extend(
            [
                "        let (node_id, store) = {",
                "            let super_ = this.as_super();",
                "            (super_.node_id(), super_.store().clone())",
                "        };",
            ]
        )

    if needs_locator:
        setup_lines.append("        let locator = this.locator.locator();")

    if needs_ast:
        setup_lines.append(
            f"        let ast = get_ast::<{type_path(node)}>(&store, node_id)?;"
        )

    setup = "\n".join(setup_lines)
    return dedent(
        f"""
        fn load_{field_name}(mut this: PyRefMut<'_, Self>) -> PyResult<PyObject> {{
{setup}
            let value = {conversion};
            cache_value(&mut this.{field_name}, py, value)
        }}
        """
    ).strip("\n")


def generate_projection_bindings(ast: Ast) -> list[str]:
    projection = ProjectionModel.from_ast(ast)
    out: list[str] = []
    write_projection_bindings(out, projection)
    return out


def write_projection_bindings_output(root: Path, out: list[str]) -> None:
    out_path = root.joinpath(
        "crates", "ruff_linter", "src", "external", "ast", "python", "generated.rs"
    )
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(rustfmt("\n".join(out)))


def write_projection_helpers(out: list[str], projection: ProjectionModel) -> None:
    projected_nodes = projection.nodes

    eager_fields = projection.eager_fields()
    uses_expr_fields = any(field.behavior.is_expr for _, field in eager_fields)
    uses_stmt_fields = any(field.behavior.is_stmt for _, field in eager_fields)
    uses_py_string = any(field.behavior.uses_py_string() for _, field in eager_fields)
    uses_py_bool = any(
        field.behavior.is_bool
        for node in projected_nodes
        for field in node.fields or []
    )
    uses_optional_identifier_fields = any(
        field.behavior.is_identifier and field.behavior.is_optional
        for node in projected_nodes
        for field in node.fields or []
    )
    has_stmt_nodes = any(node.group.name == "Stmt" for node in projected_nodes)
    has_expr_nodes = any(node.group.name == "Expr" for node in projected_nodes)
    needs_call_helpers = any(node.name == "ExprCall" for node in projected_nodes)

    append_block(
        out,
        """
        // This file is auto-generated by crates/ruff_python_ast/generate.py

        use pyo3::prelude::*;
        use pyo3::types::PyTuple;
        use pyo3::PyObject;

        use ruff_python_ast::AnyNodeRef;
        use ruff_text_size::Ranged;

        use crate::Locator;
        """,
    )

    target_imports = []
    if has_expr_nodes:
        target_imports.append("ExprKind")
    if has_stmt_nodes:
        target_imports.append("StmtKind")
    if target_imports:
        joined = ", ".join(target_imports)
        out.append(f"use crate::external::ast::target::{{{joined}}};")

    super_imports = [
        "source::SourceFileHandle",
        "span_tuple",
        "ProjectionTypesRef",
    ]

    if uses_expr_fields:
        super_imports.append("expr_to_python")
    if uses_stmt_fields:
        super_imports.append("stmt_to_python")
    if uses_py_string:
        super_imports.append("py_string")
    if uses_py_bool:
        super_imports.append("py_bool")
    if uses_optional_identifier_fields:
        super_imports.append("optional_str")
    if needs_call_helpers:
        super_imports.extend(["node_to_python", "extract_callee"])

    out.append(f"use super::{{{', '.join(super_imports)}}};")
    if projected_nodes:
        class_imports = ", ".join(
            node.python_projection.class_name for node in projected_nodes
        )
        out.append(f"use super::generated::{{{class_imports}}};")

    append_block(
        out,
        """
        #[derive(Clone, Copy, Debug)]
        #[allow(dead_code)]
        pub(crate) enum ProjectionMode<'a> {
            Generic,
            Typed,
            Fields(&'a [&'a str]),
        }

        impl ProjectionMode<'_> {
            pub(crate) const fn wants_typed(&self) -> bool {
                !matches!(self, ProjectionMode::Generic)
            }
        }
        """,
    )

    append_block(
        out,
        """
        #[allow(unreachable_patterns)]
        pub(crate) fn project_typed_node(
            py: Python<'_>,
            locator: &Locator<'_>,
            node: AnyNodeRef<'_>,
            mode: ProjectionMode<'_>,
            types: ProjectionTypesRef,
        ) -> PyResult<Option<PyObject>> {
            if !mode.wants_typed() {
                return Ok(None);
            }

            match node {
        """,
    )

    if projected_nodes:
        for node in projected_nodes:
            helper_name = f"project_{to_snake_case(node.name)}"
            node_var = node_var_name(node)
            out.append(
                f"AnyNodeRef::{node.name}({node_var}) => {helper_name}(py, locator, {node_var}, types).map(Some),"
            )
        out.append("            _ => Ok(None),")
    else:
        out.append("            _ => Ok(None),")

    append_block(
        out,
        """
            }
        }
        """,
    )

    for node in projected_nodes:
        helper_name = f"project_{to_snake_case(node.name)}"
        node_var = node_var_name(node)
        helper_type = type_path(node)
        kind_enum = kind_enum_name(node)
        kind_variant = node.variant
        repr_format = "{" + f"{node_var}" + ":?}"
        if kind_enum:
            kind_line = f"let kind = {kind_enum}::{kind_variant}.as_str().to_string();"
        else:
            kind_line = f'let kind = "{kind_variant}".to_string();'

        append_block(
            out,
            f"""
            fn {helper_name}(
                py: Python<'_>,
                locator: &Locator<'_>,
                {node_var}: &{helper_type},
                types: ProjectionTypesRef,
            ) -> PyResult<PyObject> {{
                {kind_line}
                let range = {node_var}.range();
                let text = locator.slice(range).to_string();
                let repr_value = format!("{repr_format}");
                let store = super::store::current_store();
                let node_id = store.ensure({node_var});
            """,
        )

        setup_lines, metadata_values, precomputed_eager_fields = (
            projection_special_cases(node, node_var, kind_enum)
        )
        if setup_lines:
            out.append("\n".join(setup_lines))

        field_exprs: list[str] = []
        for field in node.fields or []:
            if node.python_projection.is_eager(field.name):  # type: ignore[union-attr]
                if field.name in precomputed_eager_fields:
                    field_exprs.append(precomputed_eager_fields[field.name])
                    continue
                lines, value_name = eager_field_assignment(node, field)
                out.extend(lines)
                field_exprs.append(value_name)
        for metadata in projection_metadata(node):
            field_exprs.append(metadata_values.get(metadata.name, "py_none(py)"))
        field_exprs.extend(
            [
                "SourceFileHandle::new()",
                "types",
            ]
        )

        base_arg_map = {arg.name: arg.expr for arg in BASE_ARGS}
        required_base_args = [arg for arg in BASE_ARGS if not arg.optional]
        optional_base_args = [arg for arg in BASE_ARGS if arg.optional]
        ordered_required = [base_arg_map[arg.name] for arg in required_base_args]
        ordered_optional = [base_arg_map[arg.name] for arg in optional_base_args]
        all_args = ordered_required + field_exprs + ordered_optional
        args_str = ",\n            ".join(all_args)

        append_block(
            out,
            f"""
                {node.python_projection.class_name}::new_instance(
                    py,
            {args_str},
                )
            }}
            """,
        )


def generate_projection_helpers(ast: Ast) -> list[str]:
    projection = ProjectionModel.from_ast(ast)
    out: list[str] = []
    write_projection_helpers(out, projection)
    return out


def write_projection_helpers_output(root: Path, out: list[str]) -> None:
    out_path = root.joinpath(
        "crates",
        "ruff_linter",
        "src",
        "external",
        "ast",
        "python",
        "projection.rs",
    )
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(rustfmt("\n".join(out)))


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
    python_stub = generate_python_stub(ast)
    write_python_stub_output(root, python_stub)
    projection_bindings = generate_projection_bindings(ast)
    write_projection_bindings_output(root, projection_bindings)
    projection_helpers = generate_projection_helpers(ast)
    write_projection_helpers_output(root, projection_helpers)


if __name__ == "__main__":
    main()
