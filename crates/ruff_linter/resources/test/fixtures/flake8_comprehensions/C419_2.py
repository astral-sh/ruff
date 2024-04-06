# no lint if shadowed
def all(x): pass
all([x.id for x in bar])
