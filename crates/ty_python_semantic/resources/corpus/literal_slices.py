from typing import Literal

class Format:
    STRING = "string"

def evaluate(a: Literal[Format.STRING], b: Literal[-1]) -> str: ...
