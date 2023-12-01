# OK (88 characters)
"shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:aaa"  # type: ignore

# OK (88 characters)
"shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:aaa"# type: ignore

# OK (88 characters)
"shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:aaa"    # type: ignore

# Error (89 characters)
"shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:aaaa"  # type: ignore
