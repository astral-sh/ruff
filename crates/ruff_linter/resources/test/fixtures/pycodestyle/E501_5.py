# OK - trailing noqa after another comment (88 characters before noqa pragma)
"shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:aaa"  # comment  # noqa: F401

# Error - trailing noqa after another comment (89 characters before noqa pragma)
"shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:aaaa"  # comment  # noqa: F401

# OK - double hash before noqa (80 characters before noqa pragma)
"shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:aaa"  ## noqa: F401

# OK - trailing type: ignore after another comment (88 characters before pragma)
"shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:aaa"  # comment  # type: ignore

# Error - trailing type: ignore after another comment (89 characters before pragma)
"shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:aaaa"  # comment  # type: ignore
