# Architecture

This repo is intentionally small, but there are still two different halves to understand:

- Python is the **logger**.
- Rust is the **viewer extension**.

The mental model is:

- Python logs normal Rerun data.
- One of those logged entities is a Gaussian splat entity chosen by Python.
- The Rust binary is the normal native Rerun viewer, plus one extra visualizer that knows how to draw that entity as Gaussian splats.

If you already know Rerun and Python, the only "new" part is the contract between:

- `python/gaussians3d.py`
- `src/gaussian_visualizer.rs`

Everything else is standard Rerun plumbing or renderer implementation details.

## Read Order

If you are new to the repo, read files in this order:

1. `README.md`
2. `python/log_gaussian_ply.py`
3. `python/gaussians3d.py`
4. `src/main.rs`
5. `src/gaussian_visualizer.rs`
6. `src/gaussian_renderer.rs`

That gives the shortest path from "what does the user run?" to "what actually gets drawn?"

## End-To-End Flow

The runtime flow is:

1. Start the viewer:
   - `pixi run viewer`
2. Python loads a Gaussian PLY:
   - `pixi run python python/log_gaussian_ply.py`
3. Python decodes that file into arrays:
   - centers
   - quaternions
   - scales
   - opacities
   - DC color
   - optional SH coefficients
4. Python wraps those arrays in `Gaussians3D(...)`.
5. `Gaussians3D.as_component_batches()` emits custom-described Rerun component batches.
6. Python logs those batches to the chosen entity path.
7. Python sends one tiny stock `Spatial3DView` blueprint override for that same entity path.
8. The Rust viewer receives the recording over normal Rerun gRPC.
9. The custom visualizer finds whichever entities logged the Gaussian component contract, rebuilds the renderer-facing cloud, computes visible candidates, and submits draw data.
10. The renderer draws splats into the built-in `Spatial3DView`.

There is no Rust-side PLY loading in this repo anymore.

## Why This Is A Stock Viewer Extension

The Rust binary does only two non-standard things:

1. It listens for Rerun logs on port `9876`.
2. It registers one extra visualizer on the built-in `Spatial3DView`.

It does **not**:

- create a custom view class
- replace the Rerun UI
- own the recording
- generate demo data
- load PLY files itself

That means the rest of the viewer stays stock:

- normal entity logging
- built-in views
- built-in visualizers
- selection
- timelines
- images
- cameras
- points
- plots
- transforms

Splats are additive, not a replacement for the rest of Rerun.

## Python Side

### `python/log_gaussian_ply.py`

This is the example entrypoint.

Its job is deliberately small:

1. Pick a PLY path.
2. Load it with `Gaussians3D.from_ply(...)`.
3. Connect to the external viewer over gRPC.
4. Log the splats once to any chosen entity path such as `world/splats` or `scene/reconstruction/splats`.
5. Exit.

Two design choices are important here:

### 1. The logger sends the tiny custom-visualizer override

Built-in Rerun archetypes do not need any extra hint because the stock viewer already knows which
visualizer should render them.

Our splats are still a custom visualizer. The idiomatic way to bind a custom visualizer today is a
small stock blueprint override on the entity path that should use it.

So Python sends the smallest possible stock blueprint:

- create one normal `Spatial3DView`
- root it at `/`
- bind the chosen splat entity path to `Visualizer("GaussianSplats3D")`
- seed one reasonable initial camera from the splat bounds

This keeps the viewer stock and uses the normal Rerun blueprint override path for custom data.

### 2. The entity path is chosen by the logger

This repo keeps only two defaults:

- entity path: defaulting to `world/splats`, but overridable by the logger
- port: `9876`

That is intentional. This is still an example, not a generalized plugin system.

The visualizer now keys off the Gaussian component contract rather than one fixed path.

### `python/gaussians3d.py`

This file defines the Python logging helper.

`Gaussians3D` is not a formal generated Rerun archetype. It is just a tiny ergonomic wrapper that implements `rr.AsComponents`.

That means the user-facing callsite is still normal Rerun:

```python
rr.log("scene/reconstruction/splats", Gaussians3D(...), static=True)
```

The important method is:

- `Gaussians3D.as_component_batches()`

That method emits ordinary `DescribedComponentBatch` values with custom component descriptors.

Those descriptors are the contract the Rust visualizer reads.

## The Gaussian Entity Contract

The Python and Rust sides agree on the following logical fields:

- centers: `[N, 3]`
- quaternions: `[N, 4]` in `xyzw`
- scales: `[N, 3]`
- opacities: `[N]`
- colors: `[N, 3]`
- optional SH tensor: `[N, coeffs_per_channel, 3]`

In the recording, those become custom-described Rerun components:

- `GaussianSplats3D:centers`
- `GaussianSplats3D:quaternions`
- `GaussianSplats3D:scales`
- `GaussianSplats3D:opacities`
- `GaussianSplats3D:colors`
- `GaussianSplats3D:sh_coefficients`

The descriptors also declare which built-in component type the payload uses:

- `Translation3D`
- `RotationQuat`
- `Scale3D`
- `Opacity`
- `Color`
- `TensorData`

This is the main trick that makes the example small:

- Python does not need a special extension mechanism.
- Rust does not need a custom transport.
- Both sides just agree on component descriptors and payload layouts.

## PLY Loading Semantics

PLY loading moved entirely to Python.

The loader is intentionally format-specific and supports the same practical static Gaussian PLY family as the larger working repo:

- `x`, `y`, `z`
- `scale_0..2`
- `opacity`
- `rot_0..3`
- `f_dc_*`
- `f_rest_*`

The decode rules are:

- scales are activated with `exp`
- opacity is activated with `sigmoid`
- quaternions are normalized
- PLY quaternion order `wxyz` is reordered to logging order `xyzw`
- `f_dc_*` is interpreted as SH coefficient 0 and converted to base color with the same activation as the Rust renderer path
- higher-order SH is packed as `[splat, coefficient, channel]`
- coefficient 0 is included when SH is present

The `f_rest_*` layout is assumed to be channel-major:

- all red coefficients
- then all green coefficients
- then all blue coefficients

That matches the working renderer contract from the larger repo.

## Rust Side

### `src/main.rs`

`main.rs` is intentionally tiny and linear.

It does four things:

1. Set up logging and crash handlers.
2. Start a gRPC server on `127.0.0.1:9876`.
3. Launch the native Rerun viewer.
4. Register `GaussianSplatVisualizer` onto the built-in `Spatial3DView`.

It does **not** own any scene logic.

That separation is the point of this repo.

### `src/gaussian_visualizer.rs`

This is the bridge between Rerun data and the renderer.

Its responsibilities are:

1. Query the custom splat components from any entity carrying the Gaussian contract.
2. Decode the queried component batches into one renderer-facing cloud.
3. Extract the active 3D camera from the view when possible.
4. Build a visible candidate set.
5. Submit draw data to `GaussianRenderer`.

It intentionally does not try to become a generalized data layer.

It knows about:

- the custom component descriptors
- the renderer input format

and that is enough.

One subtle but important detail:

Rerun entity paths may be surfaced with a leading slash, but the visualizer no longer relies on a fixed entity path. It keys off the custom component contract instead.

### `src/gaussian_renderer.rs`

This is the complex part, and it is intentionally left mostly intact.

The renderer is already the hard problem:

- GPU resources
- compute path
- draw path
- candidate compaction
- splat shading
- shader orchestration

Trying to "simplify" it by rewriting the algorithm would make the repo less trustworthy.

So the repo simplifies everything **around** the renderer and keeps the renderer behavior.

The visualizer hands the renderer:

- a cached cloud
- a camera approximation
- sorted visible candidates

and the renderer handles the rest.

## Why The Python Side Still Sends A Blueprint

The viewer stays stock, but this example still sends one small stock blueprint from Python.

Why?

Because a recording that contains only a custom Gaussian entity does not automatically behave like a
built-in archetype yet. The viewer knows about the custom visualizer because Rust registered it, but
it still needs the normal blueprint hint that says:

- create a `Spatial3DView`
- root that view at `/` so arbitrary splat paths and built-in entities share the same scene
- bind this entity path to `Visualizer("GaussianSplats3D")`

That is not a custom view and it is not a custom transport. It is just the normal Rerun blueprint
mechanism used in the smallest possible way for a custom visualizer.

## How Normal Rerun Logging Coexists

This viewer does not special-case the whole recording. It only adds one extra visualization path.

That means a Python script could just as well do:

```python
rr.log("world/camera", rr.Transform3D(...))
rr.log("world/camera", rr.Pinhole(...))
rr.log("world/camera/image", rr.Image(...))
rr.log("world/points", rr.Points3D(...))
rr.log("world/reconstruction/splats", Gaussians3D(...), static=True)
```

The built-in archetypes would still be handled by built-in Rerun visualizers.

Any entity carrying the Gaussian component contract will be handled by the custom Gaussian visualizer.

That is the main architectural takeaway.

## What To Change If You Extend This Example

If you want to evolve the example, these are the safe places to change:

- Change Python-side PLY decode semantics in `python/gaussians3d.py`
- Change the custom component contract in both:
  - `python/gaussians3d.py`
  - `src/gaussian_visualizer.rs`
- Change rendering behavior in:
  - `src/gaussian_renderer.rs`
  - `shader/*`

These are the places to avoid growing unless truly necessary:

- `src/main.rs`
- the task surface in `pixi.toml`
- the logger shape in `python/log_gaussian_ply.py`

The repo stays understandable because the outer flow is small.

## Summary

This example works because it keeps one boundary explicit:

- Python logs the custom splat contract to any chosen entity path
- Rust teaches the stock viewer how to visualize that one entity

Everything else remains ordinary Rerun.
