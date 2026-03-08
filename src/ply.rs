//! Format-specific Gaussian `.ply` loader.
//!
//! This file intentionally handles one family of files only: the de-facto GraphDECO / Brush
//! static Gaussian layout. The rest of the repo only sees the decoded [`GaussianSet`].

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;

use glam::{Quat, Vec3};

const SH_C0: f32 = 0.282_094_8;

#[derive(Clone, Debug)]
pub struct ShCoefficients {
    /// Number of SH basis coefficients per color channel, including the DC term.
    pub coeffs_per_channel: usize,
    /// Flat `[splat][coefficient][channel]` buffer used directly by the visualizer/renderer.
    pub coefficients: Vec<f32>,
}

#[derive(Clone, Debug, Default)]
pub struct GaussianSet {
    /// World-space Gaussian means.
    pub means_world: Vec<Vec3>,
    /// Unit quaternions in `xyzw` storage order.
    pub quats: Vec<Quat>,
    /// Decoded anisotropic scales.
    pub scales: Vec<Vec3>,
    /// Decoded opacity in `[0, 1]`.
    pub opacities: Vec<f32>,
    /// Degree-0 color after the same SH activation used by the working repo.
    pub colors_dc: Vec<[f32; 3]>,
    /// Optional higher-order SH coefficients.
    pub sh_coeffs: Option<ShCoefficients>,
}

impl GaussianSet {
    pub fn len(&self) -> usize {
        self.means_world.len()
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        let len = self.means_world.len();
        if len == 0 {
            anyhow::bail!("the gaussian set is empty");
        }

        for (name, other_len) in [
            ("quaternions", self.quats.len()),
            ("scales", self.scales.len()),
            ("opacities", self.opacities.len()),
            ("colors_dc", self.colors_dc.len()),
        ] {
            if other_len != len {
                anyhow::bail!(
                    "mismatched gaussian buffer lengths: means={}, {}={}",
                    len,
                    name,
                    other_len
                );
            }
        }

        if let Some(sh) = &self.sh_coeffs {
            // Every splat contributes `coeffs_per_channel * 3` floats because SH is packed
            // channel-major as RGB triplets per basis coefficient.
            let expected = len
                .checked_mul(sh.coeffs_per_channel)
                .and_then(|v| v.checked_mul(3))
                .ok_or_else(|| anyhow::anyhow!("SH coefficient count overflow"))?;
            if sh.coefficients.len() != expected {
                anyhow::bail!(
                    "mismatched SH coefficient count: expected {}, got {}",
                    expected,
                    sh.coefficients.len()
                );
            }
        }

        Ok(())
    }

    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for mean in &self.means_world {
            min = min.min(*mean);
            max = max.max(*mean);
        }
        min.is_finite().then_some((min, max))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PlyLoadError {
    #[error("failed to read PLY file {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid PLY header: {0}")]
    Header(String),
    #[error("unsupported PLY format: {0}")]
    Unsupported(String),
    #[error("malformed PLY data: {0}")]
    Data(String),
}

#[derive(Clone, Copy, Debug)]
enum PlyFormat {
    Ascii,
    BinaryLittleEndian,
}

#[derive(Clone, Copy, Debug)]
enum ScalarType {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    F32,
    F64,
}

#[derive(Clone, Debug)]
struct Property {
    name: String,
    scalar_type: ScalarType,
}

#[derive(Clone, Debug)]
struct PlyHeader {
    format: PlyFormat,
    vertex_count: usize,
    properties: Vec<Property>,
    data_offset: usize,
}

#[derive(Clone, Debug, Default)]
struct RowAccumulator {
    // Raw property values for one PLY vertex before decode into the canonical Gaussian layout.
    position: Vec3,
    log_scale: Vec3,
    rotation_wxyz: [f32; 4],
    opacity: f32,
    color_rgb: Option<[f32; 3]>,
    dc_coeffs: Option<[f32; 3]>,
    sh_rest: BTreeMap<usize, f32>,
}

pub fn load_gaussian_ply(path: impl AsRef<Path>) -> Result<GaussianSet, PlyLoadError> {
    // The loader stays intentionally format-specific. If a property is not part of the de-facto
    // static Gaussian layout we simply ignore it instead of building a generic PLY abstraction.
    let path = path.as_ref();
    let bytes = fs::read(path).map_err(|source| PlyLoadError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let header = parse_header(&bytes)?;

    let mut set = GaussianSet::default();
    let mut sh_coefficients = Vec::new();
    let mut coeffs_per_channel = 1usize;

    match header.format {
        PlyFormat::Ascii => {
            let content = std::str::from_utf8(&bytes[header.data_offset..])
                .map_err(|err| PlyLoadError::Data(err.to_string()))?;
            for (row_index, line) in content
                .lines()
                .filter(|line| !line.trim().is_empty())
                .take(header.vertex_count)
                .enumerate()
            {
                let row = parse_ascii_row(row_index, line, &header.properties)?;
                push_row(&mut set, &mut sh_coefficients, &mut coeffs_per_channel, row)?;
            }
        }
        PlyFormat::BinaryLittleEndian => {
            let mut cursor = Cursor::new(&bytes[header.data_offset..]);
            for row_index in 0..header.vertex_count {
                let row = parse_binary_row(row_index, &mut cursor, &header.properties)?;
                push_row(&mut set, &mut sh_coefficients, &mut coeffs_per_channel, row)?;
            }
        }
    }

    if !sh_coefficients.is_empty() {
        set.sh_coeffs = Some(ShCoefficients {
            coeffs_per_channel,
            coefficients: sh_coefficients,
        });
    }

    set.validate()
        .map_err(|err| PlyLoadError::Data(err.to_string()))?;
    Ok(set)
}

fn parse_header(bytes: &[u8]) -> Result<PlyHeader, PlyLoadError> {
    // Only the vertex element matters for this example, so the header parser only records that
    // section and leaves the rest of the PLY grammar unsupported on purpose.
    let header_end = bytes
        .windows("end_header\n".len())
        .position(|window| window == b"end_header\n")
        .map(|offset| offset + "end_header\n".len())
        .or_else(|| {
            bytes
                .windows("end_header\r\n".len())
                .position(|window| window == b"end_header\r\n")
                .map(|offset| offset + "end_header\r\n".len())
        })
        .ok_or_else(|| PlyLoadError::Header("missing end_header".to_owned()))?;

    let header_str = std::str::from_utf8(&bytes[..header_end])
        .map_err(|err| PlyLoadError::Header(err.to_string()))?;
    let mut lines = header_str.lines();
    let magic = lines
        .next()
        .ok_or_else(|| PlyLoadError::Header("missing magic".to_owned()))?;
    if magic.trim() != "ply" {
        return Err(PlyLoadError::Header("missing `ply` magic".to_owned()));
    }

    let mut format = None;
    let mut vertex_count = None;
    let mut properties = Vec::new();
    let mut in_vertex = false;

    for line in lines {
        let line = line.trim();
        if line.is_empty() || line == "end_header" || line.starts_with("comment") {
            continue;
        }

        let mut tokens = line.split_whitespace();
        match tokens.next().unwrap_or_default() {
            "format" => {
                format = Some(match tokens.next().unwrap_or_default() {
                    "ascii" => PlyFormat::Ascii,
                    "binary_little_endian" => PlyFormat::BinaryLittleEndian,
                    other => return Err(PlyLoadError::Unsupported(other.to_owned())),
                });
            }
            "element" => {
                in_vertex = tokens.next() == Some("vertex");
                if in_vertex {
                    let count = tokens
                        .next()
                        .ok_or_else(|| PlyLoadError::Header("vertex count missing".to_owned()))?;
                    vertex_count = Some(
                        count
                            .parse::<usize>()
                            .map_err(|err| PlyLoadError::Header(err.to_string()))?,
                    );
                }
            }
            "property" if in_vertex => {
                let scalar =
                    parse_scalar_type(tokens.next().ok_or_else(|| {
                        PlyLoadError::Header("property type missing".to_owned())
                    })?)?;
                let name = tokens
                    .next()
                    .ok_or_else(|| PlyLoadError::Header("property name missing".to_owned()))?
                    .to_owned();
                properties.push(Property {
                    name,
                    scalar_type: scalar,
                });
            }
            _ => {}
        }
    }

    Ok(PlyHeader {
        format: format.ok_or_else(|| PlyLoadError::Header("missing format".to_owned()))?,
        vertex_count: vertex_count
            .ok_or_else(|| PlyLoadError::Header("missing vertex element".to_owned()))?,
        properties,
        data_offset: header_end,
    })
}

fn parse_scalar_type(token: &str) -> Result<ScalarType, PlyLoadError> {
    match token {
        "char" | "int8" => Ok(ScalarType::I8),
        "uchar" | "uint8" => Ok(ScalarType::U8),
        "short" | "int16" => Ok(ScalarType::I16),
        "ushort" | "uint16" => Ok(ScalarType::U16),
        "int" | "int32" => Ok(ScalarType::I32),
        "uint" | "uint32" => Ok(ScalarType::U32),
        "float" | "float32" => Ok(ScalarType::F32),
        "double" | "float64" => Ok(ScalarType::F64),
        other => Err(PlyLoadError::Unsupported(other.to_owned())),
    }
}

fn parse_ascii_row(
    row_index: usize,
    line: &str,
    properties: &[Property],
) -> Result<RowAccumulator, PlyLoadError> {
    let values: Vec<&str> = line.split_whitespace().collect();
    if values.len() < properties.len() {
        return Err(PlyLoadError::Data(format!(
            "row {} has {} values, expected {}",
            row_index,
            values.len(),
            properties.len()
        )));
    }

    let mut row = RowAccumulator::default();
    // Match the working loader's defaults: identity rotation, full opacity, white fallback color.
    row.rotation_wxyz[0] = 1.0;
    row.opacity = 1.0;
    row.color_rgb = Some([1.0, 1.0, 1.0]);

    for (property, value) in properties.iter().zip(values) {
        assign_property(
            &mut row,
            property,
            parse_ascii_scalar(value, property.scalar_type)?,
        );
    }
    Ok(row)
}

fn parse_ascii_scalar(token: &str, scalar_type: ScalarType) -> Result<f32, PlyLoadError> {
    macro_rules! parse_then_cast {
        ($ty:ty) => {
            token
                .parse::<$ty>()
                .map(|value| value as f32)
                .map_err(|err| PlyLoadError::Data(err.to_string()))
        };
    }

    match scalar_type {
        ScalarType::I8 => parse_then_cast!(i8),
        ScalarType::U8 => parse_then_cast!(u8),
        ScalarType::I16 => parse_then_cast!(i16),
        ScalarType::U16 => parse_then_cast!(u16),
        ScalarType::I32 => parse_then_cast!(i32),
        ScalarType::U32 => parse_then_cast!(u32),
        ScalarType::F32 => parse_then_cast!(f32),
        ScalarType::F64 => parse_then_cast!(f64),
    }
}

fn parse_binary_row(
    row_index: usize,
    cursor: &mut Cursor<&[u8]>,
    properties: &[Property],
) -> Result<RowAccumulator, PlyLoadError> {
    let mut row = RowAccumulator::default();
    // Binary input uses the same semantic defaults as ASCII input.
    row.rotation_wxyz[0] = 1.0;
    row.opacity = 1.0;
    row.color_rgb = Some([1.0, 1.0, 1.0]);

    for property in properties {
        let value = read_binary_scalar(cursor, property.scalar_type)
            .map_err(|err| PlyLoadError::Data(format!("row {}: {}", row_index, err)))?;
        assign_property(&mut row, property, value);
    }
    Ok(row)
}

fn read_binary_scalar(cursor: &mut Cursor<&[u8]>, scalar_type: ScalarType) -> Result<f32, String> {
    macro_rules! read_then_cast {
        ($ty:ty) => {{
            let mut bytes = [0u8; std::mem::size_of::<$ty>()];
            cursor
                .read_exact(&mut bytes)
                .map_err(|err| err.to_string())?;
            Ok(<$ty>::from_le_bytes(bytes) as f32)
        }};
    }

    match scalar_type {
        ScalarType::I8 => read_then_cast!(i8),
        ScalarType::U8 => read_then_cast!(u8),
        ScalarType::I16 => read_then_cast!(i16),
        ScalarType::U16 => read_then_cast!(u16),
        ScalarType::I32 => read_then_cast!(i32),
        ScalarType::U32 => read_then_cast!(u32),
        ScalarType::F32 => read_then_cast!(f32),
        ScalarType::F64 => read_then_cast!(f64),
    }
}

fn assign_property(row: &mut RowAccumulator, property: &Property, value: f32) {
    // Property names follow the practical GraphDECO / Brush conventions rather than a formal
    // schema, so keeping this mapping explicit is clearer than hiding it behind traits.
    match property.name.as_str() {
        "x" => row.position.x = value,
        "y" => row.position.y = value,
        "z" => row.position.z = value,
        "scale_0" => row.log_scale.x = value,
        "scale_1" => row.log_scale.y = value,
        "scale_2" => row.log_scale.z = value,
        "rot_0" => row.rotation_wxyz[0] = value,
        "rot_1" => row.rotation_wxyz[1] = value,
        "rot_2" => row.rotation_wxyz[2] = value,
        "rot_3" => row.rotation_wxyz[3] = value,
        "opacity" => row.opacity = value,
        "f_dc_0" => row.dc_coeffs.get_or_insert([0.0; 3])[0] = value,
        "f_dc_1" => row.dc_coeffs.get_or_insert([0.0; 3])[1] = value,
        "f_dc_2" => row.dc_coeffs.get_or_insert([0.0; 3])[2] = value,
        "red" | "r" => {
            row.color_rgb.get_or_insert([1.0; 3])[0] = normalize_color(value, property.scalar_type)
        }
        "green" | "g" => {
            row.color_rgb.get_or_insert([1.0; 3])[1] = normalize_color(value, property.scalar_type)
        }
        "blue" | "b" => {
            row.color_rgb.get_or_insert([1.0; 3])[2] = normalize_color(value, property.scalar_type)
        }
        _ if property.name.starts_with("f_rest_") => {
            if let Ok(index) = property.name["f_rest_".len()..].parse::<usize>() {
                row.sh_rest.insert(index, value);
            }
        }
        _ => {}
    }
}

fn normalize_color(value: f32, scalar_type: ScalarType) -> f32 {
    match scalar_type {
        ScalarType::U8 => (value / 255.0).clamp(0.0, 1.0),
        ScalarType::U16 => (value / 65535.0).clamp(0.0, 1.0),
        _ => value.clamp(0.0, 1.0),
    }
}

fn push_row(
    set: &mut GaussianSet,
    sh_coefficients: &mut Vec<f32>,
    coeffs_per_channel: &mut usize,
    row: RowAccumulator,
) -> Result<(), PlyLoadError> {
    // Decode one raw row into the canonical representation used by the visualizer and renderer.
    let scales = row.log_scale.exp();
    let quat = normalize_quat_or_identity(Quat::from_xyzw(
        row.rotation_wxyz[1],
        row.rotation_wxyz[2],
        row.rotation_wxyz[3],
        row.rotation_wxyz[0],
    ));
    let opacity = sigmoid(row.opacity);
    let color_rgb = if let Some(dc_coeffs) = row.dc_coeffs {
        sh_dc_to_rgb(dc_coeffs)
    } else {
        row.color_rgb.unwrap_or([1.0, 1.0, 1.0])
    };

    set.means_world.push(row.position);
    set.scales.push(scales);
    set.quats.push(quat);
    set.opacities.push(opacity);
    set.colors_dc.push(color_rgb);

    if !row.sh_rest.is_empty() || row.dc_coeffs.is_some() {
        // Higher-order SH is stored in channel-major layout. We only keep complete RGB triplets;
        // malformed partial payloads are ignored the same way the larger working repo handles
        // them, which keeps real-world messy files loading predictably.
        let extra_coeffs = row.sh_rest.len() / 3;
        let row_coeffs_per_channel = extra_coeffs + 1;
        *coeffs_per_channel = (*coeffs_per_channel).max(row_coeffs_per_channel);

        let dc_coeffs = row.dc_coeffs.unwrap_or([0.0, 0.0, 0.0]);
        sh_coefficients.extend_from_slice(&dc_coeffs);
        for coeff_index in 0..extra_coeffs {
            let red_index = coeff_index;
            let green_index = extra_coeffs + coeff_index;
            let blue_index = extra_coeffs * 2 + coeff_index;
            sh_coefficients.push(*row.sh_rest.get(&red_index).unwrap_or(&0.0));
            sh_coefficients.push(*row.sh_rest.get(&green_index).unwrap_or(&0.0));
            sh_coefficients.push(*row.sh_rest.get(&blue_index).unwrap_or(&0.0));
        }
    }

    Ok(())
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

fn normalize_quat_or_identity(quat: Quat) -> Quat {
    if quat.length_squared() > 1e-12 {
        quat.normalize()
    } else {
        Quat::IDENTITY
    }
}

fn sh_dc_to_rgb(dc: [f32; 3]) -> [f32; 3] {
    [
        (dc[0] * SH_C0 + 0.5).max(0.0),
        (dc[1] * SH_C0 + 0.5).max(0.0),
        (dc[2] * SH_C0 + 0.5).max(0.0),
    ]
}
