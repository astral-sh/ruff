class Person:
    def __init__(self, name: str, age: int) -> None:
        self.name = name
        self.age = age


people = [
    Person("Alice", 25),
    Person("Bob", 31),
]


def total_age(people: list[Person]) -> int:
    sum_age = 0
    for person in people:
        sum_age += person.age
    return sum_age


def average_age(people: list[Person]) -> float:
    if not people:
        return 0.0
    return total_age(people) / len(people)


print(total_age(people))
print(len(people))
print(f"Average age across {len(people)} people: {average_age(people)}")
