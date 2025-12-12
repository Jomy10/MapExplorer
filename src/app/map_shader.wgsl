// This shader simply a texture to the screen
// It requires exactly 6 vertices (draws a rectangle using 2 triangles)
// The MapDeltaUniform moves the rectangle by the given amounts

struct VertexOutput {
  @builtin(position) clip_position: vec4<f32>,
  @location(0) tex_co: vec2<f32>,
};

struct MapDeltaUniform {
  delta: vec2<f32>,
};

@group(1) @binding(0)
var<uniform> map_delta: MapDeltaUniform;

const rectangle_vertices = array<vec2<f32>, 6>(
  // First triangle
  vec2<f32>(-1, -1),  // bottom-left
  vec2<f32>(1, -1),   // bottom-right
  vec2<f32>(-1, 1),   // top-left
  // Second triangle
  vec2<f32>(-1, 1),   // top-left (repeated)
  vec2<f32>(1, -1),   // bottom-right (repeated)
  vec2<f32>(1, 1)     // top-right
);

@vertex
fn vs_main(
  @builtin(vertex_index) in_vertex_index: u32,
) -> VertexOutput {
  var out: VertexOutput;

  let vertex_idx = in_vertex_index % 6u;
  let pos = rectangle_vertices[vertex_idx];

  out.clip_position = vec4<f32>(pos + map_delta.delta, 0.0, 1.0);
  out.tex_co = pos * -0.5 + vec2(0.5);
  return out;
}

@group(0) @binding(0)
var map_tex_diffuse: texture_2d<f32>;
@group(0) @binding(1)
var map_sampler_diffuse: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
  return textureSample(map_tex_diffuse, map_sampler_diffuse, in.tex_co);
 // return vec4<f32>(0.3, 0.2, 0.1, 1.0);
}
