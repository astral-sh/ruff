import unittest

class MyViolatingTestCase(unittest.TestCase):
    def setUp(self):
        # Missing super().setUp()
        self.resource = "Test resource"

    def tearDown(self):
        del self.resource
        # Missing super().tearDown()

    def setUpClass(cls):
        # Missing super().setUpClass()
        cls.shared_resource = "Shared resource"

    def tearDownClass(cls):
        del cls.shared_resource
        # Missing super().tearDownClass()

class MyNonViolatingTestCase(unittest.TestCase):
    def setUp(self):
        super().setUp()
        self.resource = "Test resource"

    def tearDown(self):
        del self.resource
        super().tearDown()

    def setUpClass(cls):
        super().setUpClass()
        cls.shared_resource = "Shared resource"

    def tearDownClass(cls):
        del cls.shared_resource
        super().tearDownClass()