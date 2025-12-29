class Cycle:
    def method_a(self):
        return self.method_b()

    def method_b(self):
        return self.method_a()
