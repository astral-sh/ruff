# Method call as key
ages = {"Tom": 23, "Maria": 23, "Dog": 11}
key_source = {"Thomas": "Tom"}
age = ages.get(key_source.get("Thomas", "Tom"), None)

# Property access as key
class Data:
    def __init__(self):
        self.name = "Tom"

data = Data()
ages = {"Tom": 23, "Maria": 23, "Dog": 11}
age = ages.get(data.name, None)

# Complex expression as key
ages = {"Tom": 23, "Maria": 23, "Dog": 11}
key = "Tom" if True else "Maria"
age = ages.get(key, None)

# Function call as key
def get_key():
    return "Tom"

ages = {"Tom": 23, "Maria": 23, "Dog": 11}
age = ages.get(get_key(), None)

# OK - these should not trigger even in preview mode
ages = {"Tom": 23, "Maria": 23, "Dog": 11}
age = ages.get("Tom")  # No default value

ages = {"Tom": 23, "Maria": 23, "Dog": 11}
age = ages.get("Tom", "Unknown")  # Non-None default
