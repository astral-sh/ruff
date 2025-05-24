# Ruff in some cases added brackets around tuples in some cases; deviating from Black.
# Ensure we don't revert already-applied formats with the fix.
# See https://github.com/astral-sh/ruff/pull/18147
match a, b:
    case [[], []]:
        ...
    case [[], _]:
        ...
