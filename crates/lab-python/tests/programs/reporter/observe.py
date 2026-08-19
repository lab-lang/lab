"""Watch a plate until it has something worth picking from."""

from typing import Any

import lab
from lab import (
    ColonyMap,
    Duration,
    Evidential,
    Image,
    Material,
    Plate,
    detect_colonies,
)
from lab.units import h, minutes

module = lab.Module("reporter.observe", doc=__doc__)


@lab.record
class PlateObservation(Evidential):
    """One image, what was counted in it, and how long the plate had grown."""

    image: Image
    colonies: ColonyMap
    elapsed: Duration


@lab.record
class ColonyGrowth:
    """What watching a plate produced."""

    plate: Material[Plate]
    observations: list[PlateObservation]

    @lab.case
    class Ready:
        colonies: ColonyMap

    @lab.case
    class TimedOut:
        pass


@lab.workflow
def grow_colonies(wf: lab.Context, plate: Material[Plate]) -> ColonyGrowth:
    """Image every half hour, and stop at the first plate worth picking from."""
    observations = wf.state(list[PlateObservation], [])

    @wf.every(30 * minutes)
    def observe() -> Any:
        image = wf.perform(lab.capture(plate))
        colonies = detect_colonies(image)
        observations.append(PlateObservation(image=image, colonies=colonies, elapsed=wf.elapsed))
        if colonies.isolated.count >= 8:
            return ColonyGrowth.Ready(
                plate=plate,
                colonies=colonies,
                observations=observations,
            )

    @wf.after(18 * h)
    def give_up() -> Any:
        return ColonyGrowth.TimedOut(plate=plate, observations=observations)
