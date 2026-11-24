# `too-many-statements-in-try-clause` (`PLW0717`)

```toml
[lint]
preview = true
select = ["PLW0717"]
```

## Diagnostic range

Avoid marking the entire `try` statement:

```py
# snapshot: too-many-statements-in-try-clause
try:
    call1()
    call2()
    call3()
    call4()
    call5()
    call6()
except:
    ...
```

```snapshot
error[PLW0717]: Try clause contains too many statements (6 > 5)
 --> src/mdtest_snippet.py:2:1
  |
2 | try:
  | ^^^
```

## Context managers

Avoid emitting a diagnostic when the `try` statement is being used like a context manager, i.e. it
catches no exceptions and just ensures that some kind of cleanup is run in a `finally` clause:

```py
try:
    call1()
    call2()
    call3()
    call4()
    call5()
    call6()
finally:
    cleanup()
```
