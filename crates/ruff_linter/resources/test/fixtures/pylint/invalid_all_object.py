__all__ = (
    None,  # [invalid-all-object]
    Fruit,
    Worm,
)

__all__ = list([None, "Fruit", "Worm"])  # [invalid-all-object]


class Fruit:
    pass


class Worm:
    pass
