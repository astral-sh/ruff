Scripts with PEP 723 metadata are considered single-file projects. For now, they can configure
`rules` and `analysis`, but we plan to also support dependencies and changing `environment`
settings.

```toml
[environment]
python-version = "3.12"

[rules]
unresolved-reference = "error"

[analysis]
respect-type-ignore-comments = false
```

# Inline settings

A script can change its `rules` and `analysis` settings. In the future, it can also change its
`environment` settings. A script is standalone, it does not inherit any settings from the project
(that's not entirely true today, because scripts still inherit `environment` settings but it's our
end goal).

```py
# /// script
# [tool.ty.rules]
# all = "ignore"
# unresolved-reference = "error"
# [tool.ty.analysis]
# respect-type-ignore-comments = false
# ///

# error: [unresolved-reference]
print(missing)  # type: ignore
```

# A metadata block without `tool.ty`

Scripts with a valid metadata block are considered as their own project, even if the metadata block
does not contain any `tool.ty` section.

```py
# /// script
# dependencies = []
# ///

value: int = "not an int"  # error: [invalid-assignment]
suppressed: int = "not an int"  # type: ignore
```

# Other Python source kinds

Script metadata is also recognized in stubs and extensionless Python files.

## Stub

```pyi
# /// script
# dependencies = []
# ///

value: Missing  # error: [unresolved-reference]
```

## Extensionless file

`script`:

```py
# /// script
# dependencies = []
# ///

# error: [unresolved-reference]
print(missing)
```

# Invalid blocks

Invalid blocks do not establish script isolation, so the project configuration continues to apply.

## Indented opening tag

```py
if True:
    # /// script
    # [tool.ty.rules]
    # unresolved-reference = "ignore"
    # ///
    pass

# error: [unresolved-reference]
print(missing)
```

## Trailing opening tag

```py
value = 1  # /// script
# [tool.ty.rules]
# unresolved-reference = "ignore"
# ///

# error: [unresolved-reference]
print(missing)
```

## Unclosed block

```py
# /// script
# [tool.ty.rules]
# unresolved-reference = "ignore"

# error: [unresolved-reference]
print(missing)
```

## Invalid TOML

```py
# /// script
# [tool.ty.rules
# unresolved-reference = "ignore"
# ///

# error: [unresolved-reference]
print(missing)
```
