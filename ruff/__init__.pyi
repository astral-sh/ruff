class Location:
    row: int
    column: int


class Message:
    code: str
    message: str
    location: Location
    end_location: Location

def check(contents: str | None, path: str | None, options: Mapping[str, Any] | None) -> list(Message): ...