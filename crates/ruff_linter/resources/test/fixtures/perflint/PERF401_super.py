class Parent:
    def value(self):
        return 1

    def values(self):
        return [1]


class Child(Parent):
    def element(self):
        result = []
        for _ in range(1):
            result.append(super().value())

    def condition(self):
        result = []
        for value in range(1):
            if super().value():
                result.append(value)

    def extend(self):
        result = [0]
        for _ in range(1):
            result.append(super().value())

    def extend_condition(self):
        result = [0]
        for value in range(1):
            if super().value():
                result.append(value)

    async def async_list_comprehension(self, values):
        result = []
        async for _ in values:
            result.append(super().value())

    async def async_extend(self, values):
        result = [0]
        async for _ in values:
            result.append(super().value())

    def iterable(self):
        result = []
        for value in super().values():
            result.append(value + 1)

    def extend_iterable(self):
        result = [0]
        for value in super().values():
            result.append(value + 1)

    def explicit(self):
        result = []
        for _ in range(1):
            result.append(super(Child, self).value())

    def shadowed(self):
        super = lambda: 1
        result = []
        for _ in range(1):
            result.append(super())

    def starred(self):
        result = []
        for _ in range(1):
            result.append(super(*()).value())

    def extend_starred(self):
        result = [0]
        for _ in range(1):
            result.append(super(*()).value())

    def lambda_body(self):
        result = []
        for _ in range(1):
            result.append(lambda instance: super().value())

    def lambda_default(self):
        result = []
        for _ in range(1):
            result.append(lambda value=super().value(): value)

    def nested_generator_iterable(self):
        result = []
        for _ in range(1):
            result.append(value for value in super().values())
