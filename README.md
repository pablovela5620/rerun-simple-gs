# rerun-simple-gs

Minimal example of extending the stock Rerun viewer with one extra Gaussian splat visualizer.

## Run

Terminal 1:

```bash
pixi run viewer
```

Terminal 2:

```bash
pixi run python python/log_gaussian_ply.py
```

Use your own PLY:

```bash
pixi run python python/log_gaussian_ply.py /absolute/path/to/scene.ply
```

## How It Works

1. Python loads a static Gaussian-splat `.ply`.
2. Python builds `Gaussians3D(...)`.
3. Python sends a tiny stock `Spatial3DView` blueprint override for `world/splats`.
4. Python logs the splats to `world/splats` with normal Rerun gRPC logging.
5. The Rust viewer listens on port `9876` and stays otherwise stock.
6. One custom visualizer turns `world/splats` into renderer draw data inside the built-in `Spatial3DView`.

## What Stays Stock Rerun

This is still the normal Rerun viewer. Built-in logging still works normally in the same session:

- `Points3D`
- `Ellipsoids3D`
- `Transform3D`
- `Pinhole`
- `Image`
- `DepthImage`
- `Scalars`
- and other standard Rerun archetypes

Splats are just one added visualization path.

## Supported PLY Fields

- `x`, `y`, `z`
- `scale_0`, `scale_1`, `scale_2` as log-scales
- `opacity` as logit opacity
- `rot_0`, `rot_1`, `rot_2`, `rot_3` in `w, x, y, z` order
- `f_dc_*`
- `f_rest_*`

ASCII and binary little-endian PLY are supported.

## Non-Goals

- custom view classes
- validation or benchmarking
- offscreen rendering
- Rust-side PLY demo logging
- Python custom archetype registration
- InstantSplat or training integration
- COLMAP, timelines, animation, or compressed splat formats

## Attribution

This example was extracted from the larger working repo at `pablovela5620/rerun-custom-gs-visualizer`. The renderer behavior is kept from that repo, and the overall shape is intentionally reduced to a tiny stock-viewer extension.
