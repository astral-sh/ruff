#! /usr/bin/python
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///

"""See CONTRIBUTING.md"""

# %%
from __future__ import annotations

import re
from collections import defaultdict
from pathlib import Path
from subprocess import check_output

import tomllib


def rustfmt(code: str) -> str:
    return check_output(["rustfmt", "--emit=stdout"], input=code, text=True)


# %%
# Read nodes

root = Path(
    check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip(),
)
formatted_by_parent = {
    "Identifier",
    "InterpolatedElement",
    "InterpolatedStringFormatSpec",
    "InterpolatedStringLiteralElement",
}
with (root / "crates" / "ruff_python_ast" / "ast.toml").open("rb") as schema:
    ast = tomllib.load(schema)
    node_groups = [name for name in ast if name != "ungrouped"]
    nodes = [
        node
        for group in ast.values()
        for node in group["nodes"]
        if node not in formatted_by_parent
    ]

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
    return "other"


def to_camel_case(node: str) -> str:
    """Converts PascalCase to camel_case"""
    return re.sub(r"([A-Z])", r"_\1", node).lower().lstrip("_")


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

            impl FormatNodeRule<ruff_python_ast::kind::{node}> for Format{node} {{
                fn fmt_fields(&self, item: {node}<'_>, f: &mut PyFormatter) -> FormatResult<()> {{
                    write!(f, [verbatim_text(item)])
                }}
            }}
            """.strip()

        node_path.write_text(rustfmt(code))

# %%
# Generate `FormatRule` and `IntoFormat`

generated = """//! This is a generated file. Don't modify it by hand! Run `crates/ruff_python_formatter/generate.py` to re-generate the file.
#![allow(unknown_lints, clippy::default_constructed_unit_structs)]

use crate::context::PyFormatContext;
use crate::{AsFormat, FormatNodeRule, IntoFormat, PyFormatter};
use ruff_formatter::{FormatOwnedWithRule, FormatResult, FormatRule};
use ruff_python_ast as ast;

"""
generated += """
macro_rules! impl_as_format {
    ($($node:ident),* $(,)?) => { $(
        impl<C> AsFormat<C> for ast::$node<'_> where Self: IntoFormat<C> {
            type Format<'a> = <Self as IntoFormat<C>>::Format where Self: 'a;
            fn format(&self) -> Self::Format<'_> { (*self).into_format() }
        }
    )* };
}
"""
generated += (
    "impl_as_format!(" + ", ".join([*node_groups, "Identifier", *nodes]) + ");\n"
)
for node in nodes:
    text = f"""
        impl FormatRule<ast::{node}<'_>, PyFormatContext<'_>>
            for crate::{groups[group_for_node(node)]}::{to_camel_case(node)}::Format{node}
        {{
            #[inline]
            fn fmt(&self, node: &ast::{node}<'_>, f: &mut PyFormatter) -> FormatResult<()> {{
                FormatNodeRule::<ast::kind::{node}>::fmt(self, *node, f)
            }}
        }}
        impl<'ast> IntoFormat<PyFormatContext<'ast>> for ast::{node}<'_> {{
            type Format = FormatOwnedWithRule<
                Self,
                crate::{groups[group_for_node(node)]}::{to_camel_case(node)}::Format{node},
                PyFormatContext<'ast>,
            >;
            fn into_format(self) -> Self::Format {{
                FormatOwnedWithRule::new(self, crate::{groups[group_for_node(node)]}::{to_camel_case(node)}::Format{node}::default())
            }}
        }}
    """
    generated += text

out.write_text(rustfmt(generated))
