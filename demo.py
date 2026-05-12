class Person:
    def __init__(self, name: str, age: int):
        self.name = name
        self.age = age


people = [
    Person("Alice", 25),
    Person("Bob", 38),
]


def average_age(people: list[Person]):
    if not people:
        return 0.0
    sum_age = 0
    for person in people:
        sum_age += person.age
    return sum_age / len(people)


print(f"Average age across {len(people)} people: {average_age(people)}")
