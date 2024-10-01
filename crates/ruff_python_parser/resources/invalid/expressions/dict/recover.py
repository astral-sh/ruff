# Test cases for dictionary expressions where the parser recovers from a syntax error.

{,}

{1: 2,,3: 4}

{1: 2,,}

# Missing comma
{1: 2 3: 4}

# No value
{1: }

# No value for double star unpacking
{**}
{x: y, **, a: b}

# This is not a double star unpacking
# {* *data}

# Star expression not allowed here
{*x: y, z: a, *b: c}
{x: *y, z: *a}
