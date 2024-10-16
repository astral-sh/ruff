# Except star

TODO(Alex): Once we support `sys.version_info` branches, we can set `--target-version=py311` in these tests and the inferred type will just be `BaseExceptionGroup`

## Except\* with BaseException

```py
try:
    x
except* BaseException as e:
    reveal_type(e)  # revealed: Unknown | BaseExceptionGroup
```

## Except\* with specific exception

```py
try:
    x
except* OSError as e:
    # TODO(Alex): more precise would be `ExceptionGroup[OSError]`
    reveal_type(e)  # revealed: Unknown | BaseExceptionGroup
```

## Except\* with multiple exceptions

```py
try:
    x
except* (TypeError, AttributeError) as e:
    #TODO(Alex): more precise would be `ExceptionGroup[TypeError | AttributeError]`.
    reveal_type(e)  # revealed: Unknown | BaseExceptionGroup
```
