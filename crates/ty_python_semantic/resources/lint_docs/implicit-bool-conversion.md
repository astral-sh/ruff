## What it does

Detects implicit conversions of non-boolean values to `bool` in conditions, assertions,
comprehension filters, and boolean operations.

## Why is this bad?

A truthiness test can conflate missing data with a valid zero or empty container. Requiring an
explicit check makes the intended meaning clear:

```py
def process(limit: int | None):
    if not limit:  # error: [implicit-bool-conversion]
        print("No limit specified")
```

If zero is a valid limit, use `if limit is None` instead. If testing truthiness is intentional, an
explicit conversion such as `if not bool(limit)` is allowed.

This rule is disabled by default because implicit truthiness tests are idiomatic Python. It also
flags intentional checks such as `if items` for a list. Values assignable to `bool`, including `Any`
and `Unknown`, are allowed.

Unlike `redundant-condition` and `redundant-condition-strict`, this rule does not require the
condition to be always true or always false.
