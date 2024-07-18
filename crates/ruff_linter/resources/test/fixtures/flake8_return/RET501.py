def x(y):
    if not y:
        return
    return None  # error


class BaseCache:
    def get(self, key: str) -> str | None:
        print(f"{key} not found")
        return None

    def get(self, key: str) -> None:
        print(f"{key} not found")
        return None

    @property
    def prop(self) -> None:
        print("Property not found")
        return None
