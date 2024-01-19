"""
Should emit:
B999 - on lines 11, 25, 26, 40, 46
"""


some_list = [1, 2, 3]
for elem in some_list:
    print(elem)
    if elem % 2 == 0:
        some_list.remove(elem)  # should error

some_list = [1, 2, 3]
some_other_list = [1, 2, 3]
for elem in some_list:
    print(elem)
    if elem % 2 == 0:
        some_other_list.remove(elem)  # should not error
        del some_other_list


some_list = [1, 2, 3]
for elem in some_list:
    print(elem)
    if elem % 2 == 0:
        del some_list[2]  # should error
        del some_list


class A:
    some_list: list

    def __init__(self, ls):
        self.some_list = list(ls)


a = A((1, 2, 3))
for elem in a.some_list:
    print(elem)
    if elem % 2 == 0:
        a.some_list.remove(elem)  # should error

a = A((1, 2, 3))
for elem in a.some_list:
    print(elem)
    if elem % 2 == 0:
        del a.some_list[2]  # should error



some_list = [1, 2, 3]
for elem in some_list:
    print(elem)
    if elem == 2:
        found_idx = some_list.index(elem)  # should not error
        some_list.append(elem)  # should error
        some_list.sort()  # should error
        some_list.reverse()  # should error
        some_list.clear()  # should error
        some_list.extend([1,2])  # should error
        some_list.insert(1, 1)  # should error
        some_list.pop(1) # should error
        some_list.pop() # should error
        some_list = 3 # should error
        break



mydicts = {'a': {'foo': 1, 'bar': 2}}

for mydict in mydicts:
    if mydicts.get('a', ''):
        print(mydict['foo'])  # should not error
        mydicts.popitem() # should error
        
