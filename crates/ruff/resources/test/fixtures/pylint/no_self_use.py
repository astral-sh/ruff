class Person:
    def developer_greeting(self, name):  # [no-self-use]
        print(f"Greetings {name}!")

    def greeting_1(self):  # [no-self-use]
        print("Hello!")

    def greeting_2(self):  # [no-self-use]
        print("Hi!")


# OK
def developer_greeting():
    print("Greetings developer!")


# OK
class Person:
    name = "Paris"

    def __init__(self):
        pass

    def __repr__(self):
        return "Person"

    def greeting_1(self):
        print(f"Hello from {self.name} !")

    @staticmethod
    def greeting_2():
        print("Hi!")


