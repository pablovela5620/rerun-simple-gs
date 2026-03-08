//! Custom Rerun archetype used to log one batched Gaussian cloud per entity path.
//!
//! The archetype mirrors the renderer-facing data model closely so the visualizer can query one
//! entity, rebuild a render cloud, and hand that directly to the custom renderer.

use re_sdk_types::try_serialize_field;
use rerun::Component as _;

#[derive(Clone, Debug, Default)]
pub struct GaussianSplats3D {
    /// Per-splat world-space means.
    pub centers: Option<rerun::SerializedComponentBatch>,
    /// Per-splat orientation as unit quaternions.
    pub quaternions: Option<rerun::SerializedComponentBatch>,
    /// Per-splat anisotropic scale.
    pub scales: Option<rerun::SerializedComponentBatch>,
    /// Per-splat opacity.
    pub opacities: Option<rerun::SerializedComponentBatch>,
    /// Degree-0 color / SH DC term.
    pub colors: Option<rerun::SerializedComponentBatch>,
    /// Optional higher-order SH coefficients packed as a tensor batch.
    pub sh_coefficients: Option<rerun::SerializedComponentBatch>,
}

impl rerun::Archetype for GaussianSplats3D {
    fn name() -> rerun::ArchetypeName {
        "GaussianSplats3D".into()
    }

    fn display_name() -> &'static str {
        "Gaussian Splats 3D"
    }

    fn required_components() -> std::borrow::Cow<'static, [rerun::ComponentDescriptor]> {
        vec![
            Self::descriptor_centers(),
            Self::descriptor_quaternions(),
            Self::descriptor_scales(),
            Self::descriptor_opacities(),
            Self::descriptor_colors(),
        ]
        .into()
    }

    fn optional_components() -> std::borrow::Cow<'static, [rerun::ComponentDescriptor]> {
        vec![Self::descriptor_sh_coefficients()].into()
    }
}

impl GaussianSplats3D {
    // These descriptors are queried directly by the visualizer, so keep them explicit and stable.
    #[inline]
    pub fn descriptor_centers() -> rerun::ComponentDescriptor {
        rerun::ComponentDescriptor {
            archetype: Some("GaussianSplats3D".into()),
            component: "GaussianSplats3D:centers".into(),
            component_type: Some(rerun::components::Translation3D::name()),
        }
    }

    #[inline]
    pub fn descriptor_quaternions() -> rerun::ComponentDescriptor {
        rerun::ComponentDescriptor {
            archetype: Some("GaussianSplats3D".into()),
            component: "GaussianSplats3D:quaternions".into(),
            component_type: Some(rerun::components::RotationQuat::name()),
        }
    }

    #[inline]
    pub fn descriptor_scales() -> rerun::ComponentDescriptor {
        rerun::ComponentDescriptor {
            archetype: Some("GaussianSplats3D".into()),
            component: "GaussianSplats3D:scales".into(),
            component_type: Some(rerun::components::Scale3D::name()),
        }
    }

    #[inline]
    pub fn descriptor_opacities() -> rerun::ComponentDescriptor {
        rerun::ComponentDescriptor {
            archetype: Some("GaussianSplats3D".into()),
            component: "GaussianSplats3D:opacities".into(),
            component_type: Some(rerun::components::Opacity::name()),
        }
    }

    #[inline]
    pub fn descriptor_colors() -> rerun::ComponentDescriptor {
        rerun::ComponentDescriptor {
            archetype: Some("GaussianSplats3D".into()),
            component: "GaussianSplats3D:colors".into(),
            component_type: Some(rerun::components::Color::name()),
        }
    }

    #[inline]
    pub fn descriptor_sh_coefficients() -> rerun::ComponentDescriptor {
        rerun::ComponentDescriptor {
            archetype: Some("GaussianSplats3D".into()),
            component: "GaussianSplats3D:sh_coefficients".into(),
            component_type: Some(rerun::components::TensorData::name()),
        }
    }

    pub fn new(
        centers: impl IntoIterator<Item = impl Into<rerun::components::Translation3D>>,
        quaternions: impl IntoIterator<Item = impl Into<rerun::components::RotationQuat>>,
        scales: impl IntoIterator<Item = impl Into<rerun::components::Scale3D>>,
        opacities: impl IntoIterator<Item = impl Into<rerun::components::Opacity>>,
        colors: impl IntoIterator<Item = impl Into<rerun::components::Color>>,
    ) -> Self {
        // Callers almost always already own batched arrays, so the builder stays intentionally flat.
        Self::default()
            .with_many_centers(centers)
            .with_many_quaternions(quaternions)
            .with_many_scales(scales)
            .with_many_opacities(opacities)
            .with_many_colors(colors)
    }

    pub fn with_many_centers(
        mut self,
        centers: impl IntoIterator<Item = impl Into<rerun::components::Translation3D>>,
    ) -> Self {
        self.centers = try_serialize_field::<rerun::components::Translation3D>(
            Self::descriptor_centers(),
            centers.into_iter().map(Into::into),
        );
        self
    }

    pub fn with_many_quaternions(
        mut self,
        quaternions: impl IntoIterator<Item = impl Into<rerun::components::RotationQuat>>,
    ) -> Self {
        self.quaternions = try_serialize_field::<rerun::components::RotationQuat>(
            Self::descriptor_quaternions(),
            quaternions.into_iter().map(Into::into),
        );
        self
    }

    pub fn with_many_scales(
        mut self,
        scales: impl IntoIterator<Item = impl Into<rerun::components::Scale3D>>,
    ) -> Self {
        self.scales = try_serialize_field::<rerun::components::Scale3D>(
            Self::descriptor_scales(),
            scales.into_iter().map(Into::into),
        );
        self
    }

    pub fn with_many_opacities(
        mut self,
        opacities: impl IntoIterator<Item = impl Into<rerun::components::Opacity>>,
    ) -> Self {
        self.opacities = try_serialize_field::<rerun::components::Opacity>(
            Self::descriptor_opacities(),
            opacities.into_iter().map(Into::into),
        );
        self
    }

    pub fn with_many_colors(
        mut self,
        colors: impl IntoIterator<Item = impl Into<rerun::components::Color>>,
    ) -> Self {
        self.colors = try_serialize_field::<rerun::components::Color>(
            Self::descriptor_colors(),
            colors.into_iter().map(Into::into),
        );
        self
    }

    pub fn with_sh_coefficients(
        mut self,
        sh_coefficients: impl Into<rerun::components::TensorData>,
    ) -> Self {
        self.sh_coefficients = try_serialize_field::<rerun::components::TensorData>(
            Self::descriptor_sh_coefficients(),
            std::iter::once(sh_coefficients.into()),
        );
        self
    }
}

impl rerun::AsComponents for GaussianSplats3D {
    fn as_serialized_batches(&self) -> Vec<rerun::SerializedComponentBatch> {
        [
            self.centers.clone(),
            self.quaternions.clone(),
            self.scales.clone(),
            self.opacities.clone(),
            self.colors.clone(),
            self.sh_coefficients.clone(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}
