{y for y in (1, 2, 3)}
{x1: x2 for y in z}
{x + 1: 'x' for i in range(5)}
{b: c * 2 for c in d if x in w if y and yy if z}
{a: a ** 2 for b in c if d and e for f in j if k > h}
{a: b for b in c if d and e async for f in j if k > h}
{a: a for b, c in d}

# Non-parenthesized iter/if for the following expressions aren't allowed, so make sure
# it parses correctly for the parenthesized cases
{x: y for x in (yield y)}
{x: y for x in (yield from y)}
{x: y for x in (lambda y: y)}
{x: y for x in data if (yield y)}
{x: y for x in data if (yield from y)}
{x: y for x in data if (lambda y: y)}
