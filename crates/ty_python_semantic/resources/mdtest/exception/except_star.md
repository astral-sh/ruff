# `except*`

`except*` is only available in Python 3.11 and later:

```toml
[environment]
python-version = "3.11"
```

## `except*` with `BaseException`

```py
try:
    help()
except* BaseException as e:
    reveal_type(e)  # revealed: BaseExceptionGroup[BaseException]
```

## `except*` with specific exception

```py
try:
    help()
except* OSError as e:
    reveal_type(e)  # revealed: ExceptionGroup[OSError]
```

## `except*` with multiple exceptions

```py
try:
    help()
except* (TypeError, AttributeError) as e:
    reveal_type(e)  # revealed: ExceptionGroup[TypeError | AttributeError]
```

## `except*` with mix of `Exception`s and `BaseException`s

```py
try:
    help()
except* (KeyboardInterrupt, AttributeError) as e:
    reveal_type(e)  # revealed: BaseExceptionGroup[KeyboardInterrupt | AttributeError]
```

## Invalid `except*` handlers

```py
try:
    help()
except* 3 as e:  # error: [invalid-exception-caught]
    reveal_type(e)  # revealed: BaseExceptionGroup[Unknown]

try:
    help()
except* (AttributeError, 42) as e:  # error: [invalid-exception-caught]
    reveal_type(e)  # revealed: BaseExceptionGroup[AttributeError | Unknown]
```
