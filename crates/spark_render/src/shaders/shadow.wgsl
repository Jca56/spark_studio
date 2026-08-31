// Shadow casting: every mesh instance, from a light's point of view,
// depth only. The instances are the mesh pass's own; only the matrix
// they are seen through changes.

struct Caster {
    view_proj: mat4x4<f32>,
};

struct Instance {
    model: mat4x4<f32>,
    normal: mat4x4<f32>,
    color: vec4<f32>,
    material: vec4<f32>,
};

@group(0) @binding(0) var<uniform> caster: Caster;
@group(0) @binding(1) var<storage, read> instances: array<Instance>;

@vertex
fn vs_main(@builtin(instance_index) ii: u32, @location(0) pos: vec3<f32>) -> @builtin(position) vec4<f32> {
    return caster.view_proj * instances[ii].model * vec4<f32>(pos, 1.0);
}
