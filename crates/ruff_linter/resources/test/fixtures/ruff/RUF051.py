x1 in {"TEST"}  # [RUF051]

x2 in ["TEST"]  # [RUF051]

x3 in {"x": "TEST"}  # [RUF051]

single = ["TEST"]
single.append("TEST2")

x4 in single

single = ["TEST"]

x5 in single

x6 in [single]  # [RUF051]

x7 in [*single]

x8 in ["TEST", "TEST"] in ["TEST"]  # [RUF051]

x9 in ["TEST"] in ["TEST", "TEST"]  # [RUF051]

2 < x10 in [3]  # [RUF051]

x10 in (2,)  # [RUF051]

x11 in {"1": 1, "2": 2}

x12 in [1, 2]

x13 in {1, 2}

x14 in (1, 4)

x15 in [x for x in range(1)]

x16 in [1] in [2]  # 2x [RUF051]

17 in [17] in [[17]]  # 2x [RUF051]
