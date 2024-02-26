class Fruit:
    # max of 7 attributes by default, can be configured
    self.attr0: int = 0
    self.attr0 = 1
    self.attr1, self.attr2 = 1, 2

    def __init__(self):
        self.worm_name = "Jimmy"
        self.worm_type = "Codling Moths"
        self.worm_color = "light brown"
        self.fruit_name = "Little Apple"
        self.fruit_color = "Bright red"
        self.fruit_vitamins = ["A", "B1"]
        self.fruit_antioxidants = None
        self.secondary_worm_name = "Kim"
        self.secondary_worm_type = "Apple maggot"
        self.secondary_worm_color = "Whitish"
        # typed assignments
        self.a_string: str = "String"
        self.a_number: float = 3.1415926
