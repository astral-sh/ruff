try:
    from aa import mixer
except AttributeError:
    from bb import mixer
except RuntimeError:
    from cc import mixer
except:
    from dd import mixer
mixer(123)
