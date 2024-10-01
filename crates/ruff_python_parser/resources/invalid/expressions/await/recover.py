# The parser parses all of the following expressions but reports an error for
# invalid expressions.

# Nested await
await await x

# Starred expressions
await *x
await (*x)

# Invalid expression as per precedence
await yield x
await lambda x: x
await +x
await -x
await ~x
await not x