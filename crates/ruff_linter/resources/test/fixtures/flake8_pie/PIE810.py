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
