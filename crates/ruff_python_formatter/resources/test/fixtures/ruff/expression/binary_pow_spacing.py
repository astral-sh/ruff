# No spacing
5 ** 5
5.0 ** 5.0
1e5 ** 2e5
True ** True
False ** False
None ** None

# Space
"a" ** "b"

# Simple power operators remain unspaced when the expression wraps.
result = 1**1**1**1**1**1**1**1**1**1**1**1**1**1**1**1**1**1**1**1**1**1**1**1**1**1**1**1**1**1

# Comments remain attached when a power expression wraps.
result = (
    base **  # exponent
    exponent
)
