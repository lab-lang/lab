"""Build the reporter strain, and watch its plate until it has something to say."""

import lab
from lab import ColonyMap, Event, Material, Strain

from .observe import ColonyGrowth, PlateObservation, grow_colonies
from .workflow import build_reporter

module = lab.Module("reporter.main", doc=__doc__)


@lab.record
class ColoniesReady(Event):
    """A plate reached the point where there was something worth picking."""

    colonies: ColonyMap


@lab.record
class PlateAbandoned(Event):
    """A plate was given up on: nothing more was going to grow."""

    observations: list[PlateObservation]


@lab.workflow
def main(wf: lab.Context) -> Material[Strain]:
    """Build the reporter strain, and watch its plate.

    The compiler derives the build order from the material each call
    consumes rather than from the order written here.
    """
    strain, plate = wf.perform(build_reporter())
    growth = wf.perform(grow_colonies(plate))

    match growth:
        case ColonyGrowth.Ready():
            wf.emit(ColoniesReady(colonies=growth.colonies))
            wf.perform(lab.dispose(growth.plate))

        case ColonyGrowth.TimedOut():
            wf.emit(PlateAbandoned(observations=growth.observations))
            wf.perform(lab.dispose(growth.plate))

    return strain
