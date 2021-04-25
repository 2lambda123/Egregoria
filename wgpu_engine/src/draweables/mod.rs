use crate::GfxContext;
use std::rc::Rc;
use wgpu::{RenderPass, RenderPipeline};

mod blit_linear;
mod instanced_mesh;
mod lit_mesh;
mod multispritebatch;
mod spritebatch;

pub use blit_linear::*;
pub use instanced_mesh::*;
pub use lit_mesh::*;
pub use multispritebatch::*;
pub use spritebatch::*;

pub type IndexType = u32;

pub trait Drawable {
    fn create_pipeline(gfx: &GfxContext) -> RenderPipeline
    where
        Self: Sized;

    fn draw<'a>(&'a self, gfx: &'a GfxContext, rp: &mut RenderPass<'a>);
}

impl<T: Drawable> Drawable for Rc<T> {
    fn create_pipeline(_: &GfxContext) -> RenderPipeline {
        panic!("dont create pipeline for Rc<T>, create it for T!")
    }

    fn draw<'a>(&'a self, gfx: &'a GfxContext, rp: &mut RenderPass<'a>) {
        let s: &T = &*self;
        s.draw(gfx, rp);
    }
}
