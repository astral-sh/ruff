class Parent:
    def condition(self):
        return True

    def pairs(self):
        return [("key", 1)]


class Child(Parent):
    def condition_in_comprehension(self, pairs):
        result = {}
        for key, value in pairs:
            if super().condition():
                result[key] = value

    def update(self, pairs):
        result = {"existing": 0}
        for key, value in pairs:
            if super().condition():
                result[key] = value

    def iterable(self):
        result = {}
        for key, value in super().pairs():
            result[key] = value

    def explicit(self, pairs):
        result = {}
        for key, value in pairs:
            if super(Child, self).condition():
                result[key] = value

    def shadowed(self, pairs):
        super = lambda: Parent()
        result = {}
        for key, value in pairs:
            if super().condition():
                result[key] = value
