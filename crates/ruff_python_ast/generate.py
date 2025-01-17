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
    ref_enum_ty: str

    add_suffix_to_is_methods: bool
    anynode_is_label: str
    rustdoc: str | None

    def __init__(self, group_name: str, group: dict[str, Any]) -> None:
        self.name = group_name
        self.owned_enum_ty = group_name
        self.ref_enum_ty = group.get("ref_enum_ty", group_name + "Ref")
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

for group in groups:
    for node in group.nodes:
        out.append(f"""
        impl ruff_text_size::Ranged for {node.ty} {{
            fn range(&self) -> ruff_text_size::TextRange {{
                self.range
            }}
        }}
        """)

for group in groups:
    if group.name == "ungrouped":
        continue
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
            f"""{group.owned_enum_ty}::{node.variant}(node) => node.visit_source_order(visitor),"""
        )
    out.append("""
                }
            }
        }
    """)

# ------------------------------------------------------------------------------
# Ref enum
#
# For each group, we create an enum that contains a reference to a syntax node.

for group in groups:
    if group.name == "ungrouped":
        continue

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
            f"""{group.owned_enum_ty}::{node.variant}(node) => {group.ref_enum_ty}::{node.variant}(node),"""
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
# AnyNode

out.append("""
#[derive(Clone, Debug, is_macro::Is, PartialEq)]
pub enum AnyNode {
""")
for group in groups:
    for node in group.nodes:
        out.append(f"{node.name}({node.ty}),")
out.append("""
}
""")

for group in groups:
    if group.name != "ungrouped":
        out.append(f"""
        impl From<{group.owned_enum_ty}> for AnyNode {{
            fn from(node: {group.owned_enum_ty}) -> AnyNode {{
                match node {{
        """)
        for node in group.nodes:
            out.append(
                f"""{group.owned_enum_ty}::{node.variant}(node) => AnyNode::{node.name}(node),"""
            )
        out.append("""
                }
            }
        }
        """)

    for node in group.nodes:
        out.append(f"""
        impl From<{node.ty}> for AnyNode {{
            fn from(node: {node.ty}) -> AnyNode {{
                AnyNode::{node.name}(node)
            }}
        }}
        """)

out.append("""
    impl ruff_text_size::Ranged for AnyNode {
        fn range(&self) -> ruff_text_size::TextRange {
            match self {
""")
for group in groups:
    for node in group.nodes:
        out.append(f"""AnyNode::{node.name}(node) => node.range(),""")
out.append("""
            }
        }
    }
""")

for group in groups:
    if group.name == "ungrouped":
        continue
    out.append(f"""
    impl AnyNode {{
        pub fn {group.anynode_is_label}(self) -> Option<{group.owned_enum_ty}> {{
            match self {{
    """)
    for node in group.nodes:
        out.append(
            f"""AnyNode::{node.name}(node) => Some({group.owned_enum_ty}::{node.variant}(node)),"""
        )
    out.append("""
                _ => None,
            }
        }
    }
    """)

for group in groups:
    if group.name == "ungrouped":
        continue
    out.append(f"""
    impl AnyNode {{
        pub const fn is_{group.anynode_is_label}(&self) -> bool {{
            matches!(self,
    """)
    for i, node in enumerate(group.nodes):
        if i > 0:
            out.append("|")
        out.append(f"""AnyNode::{node.name}(_)""")
    out.append("""
            )
        }
    }
    """)

out.append("""
impl AnyNode {
    pub const fn as_ref(&self) -> AnyNodeRef {
        match self {
""")
for group in groups:
    for node in group.nodes:
        out.append(f"""AnyNode::{node.name}(node) => AnyNodeRef::{node.name}(node),""")
out.append("""
        }
    }
}
""")

# ------------------------------------------------------------------------------
# AnyNodeRef

out.append("""
#[derive(Copy, Clone, Debug, is_macro::Is, PartialEq)]
pub enum AnyNodeRef<'a> {
""")
for group in groups:
    for node in group.nodes:
        out.append(f"""{node.name}(&'a {node.ty}),""")
out.append("""
}
""")

for group in groups:
    if group.name != "ungrouped":
        out.append(f"""
        impl<'a> From<&'a {group.owned_enum_ty}> for AnyNodeRef<'a> {{
            fn from(node: &'a {group.owned_enum_ty}) -> AnyNodeRef<'a> {{
                match node {{
        """)
        for node in group.nodes:
            out.append(
                f"""{group.owned_enum_ty}::{node.variant}(node) => AnyNodeRef::{node.name}(node),"""
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
                f"""{group.ref_enum_ty}::{node.variant}(node) => AnyNodeRef::{node.name}(node),"""
            )
        out.append("""
                }
            }
        }
        """)

    for node in group.nodes:
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
for group in groups:
    for node in group.nodes:
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
for group in groups:
    for node in group.nodes:
        out.append(
            f"""AnyNodeRef::{node.name}(node) => std::ptr::NonNull::from(*node).cast(),"""
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
for group in groups:
    for node in group.nodes:
        out.append(
            f"""AnyNodeRef::{node.name}(node) => node.visit_source_order(visitor),"""
        )
out.append("""
            }
        }
    }
""")

for group in groups:
    if group.name == "ungrouped":
        continue
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

out.append("""
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum NodeKind {
""")
for group in groups:
    for node in group.nodes:
        out.append(f"""{node.name},""")
out.append("""
}
""")

out.append("""
impl AnyNode {
    pub const fn kind(&self) -> NodeKind {
        match self {
""")
for group in groups:
    for node in group.nodes:
        out.append(f"""AnyNode::{node.name}(_) => NodeKind::{node.name},""")
out.append("""
        }
    }
}
""")

out.append("""
impl AnyNodeRef<'_> {
    pub const fn kind(self) -> NodeKind {
        match self {
""")
for group in groups:
    for node in group.nodes:
        out.append(f"""AnyNodeRef::{node.name}(_) => NodeKind::{node.name},""")
out.append("""
        }
    }
}
""")

# ------------------------------------------------------------------------------
# AstNode

for group in groups:
    if group.name != "ungrouped":
        out.append(f"""
        impl crate::AstNode for {group.owned_enum_ty} {{
            type Ref<'a> = {group.ref_enum_ty}<'a>;

            fn cast(node: AnyNode) -> Option<Self>
            where
                Self: Sized,
            {{
                match node {{
        """)
        for node in group.nodes:
            out.append(
                f"""AnyNode::{node.name}(node) => Some({group.owned_enum_ty}::{node.variant}(node)),"""
            )
        out.append("""
                    _ => None,
                }
            }

            fn cast_ref(node: AnyNodeRef) -> Option<Self::Ref<'_>> {
                match node {
        """)
        for node in group.nodes:
            out.append(
                f"""AnyNodeRef::{node.name}(node) => Some({group.ref_enum_ty}::{node.variant}(node)),"""
            )
        out.append("""
                    _ => None,
                }
            }

            fn can_cast(kind: NodeKind) -> bool {
                matches!(kind,
        """)
        for i, node in enumerate(group.nodes):
            if i > 0:
                out.append("|")
            out.append(f"""NodeKind::{node.name}""")
        out.append("""
                )
            }

            fn as_any_node_ref(&self) -> AnyNodeRef {
                AnyNodeRef::from(self)
            }

            fn into_any_node(self) -> AnyNode {
                AnyNode::from(self)
            }
        }
        """)

    for node in group.nodes:
        out.append(f"""
        impl crate::AstNode for {node.ty} {{
            type Ref<'a> = &'a Self;

            fn cast(kind: AnyNode) -> Option<Self>
            where
                Self: Sized,
            {{
                if let AnyNode::{node.name}(node) = kind {{
                    Some(node)
                }} else {{
                    None
                }}
            }}

            fn cast_ref(kind: AnyNodeRef) -> Option<&Self> {{
                if let AnyNodeRef::{node.name}(node) = kind {{
                    Some(node)
                }} else {{
                    None
                }}
            }}

            fn can_cast(kind: NodeKind) -> bool {{
                matches!(kind, NodeKind::{node.name})
            }}

            fn as_any_node_ref(&self) -> AnyNodeRef {{
                AnyNodeRef::from(self)
            }}

            fn into_any_node(self) -> AnyNode {{
                AnyNode::from(self)
            }}
        }}
        """)

# ------------------------------------------------------------------------------
# Format and write output

out_path.write_text(rustfmt("\n".join(out)))
