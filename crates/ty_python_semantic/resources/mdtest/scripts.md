Scripts with PEP 723 metadata inherit the enclosing project's ty configuration unless they have
their own `[tool.ty]` table. Each script has its own first-party roots and declares its own
dependencies.

```toml
[environment]
python-version = "3.12"

[rules]
unresolved-reference = "error"

[analysis]
respect-type-ignore-comments = false
```

# Inline settings

An inline `[tool.ty]` table replaces the enclosing project's ty configuration for that script. Like
a `ty.toml`, it can configure rules, analysis settings, and the Python environment.

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

This project disables `type: ignore` comments, so the script reports both assignments.

```py
# /// script
# dependencies = []
# ///

value: int = "not an int"  # error: [invalid-assignment]
# error: [invalid-assignment]
also_invalid: int = "not an int"  # type: ignore
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
