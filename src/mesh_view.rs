//! Depth-buffered reference surfaces. OpenGL calls run on eframe's GL thread.
use eframe::glow::{self, HasContext};
use opensourcecam::import::Geometry;
use std::sync::Arc;

pub struct MeshView {
    program: glow::Program,
    vao: glow::VertexArray,
    buffer: glow::Buffer,
    surfaces: Vec<Surface>,
    count: i32,
}

pub struct View {
    pub camera: [f32; 9],
    pub center: [f32; 2],
    pub scale: [f32; 2],
    pub pan: [f32; 2],
    pub depth: [f32; 2],
}

#[derive(Clone)]
pub struct Surface {
    pub geometry: Arc<Geometry>,
    pub units: f64,
    pub offset: [f64; 3],
    pub rotation: [f32; 9],
    pub color: [f32; 3],
}
impl Surface {
    fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.geometry, &other.geometry)
            && self.units == other.units
            && self.offset == other.offset
            && self.rotation == other.rotation
            && self.color == other.color
    }
    fn rotate(&self, p: [f64; 3]) -> [f64; 3] {
        std::array::from_fn(|i| (0..3).map(|j| self.rotation[j * 3 + i] as f64 * p[j]).sum())
    }
}

const VERTEX: &str = "#version 140
in vec3 a_position;
in vec3 a_normal;
in vec3 a_color;
uniform mat3 u_camera;
uniform vec2 u_center;
uniform vec2 u_scale;
uniform vec2 u_pan;
uniform vec2 u_depth;
out vec3 v_color;
void main() {
    vec3 view = u_camera * a_position;
    gl_Position = vec4((view.x-u_center.x)*u_scale.x+u_pan.x,
                      -(view.y-u_center.y)*u_scale.y-u_pan.y,
                      -(view.z-u_depth.x)/u_depth.y, 1.0);
    float light = abs(dot(normalize(a_normal), normalize(vec3(0.35,-0.45,0.82))));
    v_color = a_color*(0.55+0.45*light);
}";
const FRAGMENT: &str = "#version 140
in vec3 v_color;
out vec4 out_color;
void main() { out_color = vec4(v_color, 1.0); }";

impl MeshView {
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        // SAFETY: CreationContext supplies the active native OpenGL context.
        unsafe {
            let vertex = shader(gl, glow::VERTEX_SHADER, VERTEX)?;
            let fragment = match shader(gl, glow::FRAGMENT_SHADER, FRAGMENT) {
                Ok(shader) => shader,
                Err(error) => {
                    gl.delete_shader(vertex);
                    return Err(error);
                }
            };
            let program = gl.create_program()?;
            gl.attach_shader(program, vertex);
            gl.attach_shader(program, fragment);
            gl.bind_attrib_location(program, 0, "a_position");
            gl.bind_attrib_location(program, 1, "a_normal");
            gl.bind_attrib_location(program, 2, "a_color");
            gl.link_program(program);
            gl.detach_shader(program, vertex);
            gl.detach_shader(program, fragment);
            gl.delete_shader(vertex);
            gl.delete_shader(fragment);
            if !gl.get_program_link_status(program) {
                let error = gl.get_program_info_log(program);
                gl.delete_program(program);
                return Err(format!("Cannot link model renderer: {error}"));
            }
            let vao = gl.create_vertex_array()?;
            let buffer = gl.create_buffer()?;
            Ok(Self {
                program,
                vao,
                buffer,
                surfaces: Vec::new(),
                count: 0,
            })
        }
    }

    pub fn paint(&mut self, gl: &glow::Context, surfaces: Vec<Surface>, view: &View) {
        // SAFETY: Only invoked from egui_glow's native paint callback. Buffers
        // contain validated, bounded mesh data; eframe restores egui GL state.
        unsafe {
            gl.bind_vertex_array(Some(self.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.buffer));
            if self.surfaces.len() != surfaces.len()
                || !self.surfaces.iter().zip(&surfaces).all(|(a, b)| a.same(b))
            {
                let mut data = Vec::new();
                let mut count = 0;
                for surface in &surfaces {
                    for triangle in &surface.geometry.mesh.triangles {
                        let vertices = triangle.map(|i| surface.geometry.mesh.vertices[i as usize]);
                        let normal = surface.rotate(opensourcecam::import::normal(vertices));
                        let length = normal.iter().map(|v| v * v).sum::<f64>().sqrt().max(1e-20);
                        let normal = normal.map(|v| v / length);
                        for vertex in vertices {
                            let rotated = surface.rotate(vertex.map(|v| v * surface.units));
                            let position: [f64; 3] =
                                std::array::from_fn(|i| rotated[i] + surface.offset[i]);
                            for value in position
                                .into_iter()
                                .chain(normal)
                                .chain(surface.color.map(|v| v as f64))
                            {
                                data.extend_from_slice(&(value as f32).to_ne_bytes());
                            }
                            count += 1;
                        }
                    }
                }
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &data, glow::STATIC_DRAW);
                self.count = count;
                self.surfaces = surfaces;
            }
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 36, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, 36, 12);
            gl.enable_vertex_attrib_array(2);
            gl.vertex_attrib_pointer_f32(2, 3, glow::FLOAT, false, 36, 24);
            gl.use_program(Some(self.program));
            let location = |name| gl.get_uniform_location(self.program, name);
            gl.uniform_matrix_3_f32_slice(location("u_camera").as_ref(), false, &view.camera);
            for (name, vector) in [
                ("u_center", view.center),
                ("u_scale", view.scale),
                ("u_pan", view.pan),
                ("u_depth", view.depth),
            ] {
                gl.uniform_2_f32(location(name).as_ref(), vector[0], vector[1]);
            }
            gl.enable(glow::DEPTH_TEST);
            gl.depth_mask(true);
            gl.depth_func(glow::LESS);
            gl.clear_depth_f32(1.0);
            gl.clear(glow::DEPTH_BUFFER_BIT);
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::BLEND);
            gl.draw_arrays(glow::TRIANGLES, 0, self.count);
            gl.disable(glow::DEPTH_TEST);
            gl.depth_mask(false);
        }
    }

    pub fn destroy(&mut self, gl: &glow::Context) {
        // SAFETY: App::on_exit provides this same active context, exactly once.
        unsafe {
            gl.delete_buffer(self.buffer);
            gl.delete_vertex_array(self.vao);
            gl.delete_program(self.program);
        }
    }
}

unsafe fn shader(gl: &glow::Context, kind: u32, source: &str) -> Result<glow::Shader, String> {
    let shader = gl.create_shader(kind)?;
    gl.shader_source(shader, source);
    gl.compile_shader(shader);
    if !gl.get_shader_compile_status(shader) {
        let error = gl.get_shader_info_log(shader);
        gl.delete_shader(shader);
        Err(format!(
            "Model preview requires OpenGL 3.1 or newer: {error}"
        ))
    } else {
        Ok(shader)
    }
}
