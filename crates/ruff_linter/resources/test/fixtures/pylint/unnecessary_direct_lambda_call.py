"""Test unnecessary direct calls to lambda expressions."""
# pylint: disable=undefined-variable, line-too-long

y = (lambda x: x**2 + 2*x + 1)(a)  # [unnecessary-direct-lambda-call]
y = max((lambda x: x**2)(a), (lambda x: x+1)(a))  # [unnecessary-direct-lambda-call,unnecessary-direct-lambda-call]

def function():
    # Safe - no comprehensions, no class scope issues
    area = (lambda r: 3.14 * r ** 2)(radius)  # PLC3002

    # Safe - simple expression
    result = (lambda x, y: x + y)(1, 2)  # PLC3002

def function():
    numbers = [1, 2, 3]
    # Safe - comprehension but in function scope, not class scope
    result = (lambda lst: [x * 2 for x in lst])(numbers)  # PLC3002


class A:
    # Safe - comprehension doesn't reference class variables
    y = (lambda: [i for i in range(3)])()  # PLC3002

    # Safe - uses lambda parameter, not class variable
    z = (lambda data: [x for x in data])([1, 2, 3])  # PLC3002

class A:
    x = 1
    # Unsafe - would cause F821 if inlined
    # (lambda test: [_ for _ in [1] if test])(x) → [_ for _ in [1] if x]
    y = (lambda test: [_ for _ in [1] if test])(x)  # No PLC3002

    # Unsafe - comprehension references class variable
    data = [1, 2, 3]
    filtered = (lambda items: [x for x in items if x > 1])(data)  # No PLC3002 if A.threshold exists


class Config:
    default_value = 42
    # Unsafe - dict comprehension would lose access to class variable
    mapping = (lambda val: {k: val for k in ['a', 'b']})(default_value)  # No PLC3002

class DataProcessor:
    multiplier = 5
    # Unsafe - set comprehension references class variable
    processed = (lambda m: {x * m for x in range(3)})(multiplier)  # No PLC3002

class StreamProcessor:
    factor = 2
    # Unsafe - generator expression would cause undefined name
    generator = (lambda f: (x * f for x in range(5)))(factor)  # No PLC3002

class Matrix:
    size = 3
    # Unsafe - nested comprehensions with class variable reference
    matrix = (lambda s: [[i * j * s for i in range(3)] for j in range(3)])(size)  # No PLC3002
