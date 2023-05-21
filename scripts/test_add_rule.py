#!/usr/bin/env python3
"""Test add_rule on each linter."""
from _utils import get_linters
from add_rule import add_rule

if __name__ == "__main__":
    for index, linter in enumerate(get_linters()):
        add_rule(
            name=f"DoTheThing{index}",
            prefix="Z",
            code=str(index),
            linter=linter,
            should_create=False,
        )
