def func(_, a, badAllowed):
    return _, a, badAllowed

def func(_, a, stillBad):
    return _, a, stillBad

class Class:
    def method(self, _, a, badAllowed):
        return _, a, badAllowed

    def method(self, _, a, stillBad):
        return _, a, stillBad
