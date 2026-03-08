//! Small binary entry point.
//!
//! The whole example flow lives here:
//! choose a PLY, load it, log one `GaussianSplats3D` entity, register the custom visualizer, and
//! start the native Rerun viewer.

mod gaussian_archetype;
mod gaussian_renderer;
mod gaussian_visualizer;
mod ply;

use std::env;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gaussian_archetype::GaussianSplats3D;
use ply::{GaussianSet, load_gaussian_ply};
use re_log_channel::LogSender;
use re_log_types::{BlueprintActivationCommand, LogMsg};
use re_sdk::sink::CallbackSink;
use re_sdk_types::View as _;
use re_sdk_types::blueprint::archetypes::{
    Background, ContainerBlueprint, LineGrid3D, PanelBlueprint, SpatialInformation,
    TimePanelBlueprint, ViewBlueprint, ViewContents, ViewportBlueprint,
};
use re_sdk_types::blueprint::components::{
    ActiveTab, AutoLayout, AutoViews, BackgroundKind, ContainerKind, PanelState, RootContainer,
    ViewClass, ViewOrigin,
};
use re_sdk_types::components::{Color as BlueprintColor, Name, Visible};
use re_sdk_types::datatypes::{
    Bool, EntityPath as BlueprintEntityPath, Rgba32, Uuid as BlueprintUuid,
};
use re_viewer::external::eframe;

const APP_ID: &str = "gsplat-rerun-minimal";
const RECORDING_NAME: &str = "Gaussian Splats Minimal";
const SPLAT_PATH: &str = "world/splats";
const WORLD_ROOT: &str = "world";
const VIEW_ID_STR: &str = "11111111-1111-1111-1111-111111111111";
const CONTAINER_ID_STR: &str = "22222222-2222-2222-2222-222222222222";
const CONTAINER_ID_BYTES: [u8; 16] = [0x22; 16];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Keep startup linear and visible:
    // 1. load one PLY
    // 2. log one custom archetype
    // 3. start one stock Spatial3D viewer with one custom visualizer registered
    re_log::setup_logging();
    re_crash_handler::install_crash_handlers(re_viewer::build_info());

    let scene_path = scene_path_from_args()?;
    let gaussians = load_gaussian_ply(&scene_path)?;

    let (tx, rx) = re_log_channel::log_channel(re_log_channel::LogSource::Sdk);
    let recording = recording_stream(tx)?;
    send_startup_blueprint(&recording, &gaussians)?;
    log_gaussians(&recording, &gaussians)?;
    recording.flush_blocking()?;

    let main_thread_token = re_viewer::MainThreadToken::i_promise_i_am_on_the_main_thread();
    let app_env = re_viewer::AppEnvironment::Custom(RECORDING_NAME.to_owned());
    let startup_options = re_viewer::StartupOptions {
        persist_state: false,
        ..Default::default()
    };

    eframe::run_native(
        "Rerun Viewer",
        native_options(),
        Box::new(move |cc| {
            re_viewer::customize_eframe_and_setup_renderer(cc)?;

            let mut viewer = re_viewer::App::new(
                main_thread_token,
                re_viewer::build_info(),
                app_env,
                startup_options,
                cc,
                None,
                re_viewer::AsyncRuntimeHandle::from_current_tokio_runtime_or_wasmbindgen()
                    .expect("tokio runtime should exist"),
            );
            viewer.add_log_receiver(rx);
            viewer.extend_view_class(
                re_sdk_types::blueprint::views::Spatial3DView::identifier(),
                |registrator| {
                    registrator
                        .register_visualizer::<gaussian_visualizer::GaussianSplatVisualizer>()?;
                    Ok(())
                },
            )?;

            Ok(Box::new(viewer))
        }),
    )
    .map_err(|err| anyhow::anyhow!(err))
}

fn scene_path_from_args() -> anyhow::Result<PathBuf> {
    // The example supports exactly one optional positional path argument. Keeping CLI parsing this
    // small avoids hiding the real rendering flow behind a framework.
    let mut args = env::args_os();
    let _program = args.next();
    let path = match (args.next(), args.next()) {
        (None, None) => bundled_chair_path(),
        (Some(path), None) => PathBuf::from(path),
        _ => anyhow::bail!("usage: cargo run -- [scene.ply]"),
    };
    Ok(path)
}

fn bundled_chair_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("chair.ply")
}

fn recording_stream(log_tx: LogSender) -> anyhow::Result<rerun::RecordingStream> {
    let recording = rerun::RecordingStreamBuilder::new(APP_ID)
        .recording_name(RECORDING_NAME)
        .buffered()?;
    // The native viewer reads from the same in-memory stream through a callback sink, which keeps
    // the example self-contained and avoids any network or SDK server setup.
    recording.set_sink(Box::new(CallbackSink::new(move |msgs| {
        for msg in msgs {
            let _ = log_tx.send(msg.clone().into());
        }
    })));
    Ok(recording)
}

fn log_gaussians(
    recording: &rerun::RecordingStream,
    gaussians: &GaussianSet,
) -> anyhow::Result<()> {
    // The archetype mirrors the renderer-facing layout closely:
    // world-space means, quaternions, scales, opacity, DC color, and optional SH tensor.
    let colors = gaussians.colors_dc.iter().copied().map(|rgb| {
        rerun::Color::from_rgb(
            (rgb[0].clamp(0.0, 1.0) * 255.0) as u8,
            (rgb[1].clamp(0.0, 1.0) * 255.0) as u8,
            (rgb[2].clamp(0.0, 1.0) * 255.0) as u8,
        )
    });

    let mut archetype = GaussianSplats3D::new(
        gaussians.means_world.iter().copied(),
        gaussians.quats.iter().copied(),
        gaussians.scales.iter().copied(),
        gaussians.opacities.iter().copied(),
        colors,
    );

    if let Some(sh_coeffs) = &gaussians.sh_coeffs {
        // Higher-order SH stays packed as `[splat, coefficient, channel]` so the visualizer can
        // forward it directly into the compute path without repacking the recording data.
        archetype = archetype.with_sh_coefficients(
            rerun::datatypes::TensorData::new(
                vec![
                    gaussians.len() as u64,
                    sh_coeffs.coeffs_per_channel as u64,
                    3,
                ],
                rerun::datatypes::TensorBuffer::F32(sh_coeffs.coefficients.clone().into()),
            )
            .with_dim_names(["splat", "coefficient", "channel"]),
        );
    }

    recording.log_static(SPLAT_PATH, &rerun::Clear::recursive())?;
    recording.log_static(SPLAT_PATH, &archetype)?;
    Ok(())
}

fn send_startup_blueprint(
    recording: &rerun::RecordingStream,
    gaussians: &GaussianSet,
) -> anyhow::Result<()> {
    // The blueprint is intentionally tiny: one black-background Spatial3D view whose initial eye
    // is chosen from the cloud bounds so the bundled chair is visible on first launch.
    let bounds = gaussians.bounds();
    let (center, extent) = bounds
        .map(|(min, max)| (0.5 * (min + max), max - min))
        .unwrap_or((glam::Vec3::ZERO, glam::Vec3::ONE));
    let distance = extent.length().max(1.0) * 1.5;
    let position = center + glam::Vec3::new(distance, distance * 0.5, distance);

    let (blueprint, storage) = re_sdk::RecordingStreamBuilder::new(APP_ID)
        .blueprint()
        .memory()?;
    blueprint.set_time_sequence("blueprint", 0);

    let view_path = format!("view/{VIEW_ID_STR}");
    blueprint.log(
        format!("{view_path}/ViewContents"),
        &ViewContents::new(["$origin/**"]),
    )?;
    blueprint.log(
        view_path.as_str(),
        &ViewBlueprint::new(ViewClass("3D".into()))
            .with_display_name(Name("Scene".into()))
            .with_space_origin(ViewOrigin(WORLD_ROOT.into()))
            .with_visible(Visible(Bool(true))),
    )?;
    blueprint.log(
        format!("{view_path}/Background"),
        &Background::new(BackgroundKind::SolidColor).with_color(BlueprintColor(Rgba32(0x000000ff))),
    )?;
    blueprint.log(
        format!("{view_path}/LineGrid3D"),
        &LineGrid3D::new().with_visible(false),
    )?;
    blueprint.log(
        format!("{view_path}/SpatialInformation"),
        &SpatialInformation::update_fields()
            .with_show_axes(false)
            .with_show_bounding_box(false),
    )?;
    blueprint.log(
        format!("{view_path}/EyeControls3D"),
        &re_sdk_types::blueprint::archetypes::EyeControls3D::new()
            .with_position(re_sdk_types::components::Position3D::new(
                position.x, position.y, position.z,
            ))
            .with_look_target(re_sdk_types::components::Position3D::new(
                center.x, center.y, center.z,
            ))
            .with_eye_up(re_sdk_types::components::Vector3D(
                re_sdk_types::datatypes::Vec3D::from([0.0, 1.0, 0.0]),
            )),
    )?;

    let container_path = format!("container/{CONTAINER_ID_STR}");
    blueprint.log(
        container_path.as_str(),
        &ContainerBlueprint::new(ContainerKind::Tabs)
            .with_contents([view_path.as_str()])
            .with_active_tab(ActiveTab(BlueprintEntityPath::from(view_path.as_str())))
            .with_visible(Visible(Bool(true))),
    )?;
    blueprint.log(
        "viewport",
        &ViewportBlueprint::new()
            .with_root_container(RootContainer(BlueprintUuid {
                bytes: CONTAINER_ID_BYTES,
            }))
            .with_auto_layout(AutoLayout(Bool(false)))
            .with_auto_views(AutoViews(Bool(false))),
    )?;
    blueprint.log(
        "blueprint_panel",
        &PanelBlueprint::new().with_state(PanelState::Hidden),
    )?;
    blueprint.log(
        "selection_panel",
        &PanelBlueprint::new().with_state(PanelState::Hidden),
    )?;
    blueprint.log(
        "time_panel",
        &TimePanelBlueprint::new().with_state(PanelState::Collapsed),
    )?;

    let messages = storage.take();
    let blueprint_id = messages
        .iter()
        .find_map(|msg| match msg {
            LogMsg::SetStoreInfo(info) => Some(info.info.store_id.clone()),
            _ => None,
        })
        .ok_or_else(|| anyhow::anyhow!("blueprint stream missing store info"))?;
    recording.send_blueprint(
        messages,
        BlueprintActivationCommand {
            blueprint_id,
            make_active: true,
            make_default: true,
        },
    );
    Ok(())
}

fn native_options() -> eframe::NativeOptions {
    let mut native_options = re_viewer::native::eframe_options(None);
    native_options.wgpu_options = eframe::egui_wgpu::WgpuConfiguration {
        present_mode: re_renderer::external::wgpu::PresentMode::AutoVsync,
        desired_maximum_frame_latency: None,
        on_surface_error: std::sync::Arc::new(|err| {
            if err == re_renderer::external::wgpu::SurfaceError::Outdated
                && !cfg!(target_os = "windows")
            {
                eframe::egui_wgpu::SurfaceErrorAction::RecreateSurface
            } else {
                eframe::egui_wgpu::SurfaceErrorAction::SkipFrame
            }
        }),
        wgpu_setup: eframe::egui_wgpu::WgpuSetup::CreateNew(
            eframe::egui_wgpu::WgpuSetupCreateNew {
                instance_descriptor: re_renderer::device_caps::instance_descriptor(None),
                native_adapter_selector: Some(Arc::new(move |adapters, surface| {
                    re_renderer::device_caps::select_adapter(
                        adapters,
                        re_renderer::device_caps::instance_descriptor(None).backends,
                        surface,
                    )
                })),
                device_descriptor: Arc::new(|adapter| {
                    // The minimal example still requests the adapter's full limits. That keeps the
                    // known-working compute/tile path available on capable backends instead of
                    // forcing everything through the CPU fallback.
                    re_renderer::external::wgpu::DeviceDescriptor {
                        label: Some("gsplat-rerun-minimal device"),
                        required_features: adapter.features().difference(
                            re_renderer::external::wgpu::Features::MAPPABLE_PRIMARY_BUFFERS,
                        ),
                        required_limits: adapter.limits(),
                        memory_hints: re_renderer::external::wgpu::MemoryHints::MemoryUsage,
                        trace: re_renderer::external::wgpu::Trace::Off,
                        experimental_features: unsafe {
                            re_renderer::external::wgpu::ExperimentalFeatures::enabled()
                        },
                    }
                }),
                ..Default::default()
            },
        ),
    };
    native_options
}
