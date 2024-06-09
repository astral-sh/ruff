class AnimalBad:
    ...


class BeakyAnimal(AnimalBad):
    ...


class FurryAnimal(AnimalBad): 
    ...


class Swimmer(AnimalBad):
    ...


class EggLayer(AnimalBad):
    ...


class VenomousAnimal(AnimalBad):
    ...


class ProtectedSpecies(AnimalBad):
    ...


class BeaverTailedAnimal(AnimalBad): 
    ...


class VertebrateBad(AnimalBad):
    ...


# Too many ancestors.
class PlatypusBad(
    BeakyAnimal,
    FurryAnimal,
    Swimmer,
    EggLayer,
    VenomousAnimal,
    ProtectedSpecies,
    BeaverTailedAnimal,
    VertebrateBad,
):
    ...


# Not too many ancestors.
class Snake(
    Swimmer,
    EggLayer,
    VenomousAnimal,
    ProtectedSpecies,
    VertebrateBad,
):
    ...


# Not too many ancestors.
class AnimalGood:
    beaver_tailed: bool
    can_swim: bool
    has_beak: bool
    has_fur: bool
    has_vertebrae: bool
    lays_egg: bool
    protected_species: bool
    venomous: bool


class Invertebrate(AnimalGood):
    has_vertebrae = False


class VertebrateGood(AnimalGood):
    has_vertebrae = True


class Mammal(VertebrateGood):
    has_beak = False
    has_fur = True
    lays_egg = False
    venomous = False


class PlatypusGood(Mammal):
    beaver_tailed = True
    can_swim = True
    has_beak = True
    lays_egg = True
    protected_species = True
    venomous = True
