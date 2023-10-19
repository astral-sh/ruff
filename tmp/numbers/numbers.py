from typing import Sequence  # noqa: D100, UP035


def sum_even_numbers(numbers: Sequence[int]) -> int:
    """Given a sequence of integers, return the sum of all even numbers in the sequence."""
    return sum(num for num in numbers if num % 2 == 0)
