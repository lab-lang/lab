"""Isaac Lab environment construction, imported only inside Isaac Python."""

from __future__ import annotations

import math
from typing import Any

from .contract import Prototype, Quaternion


def _quaternion_to_rpy(quaternion: Quaternion) -> tuple[float, float, float]:
    w, x, y, z = quaternion
    sin_roll_cos_pitch = 2.0 * (w * x + y * z)
    cos_roll_cos_pitch = 1.0 - 2.0 * (x * x + y * y)
    roll = math.atan2(sin_roll_cos_pitch, cos_roll_cos_pitch)
    sin_pitch = 2.0 * (w * y - z * x)
    pitch = (
        math.copysign(math.pi / 2.0, sin_pitch) if abs(sin_pitch) >= 1.0 else math.asin(sin_pitch)
    )
    sin_yaw_cos_pitch = 2.0 * (w * z + x * y)
    cos_yaw_cos_pitch = 1.0 - 2.0 * (y * y + z * z)
    yaw = math.atan2(sin_yaw_cos_pitch, cos_yaw_cos_pitch)
    return roll, pitch, yaw


def object_reached_goal(
    env: Any,
    position_tolerance_m: float,
    orientation_tolerance_rad: float,
    max_linear_velocity_mps: float,
    max_angular_velocity_radps: float,
    minimum_gripper_open_m: float,
    robot_cfg: Any,
) -> Any:
    """Require the transferred object to be released at rest at the goal."""
    import torch
    from isaaclab.utils.math import combine_frame_transforms, quat_error_magnitude

    command = env.command_manager.get_command("object_pose")
    robot = env.scene[robot_cfg.name]
    object_asset = env.scene["object"]
    goal_position_w, goal_quaternion_w = combine_frame_transforms(
        robot.data.root_pos_w,
        robot.data.root_quat_w,
        command[:, :3],
        command[:, 3:7],
    )
    position_error = torch.linalg.vector_norm(goal_position_w - object_asset.data.root_pos_w, dim=1)
    orientation_error = quat_error_magnitude(goal_quaternion_w, object_asset.data.root_quat_w)
    linear_speed = torch.linalg.vector_norm(object_asset.data.root_lin_vel_w, dim=1)
    angular_speed = torch.linalg.vector_norm(object_asset.data.root_ang_vel_w, dim=1)
    gripper_released = torch.all(
        robot.data.joint_pos[:, robot_cfg.joint_ids] > minimum_gripper_open_m,
        dim=1,
    )
    return (
        (position_error < position_tolerance_m)
        & (orientation_error < orientation_tolerance_rad)
        & (linear_speed < max_linear_velocity_mps)
        & (angular_speed < max_angular_velocity_radps)
        & gripper_released
    )


def object_goal_success_reward(env: Any, **params: Any) -> Any:
    """Sparse completion reward paired exactly with the terminal predicate."""
    return object_reached_goal(env, **params).float()


def build_env_cfg(prototype: Prototype, *, num_envs: int | None = None) -> Any:
    """Adapt Isaac Lab's Franka relative-IK lift task to the transfer contract."""
    try:
        import isaaclab.sim as sim_utils
        from isaaclab.managers import RewardTermCfg, SceneEntityCfg, TerminationTermCfg
        from isaaclab_tasks.manager_based.manipulation.lift.config.franka.ik_rel_env_cfg import (
            FrankaCubeLiftEnvCfg,
        )
    except ModuleNotFoundError as error:
        raise RuntimeError(
            "Isaac Lab is unavailable; run this command inside an Isaac Lab Python environment"
        ) from error

    binding = prototype.binding
    cfg = FrankaCubeLiftEnvCfg()
    cfg.scene.num_envs = num_envs or binding.simulation.num_envs
    cfg.scene.env_spacing = binding.simulation.env_spacing_m
    cfg.sim.dt = binding.simulation.dt_seconds
    cfg.decimation = binding.simulation.decimation
    cfg.sim.render_interval = binding.simulation.decimation
    cfg.episode_length_s = binding.simulation.episode_length_seconds

    physics = binding.object
    cfg.scene.object.spawn = sim_utils.CuboidCfg(
        size=physics.size_m,
        rigid_props=sim_utils.RigidBodyPropertiesCfg(
            solver_position_iteration_count=16,
            solver_velocity_iteration_count=1,
            max_depenetration_velocity=5.0,
        ),
        mass_props=sim_utils.MassPropertiesCfg(mass=physics.mass_kg),
        collision_props=sim_utils.CollisionPropertiesCfg(),
        physics_material=sim_utils.RigidBodyMaterialCfg(
            static_friction=physics.static_friction,
            dynamic_friction=physics.dynamic_friction,
        ),
        visual_material=sim_utils.PreviewSurfaceCfg(diffuse_color=(0.82, 0.76, 0.30)),
    )
    cfg.scene.object.init_state.pos = binding.source.position_m
    cfg.scene.object.init_state.rot = binding.source.quaternion_wxyz

    source = binding.source
    jitter = source.position_jitter_m
    pose_range = cfg.events.reset_object_position.params["pose_range"]
    pose_range["x"] = (-jitter[0], jitter[0])
    pose_range["y"] = (-jitter[1], jitter[1])
    pose_range["z"] = (-jitter[2], jitter[2])

    destination = binding.destination
    destination_jitter = destination.position_jitter_m
    roll, pitch, yaw = _quaternion_to_rpy(destination.quaternion_wxyz)
    ranges = cfg.commands.object_pose.ranges
    ranges.pos_x = (
        destination.position_m[0] - destination_jitter[0],
        destination.position_m[0] + destination_jitter[0],
    )
    ranges.pos_y = (
        destination.position_m[1] - destination_jitter[1],
        destination.position_m[1] + destination_jitter[1],
    )
    ranges.pos_z = (
        destination.position_m[2] - destination_jitter[2],
        destination.position_m[2] + destination_jitter[2],
    )
    ranges.roll = (roll, roll)
    ranges.pitch = (pitch, pitch)
    ranges.yaw = (yaw, yaw)
    cfg.commands.object_pose.resampling_time_range = (
        binding.simulation.episode_length_seconds,
        binding.simulation.episode_length_seconds,
    )

    # The stock lift task gates goal rewards on a cube being lifted. The
    # transfer endpoint is the table, so keep the lift incentive but allow
    # goal tracking down to the plate's settled center height.
    settled_center_height = min(source.position_m[2], destination.position_m[2])
    cfg.rewards.lifting_object.params["minimal_height"] = settled_center_height + 0.05
    minimum_goal_height = max(0.001, settled_center_height - 0.005)
    cfg.rewards.object_goal_tracking.params["minimal_height"] = minimum_goal_height
    cfg.rewards.object_goal_tracking_fine_grained.params["minimal_height"] = minimum_goal_height
    success_params = {
        "position_tolerance_m": binding.goal.position_tolerance_m,
        "orientation_tolerance_rad": binding.goal.orientation_tolerance_rad,
        "max_linear_velocity_mps": binding.goal.max_linear_velocity_mps,
        "max_angular_velocity_radps": binding.goal.max_angular_velocity_radps,
        "minimum_gripper_open_m": binding.goal.minimum_gripper_open_m,
    }
    cfg.rewards.task_success = RewardTermCfg(
        func=object_goal_success_reward,
        params={
            **success_params,
            "robot_cfg": SceneEntityCfg("robot", joint_names=["panda_finger_joint.*"]),
        },
        weight=100.0,
    )
    cfg.terminations.task_success = TerminationTermCfg(
        func=object_reached_goal,
        params={
            **success_params,
            "robot_cfg": SceneEntityCfg("robot", joint_names=["panda_finger_joint.*"]),
        },
    )
    return cfg


def run_smoke(prototype: Prototype, *, num_envs: int | None, steps: int) -> JsonObject:
    """Launch PhysX, reset parallel environments, and take policy-shaped steps."""
    try:
        import torch
        from isaaclab.app import AppLauncher
    except ModuleNotFoundError as error:
        raise RuntimeError(
            "Isaac Lab is unavailable; run this command inside an Isaac Lab Python environment"
        ) from error

    launcher = AppLauncher(headless=True)
    simulation_app = launcher.app
    environment: Any | None = None
    try:
        from isaaclab.envs import ManagerBasedRLEnv

        cfg = build_env_cfg(prototype, num_envs=num_envs)
        environment = ManagerBasedRLEnv(cfg=cfg)
        environment.reset()
        action = torch.zeros(
            (environment.num_envs, environment.action_manager.total_action_dim),
            device=environment.device,
        )
        for _ in range(steps):
            environment.step(action)
        return {
            "status": "isaac-smoke-passed",
            "task": prototype.task.task_id,
            "environments": environment.num_envs,
            "action_dimensions": environment.action_manager.total_action_dim,
            "steps": steps,
        }
    finally:
        if environment is not None:
            environment.close()
        simulation_app.close()


JsonObject = dict[str, object]
