@dataclass
class Foo:
    foo: List[str] = field(default_factory=lambda: [])  # PIE807
    bar: Dict[str, int] = field(default_factory=lambda: {})  # PIE807


class FooTable(BaseTable):
    foo = fields.ListField(default=lambda: [])  # PIE807
    bar = fields.ListField(default=lambda: {})  # PIE807


class FooTable(BaseTable):
    foo = fields.ListField(lambda: [])  # PIE807
    bar = fields.ListField(default=lambda: {})  # PIE807


@dataclass
class Foo:
    foo: List[str] = field(default_factory=list)
    bar: Dict[str, int] = field(default_factory=dict)


class FooTable(BaseTable):
    foo = fields.ListField(list)
    bar = fields.ListField(dict)


lambda *args, **kwargs: []
lambda *args, **kwargs: {}

lambda *args: []
lambda *args: {}

lambda **kwargs: []
lambda **kwargs: {}

lambda: {**unwrap}
