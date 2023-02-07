@dataclass
class Foo:
    foo: List[str] = field(default_factory=lambda: [])  # PIE807


class FooTable(BaseTable):
    bar = fields.ListField(default=lambda: [])  # PIE807


class FooTable(BaseTable):
    bar = fields.ListField(lambda: [])  # PIE807


@dataclass
class Foo:
    foo: List[str] = field(default_factory=list)


class FooTable(BaseTable):
    bar = fields.ListField(list)
