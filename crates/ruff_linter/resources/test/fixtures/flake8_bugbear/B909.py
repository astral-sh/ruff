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

# should error (?)
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

# should not error
for elem in some_list:
    del some_list[elem]
    some_list[elem] = 1
    some_list.remove(elem)
    some_list.discard(elem)
