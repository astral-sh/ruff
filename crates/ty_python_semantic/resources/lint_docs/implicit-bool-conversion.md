## What it does

Detects truthiness checks of optional values whose non-`None` part can also be false, in conditions,
assertions, comprehension filters, and boolean operations.

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

This rule is disabled by default because combining `None` with other falsy values can be
intentional. It flags types such as `int | None`, `list[str] | None`, and `bool | None`.
Non-optional values and optional values whose non-`None` part is always truthy, such as
`re.Match[str] | None`, are allowed. Unions containing `Any` or `Unknown` are also allowed.

Unlike `redundant-condition` and `redundant-condition-strict`, this rule does not require the
condition to be always true or always false.
