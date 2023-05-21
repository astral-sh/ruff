#!/usr/bin/env python3
"""Test add_rule on each linter."""
from _utils import get_linters
from add_rule import add_rule

if __name__ == "__main__":
    for index, (linter, (_, use_deprecated_rules_arch)) in enumerate(
        get_linters().items(),
    ):
        if use_deprecated_rules_arch:
            print(f"'{linter}' use a deprecated rules architecture, skip adding rule.")
            continue
        add_rule(
            name=f"DoTheThing{index}",
            prefix="Z",
            code=str(index),
            linter=linter,
            use_deprecated_rules_arch=use_deprecated_rules_arch,
        )
