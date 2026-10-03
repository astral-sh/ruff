# `assert-unfiltered-comprehension`

```toml
lint.preview = true
lint.select = ["assert-unfiltered-comprehension"]
```

## Unfiltered comprehensions

These assertions check whether the collection has elements, not whether those elements are true.
The rule applies to all three collection comprehensions, including multiple and asynchronous loops.

```py
assert [valid(item) for item in items]  # error: [assert-unfiltered-comprehension]
assert {valid(item) for item in items}  # error: [assert-unfiltered-comprehension]
assert {item: valid(item) for item in items}  # error: [assert-unfiltered-comprehension]
assert [valid(item) for group in groups for item in group]  # error: [assert-unfiltered-comprehension]
assert [item if valid(item) else None for item in items]  # error: [assert-unfiltered-comprehension]


async def check(items):
    assert [valid(item) async for item in items]  # error: [assert-unfiltered-comprehension]
```

## Boolean conditions within assertions

Boolean operators, conditional expressions, and assignment expressions also test their operands'
truthiness. Each unfiltered comprehension in these positions is reported.

```py
assert ready and [valid(item) for item in items]  # error: [assert-unfiltered-comprehension]
assert [valid(item) for item in items] or fallback  # error: [assert-unfiltered-comprehension]
assert not {valid(item) for item in items}  # error: [assert-unfiltered-comprehension]
assert (results := [valid(item) for item in items])  # error: [assert-unfiltered-comprehension]
assert [valid(item) for item in items] if ready else fallback  # error: [assert-unfiltered-comprehension]
assert ready if condition else {item: valid(item) for item in items}  # error: [assert-unfiltered-comprehension]
assert ready if [valid(item) for item in items] else fallback  # error: [assert-unfiltered-comprehension]
```

## Filtered comprehensions

A filter on any loop can make the collection empty. Testing the collection can intentionally check
whether any items satisfy the filter, so these comprehensions are allowed.

```py
assert [item for item in items if valid(item)]
assert {item for item in items if valid(item)}
assert {item: value for item, value in items if valid(value)}
assert [item for group in groups if group for item in group]
assert [item for group in groups for item in group if valid(item)]
assert not [item for item in items if invalid(item)]
```

## Comprehensions used as values

Comparisons and function calls can inspect the collection's contents. The assertion message is
also a value, not a boolean condition. Generator expressions are outside this rule's scope.

```py
assert [transform(item) for item in items] == expected
assert {transform(item) for item in items} != unexpected
assert {item: transform(item) for item in items} == expected
assert all([valid(item) for item in items])
assert any({valid(item) for item in items})
assert len([transform(item) for item in items]) == 3
assert item in [transform(item) for item in items]
assert ready, [describe(item) for item in items]
assert (valid(item) for item in items)
assert all(valid(item) for item in items)
```

## Suppressions

An intentional emptiness check can be suppressed without changing its evaluation behavior.

```py
assert [transform(item) for item in items]  # ruff: ignore[assert-unfiltered-comprehension]
```
