# Template strings

(NB: `black` does not support Python 3.14 at the time of this writing)

<!-- blacken-docs:off -->

```toml
[environment]
python-version = "3.14"
```

Template strings, or t-strings, were added in Python 3.14.

They may be specified as literals and are objects of type `string.templatelib.Template`.

## Empty template string

```py
reveal_type(t"")  # revealed: Template
```
