# OK
def add_numbers(a, b):
    """
    Adds two numbers and returns the result.

    Args:
        a (int): The first number to add.
        b (int): The second number to add.

    Returns:
        int: The sum of the two numbers.
    """
    return a + b


# OK
def multiply_list_elements(lst, multiplier):
    """
    Multiplies each element in a list by a given multiplier.

    Args:
        lst (list of int): A list of integers.
        multiplier (int): The multiplier for each element in the list.

    Returns:
        list of int: A new list with each element multiplied.
    """
    return [x * multiplier for x in lst]


# OK
def find_max_value(numbers):
    """
    Finds the maximum value in a list of numbers.

    Args:
        numbers (list of int): A list of integers to search through.

    Returns:
        int: The maximum value found in the list.
    """
    return max(numbers)


# OK
def create_user_profile(name, age, email, location="here"):
    """
    Creates a user profile with basic information.

    Args:
        name (str): The name of the user.
        age (int): The age of the user.
        email (str): The user's email address.
        location (str): The location of the user.

    Returns:
        dict: A dictionary containing the user's profile.
    """
    return {
        'name': name,
        'age': age,
        'email': email,
        'location': location
    }


# OK
def calculate_total_price(item_prices, tax_rate, discount):
    """
    Calculates the total price after applying tax and a discount.

    Args:
        item_prices (list of float): A list of prices for each item.
        tax_rate (float): The tax rate to apply.
        discount (float): The discount to subtract from the total.

    Returns:
        float: The final total price after tax and discount.
    """
    total = sum(item_prices)
    total_with_tax = total + (total * tax_rate)
    final_total = total_with_tax - discount
    return final_total


# OK
def send_email(subject, body, to_address, cc_address=None, bcc_address=None):
    """
    Sends an email to the specified recipients.

    Args:
        subject (str): The subject of the email.
        body (str): The content of the email.
        to_address (str): The recipient's email address.
        cc_address (str, optional): The email address for CC. Defaults to None.
        bcc_address (str, optional): The email address for BCC. Defaults to None.

    Returns:
        bool: True if the email was sent successfully, False otherwise.
    """
    return True


# OK
def concatenate_strings(separator, *args):
    """
    Concatenates multiple strings with a specified separator.

    Args:
        separator (str): The separator to use between strings.
        *args (str): Variable length argument list of strings to concatenate.

    Returns:
        str: A single concatenated string.
    """
    return separator.join(args)


# OK
def process_order(order_id, *items, **details):
    """
    Processes an order with a list of items and optional order details.

    Args:
        order_id (int): The unique identifier for the order.
        *items (str): Variable length argument list of items in the order.
        **details (dict): Additional details such as shipping method and address.

    Returns:
        dict: A dictionary containing the order summary.
    """
    return {
        'order_id': order_id,
        'items': items,
        'details': details
    }


class Calculator:
    """
    A simple calculator class that can perform basic arithmetic operations.
    """

    # OK
    def __init__(self, value=0):
        """
        Initializes the calculator with an initial value.

        Args:
            value (int, optional): The initial value of the calculator. Defaults to 0.
        """
        self.value = value

    # OK
    def add(self, number):
        """
        Adds a number to the current value.

        Args:
            number (int or float): The number to add to the current value.

        Returns:
            int or float: The updated value after addition.
        """
        self.value += number + number2
        return self.value

    # OK
    @classmethod
    def from_string(cls, value_str):
        """
        Creates a Calculator instance from a string representation of a number.

        Args:
            value_str (str): The string representing the initial value.

        Returns:
            Calculator: A new instance of Calculator initialized with the value from the string.
        """
        value = float(value_str)
        return cls(value)

    # OK
    @staticmethod
    def is_valid_number(number):
        """
        Checks if a given number is valid (int or float).

        Args:
            number (any): The value to check.

        Returns:
            bool: True if the number is valid, False otherwise.
        """
        return isinstance(number, (int, float))


# DOC101
def add_numbers(a, b):
    """
    Adds two numbers and returns the result.

    Args:
        a (int): The first number to add.

    Returns:
        int: The sum of the two numbers.
    """
    return a + b


# DOC101
def multiply_list_elements(lst, multiplier):
    """
    Multiplies each element in a list by a given multiplier.

    Args:
        lst (list of int): A list of integers.

    Returns:
        list of int: A new list with each element multiplied.
    """
    return [x * multiplier for x in lst]


# DOC101
def find_max_value(numbers):
    """
    Finds the maximum value in a list of numbers.

    Returns:
        int: The maximum value found in the list.
    """
    return max(numbers)


# DOC101
def create_user_profile(name, age, email, location="here"):
    """
    Creates a user profile with basic information.

    Args:
        email (str): The user's email address.
        location (str): The location of the user.

    Returns:
        dict: A dictionary containing the user's profile.
    """
    return {
        'name': name,
        'age': age,
        'email': email,
        'location': location
    }


# DOC101
def calculate_total_price(item_prices, tax_rate, discount):
    """
    Calculates the total price after applying tax and a discount.

    Args:
        item_prices (list of float): A list of prices for each item.

    Returns:
        float: The final total price after tax and discount.
    """
    total = sum(item_prices)
    total_with_tax = total + (total * tax_rate)
    final_total = total_with_tax - discount
    return final_total


# DOC101
def send_email(subject, body, to_address, cc_address=None, bcc_address=None):
    """
    Sends an email to the specified recipients.

    Args:
        subject (str): The subject of the email.
        body (str): The content of the email.
        to_address (str): The recipient's email address.

    Returns:
        bool: True if the email was sent successfully, False otherwise.
    """
    return True


# DOC101
def concatenate_strings(separator, *args):
    """
    Concatenates multiple strings with a specified separator.

    Args:
        separator (str): The separator to use between strings.

    Returns:
        str: A single concatenated string.
    """
    return separator.join(args)


# DOC101
def process_order(order_id, *items, **details):
    """
    Processes an order with a list of items and optional order details.

    Args:
        order_id (int): The unique identifier for the order.

    Returns:
        dict: A dictionary containing the order summary.
    """
    return {
        'order_id': order_id,
        'items': items,
        'details': details
    }


class Calculator:
    """
    A simple calculator class that can perform basic arithmetic operations.
    """

    # DOC101
    def __init__(self, value=0):
        """
        Initializes the calculator with an initial value.

        Returns:
            None
        """
        self.value = value

    # DOC101
    def add(self, number, number2):
        """
        Adds a number to the current value.

        Args:
            number (int or float): The number to add to the current value.

        Returns:
            int or float: The updated value after addition.
        """
        self.value += number + number2
        return self.value

    # DOC101
    @classmethod
    def from_string(cls, value_str):
        """
        Creates a Calculator instance from a string representation of a number.

        Returns:
            Calculator: A new instance of Calculator initialized with the value from the string.
        """
        value = float(value_str)
        return cls(value)

    # DOC101
    @staticmethod
    def is_valid_number(number):
        """
        Checks if a given number is valid (int or float).

        Returns:
            bool: True if the number is valid, False otherwise.
        """
        return isinstance(number, (int, float))

    # OK
    @classmethod
    def from_bytes(cls, value_bytes):
        """
        Creates a Calculator instance from a bytes representation of a number.

        Args:
            value_bytes (bytes): Bytes.

        Returns:
            Calculator: A new instance of Calculator initialized with the value from the string.
        """
        value = float(value_bytes)
        return cls(value)

