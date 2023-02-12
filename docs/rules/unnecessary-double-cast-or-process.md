# unnecessary-double-cast-or-process (C414)

Derived from the **flake8-comprehensions** linter.

Autofix is always available.

## What it does
Checks for unnecessary `list`, `reversed`, `set`, `sorted`, and `tuple`
call within `list`, `set`, `sorted`, and `tuple` calls.

## Why is this bad?
It's unnecessary to double-cast or double-process iterables by wrapping
the listed functions within an additional `list`, `set`, `sorted`, or
`tuple` call. Doing so is redundant and can be confusing for readers.

## Examples
```python
list(tuple(iterable))
```

Use instead:
```python
list(iterable)
```

This rule applies to a variety of functions, including `list`, `reversed`,
`set`, `sorted`, and `tuple`. For example:
* Instead of `list(list(iterable))`, use `list(iterable)`.
* Instead of `list(tuple(iterable))`, use `list(iterable)`.
* Instead of `tuple(list(iterable))`, use `tuple(iterable)`.
* Instead of `tuple(tuple(iterable))`, use `tuple(iterable)`.
* Instead of `set(set(iterable))`, use `set(iterable)`.
* Instead of `set(list(iterable))`, use `set(iterable)`.
* Instead of `set(tuple(iterable))`, use `set(iterable)`.
* Instead of `set(sorted(iterable))`, use `set(iterable)`.
* Instead of `set(reversed(iterable))`, use `set(iterable)`.
* Instead of `sorted(list(iterable))`, use `sorted(iterable)`.
* Instead of `sorted(tuple(iterable))`, use `sorted(iterable)`.
* Instead of `sorted(sorted(iterable))`, use `sorted(iterable)`.
* Instead of `sorted(reversed(iterable))`, use `sorted(iterable)`.