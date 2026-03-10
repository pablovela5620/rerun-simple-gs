# Rust Viewer Walkthrough

This document is for a reader who is comfortable with Python and Rerun, but not yet comfortable with Rust, WGSL, or splat rendering internals.

The goal is to explain the Rust side step by step:

- what the Rust binary is doing
- why it exists at all
- how it fits into normal Rerun logging
- how a logged Gaussian splat entity becomes pixels in the built-in 3D view

## The Big Picture

This repo has two processes:

- Python logs data
- Rust shows that data

The Python process is normal Rerun logging.

The Rust process is a normal Rerun native viewer with one extension:

- it knows how to visualize one extra kind of entity: Gaussian splats

That means the Rust side is **not** a new application framework and it is **not** a new custom UI.

It is best to think of it like this:

- take the stock Rerun viewer
- teach it one more visualization path

## Why Does Rust Listen On Port `9876`?

This is the question most people ask first.

### Short answer

Because this repo is not launching the stock `rerun` executable directly.

It is building its **own** native viewer binary that embeds the stock viewer library and registers one extra custom visualizer.

Once you make your own viewer binary, that binary has to do the same basic "receive logs" setup that the stock viewer executable would normally do for you.

### What the stock viewer normally does

If you run the normal Rerun viewer, it already knows how to:

- open a native window
- receive recordings
- listen for gRPC logs
- manage built-in views
- manage built-in visualizers

You do not usually think about any of that because the `rerun` executable hides it for you.

### What changes in this repo

This repo needs to register a custom visualizer in-process.

That means we cannot just say:

- "launch the stock binary from Python"

because the stock binary does not know about our custom Gaussian visualizer.

So instead we build a tiny Rust binary that:

1. starts the normal viewer library
2. registers our custom visualizer
3. accepts normal Rerun logs

Once you are doing that, the listener code is simply the minimal amount of stock-viewer setup that your custom binary must reproduce.

### Could this disappear in an upstream Rerun integration?

Yes.

If Gaussian splats became an official built-in Rerun visualizer, then the stock viewer itself could own that logic and this extra Rust binary would no longer be necessary.

In that world:

- Python would still log splats
- the stock viewer would already know how to render them
- this custom listening binary would be unnecessary

So the listener code exists here because this is an **extension binary**, not because listening is conceptually part of splats.

## What `main.rs` Actually Does

Read [`src/main.rs`](../src/main.rs) from top to bottom.

It is intentionally linear.

### Step 1: set up process-wide basics

The first lines do standard viewer process setup:

- logging
- crash handling

This is not splat-specific.

### Step 2: start a gRPC receiver

The call to `re_grpc_server::spawn_with_recv(...)` starts the network endpoint that accepts ordinary Rerun logging traffic.

That gives us a `log_rx` channel.

Conceptually:

- Python logs over gRPC
- the Rust side converts those incoming messages into a receiver
- the viewer reads from that receiver

The important point is that this is still **normal Rerun transport**.

There is no custom Python/Rust bridge here.

### Step 3: create the stock viewer app

`re_viewer::App::new(...)` constructs the actual Rerun native viewer.

This is the same viewer engine that the stock app uses.

At this point we still just have a standard viewer.

### Step 4: feed incoming recordings into the viewer

`viewer.add_log_receiver(log_rx);`

This is the line that makes the live gRPC stream become visible to the viewer.

Without it, the viewer would exist, but it would not receive the data Python is sending.

### Step 5: extend the built-in 3D view

This is the real customization:

- extend `Spatial3DView`
- register `GaussianSplatVisualizer`

That is the key architectural choice in this repo.

We are **not**:

- defining a new top-level view class
- replacing the viewer
- taking over the whole layout

We are only adding one more visualizer into the existing stock 3D view.

That is why normal Rerun 3D behavior still works.

## What A Visualizer Is In This Repo

If you know Rerun conceptually, a visualizer is:

- code that looks at logged components
- decides whether it knows how to interpret them
- turns them into draw data

In this repo, the custom visualizer only cares about one entity:

- an entity carrying the Gaussian component contract

and only one custom data contract:

- the custom Gaussian component descriptors emitted from Python

Everything else is still handled by built-in Rerun visualizers.

That is why this feels additive instead of invasive.

## What `src/gaussian_visualizer.rs` Does

This file is the bridge between "Rerun world" and "renderer world".

It is the most important Rust file to understand after `main.rs`.

### Its job

It does five things:

1. query the logged custom components from Rerun
2. rebuild a packed splat cloud from those component batches
3. read the active camera from the stock 3D view
4. compute a visible candidate set
5. submit one batch to the renderer

That is all.

### Why it exists

The viewer stores data the Rerun way:

- component batches
- entity paths
- view queries
- component descriptors

The renderer does **not** want to work at that level.

The renderer wants:

- arrays of means
- arrays of quaternions
- arrays of scales
- arrays of opacities
- arrays of colors
- optional SH coefficients
- camera information
- a candidate set to draw

So the visualizer is where we translate from:

- "Rerun storage/query model"

into:

- "renderer-friendly packed arrays"

### The contract match

The visualizer no longer looks for one hardcoded path.

Instead, it looks for any entity whose logged components match the Gaussian
contract:

- centers
- quaternions
- scales
- opacities
- colors
- optional SH coefficients

That makes the logging side feel much closer to normal Rerun usage:

```python
rr.log("scene/reconstruction/splats", Gaussians3D(...), static=True)
```

The visualizer is therefore an adapter over the **component contract**, not over
one special entity path.

### Rebuilding the render cloud

The helper that rebuilds the render cloud takes the component batches coming from Rerun and reconstructs one packed per-splat representation.

That is where the renderer gets:

- means
- unit quaternions
- positive scales
- opacities
- colors
- optional SH data

The cloud is cached so we do not rebuild it every frame unless the input actually changes.

### Camera extraction

The visualizer tries to read the active eye state from the stock `Spatial3DView`.

That gives us:

- camera transform
- projection
- viewport size

If the view is not ready yet, the code falls back to a simple bounds-based camera approximation.

This is just to keep the example robust during startup.

### Visible candidate selection

The renderer is expensive if you hand it every splat in every frame.

So the visualizer does a conservative first pass:

- reject obviously invisible splats
- keep splats that are plausibly visible
- sort them in a useful order

This is not the full draw algorithm.

It is just the prep step that keeps the renderer from doing unnecessary work.

## What `src/gaussian_renderer.rs` Does

This is the hard part of the system.

It is also the part you should not try to understand all at once.

Instead, think of it in layers.

### Layer 1: what goes in

The renderer receives:

- a packed Gaussian cloud
- camera information
- a prefiltered candidate list

It does **not** know anything about:

- Rerun entity queries
- Python
- PLY files
- gRPC

That is a useful mental boundary.

### Layer 2: what comes out

The renderer produces GPU draw data for the built-in 3D view.

Its job is to convert splat parameters into something the GPU can draw efficiently.

### Layer 3: the two paths

There are effectively two renderer modes:

- compute-capable path
- CPU fallback path

The compute path is the main one.
The CPU fallback exists so the example can still run on more limited backends.

The public interface stays small even though the internals are not.

### Why the renderer file is still big

Because that complexity is real.

The hard part of Gaussian splat rendering is not:

- wiring Rerun
- logging from Python

The hard part is:

- GPU resources
- batching
- compaction
- sorting
- tile work
- shader orchestration

Trying to artificially split that into ten tiny files would make the example harder to follow, not easier.

So the design choice here is:

- keep the outer repo tiny
- keep the renderer behavior
- accept that the renderer file is the one dense file

That is a deliberate tradeoff.

## A Gentle Explanation Of The Rendering Pipeline

You said to assume the reader does not know WGSL or the splatting algorithm, so here is the high-level picture.

Each Gaussian splat is basically:

- a center in 3D
- an orientation
- a scale
- a color
- an opacity
- optional spherical harmonics for view-dependent color

The renderer needs to answer:

- "what should this splat look like from the current camera?"

### Step 1: move from world space into camera space

The camera tells us where the viewer is looking from.

So for each splat, we first ask:

- where is its center relative to the camera?

If it is behind the camera, we can ignore it.

### Step 2: estimate the splat’s screen-space footprint

A Gaussian in 3D does not stay a Gaussian with the same shape on screen.

Perspective changes how big and how stretched it looks.

So the renderer projects each splat into an approximate 2D footprint on the screen.

Conceptually:

- far splats get smaller
- near splats get bigger
- anisotropic splats can appear stretched or rotated

That projected footprint is what the renderer actually draws.

### Step 3: determine color

At minimum, each splat has a base DC color.

If SH coefficients are present, the renderer also evaluates a view-dependent color from the current viewing direction.

You do not need to know SH math to understand the role:

- DC term = base color
- higher-order SH terms = directional color adjustment

### Step 4: sort / organize work

Transparent splats need careful ordering or organization.

The renderer therefore:

- filters candidates
- sorts or buckets them
- prepares GPU work buffers

This is where much of the performance complexity lives.

### Step 5: rasterize

Finally, the GPU turns those projected splats into pixels.

The shader code handles:

- footprint evaluation
- coverage
- color contribution
- compositing

Even if you do not know WGSL, you can still understand the purpose:

- the visualizer prepares data
- the renderer prepares GPU work
- the shaders do the per-pixel math

## What The Shader Files Are For

You do not need to read WGSL first to understand the system.

Treat the shader directory as "GPU implementation details for the renderer".

The Rust renderer code decides:

- which buffers exist
- what goes in them
- which pass runs when

The shader files define:

- what each GPU pass actually computes

So if you are Python-first and new to Rust, the right order is:

1. understand `main.rs`
2. understand `gaussian_visualizer.rs`
3. understand the public shape of `gaussian_renderer.rs`
4. only then look at shader files if you want to go deeper

## Why The Python Logger Sends A Tiny Blueprint

This is worth repeating because it is the one part that feels surprising.

Built-in archetypes like `Points3D` do not need extra binding because the stock viewer already
knows which visualizer should render them.

Gaussian splats in this repo are still a custom visualizer. The stock viewer learns about that
visualizer only because our Rust binary registers it at startup. To make a custom entity render
like part of the normal 3D scene, the logger sends one tiny stock blueprint that says:

- create a normal `Spatial3DView`
- root it at `/`
- bind the chosen entity path to `Visualizer("GaussianSplats3D")`
- start the camera in a sensible pose based on the splat bounds

That is still normal Rerun behavior.

It is just the standard blueprint override mechanism used for a custom visualizer.

## What Still Works Exactly Like Normal Rerun

This point matters if you want to explain the design to other Rerun users.

All of these still work the normal way:

- `rr.log("world/points", rr.Points3D(...))`
- `rr.log("world/camera", rr.Transform3D(...))`
- `rr.log("world/camera", rr.Pinhole(...))`
- `rr.log("world/camera/image", rr.Image(...))`
- `rr.log("plot/value", rr.Scalars(...))`

The viewer has not been turned into a "Gaussian app".

It is still the normal viewer with one extra visualization path.

That is the most important architectural idea in this repo.

## What To Read Next If You Want To Change Things

If you want to change Python logging:

- read `python/gaussians3d.py`

If you want to change the Rerun-side data contract:

- read `python/gaussians3d.py`
- then `src/gaussian_visualizer.rs`

If you want to change rendering:

- read `src/gaussian_renderer.rs`
- then the shader files

If you want to explain the system to someone else:

- start from `README.md`
- then this file
- then `python/log_gaussian_ply.py`
- then `src/main.rs`

## Summary

The Rust side exists for one reason:

- the stock viewer does not know how to visualize Gaussian splats by itself

So this repo provides the smallest custom viewer binary that:

- receives ordinary Rerun logs
- keeps the stock Rerun UX
- adds one new splat visualizer

Python remains the normal logger.

That is the design in one sentence:

- **Python logs**
- **Rust extends the stock viewer**
