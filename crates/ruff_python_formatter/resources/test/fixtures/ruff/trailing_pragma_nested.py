# Trailing pragma after another comment - the pragma portion should not count
# toward reserved width.

# The expression is long enough that the formatter would break it if the full
# comment width were reserved, but short enough to fit if only the non-pragma
# prefix is reserved.
i = ("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",)  # comment  # noqa: F401
i = ("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",)  # comment  # type: ignore
i = ("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",)  # comment  # pyright: ignore
i = ("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",)  ## noqa: F401

# Plain pragma (no nested comment) - should not reserve any width
i = ("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",)  # noqa: F401
i = ("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",)  # type: ignore

# Not a pragma - should reserve full width
i = ("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",)  # comment  # not a pragma
