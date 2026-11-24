"""
Should emit:
B909 - on lines 11, 25, 26, 40, 46
"""

# lists

some_list = [1, 2, 3]
some_other_list = [1, 2, 3]
for elem in some_list:
    # errors
    some_list.remove(0)
    del some_list[2]
    some_list.append(elem)
    some_list.sort()
    some_list.reverse()
    some_list.clear()
    some_list.extend([1, 2])
    some_list.insert(1, 1)
    some_list.pop(1)
    some_list.pop()

    # conditional break should error
    if elem == 2:
        some_list.remove(0)
        if elem == 3:
            break

    # non-errors
    some_other_list.remove(elem)
    del some_list
    del some_other_list
    found_idx = some_list.index(elem)
    some_list = 3

    # unconditional break should not error
    if elem == 2:
        some_list.remove(elem)
        break


# dicts
mydicts = {"a": {"foo": 1, "bar": 2}}

for elem in mydicts:
    # errors
    mydicts.popitem()
    mydicts.setdefault("foo", 1)
    mydicts.update({"foo": "bar"})

    # no errors
    elem.popitem()
    elem.setdefault("foo", 1)
    elem.update({"foo": "bar"})

# sets

myset = {1, 2, 3}

for _ in myset:
    # errors
    myset.update({4, 5})
    myset.intersection_update({4, 5})
    myset.difference_update({4, 5})
    myset.symmetric_difference_update({4, 5})
    myset.add(4)
    myset.discard(3)

    # no errors
    del myset


# members
class A:
    some_list: list

    def __init__(self, ls):
        self.some_list = list(ls)


a = A((1, 2, 3))
# ensure member accesses are handled as errors
for elem in a.some_list:
    a.some_list.remove(0)
    del a.some_list[2]


# Augassign should error

foo = [1, 2, 3]
bar = [4, 5, 6]
for _ in foo:
    foo *= 2
    foo += bar
    foo[1] = 9
    foo[1:2] = bar
    foo[1:2:3] = bar

foo = {1, 2, 3}
bar = {4, 5, 6}
for _ in foo:  # should error
    foo |= bar
    foo &= bar
    foo -= bar
    foo ^= bar


# more tests for unconditional breaks - should not error
for _ in foo:
    foo.remove(1)
    for _ in bar:
        bar.remove(1)
        break
    break

# should not error
for _ in foo:
    foo.remove(1)
    for _ in bar:
        ...
    break

# should not error - outer break makes the mutation safe
for _ in foo:
    foo.remove(1)
    if bar:
        bar.remove(1)
        break
    break

# should error
for _ in foo:
    if bar:
        pass
    else:
        foo.remove(1)

# should error
for elem in some_list:
    if some_list.pop() == 2:
        pass

# should not error
for elem in some_list:
    if some_list.pop() == 2:
        break

# should error
for elem in some_list:
    if some_list.pop() == 2:
        pass
    else:
        break

# should error
for elem in some_list:
    del some_list[elem]
    some_list.remove(elem)
    some_list.discard(elem)

# should not error
for elem in some_list:
    some_list[elem] = 1

# should error
for i, elem in enumerate(some_list):
    some_list.pop(0)

# should not error (list)
for i, elem in enumerate(some_list):
    some_list[i] = 1

# should not error (dict)
for i, elem in enumerate(some_list):
    some_list[elem] = 1

# should not error
def func():
    for elem in some_list:
        if some_list.pop() == 2:
            return

# should not error - direct return with mutation (Issue #18399)
def fail_map(mapping):
    for key in mapping:
        return mapping.pop(key)

def success_map(mapping):
    for key in mapping:
        ret = mapping.pop(key)  # should not error
        return ret

def fail_list(seq):
    for val in seq:
        return seq.pop(4)

# should not error - break after non-flow-altering if (issue #12402)
for item in some_list:
    some_list.append(item)
    if True:
        pass
    break

# should error - continue followed by unreachable break (issue #12402)
for item in some_list:
    some_list.remove(item)
    continue
    break

# should error - nested `break` does not exit the outer loop
for item in some_list:
    some_list.append(item)
    for _ in range(3):
        break

# should error - nested `continue` does not exit the outer loop
for item in some_list:
    some_list.append(item)
    for _ in range(3):
        continue

# should error - nested `while`'s `break` only exits the inner loop
for item in some_list:
    some_list.append(item)
    while True:
        break

# should error - nested `for`'s `break` can bypass `else: return`,
# so outer mutation may still be reached on re-iteration
def fail_nested_for_else(some_list, other):
    for item in some_list:
        some_list.append(item)
        for y in other:
            if y:
                break
        else:
            return

# should error - nested `while`'s `break` can bypass `else: return`,
# so outer mutation may still be reached on re-iteration
def fail_nested_while_else(some_list):
    for item in some_list:
        some_list.append(item)
        while True:
            break
        else:
            return

# should error - `return` in `except` must not clear mutation in `try`
def fail_try_except(some_list):
    for item in some_list:
        try:
            some_list.append(item)
        except Exception:
            return

# should error - `return` in `except` must not clear mutation in `else`
def fail_try_else(some_list):
    for item in some_list:
        try:
            pass
        except Exception:
            return
        else:
            some_list.append(item)

# should error - `return` in `except` must not clear mutation in `finally`
def fail_try_finally(some_list):
    for item in some_list:
        try:
            pass
        except Exception:
            return
        finally:
            some_list.append(item)

# should not error - `finally: return` unconditionally exits, so the `try`
# body's mutation never reaches another iteration
def pass_try_finally_return(items):
    for item in items:
        try:
            items.append(item)
        finally:
            return

# should error - nested loop's body has a `return` but no `break`, so the
# outer mutation can reach another iteration if the `return` doesn't fire
def fail_nested_return_bypasses_else(outer, inner):
    for item in outer:
        outer.append(item)
        for y in inner:
            if y:
                return
        else:
            return

# should error - nested loop's body can `raise`, which also skips `else`
def fail_nested_raise_bypasses_else(outer, inner):
    for item in outer:
        outer.append(item)
        for y in inner:
            if y:
                raise ValueError
        else:
            return

# should not error - the `break` is unreachable, so `else` always runs and `return` exits
def pass_nested_continue_before_dead_break(outer, inner):
    for x in outer:
        outer.append(x)
        for y in inner:
            continue
            break
        else:
            return
