mod camera;
mod camera_controller;
mod lights;
mod model;
mod orbit_camera;
mod resources;
mod texture;

const OBJMODEL_NAME: &str = "manycubes.obj";

//MODEL NAMES:
// JaggedLandscape
// Suzanne
// manycubes
// TwistedTorus

use camera_controller::CameraController;
use cgmath::Vector3;
use orbit_camera::OrbitCamera;
use winit::{
    event::*,
    event_loop::{ControlFlow, EventLoop},
    keyboard::{Key, NamedKey},
    window::Window,
    window::WindowBuilder,
};

use wgpu::util::DeviceExt;

use model::Vertex;
struct Application {
    window: Window,
    window_surface: wgpu::Surface,
    device: wgpu::Device,
    command_queue: wgpu::Queue,
    size: winit::dpi::PhysicalSize<u32>,
    config: wgpu::SurfaceConfiguration,
    render_pipeline: wgpu::RenderPipeline,
    camera: orbit_camera::OrbitCamera,
    camera_uniform: camera::CameraUniform,
    camera_controller: camera_controller::CameraController,
    mouse_pressed: bool,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    obj_model: model::Model,
    depth_texture: texture::Texture,
    light_bind_group: wgpu::BindGroup,
    debug_pipeline: wgpu::RenderPipeline,
    debug: bool,
}

impl Application {
    // Create new application
    async fn new(event_loop: &EventLoop<()>) -> Application {
        // Instance - Handle to the GPU. Use this to get adapter and surfce
        let wgpu_instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        // Create a winit window
        let window = WindowBuilder::new()
            .with_title("WGPU")
            .with_resizable(true)
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 720))
            // .with_fullscreen(Some(Fullscreen::Borderless(None)))
            .build(&event_loop)
            .unwrap();

        let size = window.inner_size();

        // --SAFETY--
        // The surface needs to live as long as the window that created it.
        // State owns the window, so this should be safe.
        let window_surface = unsafe { wgpu_instance.create_surface(&window) }.unwrap();

        // Handle for the actual graphics card
        let adapter = wgpu_instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance, // either low power or high performance
                compatible_surface: Some(&window_surface), // give surface and it finds an adapter thats compatible
                force_fallback_adapter: false,             //use gpu hardware
            })
            .await
            .unwrap();

        // Create device and command queue from adapter
        // Extra features from bulb example, idk what what do specifically (https://docs.rs/wgpu/latest/wgpu/struct.Features.html)
        let (device, command_queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("main device"),
                    features: wgpu::Features::default() | wgpu::Features::POLYGON_MODE_LINE, //wgpu::Features::POLYGON_MODE_LINE,
                    limits: wgpu::Limits {
                        max_push_constant_size: 8,
                        ..Default::default()
                    },
                },
                None,
            )
            .await
            .unwrap();

        let surface_caps = window_surface.get_capabilities(&adapter);
        // Shader code in this tutorial assumes an sRGB surface texture. Using a different
        // one will result in all the colors coming out darker. If you want to support non
        // sRGB surfaces, you'll need to account for that when drawing to the frame.
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .filter(|f| f.is_srgb())
            .next()
            .unwrap_or(surface_caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
        };
        window_surface.configure(&device, &config);

        // --CAMERA-- //
        let mut camera = OrbitCamera::new(
            2.0,
            0.0,
            0.0,
            Vector3::new(0.0, 0.0, 0.0),
            size.width as f32 / size.height as f32,
        );
        camera.bounds.min_distance = Some(1.1);
        let camera_controller = CameraController::new(0.0025, 0.1);
        let mut camera_uniform = camera::CameraUniform::default();
        camera_uniform.update_view_proj(&camera);

        // this is a uniform buffer for the camera
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // now lets create a bind group with the buffer, we need a layout for this
        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false, //means that the location of the data in the buffer wont change
                        min_binding_size: None, //smallest size the buffer can be (dont need to specify -> https://docs.rs/wgpu/latest/wgpu/enum.BindingType.html#variant.Buffer.field.min_binding_size)
                    },
                    count: None,
                }],
                label: Some("camera_bind_group_layout"),
            });

        // create the bind group
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
            label: Some("camera_bind_group"),
        });

        // --DEPTH-- //
        let depth_texture =
            texture::Texture::create_depth_texture(&device, &config, "depth_texture");

        // --LIGHTS-- //
        let light_uniform = lights::LightUniform {
            position: [2.0, 2.0, 2.0],
            _padding: 0,
            color: [1.0, 1.0, 1.0],
            _padding2: 0,
        };

        // We'll want to update our lights position, so we use COPY_DST
        let light_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Light VB"),
            contents: bytemuck::cast_slice(&[light_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // now create a bind group (with of course the layout as per usual)
        let light_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
                label: None,
            });

        let light_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &light_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: light_buffer.as_entire_binding(),
            }],
            label: None,
        });

        // RENDER PIPELINES

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&camera_bind_group_layout, &light_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = {
            let shader = wgpu::ShaderModuleDescriptor {
                label: Some("Normal Shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
            };
            create_render_pipeline(
                &device,
                &render_pipeline_layout,
                config.format,
                Some(texture::Texture::DEPTH_FORMAT),
                &[model::ModelVertex::desc()],
                shader,
                wgpu::PolygonMode::Fill,
            )
        };

        let debug_pipeline = {
            let shader = wgpu::ShaderModuleDescriptor {
                label: Some("Debug Pipeline"),
                source: wgpu::ShaderSource::Wgsl(include_str!("debug.wgsl").into()),
            };
            println!("Here");
            create_render_pipeline(
                &device,
                &render_pipeline_layout,
                config.format,
                Some(texture::Texture::DEPTH_FORMAT),
                &[model::ModelVertex::desc()],
                shader,
                wgpu::PolygonMode::Line,
            )
        };

        // --MODELS-- //

        let obj_model = resources::load_model(OBJMODEL_NAME, &device).await.unwrap();

        Application {
            window,
            window_surface,
            device,
            command_queue,
            size,
            config,
            render_pipeline,
            camera,
            camera_uniform,
            camera_buffer,
            camera_bind_group,
            camera_controller,
            mouse_pressed: false,
            obj_model,
            depth_texture,
            light_bind_group,
            debug_pipeline,
            debug: false,
        }
    }

    //https://docs.rs/winit/latest/winit/  helpfulf for redraw where to put
    fn run(&mut self, event_loop: EventLoop<()>) {
        // Initialize the frame counter
        event_loop.set_control_flow(ControlFlow::Poll);
        let _ = event_loop.run(move |event, elwt| {
            match event {
                Event::DeviceEvent { ref event, .. } => {
                    self.camera_controller
                        .process_events(event, &self.window, &mut self.camera);
                }

                Event::WindowEvent {
                    window_id,
                    ref event,
                } if window_id == self.window.id() && !self.input(event) => {
                    if !self.input(event) {
                        match event {
                            WindowEvent::CloseRequested
                            | WindowEvent::KeyboardInput {
                                event:
                                    KeyEvent {
                                        logical_key: Key::Named(NamedKey::Escape),
                                        ..
                                    },
                                ..
                            } => {
                                elwt.exit();
                            }

                            WindowEvent::KeyboardInput { event, .. } => {
                                self.camera_controller.process_keyed_events(&event)
                            }
                            // Resizing
                            WindowEvent::Resized(physical_size) => {
                                self.resize(*physical_size);
                            }

                            WindowEvent::RedrawRequested => {
                                // Redraw the application.
                                //
                                // It's preferable for applications that do not render continuously to render in
                                // this event rather than in AboutToWait, since rendering in here allows
                                // the program to gracefully handle redraws requested by the OS.

                                let now = instant::Instant::now();

                                self.update();

                                match self.render() {
                                    Ok(_) => {}
                                    // Reconfigure the surface if lost
                                    Err(wgpu::SurfaceError::Lost) => self.resize(self.size),
                                    // The system is out of memory, we should probably quit
                                    Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                                    // All other errors (Outdated, Timeout) should be resolved by the next frame
                                    Err(e) => eprintln!("{:?}", e),
                                }

                                // FRAMERATE CALC
                                let elapsed = now.elapsed().as_millis();
                                println!("{:#?}ms", elapsed)
                            }

                            _ => (),
                        } //  match winodw end
                    }
                } // end 1st event match
                _ => (),
            }
        });
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.window_surface.configure(&self.device, &self.config);
        }

        self.camera
            .resize_projection(new_size.width, new_size.height);

        self.depth_texture =
            texture::Texture::create_depth_texture(&self.device, &self.config, "depth_texture");

        self.window.request_redraw();
    }

    fn update(&mut self) {
        self.camera_uniform.update_view_proj(&self.camera);
        self.command_queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::cast_slice(&[self.camera_uniform]),
        );

        // // Update the light position
        // let old_position: cgmath::Vector3<_> = self.light_uniform.position.into();
        // self.light_uniform.position = (cgmath::Quaternion::from_axis_angle(
        //     (0.0, 1.0, 0.0).into(),
        //     cgmath::Deg(60.0 * dt.as_secs_f32()),
        // ) * old_position)
        //     .into();
        // self.command_queue.write_buffer(
        //     &self.light_buffer,
        //     0,
        //     bytemuck::cast_slice(&[self.light_uniform]),
        // );
    }

    fn input(&mut self, event: &WindowEvent) -> bool {
        // self.window.request_redraw();
        match event {
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => match key_event {
                KeyEvent {
                    logical_key: Key::Character(c),
                    repeat,
                    state,
                    ..
                } if c == "j" => {
                    if !repeat && state.is_pressed() {
                        if self.debug {
                            println!("Debug: false");
                            self.debug = false;
                            self.window.request_redraw();
                        } else {
                            println!("Debug: true");
                            self.debug = true;
                            self.window.request_redraw();
                        }
                    };

                    true
                }
                _ => false, //self.camera_controller.process_keyboard(key_event.clone()),
            },
            // WindowEvent::MouseWheel { delta, .. } => {
            //     self.camera_controller.process_scroll(delta);
            //     true
            // }
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                self.mouse_pressed = *state == ElementState::Pressed;
                true
            }
            _ => false,
        }
    }

    // ===================================================================== //
    // ============================= RENDER ================================ //
    // ===================================================================== //
    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        // The get_current_texture function will wait for the surface to provide a new
        // SurfaceTexture that we will render to. We'll store this in output for later.
        let output = self.window_surface.get_current_texture()?; // NOTE the '?'

        // create texture view with the default settings
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // we need a command buffer to send instructions to the gpu. This encoder does that
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        // now use the encoder to create a render pass, which has all the methods for actual drawing

        //we need the nesting because begin_render_pass BORROWS encoder mutably (&mut self) so we can't
        // call encoder.finish() until we release this mutable borrow
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.2,
                            b: 0.3,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    //attach depth texture to stencil attatchement of render pass
                    view: &self.depth_texture.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            use model::DrawModel;
            if self.debug {
                render_pass.set_pipeline(&self.debug_pipeline);
            } else {
                render_pass.set_pipeline(&self.render_pipeline);
            }
            render_pass.draw_model(
                // or could add ...model_instanced with (0..self.instances.len() as u32) parameter to do instancing
                &self.obj_model,
                &self.camera_bind_group,
                &self.light_bind_group,
            );
        }

        // could do drop(render_pass) here if we dont want braces nesting

        // submit will accept anything that implements IntoIter
        self.command_queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}

fn create_render_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    color_format: wgpu::TextureFormat,
    depth_format: Option<wgpu::TextureFormat>,
    vertex_layouts: &[wgpu::VertexBufferLayout],
    shader: wgpu::ShaderModuleDescriptor,
    poly_mode: wgpu::PolygonMode,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(shader);

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Render Pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
            buffers: vertex_layouts,
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_main",
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: Some(wgpu::BlendState {
                    alpha: wgpu::BlendComponent::REPLACE,
                    color: wgpu::BlendComponent::REPLACE,
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            // Setting this to anything other than Fill requires Features::NON_FILL_POLYGON_MODE
            polygon_mode: poly_mode,
            // Requires Features::DEPTH_CLIP_CONTROL
            unclipped_depth: false,
            // Requires Features::CONSERVATIVE_RASTERIZATION
            conservative: false,
        },
        depth_stencil: depth_format.map(|format| wgpu::DepthStencilState {
            format,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
    })
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    let mut application = pollster::block_on(Application::new(&event_loop));
    application.run(event_loop);
}
