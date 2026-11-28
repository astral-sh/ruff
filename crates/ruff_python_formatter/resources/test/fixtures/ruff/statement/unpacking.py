# Nested targets retain their unpacking structure.
((first), ((second), (third,))) = values
[(first), ((second), (third))] = values
((first), *(rest)) = values
for ((first), [(second), (third)]) in values:
    pass
[(first, second) for ((first), (second)) in values]
with context() as ((first), (second)):
    pass
del (first), ((second), (third))

# A parenthesized annotated name is not a simple annotation target.
name: int
(name): int
((name)): int = value
(obj.attribute): int = value
(obj[index]): int = value

# Comments and trailing commas can still require parentheses.
(
    # Comment attached to the first target.
    first
), (second) = values
(first, second,) = values
(first,) = values
first, ((second), (third)) = values
