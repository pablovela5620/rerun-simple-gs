"""Tiny Python-side Gaussian splat logging helper."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

import numpy as np
import numpy.typing as npt
import rerun as rr
from plyfile import PlyData

SH_C0 = np.float32(0.2820948)


def _component_descriptor(component: str, component_type: str) -> rr.ComponentDescriptor:
    return rr.ComponentDescriptor(
        archetype="GaussianSplats3D",
        component=component,
        component_type=component_type,
    )


def _as_float32(name: str, values: npt.ArrayLike, shape_tail: tuple[int, ...]) -> npt.NDArray[np.float32]:
    array = np.asarray(values, dtype=np.float32)
    if array.ndim != len(shape_tail) + 1 or tuple(array.shape[1:]) != shape_tail:
        msg = f"{name} must have shape [N, {', '.join(map(str, shape_tail))}]"
        raise ValueError(msg)
    return np.ascontiguousarray(array)


def _as_float32_1d(name: str, values: npt.ArrayLike) -> npt.NDArray[np.float32]:
    array = np.asarray(values, dtype=np.float32)
    if array.ndim != 1:
        raise ValueError(f"{name} must have shape [N]")
    return np.ascontiguousarray(array)


def _normalize_quaternions_xyzw(quaternions_xyzw: npt.NDArray[np.float32]) -> npt.NDArray[np.float32]:
    norms = np.linalg.norm(quaternions_xyzw, axis=1, keepdims=True)
    identity = np.array([[0.0, 0.0, 0.0, 1.0]], dtype=np.float32)
    return np.where(norms > 1e-12, quaternions_xyzw / np.maximum(norms, 1e-12), identity)


def _sigmoid(x: npt.NDArray[np.float32]) -> npt.NDArray[np.float32]:
    return 1.0 / (1.0 + np.exp(-x))


def _sh_dc_to_rgb(dc_coefficients: npt.NDArray[np.float32]) -> npt.NDArray[np.float32]:
    return np.maximum(dc_coefficients * SH_C0 + 0.5, 0.0)


@dataclass(frozen=True)
class Gaussians3D(rr.AsComponents):
    """Minimal Python logging wrapper for the Rust Gaussian splat visualizer."""

    centers: npt.NDArray[np.float32]
    quaternions_xyzw: npt.NDArray[np.float32]
    scales: npt.NDArray[np.float32]
    opacities: npt.NDArray[np.float32]
    colors_dc: npt.NDArray[np.float32]
    sh_coefficients: npt.NDArray[np.float32] | None = None

    def __post_init__(self) -> None:
        centers = _as_float32("centers", self.centers, (3,))
        quaternions = _normalize_quaternions_xyzw(
            _as_float32("quaternions_xyzw", self.quaternions_xyzw, (4,))
        )
        scales = np.maximum(_as_float32("scales", self.scales, (3,)), 1e-6)
        opacities = np.clip(_as_float32_1d("opacities", self.opacities), 0.0, 1.0)
        colors_dc = np.clip(_as_float32("colors_dc", self.colors_dc, (3,)), 0.0, None)

        num_splats = centers.shape[0]
        for name, array in {
            "quaternions_xyzw": quaternions,
            "scales": scales,
            "opacities": opacities,
            "colors_dc": colors_dc,
        }.items():
            if array.shape[0] != num_splats:
                raise ValueError(f"{name} must have the same leading dimension as centers")

        object.__setattr__(self, "centers", centers)
        object.__setattr__(self, "quaternions_xyzw", quaternions)
        object.__setattr__(self, "scales", scales)
        object.__setattr__(self, "opacities", opacities)
        object.__setattr__(self, "colors_dc", colors_dc)

        if self.sh_coefficients is not None:
            sh = np.asarray(self.sh_coefficients, dtype=np.float32)
            if sh.ndim != 3 or sh.shape[0] != num_splats or sh.shape[2] != 3:
                raise ValueError("sh_coefficients must have shape [N, coeffs_per_channel, 3]")
            object.__setattr__(self, "sh_coefficients", np.ascontiguousarray(sh))

    def as_component_batches(self) -> list[rr.DescribedComponentBatch]:
        color_bytes = np.round(np.clip(self.colors_dc, 0.0, 1.0) * 255.0).astype(np.uint8)
        batches: list[rr.DescribedComponentBatch] = [
            rr.components.Translation3DBatch(self.centers).described(
                _component_descriptor("GaussianSplats3D:centers", "rerun.components.Translation3D")
            ),
            rr.components.RotationQuatBatch(self.quaternions_xyzw).described(
                _component_descriptor(
                    "GaussianSplats3D:quaternions", "rerun.components.RotationQuat"
                )
            ),
            rr.components.Scale3DBatch(self.scales).described(
                _component_descriptor("GaussianSplats3D:scales", "rerun.components.Scale3D")
            ),
            rr.components.OpacityBatch(self.opacities).described(
                _component_descriptor("GaussianSplats3D:opacities", "rerun.components.Opacity")
            ),
            rr.components.ColorBatch(color_bytes).described(
                _component_descriptor("GaussianSplats3D:colors", "rerun.components.Color")
            ),
        ]

        if self.sh_coefficients is not None:
            batches.append(
                rr.components.TensorDataBatch(
                    [
                        rr.datatypes.TensorData(
                            array=self.sh_coefficients,
                            dim_names=["splat", "coefficient", "channel"],
                        )
                    ]
                ).described(
                    _component_descriptor(
                        "GaussianSplats3D:sh_coefficients",
                        "rerun.components.TensorData",
                    )
                )
            )

        return batches

    @classmethod
    def from_ply(cls, path: Path) -> "Gaussians3D":
        ply = PlyData.read(path)
        vertex = ply["vertex"].data
        names = set(vertex.dtype.names or ())

        centers = np.stack([vertex["x"], vertex["y"], vertex["z"]], axis=1).astype(np.float32)
        scales = np.exp(
            np.stack([vertex["scale_0"], vertex["scale_1"], vertex["scale_2"]], axis=1).astype(
                np.float32
            )
        )
        quaternions_xyzw = np.stack(
            [vertex["rot_1"], vertex["rot_2"], vertex["rot_3"], vertex["rot_0"]], axis=1
        ).astype(np.float32)
        opacities = _sigmoid(np.asarray(vertex["opacity"], dtype=np.float32)).astype(np.float32)

        dc_coefficients: npt.NDArray[np.float32] | None = None
        if {"f_dc_0", "f_dc_1", "f_dc_2"} <= names:
            dc_coefficients = np.stack(
                [vertex["f_dc_0"], vertex["f_dc_1"], vertex["f_dc_2"]], axis=1
            ).astype(np.float32)
            colors_dc = _sh_dc_to_rgb(dc_coefficients)
        elif {"red", "green", "blue"} <= names:
            colors_dc = (
                np.stack([vertex["red"], vertex["green"], vertex["blue"]], axis=1).astype(
                    np.float32
                )
                / 255.0
            )
        else:
            colors_dc = np.ones((len(vertex), 3), dtype=np.float32)

        rest_names = sorted(
            (
                name
                for name in names
                if name.startswith("f_rest_") and name[len("f_rest_") :].isdigit()
            ),
            key=lambda name: int(name[len("f_rest_") :]),
        )

        sh_coefficients: npt.NDArray[np.float32] | None = None
        if dc_coefficients is not None or rest_names:
            extra_coefficients = len(rest_names) // 3
            coeffs_per_channel = extra_coefficients + 1
            sh_coefficients = np.zeros((len(vertex), coeffs_per_channel, 3), dtype=np.float32)
            if dc_coefficients is not None:
                sh_coefficients[:, 0, :] = dc_coefficients

            # `f_rest_*` is channel-major: all red coefficients, then green, then blue.
            for coefficient_index in range(extra_coefficients):
                sh_coefficients[:, coefficient_index + 1, 0] = vertex[f"f_rest_{coefficient_index}"]
                sh_coefficients[:, coefficient_index + 1, 1] = vertex[
                    f"f_rest_{extra_coefficients + coefficient_index}"
                ]
                sh_coefficients[:, coefficient_index + 1, 2] = vertex[
                    f"f_rest_{extra_coefficients * 2 + coefficient_index}"
                ]

        return cls(
            centers=centers,
            quaternions_xyzw=quaternions_xyzw,
            scales=scales,
            opacities=opacities,
            colors_dc=colors_dc,
            sh_coefficients=sh_coefficients,
        )
