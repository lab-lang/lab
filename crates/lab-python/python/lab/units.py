"""Units of measure.

Lab writes a measurement as a magnitude followed by a unit and checks the unit
rather than assuming it. Python needs an operator between the two, so `20 uL`
is written `20 * uL` and `100 ng/uL` is written `100 * ng / uL`.

The common units are importable names. Any other unit the compiler accepts is
reachable by attribute, so this module never becomes a second list of what Lab
allows:

    from lab.units import uL
    import lab.units

    volume = 20 * uL
    speed = 300 * lab.units.rpm
"""

from __future__ import annotations

from ._expressions import Unit

#: Units whose Lab spelling is not a usable Python identifier. `min` is the
#: builtin, so the name here is the word rather than the abbreviation.
_ALIASES = {"minutes": "min", "seconds": "s", "hours": "h", "days": "d"}

_KNOWN = (
    "L",
    "mL",
    "uL",
    "nL",
    "g",
    "mg",
    "ug",
    "ng",
    "M",
    "mM",
    "uM",
    "nM",
    "C",
    "s",
    "h",
    "d",
    "bp",
    "kb",
)

_units: dict[str, Unit] = {name: Unit(name) for name in _KNOWN}
_units.update({alias: Unit(spelling) for alias, spelling in _ALIASES.items()})

L = _units["L"]
mL = _units["mL"]
uL = _units["uL"]
nL = _units["nL"]
g = _units["g"]
mg = _units["mg"]
ug = _units["ug"]
ng = _units["ng"]
M = _units["M"]
mM = _units["mM"]
uM = _units["uM"]
nM = _units["nM"]
C = _units["C"]
s = _units["s"]
h = _units["h"]
d = _units["d"]
bp = _units["bp"]
kb = _units["kb"]
seconds = _units["seconds"]
minutes = _units["minutes"]
hours = _units["hours"]
days = _units["days"]


def __getattr__(name: str) -> Unit:
    if name.startswith("_"):
        raise AttributeError(name)
    if name not in _units:
        _units[name] = Unit(name)
    return _units[name]
