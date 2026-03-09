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
WORLD_ROOT = "world"
SPLAT_ENTITY_PATH = "world/splats"
DEFAULT_PLY = Path(__file__).resolve().parents[1] / "examples" / "chair.ply"


def scene_path_from_argv() -> Path:
    args = sys.argv[1:]
    if len(args) > 1:
        raise SystemExit("usage: pixi run python python/log_gaussian_ply.py [scene.ply]")
    return Path(args[0]) if args else DEFAULT_PLY


def splat_blueprint(gaussians: Gaussians3D) -> rrb.Blueprint:
    # Keep the viewer stock and add only the smallest possible hint:
    # create one normal Spatial3D view rooted at `world` and tell `world/splats`
    # to use the custom Gaussian visualizer. We also seed the 3D eye pose from the cloud bounds so
    # the default stock camera starts on the scene instead of at an arbitrary origin pose.
    bounds_min = gaussians.centers.min(axis=0)
    bounds_max = gaussians.centers.max(axis=0)
    center = 0.5 * (bounds_min + bounds_max)
    extent = bounds_max - bounds_min
    distance = max(float(np.linalg.norm(extent)), 1.0) * 1.5

    return rrb.Blueprint(
        rrb.Spatial3DView(
            origin=WORLD_ROOT,
            name="Scene",
            overrides={SPLAT_ENTITY_PATH: rrb.Visualizer("GaussianSplats3D")},
            eye_controls=rrb.EyeControls3D(
                position=center + np.array([distance, distance * 0.5, distance], dtype=np.float32),
                look_target=center,
                eye_up=(0.0, 1.0, 0.0),
            ),
        )
    )


def main() -> None:
    ply_path = scene_path_from_argv()
    gaussians = Gaussians3D.from_ply(ply_path)

    rr.init(APP_ID, spawn=False)
    rr.connect_grpc(VIEWER_URL)
    rr.send_blueprint(splat_blueprint(gaussians))
    rr.log(SPLAT_ENTITY_PATH, rr.Clear(recursive=True), static=True)
    rr.log(SPLAT_ENTITY_PATH, gaussians, static=True)
    rr.disconnect()

    print(f"Logged {ply_path} to {VIEWER_URL} as {SPLAT_ENTITY_PATH}")


if __name__ == "__main__":
    main()
