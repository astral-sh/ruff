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
    except Exception:
        raise
        print("this code is pointless!")

def bad():
    try:
        process()
    except:
        # I am a comment, not a statement!
        raise

def bad():
    try:
        process()
    except Exception:
        raise

def bad():
    try:
        process()
    except Exception as e:
        raise

def bad():
    try:
        process()
    except Exception as e:
        raise e

def bad():
    try:
        process()
    except MyException:
        raise
    except Exception:
        raise

def bad():
    try:
        process()
    except MyException as e:
        raise e
    except Exception as e:
        raise e

def bad():
    try:
        process()
    except MyException as ex:
        raise ex
    except Exception as e:
        raise e

def fine():
    try:
        process()
    except Exception as e:
        raise e from None

def fine():
    try:
        process()
    except Exception as e:
        raise e from Exception

def fine():
    try:
        process()
    except Exception as e:
        raise ex

def fine():
    try:
        process()
    except MyException:
        raise
    except Exception:
        print("bar")

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
