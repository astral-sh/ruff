# Simple sets
{}
{1}
{1,}
{1, 2, 3}
{1, 2, 3,}

# Mixed with indentations
{
}
{
        1
}
{
    1,
        2,
}

# Nested
{{1}}
{{1, 2}, {3, 4}}

# Named expression
{x := 2}
{1, x := 2, 3}
{1, (x := 2),}

# Star expression
{1, *x, 3}
{1, *x | y, 3}

# Random expressions
{1 + 2, (a, b), {1, 2, 3}, {a: b, **d}}
