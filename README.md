# gsplat-rerun-minimal

Minimal Rust example showing how to load a Gaussian-splat PLY, log a custom `GaussianSplats3D` archetype, and render it inside Rerun's built-in `Spatial3DView`.

## Run

Bundled chair:

```bash
cargo run
```

Fast bundled chair run with Pixi-managed dependencies:

```bash
pixi run example-chair
```

Your own PLY:

```bash
cargo run -- /absolute/path/to/scene.ply
```

Or with Pixi-managed dependencies:

```bash
pixi run view-ply -- /absolute/path/to/scene.ply
```

For the fastest local run outside Pixi, use:

```bash
cargo run --release -- /absolute/path/to/scene.ply
```

## How It Works

1. `src/ply.rs` loads a static Gaussian-splat PLY into a typed cloud.
2. `src/main.rs` logs that cloud once as `GaussianSplats3D`.
3. `src/main.rs` starts the native Rerun viewer and registers one custom visualizer.
4. `src/gaussian_visualizer.rs` queries the archetype, builds the render cloud, and computes the visible candidate set.
5. `src/gaussian_renderer.rs` runs the known-working splat renderer and draws inside `Spatial3DView`.

## Supported PLY Fields

- `x`, `y`, `z`
- `scale_0`, `scale_1`, `scale_2` as log-scales
- `opacity` as logit opacity
- `rot_0`, `rot_1`, `rot_2`, `rot_3` in `w, x, y, z` order
- `f_dc_*`
- `f_rest_*`

ASCII and binary little-endian PLY are supported.

## Non-Goals

- validation tooling
- benchmarks
- comparison tooling
- Python logging
- COLMAP
- animation
- compressed splat formats
- extra viewer UI or research modes

## Attribution

This example was extracted from the larger working repo at `pablovela5620/rerun-custom-gs-visualizer`. The renderer path is Brush-inspired, but the repo is intentionally reduced to the smallest educational Rerun integration that still renders real Gaussian splats.
