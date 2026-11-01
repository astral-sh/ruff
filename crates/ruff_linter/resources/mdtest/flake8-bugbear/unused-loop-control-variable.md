# `unused-loop-control-variable` (`B007`)

Enable both B007 and PLW2901 to ensure that reusing an outer loop target in a nested loop produces
only the PLW2901 diagnostic.

```toml
[lint]
select = ["B007", "PLW2901"]
```

```py
for nested_loop_value in ls:
    for nested_loop_value in otherls:  # error: [redefined-loop-name]
        print(nested_loop_value)
```

Conditional deletion should retain B007's existing conservative behavior.

```py
for deleted_value in ls:
    if cond:
        del deleted_value
```
