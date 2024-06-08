# Simple
{}
{1: 2}
{1: 2, a: 1, b: 'hello'}

# Mixed indentations
{
}
{
    1:
    2,
    3
    :4
}

# Nested
{{1: 2}: {3: {4: 5}}}

# Lambda expressions
{lambda x: x: 1}
{'A': lambda p: None, 'B': C,}

# Named expressions
{(x := 1): y}
{(x := 1): (y := 2)}

# Double star unpacking
{**d}
{a: b, **d}
{**a, **b}
{"a": "b", **c, "d": "e"}
{1: 2, **{'nested': 'dict'}}
{x * 1: y ** 2, **call()}
# Here, `not` isn't allowed but parentheses resets the precedence
{**(not x)}

# Random expressions
{1: x if True else y}
{x if True else y: y for x in range(10) for y in range(10)}
{{1, 2}: 3, x: {1: 2,},}
{(x): (y), (z): (a)}
