def main(arg: int) -> str:
    a: int = 123 + arg
    b: str = "hello"
    c: ObjectType = ObjectType()
    return c(b * a)
