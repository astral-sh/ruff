def some_fun():
    print(5)
    return None

def some_other_fun():
    print(5)
    return

class SomeClass:
    def incredible(self):
        print(42)
        return None

def bare_return_not_final_statement():
    if 2 * 2 == 4:
        return
    print(5)

def tricky():
    if 2 * 2 == 4:
        return None
