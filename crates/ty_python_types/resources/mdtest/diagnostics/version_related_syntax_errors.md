# Version-related syntax error diagnostics

## `match` statement

The `match` statement was introduced in Python 3.10.

### Before 3.10

<!-- snapshot-diagnostics -->

We should emit a syntax error before 3.10.

```toml
[environment]
python-version = "3.9"
```

```py
match 2:  # error: 1 [invalid-syntax] "Cannot use `match` statement on Python 3.9 (syntax was added in Python 3.10)"
    case 1:
        print("it's one")
```

### After 3.10

On or after 3.10, no error should be reported.

```toml
[environment]
python-version = "3.10"
```

```py
match 2:
    case 1:
        print("it's one")
```
