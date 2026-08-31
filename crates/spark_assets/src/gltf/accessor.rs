//! Typed reads out of a glTF accessor: `count` elements of `dims`
//! components each, of one component type, at a stride, somewhere inside
//! a buffer view. Everything comes out as `f32` — normalised integers
//! mapped to [0, 1] or [−1, 1] as the spec says — or, for indices, `u32`.

use crate::json::Json;
use crate::{Error, invalid};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Component {
    I8,
    U8,
    I16,
    U16,
    U32,
    F32,
}

impl Component {
    fn from_code(c: usize) -> Option<Self> {
        Some(match c {
            5120 => Self::I8,
            5121 => Self::U8,
            5122 => Self::I16,
            5123 => Self::U16,
            5125 => Self::U32,
            5126 => Self::F32,
            _ => return None,
        })
    }

    fn size(self) -> usize {
        match self {
            Self::I8 | Self::U8 => 1,
            Self::I16 | Self::U16 => 2,
            Self::U32 | Self::F32 => 4,
        }
    }
}

pub struct Accessor<'a> {
    /// `None` for an accessor with no buffer view, which the spec defines
    /// as all zeros.
    data: Option<&'a [u8]>,
    count: usize,
    comp: Component,
    dims: usize,
    stride: usize,
    normalized: bool,
}

fn field(j: &Json, key: &str, what: &str) -> Result<usize, Error> {
    j.get(key)
        .and_then(Json::as_usize)
        .ok_or_else(|| invalid(format!("{what} has no valid `{key}`")))
}

/// `accessors[index]`, bound to its bytes.
pub fn accessor<'a>(json: &Json, buffers: &'a [Vec<u8>], index: usize) -> Result<Accessor<'a>, Error> {
    let a = json
        .get("accessors")
        .and_then(|l| l.at(index))
        .ok_or_else(|| invalid(format!("accessor {index} does not exist")))?;
    if a.get("sparse").is_some() {
        return Err(Error::Unsupported("sparse accessors".into()));
    }
    let what = format!("accessor {index}");
    let count = field(a, "count", &what)?;
    let comp = Component::from_code(field(a, "componentType", &what)?)
        .ok_or_else(|| invalid(format!("{what} has an unknown componentType")))?;
    let dims = match a.get("type").and_then(Json::as_str) {
        Some("SCALAR") => 1,
        Some("VEC2") => 2,
        Some("VEC3") => 3,
        Some("VEC4") | Some("MAT2") => 4,
        Some("MAT3") => 9,
        Some("MAT4") => 16,
        _ => return Err(invalid(format!("{what} has an unknown type"))),
    };
    let normalized = a.get("normalized").and_then(Json::as_bool).unwrap_or(false);
    let elem = comp.size() * dims;
    let Some(bv_i) = a.get("bufferView").and_then(Json::as_usize) else {
        return Ok(Accessor {
            data: None,
            count,
            comp,
            dims,
            stride: elem,
            normalized,
        });
    };
    let bv = json
        .get("bufferViews")
        .and_then(|l| l.at(bv_i))
        .ok_or_else(|| invalid(format!("bufferView {bv_i} does not exist")))?;
    let bwhat = format!("bufferView {bv_i}");
    let buf = buffers
        .get(field(bv, "buffer", &bwhat)?)
        .ok_or_else(|| invalid(format!("{bwhat} names a buffer that does not exist")))?;
    let bv_off = bv.get("byteOffset").and_then(Json::as_usize).unwrap_or(0);
    let bv_len = field(bv, "byteLength", &bwhat)?;
    let stride = bv.get("byteStride").and_then(Json::as_usize).unwrap_or(elem);
    let view = buf
        .get(bv_off..bv_off + bv_len)
        .ok_or_else(|| invalid(format!("{bwhat} runs past its buffer")))?;
    let acc_off = a.get("byteOffset").and_then(Json::as_usize).unwrap_or(0);
    let needed = if count == 0 { 0 } else { acc_off + (count - 1) * stride + elem };
    if needed > view.len() {
        return Err(invalid(format!("{what} runs past its buffer view")));
    }
    Ok(Accessor {
        data: Some(&view[acc_off..]),
        count,
        comp,
        dims,
        stride,
        normalized,
    })
}

impl Accessor<'_> {
    /// Component `k` of element `i`, as stored.
    fn raw(&self, i: usize, k: usize) -> f64 {
        let Some(d) = self.data else { return 0.0 };
        let at = i * self.stride + k * self.comp.size();
        match self.comp {
            Component::I8 => d[at] as i8 as f64,
            Component::U8 => d[at] as f64,
            Component::I16 => i16::from_le_bytes([d[at], d[at + 1]]) as f64,
            Component::U16 => u16::from_le_bytes([d[at], d[at + 1]]) as f64,
            Component::U32 => u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]]) as f64,
            Component::F32 => f32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]]) as f64,
        }
    }

    /// Component `k` of element `i`, normalised if the accessor says so.
    fn value(&self, i: usize, k: usize) -> f32 {
        let v = self.raw(i, k);
        if !self.normalized {
            return v as f32;
        }
        (match self.comp {
            Component::I8 => (v / 127.0).max(-1.0),
            Component::U8 => v / 255.0,
            Component::I16 => (v / 32767.0).max(-1.0),
            Component::U16 => v / 65535.0,
            _ => v,
        }) as f32
    }

    /// Every element as an `[f32; N]`; the accessor must be exactly `N`
    /// wide.
    pub fn vecs<const N: usize>(&self) -> Result<Vec<[f32; N]>, Error> {
        if self.dims != N {
            return Err(invalid(format!(
                "expected {N} components per element, accessor has {}",
                self.dims
            )));
        }
        Ok((0..self.count)
            .map(|i| std::array::from_fn(|k| self.value(i, k)))
            .collect())
    }

    /// Scalar integer elements as `u32` — what an index accessor holds.
    pub fn indices(&self) -> Result<Vec<u32>, Error> {
        if self.dims != 1 || self.comp == Component::F32 {
            return Err(invalid("indices must be integer scalars"));
        }
        Ok((0..self.count).map(|i| self.raw(i, 0) as u32).collect())
    }
}
