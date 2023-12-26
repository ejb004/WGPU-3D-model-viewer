mod camera;
mod model;
mod resources;
mod texture;
mod utils;

use cgmath::prelude::*;
use winit::{
    event::*,
    event_loop::{ControlFlow, EventLoop},
    keyboard::{Key, NamedKey},
    window::Window,
    window::WindowBuilder,
};

use utils::*;
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
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,
    camera: camera::Camera,
    camera_uniform: camera::CameraUniform,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    camera_controller: camera::CameraController,
    obj_model: model::Model,
    instances: Vec<Instance>,
    depth_texture: texture::Texture,
}

struct Instance {
    position: cgmath::Vector3<f32>,
    rotation: cgmath::Quaternion<f32>,
}

impl Instance {
    fn to_raw(&self) -> InstanceRaw {
        let model =
            cgmath::Matrix4::from_translation(self.position) * cgmath::Matrix4::from(self.rotation);
        InstanceRaw {
            model: model.into(),
            // NEW!
            normal: cgmath::Matrix3::from(self.rotation).into(),
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct InstanceRaw {
    #[allow(dead_code)]
    model: [[f32; 4]; 4],
    normal: [[f32; 3]; 3],
}

impl InstanceRaw {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        use std::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<InstanceRaw>() as wgpu::BufferAddress,
            // We need to switch from using a step mode of Vertex to Instance
            // This means that our shaders will only change to use the next
            // instance when the shader starts processing a new instance
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    // While our vertex shader only uses locations 0, and 1 now, in later tutorials we'll
                    // be using 2, 3, and 4, for Vertex. We'll start at slot 5 not conflict with them later
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4,
                },
                // A mat4 takes up 4 vertex slots as it is technically 4 vec4s. We need to define a slot
                // for each vec4. We don't have to do this in code though.
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 8]>() as wgpu::BufferAddress,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 12]>() as wgpu::BufferAddress,
                    shader_location: 8,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
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
                    features: wgpu::Features::default(), //wgpu::Features::POLYGON_MODE_LINE,
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

        // parse shader
        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

        // --CAMERA-- //

        let camera = camera::Camera {
            // position the camera 1 unit up and 2 units back
            // +z is out of the screen
            eye: (0.0, 1.0, 2.0).into(),
            // have it look at the origin
            target: (0.0, 0.0, 0.0).into(),
            // which way is "up"
            up: cgmath::Vector3::unit_y(),
            aspect: config.width as f32 / config.height as f32,
            fovy: 45.0,
            znear: 0.1,
            zfar: 100.0,
        };

        let mut camera_uniform = camera::CameraUniform::new();
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
                    visibility: wgpu::ShaderStages::VERTEX, //only really need camera information in the vertex shader, as that's what we'll use to manipulate our vertices
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

        // new controller with speed 0.2
        let camera_controller = camera::CameraController::new(0.2);

        // --DEPTH-- //
        let depth_texture =
            texture::Texture::create_depth_texture(&device, &config, "depth_texture");

        // RENDER PIPELINE

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&camera_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main", // Specifies the entry point function name in the shader program
                buffers: &[model::ModelVertex::desc()], // tells wgpu what type of vertices we want to pass to the vertex shader
            },
            fragment: Some(wgpu::FragmentState {
                // technically optional so we wrap in some. We need it to store colour data to the surface
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    //tells wgpu what color outputs it should set up
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList, // every 3 verticies will corrospond to 1 triangle
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw, // how to determine if a triangle is facing us or not
                cull_mode: Some(wgpu::Face::Back),
                // Setting this to anything other than Fill requires Features::NON_FILL_POLYGON_MODE for example line
                polygon_mode: wgpu::PolygonMode::Fill,
                // Requires Features::DEPTH_CLIP_CONTROL
                unclipped_depth: false,
                // Requires Features::CONSERVATIVE_RASTERIZATION
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: texture::Texture::DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less, // pixels drawn front to back
                stencil: wgpu::StencilState::default(),     // 2.
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,                         // how many samples the pipeline will use
                mask: !0,                         // which samples are active, here all of them
                alpha_to_coverage_enabled: false, // has to do with anti aliasing
            },
            multiview: None, // how many layers the render attatchement will have, here not rendering to array textures so none
        });

        // create a vertex buffer to store all my verticies
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        let num_indices = INDICES.len() as u32;

        // --MODEL-- //

        let obj_model = resources::load_model("suzanne.obj", &device).await.unwrap();

        // --INSTANCES-- //

        let position = cgmath::Vector3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        };
        let rotation =
            cgmath::Quaternion::from_axis_angle(cgmath::Vector3::unit_z(), cgmath::Deg(0.0));

        let mut instances: Vec<Instance> = Vec::new();

        instances.push(Instance { position, rotation });

        Application {
            window,
            window_surface,
            device,
            command_queue,
            size,
            config,
            render_pipeline,
            vertex_buffer,
            index_buffer,
            num_indices,
            camera,
            camera_uniform,
            camera_buffer,
            camera_bind_group,
            camera_controller,
            obj_model,
            instances,
            depth_texture,
        }
    }

    //https://docs.rs/winit/latest/winit/  helpfulf for redraw where to put
    fn run(&mut self, event_loop: EventLoop<()>) {
        event_loop.set_control_flow(ControlFlow::Poll);

        let _ = event_loop.run(move |event, elwt| {
            match event {
                Event::WindowEvent {
                    window_id,
                    ref event,
                } if window_id == self.window.id() => {
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

        self.depth_texture =
            texture::Texture::create_depth_texture(&self.device, &self.config, "depth_texture");
    }

    fn update(&mut self) {
        self.camera_controller.update_camera(&mut self.camera);
        self.camera_uniform.update_view_proj(&self.camera);
        self.command_queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::cast_slice(&[self.camera_uniform]),
        );
    }

    fn input(&mut self, event: &WindowEvent) -> bool {
        self.window.request_redraw();
        self.camera_controller.process_events(event)
    }

    // ======================= //
    // ====== RENDER ========= //
    // ======================= //
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

            render_pass.set_pipeline(&self.render_pipeline);

            render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));

            use model::DrawModel;

            for object in &self.obj_model.meshes {
                render_pass.draw_mesh_instanced(object, 0..self.instances.len() as u32);
            }
        }

        // could do drop(render_pass) here if we dont want braces nesting

        // submit will accept anything that implements IntoIter
        self.command_queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    let mut application = pollster::block_on(Application::new(&event_loop));
    application.run(event_loop);
}
