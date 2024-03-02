class Animal:
    """Animal"""
    pass


class Tree:
    """Tree"""
    pass


class Bug:
    """Bug"""
    pass

class CatA(Animal):
    """CatA"""
    def __init__(self):
        super(Tree, self).__init__()  # [bad-super-call]
        super(Animal, self).__init__()


class CatB(Animal):
    """CatB"""
    def __init__(self):
        super(Animal, self).__init__() # OK


class CatC(Animal):
    """CatC"""
    def some_other_func(self):
        """Some other func"""
        super(Tree, self).__init__()


class CatD(Animal):
    """CatD"""
    def __init__(self):
        def some_nested_func(self):
            super(Tree, self).__init__()


class CatE(Animal):
    """CatE"""
    def __init__(self):
        super(Tree, self).__init__()
        super(Bug, self).__init__()
        
        
class CatF(Animal):
    """CatF"""
    def __init__(self):
        super().__init__()

        def thing():
            super(Tree, self).__init__()


def hello():  # just to make sure it doesn't trigger on an ordinary function
    super(Tree, self).__init__()
