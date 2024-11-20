from foo import __version__
import bar


tuple(map(int, __version__.split(".")))
list(map(int, __version__.split(".")))
tuple(map(int, bar.__version__.split(".")))
list(map(int, bar.__version__.split(".")))

# `sep` passed as keyword argument
for part in map(int, bar.__version__.split(sep=".")):
    print(part)

for part in map(int, __version__.split(sep=".")):
    print(part)

# Comma
tuple(map(int, __version__.split(",")))
list(map(int, __version__.split(",")))
tuple(map(int, bar.__version__.split(",")))
list(map(int, bar.__version__.split(",")))

# Multiple arguments
tuple(map(int, __version__.split(",", 1)))
list(map(int, __version__.split(",", maxsplit = 2)))
tuple(map(int, bar.__version__.split(",", 1)))
list(map(int, bar.__version__.split(",", maxsplit = 2)))
