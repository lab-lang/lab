"""Watch a plate until enough colonies appear or growth times out."""

from typing import Any

import lab
from lab import ColonyMap, Duration, Evidential, Image, Material, Plate, detect_colonies
from lab.units import h, minutes

module = lab.Module("golden_gate_extended.workflows.observe", doc=__doc__)


@lab.record
class PlateObservation(Evidential):
    """One image, its colonies, and the plate's elapsed growth time."""

    image: Image
    colonies: ColonyMap
    elapsed: Duration


@lab.record
class ColonyGrowth:
    """The observations and outcome produced by watching a plate."""

    plate: Material[Plate]
    observations: list[PlateObservation]

    @lab.case
    class Ready:
        colonies: ColonyMap

    @lab.case
    class TimedOut:
        pass


@lab.workflow
def await_colonies(wf: lab.Context, plate: Material[Plate]) -> ColonyGrowth:
    """Image every half hour and stop when the plate is useful or timed out."""
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
