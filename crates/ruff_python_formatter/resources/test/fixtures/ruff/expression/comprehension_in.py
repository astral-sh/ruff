result = [
    very_very_very_very_very_long_item
    for very_very_very_very_very_long_item in some_very_very_very_very_very_very_long_function_name
]
result = {
    very_very_very_very_very_long_item
    for very_very_very_very_very_long_item in source.collection.items()
}
result = {
    very_very_very_very_very_long_key: value
    for very_very_very_very_very_long_key, value in source.collection.items()
}
result = consume(
    very_very_very_very_very_long_item
    for very_very_very_very_very_long_item in source.collection.items()
)

# Avoid wrapping an iterable that still cannot fit on its own line.
result = [item for item in very_very_very_very_very_very_very_very_very_very_very_very_very_very_very_long_name]

# Keep required grammar and user grouping in compound iterables.
result = [item for item in (lambda: source) if item]
result = [item for item in (left if condition else right)]
result = [item for item in (left or right)]
result = [item for item in left or right]

# An iterable that expands also expands its comprehension.
result = [item for item in call(first, second,)]
result = [item for item in call(["long long long long line", "long long long long line"])]

async def collect():
    return [item async for item in source.collection.items() if item]
