# Errors
"{.real}".format(1)
"{0.real}".format(1)
"{a.real}".format(a=1)

"{.real}".format(1.0)
"{0.real}".format(1.0)
"{a.real}".format(a=1.0)

"{.real}".format(1j)
"{0.real}".format(1j)
"{a.real}".format(a=1j)

"{.real}".format(0b01)
"{0.real}".format(0b01)
"{a.real}".format(a=0b01)

"{}".format(1 + 2)
"{}".format([1, 2])
"{}".format({1, 2})
"{}".format({1: 2, 3: 4})
"{}".format((i for i in range(2)))

"{.real}".format(1 + 2)
"{.real}".format([1, 2])
"{.real}".format({1, 2})
"{.real}".format({1: 2, 3: 4})
"{}".format((i for i in range(2)))
