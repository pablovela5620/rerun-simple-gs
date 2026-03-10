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

Choose an arbitrary entity path:

```bash
pixi run python python/log_gaussian_ply.py /absolute/path/to/scene.ply scene/reconstruction/splats
```

## How It Works

1. Python loads a static Gaussian-splat `.ply`.
2. Python builds `Gaussians3D(...)`.
3. Python logs the splats to the chosen entity path with normal Rerun gRPC logging.
4. Python sends one tiny stock `Spatial3DView` blueprint override rooted at `/` for the chosen splat entity path.
5. The Rust viewer listens on port `9876`, stays otherwise stock, and one custom visualizer turns any logged `GaussianSplats3D` entity into draw data inside the built-in `Spatial3DView`.

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

## Learn More

For a deeper walkthrough of the Python contract, the Rust viewer extension, and the end-to-end data flow, see [docs/architecture.md](docs/architecture.md).

For a Rust-focused walkthrough aimed at a Python-first reader, see [docs/rust_viewer.md](docs/rust_viewer.md).

For the plan to package this viewer/helper pair for reuse in other repos, see
[docs/packaging_plan.md](docs/packaging_plan.md).
