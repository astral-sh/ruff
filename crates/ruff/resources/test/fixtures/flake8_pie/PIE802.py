# no error
all((x.id for x in bar))
all(x.id for x in bar)
all(x.id for x in bar)
any(x.id for x in bar)
any({x.id for x in bar})

# PIE 802
any([x.id for x in bar])
all([x.id for x in bar])
any(  # first comment
    [x.id for x in bar],  # second comment
)  # third comment
all(  # first comment
    [x.id for x in bar],  # second comment
)  # third comment
