def func():
    try:
        print("try")
    except:
        print("except")

def func():
    x = 1
    try:
        print("try")
    except:
        print("except")
    x = 2
        
def func():
    try:
        print("try")
    except ValueError:
        print("value error")
    except TypeError:
        print("type error")

def func():
    try:
        for i in [1,2,3]:
            foo(i)
    except:
        print("whoops")

def func():
    try:
        print("try")
    except:
        print("except")
    else:
        print("else")

def func():
    x = 1
    try:
        print("try")
    except:
        print("except")
    else:
        print("else")
    x = 2
        
def func():
    try:
        print("try")
    except ValueError:
        print("value error")
    except TypeError:
        print("type error")
    else:
        print("else")

def func():
    try:
        for i in [1,2,3]:
            foo(i)
    except:
        print("whoops")
    else:
        print("else")


def func():
    try:
        print("try")
    finally:
        print("finally")

def func():
    x = 1
    try:
        print("try")
    finally:
        print("finally")
    x = 2
        

def func():
    try:
        for i in [1,2,3]:
            foo(i)
    finally:
        print("finally")

def func():
    try:
        print("try")
    except:
        print("except")
    finally:
        print("finally")

def func():
    x = 1
    try:
        print("try")
    except:
        print("except")
    finally:
        print("finally")
    x = 2
        

def func():
    try:
        for i in [1,2,3]:
            foo(i)
    except:
        print("except")
    finally:
        print("finally")
def func():
    try:
        print("try")
    except:
        print("except")
    else:
        print("else")
    finally:
        print("finally")

def func():
    x = 1
    try:
        print("try")
    except:
        print("except")
    else:
        print("else")
    finally:
        print("finally")
    x = 2
        

def func():
    try:
        for i in [1,2,3]:
            foo(i)
    except:
        print("except")
    else:
        print("else")
    finally:
        print("finally")

def func():
    try:
        return 1/0
    except:
        print("whoops!")
# def func():
#     try:
#         print("try")
#     except Exception:
#         print("Exception")
#     except OtherException as e:
#         print("OtherException")
#     else:
#         print("else")
#     finally:
#         print("finally")

# def func():
#     try:
#         print("try")
#     except:
#         print("Exception")

# def func():
#     try:
#         print("try")
#     except:
#         print("Exception")
#     except OtherException as e:
#         print("OtherException")

# def func():
#     try:
#         print("try")
#     except Exception:
#         print("Exception")
#     except OtherException as e:
#         print("OtherException")

# def func():
#     try:
#         print("try")
#     except Exception:
#         print("Exception")
#     except OtherException as e:
#         print("OtherException")
#     else:
#         print("else")

# def func():
#     try:
#         print("try")
#     finally:
#         print("finally")

# def func():
#     try:
#         return 0
#     except:
#         return 1
#     finally:
#         return 2

# def func():
#     try:
#         raise Exception()
#     except:
#         print("reached")

# def func():
#     try:
#         assert False
#         print("unreachable")
#     except:
#         print("reached")

# def func():
#     try:
#         raise Exception()
#     finally:
#         print('reached')
#         return 2

# def func():
#     try:
#         assert False
#         print("unreachable")
#     finally:
#         print("reached")

# # Test case from ibis caused overflow
# def func():
#     try:
#         if catalog is not None:
#             try:
#                 x = 0
#             except PySparkParseException:
#                 x = 1
#         try:
#             x = 2
#         except PySparkParseException:
#             x = 3
#         x = 8
#     finally:
#         if catalog is not None:
#             try:
#                 x = 4
#             except PySparkParseException:
#                 x = 5
#         try:
#             x = 6
#         except PySparkParseException:
#             x = 7


# def func():
#     try:
#         assert False
#     except ex:
#         raise ex

#     finally:
#         raise Exception("other")

# # previously caused infinite loop
# # found by fuzzer
# def func():
#  for i in():
#     try:
#         try:
#          while r:
#           if t:break
#         finally:()
#         return
#     except:l

