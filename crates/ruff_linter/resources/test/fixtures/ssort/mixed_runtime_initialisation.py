class Loopy:

    def method(self):
        return self._method()

    attr = method

    def _method(self):
        pass
