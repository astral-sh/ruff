# Except star

## Except\* with BaseException

```py
try:
    x
except* BaseException as e:
    reveal_type(e)  # revealed: BaseExceptionGroup
```

## Except\* with specific exception

```py
try:
    x
except* OSError as e:
    # TODO(Alex): more precise would be `ExceptionGroup[OSError]`
    reveal_type(e)  # revealed: BaseExceptionGroup
```

## Except\* with multiple exceptions

```py
try:
    x
except* (TypeError, AttributeError) as e:
    # TODO(Alex): more precise would be `ExceptionGroup[TypeError | AttributeError]`.
    reveal_type(e)  # revealed: BaseExceptionGroup
```
