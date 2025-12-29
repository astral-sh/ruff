def fun():
    if (a := nofun()):
        return a
    else:
        return True
def nofun():
    return False
