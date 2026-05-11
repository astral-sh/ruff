"""Regression tests for https://github.com/astral-sh/ruff/issues/20120

`from a.b.c import x` triggers `import a.b.c` as a side effect, which
binds `c` as an attribute of `a.b`. Removing the import would break code
that traverses `a.b.c.<...>` via attribute access, unless another import
in the scope provides the same side effect.
"""


# OK: the `from`-import provides the only path to `snowflake.connector.pandas_tools`.
# Removing it would break `snowflake.connector.pandas_tools.write_pandas` below.
def f():
    import snowflake.connector
    from snowflake.connector.pandas_tools import write_pandas

    snowflake.connector.pandas_tools.write_pandas


# OK: traversing the source-module path itself relies on the side effect.
def f():
    import snowflake.connector
    from snowflake.connector.pandas_tools import write_pandas

    snowflake.connector.pandas_tools


# Error: covered by sibling `import snowflake.connector.pandas_tools`,
# which already provides the same side effect.
def f():
    import snowflake.connector.pandas_tools
    from snowflake.connector.pandas_tools import write_pandas

    snowflake.connector.pandas_tools


# Error: bound name is unused and no reference traverses the source-module path.
def f():
    import snowflake.connector
    from snowflake.connector.pandas_tools import write_pandas

    snowflake.connector


# OK: both `from`-imports are needed; removing both would lose the
# `snowflake.connector.pandas_tools` side effect that `write_pandas` below
# depends on, so neither is flagged.
def f():
    import snowflake.connector
    from snowflake.connector.pandas_tools import write_pandas
    from snowflake.connector.pandas_tools import other

    snowflake.connector.pandas_tools.write_pandas


# OK: top-level `from`-import. `xml.sax.make_parser` traverses through
# `xml.sax`, which is set as an attribute of `xml` by the `from`-import.
def f():
    import xml
    from xml import sax

    xml.sax.make_parser
