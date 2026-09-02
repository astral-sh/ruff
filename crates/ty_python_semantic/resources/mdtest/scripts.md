Scripts with PEP 723 metadata are considered single-file projects. They can configure `rules`,
`analysis`, and their Python environment independently of the enclosing project. Dependencies are
resolved from existing Python environments; ty does not install them.

```toml
[environment]
python-version = "3.12"

[rules]
unresolved-reference = "error"

[analysis]
respect-type-ignore-comments = false
```

# Inline settings

A script can change its `rules`, `analysis`, and `environment` settings. A script does not inherit
the enclosing project's configuration or Python environment, but can use an activated or explicitly
configured environment. First-party imports require explicitly configured source roots or extra
search paths.

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

# Valid blocks after invalid opening tags

Invalid opening tags do not prevent a later valid metadata block from being recognized.

```py
value = 1  # /// script
# [tool.ty.rules]
# unresolved-reference = "error"
# ///

# /// script invalid
# [tool.ty.rules]
# unresolved-reference = "error"
# ///

# /// script
# [tool.ty.rules]
# unresolved-reference = "ignore"
# ///

print(missing)
```

# Valid blocks after unclosed blocks

An earlier unclosed block does not prevent a later valid metadata block from being recognized.

```py
# /// script
# [tool.ty.rules]
# unresolved-reference = "error"
value = 1

# /// script
# [tool.ty.rules]
# unresolved-reference = "ignore"
# ///

print(missing)
```
