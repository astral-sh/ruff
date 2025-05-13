import pytest

# Examples for rule PTQA002: test classes (name starts with "Test") must have a decorator @pytest.mark.team_<word>

class TestNoDecorator:                # PTQA002
    def test_something(self):
        assert True

@pytest.mark.team_alpha                # OK
class TestWithAlphaTeam:
    def test_feature(self):
        assert True

@pytest.mark.team_beta123              # OK: numbers in team name allowed
class TestWithBeta123Team:
    def test_another_feature(self):
        assert True

@pytest.mark.team_                     # PTQA002: missing team name after underscore
class TestEmptyTeamName:
    def test_empty(self):
        assert True

@pytest.mark.other_mark                # PTQA002: wrong marker, not team_*
class TestWrongMarker:
    def test_wrong(self):
        assert True

@pytest.mark.team_delta
@pytest.mark.smoke                      # OK: additional markers allowed
class TestMultipleMarkers:
    def test_multi(self):
        assert True

class HelperClass:                      # OK: does not start with "Test"
    def helper_method(self):
        pass

@pytest.mark.team_gamma
class TestWithGammaTeam:
    def test_gamma(self):
        assert True

@pytest.mark.team_delta.extra            # PTQA002: dot in team name invalid
class TestWithDotInTeam:
    def test_dot(self):
        assert True

@pytest.mark.team_underscore_allowed    # OK: underscores allowed within team name
class TestWithUnderscoreInTeam:
    def test_underscore(self):
        assert True
