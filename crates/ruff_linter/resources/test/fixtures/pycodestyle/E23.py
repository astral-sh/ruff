#: E231
a = (1,2)
#: E231
a[b1,:]
#: E231
a = [{'a':''}]
#: Okay
a = (4,)
b = (5, )
c = {'text': text[5:]}

result = {
    'key1': 'value',
    'key2': 'value',
}

def foo() -> None:
    #: E231
    if (1,2):
        pass

#: Okay
a = (1,\
2)

#: E231:2:20
mdtypes_template = {
    'tag_full': [('mdtype', 'u4'), ('byte_count', 'u4')],
    'tag_smalldata':[('byte_count_mdtype', 'u4'), ('data', 'S4')],
}

# E231
f"{(a,b)}"

# Okay because it's hard to differentiate between the usages of a colon in a f-string
f"{a:=1}"
f"{ {'a':1} }"
f"{a:.3f}"
f"{(a:=1)}"
f"{(lambda x:x)}"
f"normal{f"{a:.3f}"}normal"

#: Okay
a = (1,
