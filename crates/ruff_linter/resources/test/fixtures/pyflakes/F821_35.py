"""Test class-scope generator expression name resolution.

A generator body and its non-first iterables run lazily, after the enclosing
class is defined, so forward references to the class name within a class-scope
generator should not trigger F821.  List, set, and dict comprehensions run
eagerly and should still be flagged when they reference the class name.

Note: the first iterable of a class-scope generator is also deferred (to
simplify the implementation), which means references to the class name in the
first iterable become false negatives.  Ruff prefers false negatives over
false positives in this trade-off.
"""

# --- Cases that should NOT trigger F821 (class-scope generators) ---

class Foo:
    BAR = [1, 2, 3]
    BAZ = [4, 5, 6]

    # Basic: class name in non-first iterable
    QUX = ((first, second) for first in BAR for second in Foo.BAZ)

    # Class name in body of generator (also deferred)
    BAZ2 = (Foo.BAZ for _ in range(1))

    # Mixed with other deferred constructs should still resolve
    BAZ3 = (Foo.BAR for _ in range(1))

    # Nested generator
    BAZ4 = (x for x in (Foo.BAR for _ in range(1)))

    # First iterable using class name — false negative (runs eagerly in
    # class scope at runtime, would fail with NameError, but Ruff's
    # deferral causes it to resolve).
    gen = (x for x in Foo.BAR)


# --- Cases that SHOULD trigger F821 (list/set/dict comprehensions are eager) ---

class FailingListComp:
    BAR = [1, 2, 3]
    BAZ = [FailingListComp.BAR for _ in range(1)]  # F821

class FailingSetComp:
    items = [1, 2]
    result = {FailingSetComp.items for _ in range(1)}  # F821

class FailingDictComp:
    keys = [1, 2]
    result = {k: FailingDictComp.keys for k in range(1)}  # F821


# --- Module-level generator should not defer (ecosystem regression guard) ---

def function_scope():
    items = [1, 2]
    gen = (x for x in items)  # OK
    return gen

module_items = [1, 2]
module_gen = (x for x in module_items)  # OK
