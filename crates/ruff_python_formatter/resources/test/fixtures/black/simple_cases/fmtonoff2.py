import pytest

TmSt = 1
TmEx = 2

# fmt: off

# Test data:
#   Position, Volume, State, TmSt/TmEx/None, [call, [arg1...]]

@pytest.mark.parametrize('test', [

    # Test don't manage the volume
    [
        ('stuff', 'in')
    ],
])
def test_fader(test):
    pass

def check_fader(test):

    pass

def verify_fader(test):
  # misaligned comment
    pass

def verify_fader(test):
    """Hey, ho."""
    assert test.passed()

def test_calculate_fades():
    calcs = [
        # one is zero/none
        (0, 4, 0, 0, 10,        0, 0, 6, 10),
        (None, 4, 0, 0, 10,     0, 0, 6, 10),
    ]

# fmt: on
