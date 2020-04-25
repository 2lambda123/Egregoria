use crate::engine::{AudioContext, ClearScreen, Drawable, GfxContext, InputContext};
use crate::game_loop;
use futures::executor;
use wgpu::{Color, SwapChainOutput};
use winit::{
    dpi::PhysicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

#[allow(dead_code)]
pub struct Context {
    pub gfx: GfxContext,
    pub input: InputContext,
    pub audio: AudioContext,
}

impl Context {
    pub fn new() -> (Self, EventLoop<()>) {
        let event_loop = EventLoop::new();

        let size = event_loop.primary_monitor().size();

        let window = WindowBuilder::new()
            .with_inner_size(PhysicalSize::new(
                size.width as f32 * 0.8,
                size.height as f32 * 0.8,
            ))
            .build(&event_loop)
            .expect("Failed to create window");

        let gfx = executor::block_on(GfxContext::new(window));
        let input = InputContext::default();
        let audio = AudioContext::new(10);

        (Self { gfx, input, audio }, event_loop)
    }

    pub fn start(mut self, mut state: game_loop::State, el: EventLoop<()>) {
        let clear_screen = ClearScreen {
            clear_color: Color {
                r: 0.5,
                g: 0.5,
                b: 0.5,
                a: 1.0,
            },
        };

        let mut frame: Option<SwapChainOutput> = None;
        let mut new_size: Option<PhysicalSize<u32>> = None;

        el.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Poll;
            match event {
                Event::WindowEvent { event, .. } => {
                    let managed = self.input.handle(&event);

                    if !managed {
                        match event {
                            WindowEvent::Resized(physical_size) => {
                                new_size = Some(physical_size);
                            }
                            WindowEvent::CloseRequested => {
                                println!("The close button was pressed. stopping");
                                *control_flow = ControlFlow::Exit
                            }
                            _ => (),
                        }
                    }
                }
                Event::MainEventsCleared => {
                    if frame.is_none() {
                        if let Some(new_size) = new_size.take() {
                            self.gfx.resize(new_size);
                            state.resized(&mut self, new_size);
                        }
                        frame = Some(
                            self.gfx
                                .swapchain
                                .get_next_texture()
                                .expect("Timeout getting texture"),
                        );
                    } else {
                        self.input.mouse.unprojected = state.unproject(self.input.mouse.screen);

                        state.update(&mut self);
                        let mut frame_ctx = self.gfx.begin_frame(frame.take().unwrap());
                        clear_screen.draw(&mut frame_ctx);
                        state.render(&mut frame_ctx);
                        frame_ctx.finish();
                        self.input.end_frame();
                    }
                }
                _ => (),
            }
        })
    }
}
