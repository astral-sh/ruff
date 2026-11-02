# SIM109
if a == b or a == c:
    d

# SIM109
if (a == b or a == c) and None:
    d

# SIM109
if a == b or a == c or None:
    d

# SIM109
# Known limitation (see #18945): the unmatched operand falls *between* the
# two merged comparisons, so it's moved after the merged `in` comparison
# instead of staying in its original position.
if a == b or None or a == c:
    d

# SIM109
# Regression test for #18945: the unmatched operand precedes both merged
# comparisons, so it must stay first in the fix instead of being moved
# after the merged `in` comparison.
if None or a == b or a == c:
    d

# OK
if a in (b, c):
    d

# OK
if a == b or a == c():
    d

# OK
if (
    a == b
    # This comment prevents us from raising SIM109
    or a == c
):
    d
