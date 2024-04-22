def func():
    try:
        print("try")
    except Exception:
        print("Exception")
    except OtherException as e:
        print("OtherException")
    else:
        print("else")
    finally:
        print("finally")

def func():
    try:
        print("try")
    except:
        print("Exception")

def func():
    try:
        print("try")
    except:
        print("Exception")
    except OtherException as e:
        print("OtherException")

def func():
    try:
        print("try")
    except Exception:
        print("Exception")
    except OtherException as e:
        print("OtherException")

def func():
    try:
        print("try")
    except Exception:
        print("Exception")
    except OtherException as e:
        print("OtherException")
    else:
        print("else")

def func():
    try:
        print("try")
    finally:
        print("finally")

def func():
    try:
        return 0
    except:
        return 1
    finally:
        return 2

def func():
    try:
        raise Exception()
    except:
        print("reached")

def func():
    try:
        assert False
        print("unreachable")
    except:
        print("reached")

def func():
    try:
        raise Exception()
    finally:
        print('reached')
        return 2

def func():
    try:
        assert False
        print("unreachable")
    finally:
        print("reached")

