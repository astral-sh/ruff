@first
@second(param=4)
@third()
def function(): ...


@first
@second(param=4)
@third()
class Test(object): ...


class Test(object):
    @first
    @second(param=4)
    @third()
    def method(self): ...


class Test(object):
    @second(param=4)
    @third()
    @classmethod
    def method(cls): ...
