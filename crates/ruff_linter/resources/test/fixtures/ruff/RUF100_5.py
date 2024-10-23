#import os  # noqa
#import os  # noqa: ERA001

dictionary = {
    # "key1": 123,  # noqa: ERA001
    # "key2": 456,  # noqa
    # "key3": 789,
}


#import os  # noqa: E501

def f():
    data = 1
    # line below should autofix to `return data  # fmt: skip`
    return data  # noqa: RET504 # fmt: skip

def f():
    data = 1
    # line below should autofix to `return data`
    return data  # noqa: RET504 - intentional incorrect noqa, will be removed
