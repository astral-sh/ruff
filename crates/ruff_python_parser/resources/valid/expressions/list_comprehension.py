x = [y for y in (1, 2, 3)]

[x for i in range(5)]
[b for c in d if x in w if y and yy if z]
[a for b in c if d and e for f in j if k > h]
[a for b in c if d and e async for f in j if k > h]
[1 for i in x in a]
[a for a, b in G]
[
    await x for a, b in C
]
[i for i in await x if entity is not None]
[x for x in (l if True else L) if T]
[i for i in (await x if True else X) if F]
[i for i in await (x if True else X) if F]
[f for f in c(x if True else [])]

# Non-parenthesized iter/if for the following expressions aren't allowed, so make sure
# it parses correctly for the parenthesized cases
[x for x in (yield y)]
[x for x in (yield from y)]
[x for x in (lambda y: y)]
[x for x in data if (yield y)]
[x for x in data if (yield from y)]
[x for x in data if (lambda y: y)]
