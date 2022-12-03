"""Test unnecessary direct calls to lambda expressions."""
# pylint: disable=undefined-variable, line-too-long

y = (lambda x: x**2 + 2*x + 1)(a)  # [unnecessary-direct-lambda-call]
y = max((lambda x: x**2)(a), (lambda x: x+1)(a))  # [unnecessary-direct-lambda-call,unnecessary-direct-lambda-call]
