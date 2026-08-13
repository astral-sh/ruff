# DOC102
def add_numbers(b):
    """
    Adds two numbers and returns the result.

    Args:
        a (int): The first number to add.
        b (int): The second number to add.

    Returns:
        int: The sum of the two numbers.
    """
    return a + b


# DOC102
def multiply_list_elements(lst):
    """
    Multiplies each element in a list by a given multiplier.

    Args:
        lst (list of int): A list of integers.
        multiplier (int): The multiplier for each element in the list.

    Returns:
        list of int: A new list with each element multiplied.
    """
    return [x * multiplier for x in lst]


# DOC102
def find_max_value():
    """
    Finds the maximum value in a list of numbers.

    Args:
        numbers (list of int): A list of integers to search through.

    Returns:
        int: The maximum value found in the list.
    """
    return max(numbers)


# DOC102
def create_user_profile(location="here"):
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


# DOC102
def calculate_total_price(item_prices, discount):
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


# DOC102
def send_email(subject, body, bcc_address=None):
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


# DOC102
def concatenate_strings(*args):
    """
    Concatenates multiple strings with a specified separator.

    Args:
        separator (str): The separator to use between strings.
        *args (str): Variable length argument list of strings to concatenate.

    Returns:
        str: A single concatenated string.
    """
    return separator.join(args)


# DOC102
def process_order(order_id):
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

    # DOC102
    def __init__(self):
        """
        Initializes the calculator with an initial value.

        Args:
            value (int, optional): The initial value of the calculator. Defaults to 0.
        """
        self.value = value

    # DOC102
    def add(self, number2):
        """
        Adds a number to the current value.

        Args:
            number (int or float): The number to add to the current value.

        Returns:
            int or float: The updated value after addition.
        """
        self.value += number + number2
        return self.value

    # DOC102
    @classmethod
    def from_string(cls):
        """
        Creates a Calculator instance from a string representation of a number.

        Args:
            value_str (str): The string representing the initial value.

        Returns:
            Calculator: A new instance of Calculator initialized with the value from the string.
        """
        value = float(value_str)
        return cls(value)

    # DOC102
    @staticmethod
    def is_valid_number():
        """
        Checks if a given number is valid (int or float).

        Args:
            number (any): The value to check.

        Returns:
            bool: True if the number is valid, False otherwise.
        """
        return isinstance(number, (int, float))

# OK
def foo(param1, param2, *args, **kwargs):
    """Foo.

    Args:
        param1 (int): The first parameter.
        param2 (:obj:`str`, optional): The second parameter. Defaults to None.
            Second line of description: should be indented.
        *args: Variable length argument list.
        **kwargs: Arbitrary keyword arguments.
    """
    return

# OK
def on_server_unloaded(self, server_context: ServerContext) -> None:
    ''' Execute ``on_server_unloaded`` from ``server_lifecycle.py`` (if
    it is defined) when the server cleanly exits. (Before stopping the
    server's ``IOLoop``.)

    Args:
        server_context (ServerContext) :

    .. warning::
        In practice this code may not run, since servers are often killed
        by a signal.


    '''
    return self._lifecycle_handler.on_server_unloaded(server_context)

# OK
def function_with_kwargs(param1, param2, **kwargs):
    """Function with **kwargs parameter.

    Args:
        param1 (int): The first parameter.
        param2 (str): The second parameter.
        extra_param (str): An extra parameter that may be passed via **kwargs.
        another_extra (int): Another extra parameter.
    """
    return

# OK
def add_numbers(b):
    """
    Adds two numbers and returns the result.

    Args:
        b: The second number to add.

    Returns:
        int: The sum of the two numbers.
    """
    return

# DOC102
def add_numbers(b):
    """
    Adds two numbers and returns the result.

    Args:
        a: The first number to add.
        b: The second number to add.

    Returns:
        int: The sum of the two numbers.
    """
    return a + b
