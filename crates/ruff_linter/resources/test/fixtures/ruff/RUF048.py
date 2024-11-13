__version__ = (0, 1, 0)


tuple(map(int, __version__.split(".")))
list(map(int, __version__.split(".")))

# Comma
tuple(map(int, __version__.split(",")))
list(map(int, __version__.split(",")))

# Multiple arguments
tuple(map(int, __version__.split(".", 1)))
list(map(int, __version__.split(".", maxsplit=2)))
