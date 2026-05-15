###
# Fixable on Python 3.11+: argument's quote does not collide with the outer string's.
###

'Magic wand: {}'.format(bag["wand"])

'{}'.format(len(l) * "─")

'Hello {}'.format("world")

###
# Fixable only on Python 3.12+ (PEP 701): same-quote nesting or multi-line interpolation.
###

"Hello {}".format("world")

"Magic wand: {}".format(bag["wand"])

"{}".format(len(l) * "─")

"{}".format(
    [
        1,
        2,
        3,
    ]
)

"{a}".format(
    a=[
        1,
        2,
        3,
    ]
)
