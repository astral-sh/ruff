# Invalid target
{x: y for 1 in y}
{x: y for 'a' in y}
{x: y for call() in y}
{x: y for {a, b} in y}

# Invalid iter
{x: y for x in *y}
{x: y for x in yield y}
{x: y for x in yield from y}
{x: y for x in lambda y: y}

# Invalid if
{x: y for x in data if *y}
{x: y for x in data if yield y}
{x: y for x in data if yield from y}
{x: y for x in data if lambda y: y}