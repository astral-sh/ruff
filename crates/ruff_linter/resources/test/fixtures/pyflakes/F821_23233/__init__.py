# Case 1: Simple relative import
from .foo import Bar
print(foo.Bar)

# Case 2: Nested relative import
from .foo.bar import Baz
print(foo.bar.Baz)
