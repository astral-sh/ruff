The fuzzer job on <https://github.com/astral-sh/ruff/pull/20962> revealed the following panic, which
should eventually be fixed. It is a case where the type flips back and forth between `Unknown` and
`list[Unknown]`, until the cycle limit is reached.

<!-- expect-panic: too many cycle iterations -->

```py
x
[0 for _2 in [] for _3 in x]
y = [x for _1 in z] + []
x = y
```
