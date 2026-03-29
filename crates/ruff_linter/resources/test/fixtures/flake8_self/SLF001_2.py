class C:
    def non_self_named_method_receiver(this):
        this._x = 0  # stable: fine, preview: fine

    @classmethod
    def non_self_named_classmethod_receiver(that):
        return that._x  # stable: fine, preview: fine

    def non_receiver_named_self_parameter(this, self):
        self._x = 1  # stable: fine, preview: error

    @classmethod
    def classmethod_named_self(self):
        return self._x  # stable: fine, preview: fine

    @staticmethod
    def staticmethod_named_self(self):
        return self._x  # stable: fine, preview: error


def top_level_self_parameter(self):
    return self._x  # stable: fine, preview: error


def local_self_binding():
    self = C()
    return self._x  # stable: fine, preview: error


self = C()


def global_self_binding():
    return self._x  # stable: fine, preview: error


def top_level_cls_parameter(cls):
    return cls._x  # stable: fine, preview: error


def local_mcs_binding():
    mcs = C
    return mcs._x  # stable: fine, preview: error
