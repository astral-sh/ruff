# parse_options: {"target-version": "3.12"}
a = 10
def g():
    try:
        1 / 0
    except:
        a = 1
    else:
        global a
