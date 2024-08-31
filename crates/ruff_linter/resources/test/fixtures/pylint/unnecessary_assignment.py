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

bad5 = 'example'
if bad5: # [consider-using-assignment-expr]
    print(bad5)


bad6_1 = 0
bad6_2 = 0
if True:
    pass
elif bad6_1: # [consider-using-assignment-expr]
    pass
elif bad6_2: # [consider-using-assignment-expr]
    pass

bad7 = (
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
if bad7: # [consider-using-assignment-expr]
    pass

bad8 = 'example'
if bad8 is not None: # [consider-using-assignment-expr]
    pass

good1_1 = 'example'
good1_2 = good1_1
if good1_1:  # correct, walrus cannot be used because expression is already used before if 
    pass

if good2 := 'example': # correct
    pass

def test(good3: str | None = None):
    if good3 is None:
        good3 = 'test'
