#! /usr/bin/python

"""See CONTRIBUTING.md"""

# %%
from __future__ import annotations

import re
from collections import defaultdict
from pathlib import Path
from subprocess import check_output


def rustfmt(code: str) -> str:
    return check_output(["rustfmt", "--emit=stdout"], input=code, text=True)


# %%
# Read nodes

root = Path(
    check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip(),
)
nodes_file = (
    root.joinpath("crates")
    .joinpath("ruff_python_ast")
    .joinpath("src")
    .joinpath("generated.rs")
    .read_text()
)
node_lines = (
    nodes_file.split("pub enum AnyNodeRef<'a> {")[1].split("}")[0].strip().splitlines()
)
nodes = []
for node_line in node_lines:
    node = re.search(r"crate::([A-Za-z0-9_]+)", node_line).group(1)
    # `FString` has a custom implementation while the formatting for
    # `FStringLiteralElement`, `FStringFormatSpec` and `FStringExpressionElement` are
    # handled by the `FString` implementation.
    if node in (
        "InterpolatedStringLiteralElement",
        "InterpolatedElement",
        "InterpolatedStringFormatSpec",
        "Identifier",
    ):
        continue
    nodes.append(node)
print(nodes)

ast_sources = (
    nodes_file
    + root.joinpath("crates", "ruff_python_ast", "src", "nodes.rs").read_text()
)
generic_nodes = {
    node
    for node in nodes
    if re.search(rf"pub (?:struct|enum) {node}<'ast>", ast_sources)
}


def ast_type(node: str, lifetime: str = "'ast") -> str:
    if node in generic_nodes:
        return f"ast::{node}<{lifetime}>"
    return f"ast::{node}"


# %%
# Generate newtypes with dummy FormatNodeRule implementations

out = (
    root.joinpath("crates")
    .joinpath("ruff_python_formatter")
    .joinpath("src")
    .joinpath("generated.rs")
)
src = root.joinpath("crates").joinpath("ruff_python_formatter").joinpath("src")

nodes_grouped = defaultdict(list)
# We rename because mod is a keyword in rust
groups = {
    "mod": "module",
    "expr": "expression",
    "stmt": "statement",
    "pattern": "pattern",
    "type_param": "type_param",
    "other": "other",
}


def group_for_node(node: str) -> str:
    for group in groups:
        if node.startswith(group.title().replace("_", "")):
            return group
    else:
        return "other"


def to_camel_case(node: str) -> str:
    """Converts PascalCase to camel_case"""
    return re.sub("([A-Z])", r"_\1", node).lower().lstrip("_")


for node in nodes:
    nodes_grouped[group_for_node(node)].append(node)

for group, group_nodes in nodes_grouped.items():
    # These conflict with the manually content of the mod.rs files
    # src.joinpath(groups[group]).mkdir(exist_ok=True)
    # mod_section = "\n".join(
    #     f"pub(crate) mod {to_camel_case(node)};" for node in group_nodes
    # )
    # src.joinpath(groups[group]).joinpath("mod.rs").write_text(rustfmt(mod_section))
    for node in group_nodes:
        node_path = src.joinpath(groups[group]).joinpath(f"{to_camel_case(node)}.rs")
        # Don't override existing manual implementations
        if node_path.exists():
            continue

        code = f"""
            use ruff_formatter::write;
            use ruff_python_ast::{node};
            use crate::verbatim_text;
            use crate::prelude::*;

            #[derive(Default)]
            pub struct Format{node};

            impl FormatNodeRule<{node}> for Format{node} {{
                fn fmt_fields(&self, item: &{node}, f: &mut PyFormatter) -> FormatResult<()> {{
                    write!(f, [verbatim_text(item)])
                }}
            }}
            """.strip()

        node_path.write_text(rustfmt(code))

# %%
# Generate `FormatRule`, `AsFormat` and `IntoFormat`

generated = """//! This is a generated file. Don't modify it by hand! Run `crates/ruff_python_formatter/generate.py` to re-generate the file.
#![allow(unknown_lints, clippy::default_constructed_unit_structs)]

use crate::context::PyFormatContext;
use crate::{AsFormat, FormattableNode, FormatNodeRule, IntoFormat, PyFormatter};
use ruff_formatter::{FormatOwnedWithRule, FormatRefWithRule, FormatResult, FormatRule};
use ruff_python_ast as ast;

"""
for node in nodes:
    node_type = ast_type(node)
    node_type_anon = ast_type(node, "'_")
    node_impl_generics = "<'ast>" if node in generic_nodes else ""
    context_impl_generics = (
        "<'ast, 'context>" if node in generic_nodes else "<'context>"
    )
    text = f"""
        impl FormattableNode for {node_type_anon} {{
            fn as_any_node_ref(&self) -> ast::AnyNodeRef<'_> {{
                self.into()
            }}
        }}
        impl{node_impl_generics} FormatRule<{node_type}, PyFormatContext<'_>>
            for crate::{groups[group_for_node(node)]}::{to_camel_case(node)}::Format{node}
        {{
            #[inline]
            fn fmt(
                &self,
                node: &{node_type},
                f: &mut PyFormatter,
            ) -> FormatResult<()> {{
                FormatNodeRule::<{node_type}>::fmt(self, node, f)
            }}
        }}
        impl{context_impl_generics} AsFormat<PyFormatContext<'context>> for {node_type} {{
            type Format<'a> = FormatRefWithRule<
                'a,
                {node_type},
                crate::{groups[group_for_node(node)]}::{to_camel_case(node)}::Format{node},
                PyFormatContext<'context>,
            >
            where
                Self: 'a;
            fn format(&self) -> Self::Format<'_> {{
                FormatRefWithRule::new(
                    self,
                    crate::{groups[group_for_node(node)]}::{to_camel_case(node)}::Format{node}::default(),
                )
            }}
        }}
        impl{context_impl_generics} IntoFormat<PyFormatContext<'context>> for {node_type} {{
            type Format = FormatOwnedWithRule<
                {node_type},
                crate::{groups[group_for_node(node)]}::{to_camel_case(node)}::Format{node},
                PyFormatContext<'context>,
            >;
            fn into_format(self) -> Self::Format {{
                FormatOwnedWithRule::new(
                    self,
                    crate::{groups[group_for_node(node)]}::{to_camel_case(node)}::Format{node}::default(),
                )
            }}
        }}
    """
    generated += text

out.write_text(rustfmt(generated))
