"""Command-line checks and the Linux/CUDA Isaac smoke gate."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import NoReturn

from .contract import ContractError, load_prototype


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="lab-isaac")
    commands = parser.add_subparsers(dest="command", required=True)
    inspect = commands.add_parser("inspect", help="validate and resolve a task plus binding")
    inspect.add_argument("--task", type=Path, required=True)
    inspect.add_argument("--binding", type=Path, required=True)
    inspect.add_argument("--json", action="store_true")
    smoke = commands.add_parser("smoke", help="launch PhysX and step the manager-based RL env")
    smoke.add_argument("--task", type=Path, required=True)
    smoke.add_argument("--binding", type=Path, required=True)
    smoke.add_argument("--num-envs", type=int)
    smoke.add_argument("--steps", type=int, default=4)
    return parser


def _fail(parser: argparse.ArgumentParser, message: str) -> NoReturn:
    parser.exit(2, f"error: {message}\n")


def main() -> None:
    parser = _parser()
    arguments = parser.parse_args()
    try:
        prototype = load_prototype(arguments.task, arguments.binding)
        if arguments.command == "inspect":
            summary = prototype.summary()
            if arguments.json:
                print(json.dumps(summary, indent=2))
            else:
                print(
                    f"task '{prototype.task.task_id}': {prototype.task.object_name} "
                    f"from {prototype.task.source_station} to "
                    f"{prototype.task.destination_station}\n"
                    f"  robot: {prototype.binding.robot_model} "
                    f"({prototype.binding.controller})\n"
                    f"  calibration: {prototype.binding.calibration}\n"
                    f"  scene: {prototype.task.scene_path}"
                )
            return
        if arguments.num_envs is not None and arguments.num_envs <= 0:
            _fail(parser, "--num-envs must be positive")
        if arguments.steps <= 0:
            _fail(parser, "--steps must be positive")
        from .isaac_env import run_smoke

        print(
            json.dumps(
                run_smoke(
                    prototype,
                    num_envs=arguments.num_envs,
                    steps=arguments.steps,
                ),
                indent=2,
            )
        )
    except (ContractError, RuntimeError) as error:
        _fail(parser, str(error))


if __name__ == "__main__":
    main()
