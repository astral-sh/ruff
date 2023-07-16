(a for b in c)

# parens around generator expression not required
len(a for b in c)

# parens around generator expression required
sum((a for b in c), start=0)
