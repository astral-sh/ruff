from typing import Literal

class Format:
    STRING = "string"

def evaluate(format: Literal[Format.STRING]) -> str: ...
