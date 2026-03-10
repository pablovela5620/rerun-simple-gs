"""Load a Gaussian PLY in Python and log it to the external Rust viewer."""

from __future__ import annotations

import sys
from pathlib import Path

import numpy as np
import rerun as rr
import rerun.blueprint as rrb

from gaussians3d import Gaussians3D

APP_ID = "rerun-simple-gs"
VIEWER_URL = "rerun+http://127.0.0.1:9876/proxy"
VIEW_ROOT = "/"
DEFAULT_ENTITY_PATH = "world/splats"
DEFAULT_PLY = Path(__file__).resolve().parents[1] / "examples" / "chair.ply"


def args_from_argv() -> tuple[Path, str]:
    args = sys.argv[1:]
    if len(args) > 2:
        raise SystemExit(
            "usage: pixi run python python/log_gaussian_ply.py [scene.ply] [entity/path]"
        )
    ply_path = Path(args[0]) if args else DEFAULT_PLY
    entity_path = args[1] if len(args) == 2 else DEFAULT_ENTITY_PATH
    return ply_path, entity_path


def splat_blueprint(entity_path: str, gaussians: Gaussians3D) -> rrb.Blueprint:
    """Create the smallest stock blueprint that binds one entity to the custom visualizer."""
    bounds_min = gaussians.centers.min(axis=0)
    bounds_max = gaussians.centers.max(axis=0)
    center = 0.5 * (bounds_min + bounds_max)
    extent = bounds_max - bounds_min
    distance = max(float(np.linalg.norm(extent)), 1.0) * 1.5

    return rrb.Blueprint(
        rrb.Spatial3DView(
            origin=VIEW_ROOT,
            name="Scene",
            overrides={entity_path: rrb.Visualizer("GaussianSplats3D")},
            eye_controls=rrb.EyeControls3D(
                position=center + np.array([distance, distance * 0.5, distance], dtype=np.float32),
                look_target=center,
                eye_up=(0.0, 1.0, 0.0),
            ),
        )
    )


def main() -> None:
    ply_path, entity_path = args_from_argv()
    gaussians = Gaussians3D.from_ply(ply_path)

    rr.init(APP_ID, spawn=False)
    rr.connect_grpc(VIEWER_URL)
    rr.send_blueprint(splat_blueprint(entity_path, gaussians))
    rr.log(entity_path, rr.Clear(recursive=True), static=True)
    rr.log(entity_path, gaussians, static=True)
    rr.disconnect()

    print(f"Logged {ply_path} to {VIEWER_URL} as {entity_path}")


if __name__ == "__main__":
    main()
