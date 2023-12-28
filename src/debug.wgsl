struct Camera {
    view_pos: vec4<f32>,
    view_proj: mat4x4<f32>,
}
@group(0) @binding(0)
var<uniform> camera: Camera;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) normal: vec3<f32>,
}
struct InstanceInput { //Links to instaneRaw struct in main
    //locations
    @location(5) model_matrix_0: vec4<f32>,
    @location(6) model_matrix_1: vec4<f32>,
    @location(7) model_matrix_2: vec4<f32>,
    @location(8) model_matrix_3: vec4<f32>,
    //normals
    @location(9) normal_matrix_0: vec3<f32>,
    @location(10) normal_matrix_1: vec3<f32>,
    @location(11) normal_matrix_2: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) world_position: vec3<f32>,
}

struct Light {
    position: vec3<f32>,
    color: vec3<f32>,
}
@group(1) @binding(0)
var<uniform> light: Light;

@vertex
fn vs_main(
    model: VertexInput,
    instance: InstanceInput,
) -> VertexOutput {

    let model_matrix = mat4x4<f32>(
        instance.model_matrix_0,
        instance.model_matrix_1,
        instance.model_matrix_2,
        instance.model_matrix_3,
    );

    let normal_matrix = mat3x3<f32>(
        instance.normal_matrix_0,
        instance.normal_matrix_1,
        instance.normal_matrix_2,
    );

    var out: VertexOutput;

    out.tex_coords = model.tex_coords;
    out.world_normal = model.normal; //*normal_matrix SAME PROBLEM AS BELOW, MAYBE WRONG ASSIGNMENT SOMEWHERE

    //  Proper transformation is:
    //  var world_position: vec4<f32> = model_matrix * vec4<f32>(model.position, 1.0);
    //  HOWEVER
    //  This causes weird stretching. Must be to do with the model matrix itself and how it's calcualted i suspect in the negative range but possibly anoter reason
    //  Stick with the below method for now 
    var world_position: vec4<f32> = vec4<f32>(model.position, 1.0); //TODO!(Investigate) Will need to correct before allowing users to rotate the models

    out.world_position = world_position.xyz;
    out.clip_position = camera.view_proj * world_position;
    return out;
}


// Fragment shader
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // let object_color = vec4<f32>(0.9,0.8,0.8, 1.0);
    let object_color = vec4<f32>(in.world_normal, 1.0);

    return vec4<f32>(object_color);
}