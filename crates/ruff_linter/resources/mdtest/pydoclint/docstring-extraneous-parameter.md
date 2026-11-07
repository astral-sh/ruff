# `docstring-extraneous-parameter` (`DOC102`)

```toml
lint.preview = true
lint.select = ["DOC102"]
lint.pydocstyle.convention = "numpy"
```

## Unknown section headings

An underlined section ends the parameter list even when its name is not a recognized NumPy section.
Neither the heading nor its contents are parameter declarations.

```py
def unknown_section(value):
    """Describe a value.

    Parameters
    ----------
    value : int
        A value.

    Note
    ----
    explanation
    """
    return value


def multiword_section(value):
    """Describe a value.

    Parameters
    ----------
    value : int
        A value.

    Custom Details
    --------------
    metadata
    """
    return value
```

## Equals underlines

Equals underlines mark section boundaries just like dashed underlines.

```py
def equals_underline(value):
    """Describe a value.

    Parameters
    ----------
    value : int
        A value.

    metadata
    ========
    explanation
    """
    return value
```

## Untyped parameters

Parameters without type annotations remain part of the parameter list.

```py
def untyped_parameters(value):
    """Describe a value.

    Parameters
    ----------
    value
    """
    return value
```

## Indented headings in descriptions

An underlined heading inside a parameter description does not end the parameter list.

```py
def indented_heading(value):
    """Describe a value.

    Parameters
    ----------
    value : int
        Note
        ----
        More information about the value.
    """
    return value
```
