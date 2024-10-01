# This is similar to `./invalid_assignment_targets.py`, but for augmented
# assignment targets.

x or y += 42
(x := 5) += 42
x + y += 42
-x += 42
(lambda _: 1) += 42
a if b else c += 42
{"a": 5} += 42
{a} += 42
[x for x in xs] += 42
{x for x in xs} += 42
{x: x * 2 for x in xs} += 42
(x for x in xs) += 42
await x += 42
(yield x) += 42
(yield from xs) += 42
a < b < c += 42
foo() += 42

f"{quux}" += 42
f"{foo} and {bar}" += 42

"foo" += 42
b"foo" += 42
123 += 42
True += 42
None += 42
... += 42
*foo() += 42
[x, foo(), y] += [42, 42, 42]
[[a, b], [[42]], d] += [[1, 2], [[3]], 4]
(x, foo(), y) += (42, 42, 42)
