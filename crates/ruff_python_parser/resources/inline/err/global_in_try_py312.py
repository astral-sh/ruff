# parse_options: {"target-version": "3.12"}
a = 10
def f():
    try:
        pass
    except:
        global a
    else:
        print(a)
