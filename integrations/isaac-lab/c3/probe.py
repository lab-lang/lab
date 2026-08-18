"""Bounded C3 host and Isaac Lab capability probe."""

from __future__ import annotations

import argparse
import json
import os
import platform
import shutil
import subprocess
import sys
from pathlib import Path

JsonObject = dict[str, object]


def _arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--task", type=Path, required=True)
    parser.add_argument("--binding", type=Path, required=True)
    parser.add_argument("--num-envs", type=int, default=32)
    parser.add_argument("--steps", type=int, default=8)
    return parser.parse_args()


def _command(arguments: list[str]) -> str:
    result = subprocess.run(arguments, check=True, capture_output=True, text=True)
    return result.stdout.strip()


def _host_report() -> JsonObject:
    disk = shutil.disk_usage(Path.cwd())
    return {
        "platform": platform.platform(),
        "machine": platform.machine(),
        "python": sys.version,
        "libc": platform.libc_ver(),
        "cpu_count": os.cpu_count(),
        "workspace_disk_bytes": {
            "total": disk.total,
            "free": disk.free,
        },
        "c3": {
            "hardware_profile": os.environ.get("C3_HARDWARE_PROFILE"),
            "hardware_kind": os.environ.get("C3_HARDWARE_KIND"),
            "accelerator_kind": os.environ.get("C3_ACCELERATOR_KIND"),
        },
        "nvidia_smi": _command(
            [
                "nvidia-smi",
                "--query-gpu=name,driver_version,memory.total",
                "--format=csv,noheader,nounits",
            ]
        ),
    }


def _write_report(report: JsonObject) -> Path:
    destination = Path(os.environ.get("C3_ARTIFACTS_DIR", "artifacts"))
    destination.mkdir(parents=True, exist_ok=True)
    path = destination / "capability.json"
    path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    return path


def main() -> None:
    arguments = _arguments()
    report: JsonObject = {
        "format": "lab.compute-capability.v0",
        "provider": "c3",
        "status": "failed",
    }
    try:
        report["host"] = _host_report()

        import torch

        report["torch"] = {
            "version": torch.__version__,
            "cuda_available": torch.cuda.is_available(),
            "cuda_version": torch.version.cuda,
            "device_count": torch.cuda.device_count(),
            "device_name": torch.cuda.get_device_name(0) if torch.cuda.is_available() else None,
        }
        if not torch.cuda.is_available():
            raise RuntimeError("PyTorch cannot see a CUDA device")

        from lab_isaac.contract import load_prototype
        from lab_isaac.isaac_env import run_smoke

        prototype = load_prototype(arguments.task, arguments.binding)
        report["smoke"] = run_smoke(
            prototype,
            num_envs=arguments.num_envs,
            steps=arguments.steps,
        )
        report["status"] = "passed"
    except Exception as error:
        report["error"] = {
            "type": type(error).__name__,
            "message": str(error),
        }
        _write_report(report)
        raise
    path = _write_report(report)
    print(json.dumps({"status": report["status"], "artifact": str(path)}))


if __name__ == "__main__":
    main()
