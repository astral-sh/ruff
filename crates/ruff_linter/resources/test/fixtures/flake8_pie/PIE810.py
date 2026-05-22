# error
obj.startswith("foo") or obj.startswith("bar")
# error
obj.endswith("foo") or obj.endswith("bar")
# error
obj.startswith(foo) or obj.startswith(bar)
# error
obj.startswith(foo) or obj.startswith("foo")
# error
obj.endswith(foo) or obj.startswith(foo) or obj.startswith("foo")

def func():
    msg = "hello world"

    x = "h"
    y = ("h", "e", "l", "l", "o")  
    z = "w"

    if msg.startswith(x) or msg.startswith(y) or msg.startswith(z): # Error
        print("yes") 

def func():
    msg = "hello world"

    if msg.startswith(("h", "e", "l", "l", "o")) or msg.startswith("h") or msg.startswith("w"): # Error
        print("yes") 

# ok
obj.startswith(("foo",  "bar"))
# ok
obj.endswith(("foo",  "bar"))
# ok
obj.startswith("foo") or obj.endswith("bar")
# ok
obj.startswith("foo") or abc.startswith("bar")

def func():
    msg = "hello world"

    x = "h"
    y = ("h", "e", "l", "l", "o")  

    if msg.startswith(x) or msg.startswith(y): # OK
        print("yes") 

def func():
    msg = "hello world"

    y = ("h", "e", "l", "l", "o")  

    if msg.startswith(y): # OK
        print("yes") 

def func():
    msg = "hello world"

    y = ("h", "e", "l", "l", "o")  

    if msg.startswith(y) or msg.startswith(y): # OK
        print("yes") 

def func():
    msg = "hello world"

    y = ("h", "e", "l", "l", "o")  
    x = ("w", "o", "r", "l", "d")

    if msg.startswith(y) or msg.startswith(x) or msg.startswith("h"): # OK
        print("yes") 

def func():
    msg = "hello world"

    y = ("h", "e", "l", "l", "o")  
    x = ("w", "o", "r", "l", "d")

    if msg.startswith(y) or msg.endswith(x) or msg.startswith("h"): # OK
        print("yes")


def func():
    "Regression test for https://github.com/astral-sh/ruff/issues/9663"
    if x.startswith("a") or x.startswith("b") or re.match(r"a\.b", x):
        print("yes")


# Regression test for https://github.com/astral-sh/ruff/issues/25232
# any(s.startswith(prefix) for prefix in (...))  is the generator form of the
# same anti-pattern this rule catches for chained `or`. Tuple/list literals are
# safe to fold; anything else is not (str.startswith rejects non-tuple).
msg = "Hello, world!"
if any(msg.startswith(p) for p in ("Hello", "Hi")):  # Error
    print("greet")

if any(msg.endswith(p) for p in ("!", "?")):  # Error
    print("punct")

if any(msg.startswith(p) for p in ["a", "b", "c"]):  # Error (list literal also folded to tuple)
    print("yes")

if any([msg.startswith(p) for p in ("x", "y")]):  # Error (list comprehension form)
    print("yes")

prefixes = ("a", "b")
if any(msg.startswith(p) for p in prefixes):  # OK (iterable is a bare name; can't be sure it's a tuple at the call site)
    print("yes")

if any(msg.startswith(p, 1) for p in ("a", "b")):  # OK (.startswith has extra args; not equivalent)
    print("yes")

if any(msg.startswith(p) for p in ("a", "b") if p != "skip"):  # OK (filter clause)
    print("yes")

if any(msg.startswith(other) for p in ("a", "b")):  # OK (call arg is not the loop var)
    print("yes")

if all(msg.startswith(p) for p in ("a", "b")):  # OK (`all`, not `any` — different semantics)
    print("yes")

class Wrap:
    msg = "Hello, world!"

w = Wrap()
if any(w.msg.startswith(p) for p in ("a", "b")):  # OK (receiver is an attribute, not a bare name)
    print("yes")

def _get():
    return "hello"

if any(_get().startswith(p) for p in ("a", "b")):  # OK (receiver is a call; folding would change call count)
    print("yes")

if any({msg.startswith(p) for p in ("a", "b")}):  # OK (set comprehension form is not folded)
    print("yes")
