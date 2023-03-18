x = 0 # [VN001]
y = 1 # [VN001]
z = 3 # [VN001]
i = 4
_ = 21
T = 12

def a():
    a = "hi"

def test(test=False):
    pass

def test2(t=False): # [VN001]
    pass

try:
    pass
except Exception as e: # [VN001]
    pass
