try:
    from aa import mixer
except ImportError:
    pass
else:
    from bb import mixer
mixer(123)
