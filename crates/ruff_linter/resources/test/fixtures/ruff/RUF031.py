# Standard Case
# Expects:
# - violation
raise Exception("This is not a helpful error message.")

# Concatenated string literals
# Expects:
# - violation
raise Exception("This is not " "a helpful " "error message.")

# Unpopulated string template
# Expects:
# - violation
raise Exception("This is not a helpful error message: {reason}")

# Numerical literals
# Expects:
# - violation
raise Exception(42)

# Boolean literals
# Expects:
# - violation
raise Exception(True)

# None literal
# Expects:
# - violation
raise Exception(None)

# Single string literal assigned to a variable
# Expects:
# - violation
my_message = "This is a static error message."
raise Exception(my_message)

# Single string literal assigned with type to a variable
# Expects:
# - violation
my_message: str = "This is a static error message."
raise Exception(my_message)

# Single string literal re-assigned to a variable
# Expects:
# - violation
my_message_2: str = my_message
my_message_3: str = my_message_2
my_message_4 = my_message_3
raise Exception(my_message_4)

# Concatenated string literals assigned to a variable
# Expects:
# - violation
my_message_5 = "This is not " "a helpful " "error message."
raise Exception(my_message_5)

# Concatenated string literals re-assigned to a variable with type
# Expects:
# - violation
my_message_6: str = my_message_5
raise Exception(my_message_6)

# Single f-string literal assigned to a variable
# Expects:
# - violation
my_message_7 = f"This is not a helpful error message."
raise Exception(my_message_7)

# Single f-string literal assigned to a variable with type
# Expects:
# - violation
my_message_8: str = f"This is not a helpful error message."
raise Exception(my_message_8)

# Single string literal formatted with string literals
# Expects:
# - violation
raise Exception("This is your error: {reason}".format(reason="reasons"))

# Single string literal formatted with f-string literals
# Expects:
# - violation
raise Exception("This is your error: {reason}".format(reason=f"reasons"))

# Assignment of Exception
# Expects:
# - violation
exc = Exception("This is not a helpful error message.")

# Instantiation of Exception and dropping it
# Expects:
# - violation
Exception("This is not a helpful error message.")

# FString with no variables
# Expects:
# - violation
raise Exception(f"This is not a helpful error message.")

# FString concatenated literals
# Expects:
# - violation
raise Exception(f"This is not " "a helpful " f"error message.")

# FString with literal placeholders
# Expects:
# - violation
raise Exception(f"This is {'not':-^11} a helpful error message {10:04} out of {10:.2f} times.")


# ArithmeticError with single string literal
# Expects:
# - violation
raise ArithmeticError("This is not a helpful error message.")

# AssertionError with single string literal
# Expects:
# - violation
raise AssertionError("This is not a helpful error message.")

# AttributeError with single string literal
# Expects:
# - violation
raise AttributeError("This is not a helpful error message.")

# BaseException with single string literal
# Expects:
# - violation
raise BaseException("This is not a helpful error message.")

# BlockingIOError with single string literal
# Expects:
# - violation
raise BlockingIOError("This is not a helpful error message.")

# BrokenPipeError with single string literal
# Expects:
# - violation
raise BrokenPipeError("This is not a helpful error message.")

# BufferError with single string literal
# Expects:
# - violation
raise BufferError("This is not a helpful error message.")

# ChildProcessError with single string literal
# Expects:
# - violation
raise ChildProcessError("This is not a helpful error message.")

# ConnectionAbortedError with single string literal
# Expects:
# - violation
raise ConnectionAbortedError("This is not a helpful error message.")

# ConnectionError with single string literal
# Expects:
# - violation
raise ConnectionError("This is not a helpful error message.")

# ConnectionRefusedError with single string literal
# Expects:
# - violation
raise ConnectionRefusedError("This is not a helpful error message.")

# ConnectionResetError with single string literal
# Expects:
# - violation
raise ConnectionResetError("This is not a helpful error message.")

# EOFError with single string literal
# Expects:
# - violation
raise EOFError("This is not a helpful error message.")

# EnvironmentError with single string literal
# Expects:
# - violation
raise EnvironmentError("This is not a helpful error message.")

# Exception with single string literal
# Expects:
# - violation
raise Exception("This is not a helpful error message.")

# FileExistsError with single string literal
# Expects:
# - violation
raise FileExistsError("This is not a helpful error message.")

# FileNotFoundError with single string literal
# Expects:
# - violation
raise FileNotFoundError("This is not a helpful error message.")

# FloatingPointError with single string literal
# Expects:
# - violation
raise FloatingPointError("This is not a helpful error message.")

# GeneratorExit with single string literal
# Expects:
# - violation
raise GeneratorExit("This is not a helpful error message.")

# IOError with single string literal
# Expects:
# - violation
raise IOError("This is not a helpful error message.")

# ImportError with single string literal
# Expects:
# - violation
raise ImportError("This is not a helpful error message.")

# IndentationError with single string literal
# Expects:
# - violation
raise IndentationError("This is not a helpful error message.")

# IndexError with single string literal
# Expects:
# - violation
raise IndexError("This is not a helpful error message.")

# InterruptedError with single string literal
# Expects:
# - violation
raise InterruptedError("This is not a helpful error message.")

# IsADirectoryError with single string literal
# Expects:
# - violation
raise IsADirectoryError("This is not a helpful error message.")

# KeyError with single string literal
# Expects:
# - violation
raise KeyError("This is not a helpful error message.")

# KeyboardInterrupt with single string literal
# Expects:
# - violation
raise KeyboardInterrupt("This is not a helpful error message.")

# LookupError with single string literal
# Expects:
# - violation
raise LookupError("This is not a helpful error message.")

# MemoryError with single string literal
# Expects:
# - violation
raise MemoryError("This is not a helpful error message.")

# ModuleNotFoundError with single string literal
# Expects:
# - violation
raise ModuleNotFoundError("This is not a helpful error message.")

# NameError with single string literal
# Expects:
# - violation
raise NameError("This is not a helpful error message.")

# NotADirectoryError with single string literal
# Expects:
# - violation
raise NotADirectoryError("This is not a helpful error message.")

# OSError with single string literal
# Expects:
# - violation
raise OSError("This is not a helpful error message.")

# OverflowError with single string literal
# Expects:
# - violation
raise OverflowError("This is not a helpful error message.")

# PermissionError with single string literal
# Expects:
# - violation
raise PermissionError("This is not a helpful error message.")

# ProcessLookupError with single string literal
# Expects:
# - violation
raise ProcessLookupError("This is not a helpful error message.")

# RecursionError with single string literal
# Expects:
# - violation
raise RecursionError("This is not a helpful error message.")

# ReferenceError with single string literal
# Expects:
# - violation
raise ReferenceError("This is not a helpful error message.")

# RuntimeError with single string literal
# Expects:
# - violation
raise RuntimeError("This is not a helpful error message.")

# StopAsyncIteration with single string literal
# Expects:
# - violation
raise StopAsyncIteration("This is not a helpful error message.")

# StopIteration with single string literal
# Expects:
# - violation
raise StopIteration("This is not a helpful error message.")

# SyntaxError with single string literal
# Expects:
# - violation
raise SyntaxError("This is not a helpful error message.")

# SystemError with single string literal
# Expects:
# - violation
raise SystemError("This is not a helpful error message.")

# SystemExit with single string literal
# Expects:
# - violation
raise SystemExit("This is not a helpful error message.")

# TabError with single string literal
# Expects:
# - violation
raise TabError("This is not a helpful error message.")

# TimeoutError with single string literal
# Expects:
# - violation
raise TimeoutError("This is not a helpful error message.")

# TypeError with single string literal
# Expects:
# - violation
raise TypeError("This is not a helpful error message.")

# UnboundLocalError with single string literal
# Expects:
# - violation
raise UnboundLocalError("This is not a helpful error message.")

# UnicodeError with single string literal
# Expects:
# - violation
raise UnicodeError("This is not a helpful error message.")

# ValueError with single string literal
# Expects:
# - violation
raise ValueError("This is not a helpful error message.")

# WindowsError with single string literal
# Expects:
# - violation
raise WindowsError("This is not a helpful error message.")

# ZeroDivisionError with single string literal
# Expects:
# - violation
raise ZeroDivisionError("This is not a helpful error message.")

# ======================================================================================
# No Violations
# The following snippets are expected to have no violations.

# Exception with no message
# Expects:
# - no violations
raise Exception()

# Placeholder string variable formatted
# This is permitted. Since the template could have been declared as a constant and used
# multiple times, the string literal `"reasons"` could be considered the contextual
# information here, hence this exception does not violate this rule.
# Expects:
# - no violations
my_template = "Your error is because of {reason}."
raise Exception(my_template.format(reason="reasons"))

# Static string literal formatted with variables
# Expects:
# - no violations
reasons = "reasons"
raise Exception("Your error is because of {reason}.".format(reason=reasons))

# Placeholder string variable formatted with f-string
# Expects:
# - no violations
reasons = "reasons"
my_f_message = f"Your error is because of {reasons}."
raise Exception(my_f_message)

# FString with variables
# Expects:
# - no violations
a = 0
if not a:
    raise Exception(f"{a!r} cannot be falsey.")

# Special Case: NotImplementedError with single string literal
# This is a special case because by definition, NotImplementedError is not
# supposed to contain any runtime context.
#
# This also provides a nice way of checking if errors from builtins will
# trigger false positives.
#
# Expects:
# - no violation
raise NotImplementedError("This is not a helpful error message.")

class Exception(Exception):
    """
    My super special exception class with a poor choice of name.
    """

# Custom exception class
# Expects:
# - no violations
raise Exception("This is not a helpful error message.")