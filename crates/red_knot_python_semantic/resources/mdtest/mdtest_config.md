This test makes sure that `red_knot_test` correctly parses the TOML configuration blocks and applies
the correct settings hierarchically.

The following configuration will be attached to the *root* section (without any heading):

```toml
[environment]
python-version = "3.10"
```

# Basic

Here, we simply make sure that we pick up the global configuration from the root section:

```py
import sys

reveal_type(sys.version_info[:2] == (3, 10))  # revealed: Literal[True]
```

# Inheritance

## Child

### Grandchild

The same should work for arbitrarily nested sections:

```py
import sys

reveal_type(sys.version_info[:2] == (3, 10))  # revealed: Literal[True]
```

# Overwriting

Here, we make sure that we can overwrite the global configuration in a child section:

```toml
[environment]
python-version = "3.11"
```

```py
import sys

reveal_type(sys.version_info[:2] == (3, 11))  # revealed: Literal[True]
```

# No global state

There is no global state. This section should again use the root configuration:

```py
import sys

reveal_type(sys.version_info[:2] == (3, 10))  # revealed: Literal[True]
```

# Overwriting affects children

Children in this section should all use the section configuration:

```toml
[environment]
python-version = "3.12"
```

## Child

### Grandchild

```py
import sys

reveal_type(sys.version_info[:2] == (3, 12))  # revealed: Literal[True]
```
