bad1 = 'example'
if bad1: # [consider-using-assignment-expr]
    pass

bad2 = 'example'
if bad2 and True: # [consider-using-assignment-expr]
    pass

bad3 = 'example'
if bad3 and bad3 == 'example': # [consider-using-assignment-expr]
    pass


def foo():
    bad4 = 0
    if bad4: # [consider-using-assignment-expr]
        pass

bad5 = (
    'example',
    'example',
    'example',
    'example',
    'example',
    'example',
    'example',
    'example',
    'example',
    'example',
)
if bad5: # [consider-using-assignment-expr]
    pass

bad6 = 'example'
if bad6 is not None: # [consider-using-assignment-expr]
    pass

bad7 = 'example'
if bad7 == 'something': # [consider-using-assignment-expr]
    pass
elif bad7 == 'something else':
    pass

bad8 = 'example'
if bad8: # [consider-using-assignment-expr]
    pass
print(bad8)

good1_1 = 'example'
good1_2 = 'example'
if good1_1: # correct, assignment is not the previous statement
    pass

good2_1 = 'example'
good2_2 = good2_1
if good2_1: # correct, assignment is not the previous statement
    pass

if good3 := 'example': # correct, used like it is intented
    pass

def test(good4: str | None = None):
    if good4 is None:
        good4 = 'test'

def bar():
    good5_5 = 'example'
    good5_2 = good5_5
    if good5_5: # correct, assignment is not the previous statement
        pass

for good6 in [1, 2, 3]:
    if good6: # correct, used like it is intented
        pass

if good7 := 'example': # [consider-using-assignment-expr]
    pass
print(good7)