def custom_classmethod(func):
    return classmethod(func)


def custom_staticmethod(func):
    return staticmethod(func)


class C:
    def ok(this):
        return this._x  # fine

    @custom_classmethod
    def ok_classmethod(this):
        return this._x  # fine

    @custom_staticmethod
    def bad_staticmethod(self):
        return self._x  # error
