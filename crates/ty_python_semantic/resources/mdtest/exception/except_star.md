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
    # TODO: more precise would be `ExceptionGroup[OSError]` --Alex
    # (needs homogeneous tuples + generics)
    reveal_type(e)  # revealed: BaseExceptionGroup[BaseException]
```

## `except*` with multiple exceptions

```py
try:
    help()
except* (TypeError, AttributeError) as e:
    # TODO: more precise would be `ExceptionGroup[TypeError | AttributeError]` --Alex
    # (needs homogeneous tuples + generics)
    reveal_type(e)  # revealed: BaseExceptionGroup[BaseException]
```

## `except*` with mix of `Exception`s and `BaseException`s

```py
try:
    help()
except* (KeyboardInterrupt, AttributeError) as e:
    # TODO: more precise would be `BaseExceptionGroup[KeyboardInterrupt | AttributeError]` --Alex
    reveal_type(e)  # revealed: BaseExceptionGroup[BaseException]
```

## Invalid `except*` handlers

```py
try:
    help()
except* 3 as e:  # error: [invalid-exception-caught]
    reveal_type(e)  # revealed: BaseExceptionGroup[BaseException]

try:
    help()
except* (AttributeError, 42) as e:  # error: [invalid-exception-caught]
    reveal_type(e)  # revealed: BaseExceptionGroup[BaseException]
```
