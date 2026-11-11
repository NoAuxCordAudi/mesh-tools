//! # Morph Target Demo
//!
//! A flat sheet that can bulge into a dome and ripple like water, driven by two morph targets
//! (blend shapes) whose weights are animated.
//!
//! What it shows:
//! - building a mesh with morph targets through `MeshBuilder` (position and normal deltas)
//! - default weights and target names stored on the mesh
//! - a `CUBICSPLINE` weights animation that loops
//! - **self-verification**: after exporting, the example re-opens its own file with the `gltf`
//!   crate and checks the target count, default weights, a morphed vertex, and the animation
//!   data, and runs the library's validator. It exits non-zero if any check fails.
//!
//! The sheet lies in the XZ plane (y is up) from -2 to 2 in both directions.
//!
//! Usage: `cargo run --example morph_demo [output.glb]`

use gltf::animation::util::ReadOutputs;
use mesh_tools::mesh::MeshBuilder;
use mesh_tools::{CubicKeyframe, GltfBuilder, MorphTarget};

const DEFAULT_OUTPUT: &str = "morph_demo.glb";
/// Grid cells per side; the sheet has `(CELLS + 1)^2` vertices (fits in `u16` indices).
const CELLS: usize = 16;
const HALF: f32 = 2.0;
/// Default weights: a slightly raised dome, no ripple.
const DEFAULT_WEIGHTS: [f32; 2] = [0.25, 0.0];

type Check = Result<(), String>;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args().nth(1).unwrap_or_else(|| DEFAULT_OUTPUT.to_string());
    let mut builder = GltfBuilder::new();

    let material = builder.create_basic_material(Some("SheetMaterial".to_string()), [0.2, 0.55, 0.9, 1.0]);
    let sheet = Sheet::new();

    // Two targets, in the order of the weights below: 0 = Bulge, 1 = Ripple.
    let bulge = MorphTarget::new().with_positions(sheet.bulge_positions()).with_normals(sheet.bulge_normals());
    let ripple = MorphTarget::new().with_positions(sheet.ripple_positions());

    let mesh = MeshBuilder::new(Some("Sheet".to_string()))
        .with_positions(sheet.positions.clone())
        .with_normals(sheet.normals.clone())
        .with_indices(sheet.indices.clone())
        .with_material(material)
        .with_morph_targets(vec![bulge, ripple])
        .with_default_weights(DEFAULT_WEIGHTS.to_vec())
        .with_target_names(vec!["Bulge".to_string(), "Ripple".to_string()])
        .build_into(&mut builder)?;
    let node = builder.add_node(Some("SheetNode".to_string()), Some(mesh), None, None, None);
    builder.add_scene(Some("MorphScene".to_string()), Some(vec![node]));

    // A looping 4.5 second cycle: rest -> bulge -> ripple -> rest. Zero tangents ease in and out.
    let animation = builder.add_animation(Some("Breathe".to_string()));
    let key = |bulge: f32, ripple: f32| CubicKeyframe {
        in_tangent: vec![0.0, 0.0],
        value: vec![bulge, ripple],
        out_tangent: vec![0.0, 0.0],
    };
    builder.add_cubic_weights_track(
        animation,
        node,
        &[0.0, 1.5, 3.0, 4.5],
        &[key(0.0, 0.0), key(1.0, 0.0), key(0.0, 1.0), key(0.0, 0.0)],
    )?;

    // Export validates the document and fails with a list of problems if it is invalid.
    builder.export_glb(&output)?;
    println!("Exported {output}");

    verify(&builder, &output, &sheet).map_err(|e| format!("self-check failed: {e}"))?;
    println!("Self-check passed: 2 targets, default weights {DEFAULT_WEIGHTS:?}, 4-keyframe cubic weights animation");
    Ok(())
}

/// Grid geometry plus the delta data for the two targets.
struct Sheet {
    positions: Vec<f32>,
    normals: Vec<f32>,
    indices: Vec<u16>,
}

impl Sheet {
    fn new() -> Self {
        let n = CELLS + 1;
        let mut positions = Vec::with_capacity(n * n * 3);
        for iz in 0..n {
            for ix in 0..n {
                let x = -HALF + 2.0 * HALF * ix as f32 / CELLS as f32;
                let z = -HALF + 2.0 * HALF * iz as f32 / CELLS as f32;
                positions.extend_from_slice(&[x, 0.0, z]);
            }
        }
        let normals = [0.0, 1.0, 0.0].repeat(n * n);
        let mut indices = Vec::with_capacity(CELLS * CELLS * 6);
        for iz in 0..CELLS {
            for ix in 0..CELLS {
                let a = (iz * n + ix) as u16;
                let (b, c, d) = (a + 1, a + n as u16, a + n as u16 + 1);
                // Counter-clockwise when viewed from above (+Y), matching the +Y normals.
                indices.extend_from_slice(&[a, c, b, b, c, d]);
            }
        }
        Sheet { positions, normals, indices }
    }

    fn xz(&self, v: usize) -> (f32, f32) {
        (self.positions[v * 3], self.positions[v * 3 + 2])
    }

    fn vertex_count(&self) -> usize {
        self.positions.len() / 3
    }

    /// Dome height at (x, z): `1 - r^2 / HALF^2` inside the unit disc scaled to the sheet, else 0.
    fn dome(x: f32, z: f32) -> f32 {
        (1.0 - (x * x + z * z) / (HALF * HALF)).max(0.0)
    }

    fn bulge_positions(&self) -> Vec<f32> {
        (0..self.vertex_count())
            .flat_map(|v| {
                let (x, z) = self.xz(v);
                [0.0, Self::dome(x, z), 0.0]
            })
            .collect()
    }

    /// Normal deltas for the dome: (new unit normal) - (base normal (0,1,0)). Renderers blend these
    /// linearly with the weight and renormalize, so the shading follows the surface.
    fn bulge_normals(&self) -> Vec<f32> {
        (0..self.vertex_count())
            .flat_map(|v| {
                let (x, z) = self.xz(v);
                // h = 1 - (x^2+z^2)/HALF^2 inside the disc: dh/dx = -2x/HALF^2, dh/dz = -2z/HALF^2.
                let inside = Self::dome(x, z) > 0.0;
                let (hx, hz) = if inside { (-2.0 * x / (HALF * HALF), -2.0 * z / (HALF * HALF)) } else { (0.0, 0.0) };
                let len = (hx * hx + 1.0 + hz * hz).sqrt();
                [-hx / len, 1.0 / len - 1.0, -hz / len]
            })
            .collect()
    }

    fn ripple_positions(&self) -> Vec<f32> {
        (0..self.vertex_count())
            .flat_map(|v| {
                let (x, z) = self.xz(v);
                let r = (x * x + z * z).sqrt();
                [0.0, 0.2 * (r * 4.0).sin(), 0.0]
            })
            .collect()
    }
}

/// Re-open the exported file with the independent `gltf` crate and check what we meant to write.
fn verify(builder: &GltfBuilder, path: &str, sheet: &Sheet) -> Check {
    let report = builder.validate();
    if !report.is_valid() || !report.warnings.is_empty() {
        return Err(format!("validation: {report:?}"));
    }

    let bytes = std::fs::read(path).map_err(|e| format!("cannot re-read {path}: {e}"))?;
    let glb = gltf::Gltf::from_slice(&bytes).map_err(|e| format!("the reference loader rejects the file: {e}"))?;
    let blob = glb.blob.as_deref();

    let mesh = glb.document.meshes().next().ok_or("no mesh")?;
    let prim = mesh.primitives().next().ok_or("no primitive")?;
    expect(prim.morph_targets().count() == 2, "the primitive should have 2 morph targets")?;
    expect(mesh.weights() == Some(&DEFAULT_WEIGHTS[..]), "default weights should round-trip")?;

    // A morphed vertex: the sheet's center (index CELLS/2 in both directions) rises by exactly 1
    // when the Bulge weight is 1, and not at all for Ripple (sin(0) = 0 at the center).
    let reader = prim.reader(|_| blob);
    let center = (CELLS / 2) * (CELLS + 1) + CELLS / 2;
    let base: Vec<[f32; 3]> = reader.read_positions().ok_or("no positions")?.collect();
    let targets: Vec<_> = reader.read_morph_targets().collect();
    let bulge: Vec<[f32; 3]> = targets[0].0.clone().ok_or("no bulge deltas")?.collect();
    let morphed_y = base[center][1] + 1.0 * bulge[center][1];
    expect((morphed_y - 1.0).abs() < 1e-6, "the center should rise by 1 at Bulge weight 1")?;
    expect(base.len() == sheet.vertex_count(), "vertex count should round-trip")?;

    // The animation: 4 keyframes x 3 (in, value, out) x 2 targets = 24 weights, with the second
    // keyframe's value (index 1) being [1, 0].
    let chan = glb.document.animations().next().ok_or("no animation")?.channels().next().ok_or("no channel")?;
    let anim = chan.reader(|_| blob);
    let ReadOutputs::MorphTargetWeights(w) = anim.read_outputs().ok_or("no outputs")? else {
        return Err("the channel should output morph target weights".to_string());
    };
    let w: Vec<f32> = w.into_f32().collect();
    expect(w.len() == 4 * 3 * 2, "cubic weights should hold 3 * keyframes * targets values")?;
    expect(w[(3 * 2) + 2..(3 * 2) + 4] == [1.0, 0.0], "keyframe 1's value should be [1, 0]")?;
    Ok(())
}

fn expect(condition: bool, what: &str) -> Check {
    if condition { Ok(()) } else { Err(what.to_string()) }
}
