class C:
    def non_self_named_method_receiver(this):
        this._x = 0  # fine

    @classmethod
    def non_self_named_classmethod_receiver(that):
        return that._x  # fine

    def non_receiver_named_self_parameter(this, self):
        self._x = 1  # fine

    @classmethod
    def classmethod_named_self(self):
        return self._x  # fine

    @staticmethod
    def staticmethod_named_self(self):
        return self._x  # fine


def top_level_self_parameter(self):
    return self._x  # fine


def local_self_binding():
    self = C()
    return self._x  # fine


self = C()


def global_self_binding():
    return self._x  # fine


def top_level_cls_parameter(cls):
    return cls._x  # fine


def local_mcs_binding():
    mcs = C
    return mcs._x  # fine
