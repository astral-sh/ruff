# `multiple-starts-ends-with` (`PIE810`): `any(...)` form

Tests for the preview-gated `any(<x>.startswith(p) for p in (...))`
generalisation of `PIE810`. The chained `or` form is covered by the existing
`PIE810.py` fixture.

```toml
[lint]
select = ["PIE810"]
preview = true
```

## Generator expression with a tuple iterable

```py
msg = "Hello, world!"
any(msg.startswith(p) for p in ("Hello", "Hi"))  # snapshot: multiple-starts-ends-with
any(msg.endswith(p) for p in ("!", "?"))  # error: [multiple-starts-ends-with]
```

```snapshot
error[PIE810]: Call `startswith` once with a `tuple`
 --> src/mdtest_snippet.py:2:1
  |
2 | any(msg.startswith(p) for p in ("Hello", "Hi"))  # snapshot: multiple-starts-ends-with
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
help: Merge into a single `startswith` call
1 | msg = "Hello, world!"
  - any(msg.startswith(p) for p in ("Hello", "Hi"))  # snapshot: multiple-starts-ends-with
2 + msg.startswith(("Hello", "Hi"))  # snapshot: multiple-starts-ends-with
3 | any(msg.endswith(p) for p in ("!", "?"))  # error: [multiple-starts-ends-with]
note: This is an unsafe fix and may change runtime behavior
```

## List literal and list / set comprehensions

A list iterable folds into a tuple — `str.startswith` rejects lists at
runtime. `[…]`, `(…)`, and `{…}` comprehension forms are all flagged.

```py
msg = "Hello, world!"
any(msg.startswith(p) for p in ["a", "b", "c"])  # error: [multiple-starts-ends-with]
any([msg.startswith(p) for p in ("x", "y")])  # error: [multiple-starts-ends-with]
any({msg.startswith(p) for p in ("a", "b")})  # error: [multiple-starts-ends-with]
```

## Nested tuple elements are flattened

Mirrors the chained-`or` path: `s.startswith(("a", "b")) or s.startswith("c")`
collapses to `s.startswith(("a", "b", "c"))`, so the generator form does too.

```py
msg = "Hello, world!"
any(msg.startswith(p) for p in (("a", "b"), "c"))  # error: [multiple-starts-ends-with]
```

## Cases that are intentionally not folded

```py
msg = "Hello, world!"

# Bare-name iterable: we can't prove `prefixes` is a tuple at runtime.
prefixes = ("a", "b")
any(msg.startswith(p) for p in prefixes)

# Extra positional arg: `s.startswith(p, 1)` and `s.startswith((p,), 1)` differ
# semantically, so this isn't the same anti-pattern.
any(msg.startswith(p, 1) for p in ("a", "b"))

# Filter clause: the prefixes actually iterated depend on the filter at
# runtime, so we can't enumerate them statically.
any(msg.startswith(p) for p in ("a", "b") if p != "skip")

# The call argument is not the loop variable.
any(msg.startswith(other) for p in ("a", "b"))

# `all` is not equivalent: `all(s.startswith(p) for p in ())` is `True` but
# `s.startswith(())` is `False`.
all(msg.startswith(p) for p in ("a", "b"))

# Receiver isn't a bare name — folding `f().startswith(...)` would change the
# number of `f()` calls (N lazy calls under `any` short-circuiting → 1 eager).
class Wrap:
    msg = "Hello, world!"

w = Wrap()
any(w.msg.startswith(p) for p in ("a", "b"))

def _get():
    return "hello"

any(_get().startswith(p) for p in ("a", "b"))

# An iterable element is already a tuple: folding would yield
# `msg.startswith((y, "c"))` which `str.startswith` rejects.
y = ("a", "b")
any(msg.startswith(p) for p in (y, "c"))
```
