(a for b in c)

# parens around generator expression not required
len(a for b in c)

# parens around generator expression required
sum((a for b in c), start=0)

# parens not needed but keep since they are in the source
f((1 for _ in a))

# make sure source parenthesis detection isn't fooled by these
f((1) for _ in (a))

# combination of the two above
f(((1) for _ in (a)))
