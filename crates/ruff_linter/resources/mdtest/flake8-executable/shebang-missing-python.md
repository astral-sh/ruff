# `shebang-missing-python` (`EXE003`)

Each shebang needs its own Markdown section. Python code blocks in the same section are concatenated,
so only the first shebang would appear at the beginning of the file.

```toml
lint.select = ["EXE003"]
```

## `uv run`

```py
#!/usr/bin/env -S uv run
print("hello world")
```

## `uv` with global flags

`uv` accepts global flags before `run`.

```py
#!/usr/bin/env uv --offline run
print("hello world")
```

## `uv` with the `--color` global flag

```py
#!/usr/bin/env uv --color=auto run
print("hello world")
```

## `uv` with the `--quiet` global flag

```py
#!/usr/bin/env uv --quiet run --script
print("hello world")
```

## `uv tool run`

```py
#!/usr/bin/env uv tool run
print("hello world")
```

## `uv tool run` with `env -S`

```py
#!/usr/bin/env -S uv tool run ruff check --isolated --select EXE003
print("hello world")
```

## `uvx`

```py
#!/usr/bin/env uvx
print("hello world")
```

## `uvx --quiet`

```py
#!/usr/bin/env uvx --quiet
print("hello world")
```

## `uvx` with `env -S`

```py
#!/usr/bin/env -S uvx ruff check --isolated --select EXE003
print("hello world")
```

## `python`

```py
#!/usr/bin/env python3
print("hello world")
```

## Invalid interpreter

A shebang at the beginning of the file is still checked by `EXE003`, even when the interpreter name only resembles `uv run`.

```py
#!/usr/bin/env uv_not_really_run  # error: [shebang-missing-python]
print("this should fail")
```

## Non-Python interpreter

```py
#!/usr/bin/bash  # error: [shebang-missing-python]
print("hello world")
```
