# parse_options: {"target-version": "3.15"}
[*x for x in y]
{*x for x in y}
{**x for x in y}
(*x for x in y)
f(*x for x in y)
[*x async for x in y]
{*x async for x in y}
{**x async for x in y}
(*x async for x in y)
{**x or {} for x in []}
{**x and {} for x in []}
{**not x for x in []}
{**x if b else {} for x in []}
