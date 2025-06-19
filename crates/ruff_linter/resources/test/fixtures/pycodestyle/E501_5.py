# OK (88 characters)
"shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:aaa"  # comment # noqa

# OK (88 characters) - Multiple comments with pragma
"shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:aaa"  # test # type: ignore

# OK (88 characters) - Multiple comments with different pragma
"shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:aaa"  # see issue # noqa: E501

# Error (89 characters) - Multiple comments but pragma not recognized because it's too long
"shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:aaaa" # test # comment

# OK (89 characters) - Multiple comments with pragma (should not trigger E501)
"shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:" + "shape:aaaa" # test # noqa: E501