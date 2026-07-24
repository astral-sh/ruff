# `shebang-not-executable` (`EXE001`)

```toml
lint.select = ["EXE001", "EXE003"]
```

## Pseudo-shebang comments in body

Comments starting with `#!` that are not at the beginning of the file (e.g. inside functions or inline comments) are not treated as valid shebangs and do not trigger `EXE001` or `EXE003`.

```py
def f():
    #! not a shebang — just a comment
    return 1


x = 1  #! inline pseudo-shebang
```

## Invalid interpreter

A shebang at the beginning of the file is still checked by `EXE003`, even when the interpreter name only resembles `uv run`.

```toml
lint.select = ["EXE003"]
```

```py
#!/usr/bin/env uv_not_really_run  # error: [shebang-missing-python]
print("this should fail")
```
