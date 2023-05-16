class Animal:
    pass

class Tree:
    pass

class Feline(Animal):
    def __init__(self):
        super(Tree, self).__init__()  # error
        super(Animal, self).__init__() # ok
        # inherits from parent


class Cat(Feline):
    def __init__(self):
        super(Tree, self).__init__()  # error
        super(Feline, self).__init__() # ok 
        # inherited parent
        super(Cat, self).__init__() # ok
        # own class
        super(Animal, self).__init__() # error 
        # TODO: false negative
        # Since Feline inherits Animal, it should be ok
