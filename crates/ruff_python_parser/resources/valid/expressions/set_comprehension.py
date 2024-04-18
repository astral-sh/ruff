{x for i in ll}
{b for c in d if x in w if y and yy if z}
{a for b in c if d and e for f in j if k > h}
{a for b in c if d and e async for f in j if k > h}
{a for a, b in G}

# Non-parenthesized iter/if for the following expressions aren't allowed, so make sure
# it parses correctly for the parenthesized cases
{x for x in (yield y)}
{x for x in (yield from y)}
{x for x in (lambda y: y)}
{x for x in data if (yield y)}
{x for x in data if (yield from y)}
{x for x in data if (lambda y: y)}
