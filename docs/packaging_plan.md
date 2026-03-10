## Packaging Plan

This document describes the packaging target for this repo.

The plan is now:

- **one package**
- **one install**
- **one extra viewer executable**
- **one tiny Python helper**

The user experience we want is:

- install a single package, for example `rerun-sdk-gs`
- keep using normal `rerun` logging
- import one extra helper:
  - `from rerun_gs import Gaussians3D, load_gaussian_ply`
- launch the custom viewer executable from that same install:
  - `rerun-gs-viewer`

That package should feel like:

- normal Rerun logging
- plus one added splat capability

It should not feel like:

- a separate product
- two separate installs
- a forked logging stack

## Core design

The package should contain both of these things:

1. A Python module
   - extends normal `rerun` usage with:
     - `Gaussians3D`
     - `load_gaussian_ply`
     - optional tiny helpers

2. A Rust viewer executable
   - launches the stock native Rerun viewer with one extra Gaussian visualizer
   - listens for normal Rerun gRPC logs

That means the package is a **platform-specific Python package with an included
binary**.

This is the simplest shape that matches the desired user experience.

## What the installed UX should look like

### Install

One install in Pixi or Conda:

```toml
[dependencies]
python = "3.12.*"
rerun-sdk-gs = "0.1.*"
```

### Run the viewer

```bash
rerun-gs-viewer
```

### Log from Python

```python
import rerun as rr
from rerun_gs import Gaussians3D, load_gaussian_ply

rr.init("my-app", spawn=False)
rr.connect_grpc("rerun+http://127.0.0.1:9876/proxy")

cloud = load_gaussian_ply("/path/to/scene.ply")
rr.log("scene/reconstruction/splats", Gaussians3D.from_cloud(cloud), static=True)
```

That is the target mental model:

- still ordinary `rerun`
- still ordinary `rr.log(...)`
- splats are just one more thing you can log

## Arbitrary paths are required

The current minimal example hardcodes `world/splats` as a simplification.

That is **not** the right long-term contract.

The package should support arbitrary entity paths, exactly like normal Rerun
logging.

Examples:

```python
rr.log("world/splats/main", Gaussians3D(...), static=True)
rr.log("scene/object_1/splats", Gaussians3D(...), static=True)
rr.log("recon/gaussians", Gaussians3D(...), static=True)
```

The viewer should render any entity whose logged components match the Gaussian
contract and whose blueprint binds that entity to the custom visualizer.

That means the eventual packaged version should:

- remove the Rust-side hardcoded `world/splats` filter
- let Python choose the entity path freely
- generate the matching blueprint override for whatever entity path was used

This is important because it keeps the behavior aligned with every other Rerun
archetype.

## Why one package is reasonable

This works because Conda/Prefix packages can ship:

- Python modules
- native executables
- shared libraries if needed

So there is no architectural reason to split the helper and the viewer into two
separate packages unless we specifically want that maintenance model.

For your use case, one package is the better fit:

- simpler install story
- easier to explain to downstream users
- closer to “just use it like Rerun, but with splats”

## What the package should contain

Recommended installed contents:

- Python module:
  - `rerun_gs/__init__.py`
  - `rerun_gs/gaussians3d.py`
  - `rerun_gs/ply.py`
- executable:
  - `bin/rerun-gs-viewer`

And package dependencies:

- `rerun-sdk == 0.30.*`
- `numpy`
- `plyfile`

The Rust viewer binary and Python helper should both stay pinned to the same
Rerun minor version.

## Recommended package name

The cleanest name is still something like:

- `rerun-sdk-gs`

Why:

- it reads as “Rerun SDK plus Gaussian splats”
- it does not pretend to replace upstream `rerun-sdk`
- it can legitimately contain both:
  - Python helper code
  - the custom viewer binary

## What should stay stock

The packaged viewer must still behave like the normal Rerun viewer.

That means:

- built-in views stay built-in
- built-in visualizers stay built-in
- timelines still work normally
- selection and inspection still work normally
- normal logged data still works normally

Examples of things that should coexist in the same viewer session:

```python
rr.log("world/points", rr.Points3D(...))
rr.log("world/camera", rr.Pinhole(...))
rr.log("world/camera/image", rr.Image(...))
rr.log("world/splats/main", Gaussians3D(...))
```

This package is an extension, not a replacement.

## Packaging source repo

This minimal repo remains the right source repo for packaging.

Reasons:

- it already has the right shape:
  - Python logger/helper
  - Rust viewer extension
- it does not carry the research and validation surface from the larger repo
- it is easier to maintain as a distributable package

The large repo should stay the implementation reference, not the thing that gets
packaged directly.

## Rattler-build shape

The package should be a platform-specific Python package built from this repo.

That means:

- not `noarch: python`
- platform builds for:
  - `linux-64`
  - `osx-arm64`

The build should:

1. install the Python helper into site-packages
2. build the Rust viewer binary
3. install the binary into `$PREFIX/bin`

In other words, one recipe should produce one installable package that includes
both the Python module and the viewer executable.

## Migration plan

### Phase 1: freeze the public contract

Before packaging, the following should be treated as the public surface:

- package name
- Python module name
- viewer executable name
- gRPC port default
- Gaussian component contract
- arbitrary entity-path support
- pinned Rerun version

### Phase 2: remove the hardcoded path assumption

The minimal example currently assumes `world/splats`.

Before packaging, the implementation should be updated so:

- Python can log splats to any entity path
- the blueprint override is generated for that exact path
- Rust visualizes any matching Gaussian entity, not just one fixed path

This is the biggest functional requirement still missing for the package target.

### Phase 3: turn the Python helper into an installable module layout

Move the current helper into a stable package layout such as:

- `python/rerun_gs/__init__.py`
- `python/rerun_gs/gaussians3d.py`
- `python/rerun_gs/ply.py`

The logging example can stay as a small script, but the helper should become a
real installed module.

### Phase 4: add a `rerun-gs-viewer` executable to the package

The installed binary should simply launch the stock viewer extension.

The package should not try to replace the stock `rerun` executable name.

Keeping the custom executable explicit is clearer and safer.

### Phase 5: package it in `ai-demos`

Add one recipe that installs:

- the Python helper module
- the Rust viewer executable

This is where your existing `rattler-build` workflow is useful.

### Phase 6: pilot in one downstream repo

Use a real downstream repo such as `vistadream`.

Success should look like:

- install one package
- run one viewer command
- log normal Rerun data plus splats in the same session

## What the downstream usage should eventually look like

```python
import rerun as rr
from rerun_gs import Gaussians3D, load_gaussian_ply

rr.init("vistadream", spawn=False)
rr.connect_grpc("rerun+http://127.0.0.1:9876/proxy")

cloud = load_gaussian_ply("/path/to/scene.ply")
rr.log("world/camera", rr.Pinhole(...))
rr.log("world/camera/image", rr.Image(...))
rr.log("world/points", rr.Points3D(...))
rr.log("world/reconstruction/splats", Gaussians3D.from_cloud(cloud), static=True)
```

That is the end goal:

- normal Rerun usage
- one extra helper import
- one extra viewer executable
- arbitrary paths
- no separate package juggling

## Recommendation

The packaging target should now be:

- **one package**: `rerun-sdk-gs`
- contains:
  - Python helper module
  - custom viewer executable
- supports:
  - arbitrary entity paths
  - normal mixed Rerun logging in the same session

That is the most natural shape for reuse in other repos.
