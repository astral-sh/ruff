"""
Violation:

Checks for uses of `raise` directly after a `rescue`.
"""
class MyException(Exception):
    pass

def bad():
    try:
        process()
    except Exception:
        raise

def bad():
    try:
        process()
    except:
        # I am a comment, not a statement!
        raise

def fine():
    try:
        process()
    except Exception:
        print("initiating rapid unscheduled disassembly of program")

def fine():
    try:
        process()
    except MyException:
        print("oh no!")
        raise

def fine():
    try:
        process()
    except Exception:
        if True:
            raise


def fine():
    try:
        process()
    finally:
        # I am a comment, not a statement!
        print("but i am a statement")
        raise
