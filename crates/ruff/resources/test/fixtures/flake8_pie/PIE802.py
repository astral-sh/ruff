# no error
all((x.id for x in bar))
all(x.id for x in bar)
all(x.id for x in bar)
any(x.id for x in bar)
any({x.id for x in bar})

# PIE 802
any([x.id for x in bar])
all([x.id for x in bar])
