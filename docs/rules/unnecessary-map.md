# unnecessary-map (C417)

Derived from the **flake8-comprehensions** linter.

Autofix is always available.

### What it does
Checks for unnecessary `map` usage.

## Why is this bad?
`map(func, iterable)` has great performance when func is a built-in function, and it
makes sense if your function already has a name. But if your func is a lambda, it’s
faster to use a generator expression or a comprehension, as it avoids the function call
overhead. For example:

Rewrite `map(lambda x: x + 1, iterable)` to `(x + 1 for x in iterable)`
Rewrite `map(lambda item: get_id(item), items)` to `(get_id(item) for item in items)`
Rewrite `list(map(lambda num: num * 2, nums))` to `[num * 2 for num in nums]`
Rewrite `set(map(lambda num: num % 2 == 0, nums))` to `{num % 2 == 0 for num in nums}`
Rewrite `dict(map(lambda v: (v, v ** 2), values))` to `{v : v ** 2 for v in values}`