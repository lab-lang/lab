import math

import pytest

from lab_isaac.isaac_env import _quaternion_to_rpy


def test_wxyz_quaternion_becomes_isaac_command_euler_angles() -> None:
    half_turn = math.sqrt(0.5)

    roll, pitch, yaw = _quaternion_to_rpy((half_turn, 0.0, 0.0, half_turn))

    assert roll == pytest.approx(0.0)
    assert pitch == pytest.approx(0.0)
    assert yaw == pytest.approx(math.pi / 2.0)
