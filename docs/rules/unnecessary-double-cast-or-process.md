# unnecessary-double-cast-or-process (C414)

Derived from the **flake8-comprehensions** linter.

Autofix is always available.

## What it does
Checks for unnecessary `list/reversed/set/sorted/tuple` call within `list/set/sorted/tuple`.

## Why is this bad?
It's unnecessary to double-cast or double-process iterables by wrapping the listed functions within `list/set/sorted/tuple`.

## Examples
Rewrite `list(list(iterable))` as `list(iterable)`
Rewrite `list(tuple(iterable))` as `list(iterable)`
Rewrite `tuple(list(iterable))` as `tuple(iterable)`
Rewrite `tuple(tuple(iterable))` as `tuple(iterable)`
Rewrite `set(set(iterable))` as `set(iterable)`
Rewrite `set(list(iterable))` as `set(iterable)`
Rewrite `set(tuple(iterable))` as `set(iterable)`
Rewrite `set(sorted(iterable))` as `set(iterable)`
Rewrite `set(reversed(iterable))` as `set(iterable)`
Rewrite `sorted(list(iterable))` as `sorted(iterable)`
Rewrite `sorted(tuple(iterable))` as `sorted(iterable)`
Rewrite `sorted(sorted(iterable))` as `sorted(iterable)`
Rewrite `sorted(reversed(iterable))` as `sorted(iterable)`