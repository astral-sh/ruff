#!/usr/bin/python

from __future__ import annotations
from dataclasses import dataclass
from pathlib import Path
import re
from subprocess import check_output
import tomllib
from typing import Optional


def rustfmt(code: str) -> str:
    return check_output(["rustfmt", "--emit=stdout"], input=code, text=True)


def to_snake_case(node: str) -> str:
    """Converts CamelCase to snake_case"""
    return re.sub("([A-Z])", r"_\1", node).lower().lstrip("_")


# ------------------------------------------------------------------------------
# Read AST description

root = check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip()
ast_path = Path(root).joinpath("crates", "ruff_python_ast", "ast.toml")
out_path = Path(root).joinpath("crates", "ruff_python_ast", "src", "generated.rs")
with ast_path.open("rb") as ast_file:
    ast = tomllib.load(ast_file)

# ------------------------------------------------------------------------------
# Preprocess


@dataclass
class Group:
    name: str
    nodes: list[Node]
    owned_enum_ty: str

    add_suffix_to_is_methods: bool
    comment: Optional[str]

    def __init__(self, group_name, group):
        self.name = group_name
        self.owned_enum_ty = group_name
        self.add_suffix_to_is_methods = group.get("add_suffix_to_is_methods", False)
        self.comment = group.get("comment")
        self.nodes = [
            Node(self, node_name, node) for node_name, node in group["nodes"].items()
        ]


@dataclass
class Node:
    name: str
    variant: str
    ty: str

    def __init__(self, group, node_name, node):
        self.name = node_name
        self.variant = node.get("variant", node_name.removeprefix(group.name))
        self.ty = f"crate::{node_name}"


groups = [Group(group_name, group) for group_name, group in ast.items()]

# ------------------------------------------------------------------------------
# Preamble

out = ["""
// This is a generated file. Don't modify it by hand!
// Run `crates/ruff_python_ast/generate.py` to re-generate the file.
"""]  # fmt: skip

# ------------------------------------------------------------------------------
# Owned enum
#
# For each group, we create an enum that contains an owned copy of a syntax
# node.

for group in groups:
    if group.name == "ungrouped":
        continue

    out.append("")
    if group.comment is not None:
        out.append(group.comment)
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

# ------------------------------------------------------------------------------
# Format and write output

out_path.write_text(rustfmt("\n".join(out)))
