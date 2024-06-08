# Double star expression starts with bitwise OR precedence. Make sure we don't parse
# the ones which are higher than that.

{**x := 1}
{a: 1, **x if True else y}
{**lambda x: x, b: 2}
{a: 1, **x or y}
{**x and y, b: 2}
{a: 1, **not x, b: 2}
{**x in y}
{**x not in y}
{**x < y}
