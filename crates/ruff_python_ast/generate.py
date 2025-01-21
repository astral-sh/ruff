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
from typing import Any

import tomllib


def rustfmt(code: str) -> str:
    return check_output(["rustfmt", "--emit=stdout"], input=code, text=True)


def to_snake_case(node: str) -> str:
    """Converts CamelCase to snake_case"""
    return re.sub("([A-Z])", r"_\1", node).lower().lstrip("_")


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
    rustdoc: str | None

    def __init__(self, group_name: str, group: dict[str, Any]) -> None:
        self.name = group_name
        self.owned_enum_ty = group_name
        self.ref_enum_ty = group_name + "Ref"
        self.add_suffix_to_is_methods = group.get("add_suffix_to_is_methods", False)
        self.anynode_is_label = group.get("anynode_is_label", to_snake_case(group_name))
        self.rustdoc = group.get("rustdoc")
        self.nodes = [
            Node(self, node_name, node) for node_name, node in group["nodes"].items()
        ]


@dataclass
class Node:
    name: str
    variant: str
    ty: str

    def __init__(self, group: Group, node_name: str, node: dict[str, Any]) -> None:
        self.name = node_name
        self.variant = node.get("variant", node_name.removeprefix(group.name))
        self.ty = f"crate::{node_name}"


# ------------------------------------------------------------------------------
# Preamble


def write_preamble(out: list[str]) -> None:
    out.append("""
    // This is a generated file. Don't modify it by hand!
    // Run `crates/ruff_python_ast/generate.py` to re-generate the file.
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
    - `TypeParam::visit_source_order`
    - `impl From<TypeParamTypeVar> for TypeParam`
    - `impl Ranged for TypeParamTypeVar`
    - `fn TypeParam::is_type_var() -> bool`

    If the `add_suffix_to_is_methods` group option is true, then the
    `is_type_var` method will be named `is_type_var_type_param`.
    """

    for group in ast.groups:
        out.append("")
        if group.rustdoc is not None:
            out.append(group.rustdoc)
        out.append("#[derive(Clone, Debug, PartialEq, is_macro::Is)]")
        out.append(f"pub enum {group.owned_enum_ty} {{")
        for node in group.nodes:
            if group.add_suffix_to_is_methods:
                is_name = to_snake_case(node.variant + group.name)
                out.append(f'#[is(name = "{is_name}")]')
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

    for node in ast.all_nodes:
        out.append(f"""
            impl ruff_text_size::Ranged for {node.ty} {{
                fn range(&self) -> ruff_text_size::TextRange {{
                    self.range
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
    - `fn TypeParamRef::is_type_var() -> bool`

    The name of each variant can be customized via the `variant` node option. If
    the `add_suffix_to_is_methods` group option is true, then the `is_type_var`
    method will be named `is_type_var_type_param`.
    """

    for group in ast.groups:
        out.append("")
        if group.rustdoc is not None:
            out.append(group.rustdoc)
        out.append("""#[derive(Clone, Copy, Debug, PartialEq, is_macro::Is)]""")
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
    - `fn AnyNodeRef::as_ptr(&self) -> std::ptr::NonNull<()>`
    - `fn AnyNodeRef::visit_preorder(self, visitor &mut impl SourceOrderVisitor)`
    """

    out.append("""
    #[derive(Copy, Clone, Debug, is_macro::Is, PartialEq)]
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
            pub fn visit_preorder<'b, V>(self, visitor: &mut V)
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
# Format and write output


def generate(ast: Ast) -> list[str]:
    out = []
    write_preamble(out)
    write_owned_enum(out, ast)
    write_ref_enum(out, ast)
    write_anynoderef(out, ast)
    write_nodekind(out, ast)
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
