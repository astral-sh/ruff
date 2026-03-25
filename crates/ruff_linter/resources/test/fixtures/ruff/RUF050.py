### Errors (condition removed entirely)

# Simple if with pass
if True:
    pass

# Simple if with ellipsis
if True:
    ...

# Side-effect-free condition (comparison)
import sys
if sys.version_info >= (3, 11):
    pass

# Side-effect-free condition (boolean operator)
if x and y:
    pass

# Nested in function
def nested():
    if a:
        pass

# Single-line form (pass)
if True: pass

# Single-line form (ellipsis)
if True: ...

# Multiple pass statements
if True:
    pass
    pass

# Mixed pass and ellipsis
if True:
    pass
    ...

# Only statement in a with block
with pytest.raises(ValueError, match=msg):
    if obj1:
        pass


### Errors (condition preserved as expression statement)

# Function call
if foo():
    pass

# Method call
if bar.baz():
    pass

# Nested call in boolean operator
if x and foo():
    pass

# Walrus operator with call
if (x := foo()):
    pass

# Walrus operator without call
if (x := y):
    pass

# Only statement in a suite
class Foo:
    if foo():
        pass


### No errors

# Non-empty body
if True:
    bar()

# Body with non-stub statement
if True:
    pass
    foo()

# Has elif clause (handled by RUF047)
if True:
    pass
elif True:
    pass

# Has else clause (handled by RUF047)
if True:
    pass
else:
    pass

# TYPE_CHECKING block (handled by TC005)
from typing import TYPE_CHECKING
if TYPE_CHECKING:
    pass

# Comment in body
if True:
    # comment
    pass

# Inline comment after pass
if True:
    pass  # comment

# Inline comment on if line
if True:  # comment
    pass

# Trailing comment at body indentation
if True:
    pass
    # trailing comment

# Trailing comment deeper than body indentation
if True:
    pass
        # deeper trailing comment
