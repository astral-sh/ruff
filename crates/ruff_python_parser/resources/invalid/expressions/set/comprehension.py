# Iterable unpacking not allowed
{*x for x in y}

# Invalid target
{x for 1 in y}
{x for 'a' in y}
{x for call() in y}
{x for {a, b} in y}

# Invalid iter
{x for x in *y}
{x for x in yield y}
{x for x in yield from y}
{x for x in lambda y: y}

# Invalid if
{x for x in data if *y}
{x for x in data if yield y}
{x for x in data if yield from y}
{x for x in data if lambda y: y}
