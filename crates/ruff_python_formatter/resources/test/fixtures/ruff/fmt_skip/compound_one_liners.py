# Test cases for fmt: skip on compound statements that fit on one line

# Basic single-line compound statements
def simple_func(): return "hello"  # fmt: skip
if True: print("condition met")  # fmt: skip
for i in range(5): print(i)  # fmt: skip
while x < 10: x += 1  # fmt: skip

# With expressions that would normally trigger formatting
def long_params(a, b, c, d, e, f, g): return a + b + c + d + e + f + g  # fmt: skip
if some_very_long_condition_that_might_wrap: do_something_else_that_is_long()  # fmt: skip

# Nested compound statements (outer should be preserved)
if True:
    for i in range(10): print(i)  # fmt: skip

# Multiple statements in body (should not apply - multiline)
if True:
    x = 1
    y = 2  # fmt: skip

# With decorators - decorated function on one line
@overload
def decorated_func(x: int) -> str: return str(x)  # fmt: skip

@property
def prop_method(self): return self._value  # fmt: skip

# Class definitions on one line
class SimpleClass: pass  # fmt: skip
class GenericClass(Generic[T]): pass  # fmt: skip

# Try/except blocks
try: risky_operation()  # fmt: skip
except ValueError: handle_error()  # fmt: skip
except: handle_any_error()  # fmt: skip
else: success_case()  # fmt: skip
finally: cleanup()  # fmt: skip

# Match statements (Python 3.10+)
match value:
    case 1: print("one")  # fmt: skip
    case _: print("other")  # fmt: skip

# With statements
with open("file.txt") as f: content = f.read()  # fmt: skip
with context_manager() as cm: result = cm.process()  # fmt: skip

# Async variants
async def async_func(): return await some_call()  # fmt: skip
async for item in async_iterator(): await process(item)  # fmt: skip
async with async_context() as ctx: await ctx.work()  # fmt: skip

# Complex expressions that would normally format
def complex_expr(): return [x for x in range(100) if x % 2 == 0 and x > 50]  # fmt: skip
if condition_a and condition_b or (condition_c and not condition_d): execute_complex_logic()  # fmt: skip

# Edge case: comment positioning
def func_with_comment():  # some comment
    return "value"  # fmt: skip

# Edge case: multiple fmt: skip (only last one should matter)
def multiple_skip(): return "test"  # fmt: skip  # fmt: skip

# Should NOT be affected (already multiline)
def multiline_func():
    return "this should format normally"

if long_condition_that_spans \
   and continues_on_next_line:
    print("multiline condition")

# Mix of skipped and non-skipped
for i in range(10): print(f"item {i}")  # fmt: skip
for j in range(5):
    print(f"formatted item {j}")

# With trailing comma that would normally be removed
def trailing_comma_func(a, b, c,): return a + b + c  # fmt: skip

# Dictionary/list comprehensions
def dict_comp(): return {k: v for k, v in items.items() if v is not None}  # fmt: skip
def list_comp(): return [x * 2 for x in numbers if x > threshold_value]  # fmt: skip

# Lambda in one-liner
def with_lambda(): return lambda x, y, z: x + y + z if all([x, y, z]) else None  # fmt: skip

# String formatting that would normally be reformatted
def format_string(): return f"Hello {name}, you have {count} items in your cart totaling ${total:.2f}"  # fmt: skip

# loop else clauses
for i in range(2): print(i) # fmt: skip
else: print("this") # fmt: skip


while foo(): print(i) # fmt: skip
else: print("this") # fmt: skip

# again but only the first skip
for i in range(2): print(i) # fmt: skip
else: print("this")


while foo(): print(i) # fmt: skip
else: print("this")

# again but only the second skip
for i in range(2): print(i)
else: print("this") # fmt: skip


while foo(): print(i)
else: print("this") # fmt: skip

# multiple statements in body
if True: print("this"); print("that") # fmt: skip

# Examples with more comments

try: risky_operation()  # fmt: skip
# leading 1
except ValueError: handle_error()  # fmt: skip
# leading 2
except: handle_any_error()  # fmt: skip
# leading 3
else: success_case()  # fmt: skip
# leading 4
finally: cleanup()  # fmt: skip
# trailing
