use crate::VBDesc;
use wgpu::VertexAttribute;

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct ColoredUvVertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
    pub uv: [f32; 2],
}

u8slice_impl!(ColoredUvVertex);
const ATTRS: &[VertexAttribute] = &wgpu::vertex_attr_array![0 => Float3, 1 => Float4, 2 => Float2];

impl VBDesc for ColoredUvVertex {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ColoredUvVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::InputStepMode::Vertex,
            attributes: ATTRS,
        }
    }
}
