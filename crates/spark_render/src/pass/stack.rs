//! The stack: everything drawn over the opaque picture, back to front —
//! the shapes, and the meshes that are see-through.
//!
//! An opaque mesh is laid down first and writes depth; everything after
//! it tests against that. A mesh with an opacity under one can't be drawn
//! that way: it has to blend over what is behind it, which has to be there
//! already, and it has to sit under what is in front of it, which can't
//! have been drawn yet. So it sorts among the shapes, and the stage draws
//! the stack in runs — a run of shapes, a run of see-through meshes, a run
//! of shapes — each mesh run through the mesh pass's multisampled targets
//! and blitted onto the stage in its turn.
//!
//! Where a see-through mesh sorts: in its shape's place. A mesh object is
//! a kind-6 shape that draws no quad of its own, already in the list and
//! already sorted — by depth, list order on ties — so a 2D comp stacks the
//! way the outliner reads, mesh or not. An instance that names no shape
//! (`slot: None`) sorts by its own centre, after the shapes at the same
//! depth. The marks drawn over everything keep to the end, as they do
//! without meshes.

use std::cmp::Ordering;
use std::ops::Range;

use super::Scene;
use crate::math::{Mat4, Vec3};
use crate::shapes::Shape;

/// One run of the stack, in drawing order.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Run {
    /// A range of the sorted shapes.
    Shapes(Range<usize>),
    /// See-through meshes, as indices into `Scene::meshes`, back to front.
    Meshes(Vec<usize>),
}

/// The stack sorted: the shapes with their models and clocks in drawing
/// order, and the runs that put each see-through mesh in among them.
pub struct Stack {
    pub shapes: Vec<Shape>,
    pub models: Vec<Mat4>,
    pub clocks: Vec<f32>,
    pub runs: Vec<Run>,
}

impl Stack {
    fn push_shape(&mut self, scene: &Scene, i: usize) {
        self.shapes.push(scene.shapes[i]);
        self.models.push(scene.model(i));
        self.clocks.push(scene.clock(i));
    }

    /// Close the run of shapes since `from`, if there is one.
    fn close_shapes(&mut self, from: &mut usize) {
        let to = self.shapes.len();
        if *from < to {
            self.runs.push(Run::Shapes(*from..to));
        }
        *from = to;
    }

    /// A run of meshes — joined onto the last one when that is a mesh run
    /// too, so two neighbours cost one pass.
    fn push_meshes(&mut self, from: &mut usize, which: Vec<usize>) {
        if which.is_empty() {
            return;
        }
        self.close_shapes(from);
        match self.runs.last_mut() {
            Some(Run::Meshes(run)) => run.extend(which),
            _ => self.runs.push(Run::Meshes(which)),
        }
    }
}

/// What sorts in the scene proper: a shape by index, or a see-through
/// mesh tied to no shape.
#[derive(Clone, Copy)]
enum Item {
    Shape(usize),
    Mesh(usize),
}

fn by_depth_back_to_front<T>(items: &mut [(f32, T)]) {
    items.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(Ordering::Equal));
}

impl Scene<'_> {
    /// The stack in drawing order: back to front by view depth, stably, so
    /// shapes at one depth keep their list order — which is how a 2D
    /// comp, all of it on one plane, stacks exactly the way it did. The
    /// see-through meshes take their shapes' places (see the module doc),
    /// and the marks drawn over everything keep to the end, sorted among
    /// themselves, so `over` still counts them off the tail.
    pub fn sorted(&self) -> Stack {
        let n = self.shapes.len();
        let split = n.saturating_sub(self.over);
        let depth_of = |i: usize| {
            let c = self.shapes[i].center();
            let p = self.model(i).transform_point(Vec3::new(c[0], c[1], 0.0));
            self.camera.depth(p)
        };
        // Which see-through meshes each shape brings with it, and the
        // ones tied to no shape, which sort on their own.
        let mut attached: Vec<Vec<usize>> = vec![Vec::new(); split];
        let mut items: Vec<(f32, Item)> = (0..split).map(|i| (depth_of(i), Item::Shape(i))).collect();
        for (j, m) in self.meshes.iter().enumerate() {
            if !m.visible() || m.opaque() {
                continue;
            }
            match m.slot.filter(|&s| s < split) {
                Some(s) => attached[s].push(j),
                None => items.push((self.camera.depth(m.centre()), Item::Mesh(j))),
            }
        }
        by_depth_back_to_front(&mut items);
        let mut tail: Vec<(f32, usize)> = (split..n).map(|i| (depth_of(i), i)).collect();
        by_depth_back_to_front(&mut tail);

        let mut stack = Stack {
            shapes: Vec::with_capacity(n),
            models: Vec::with_capacity(n),
            clocks: Vec::with_capacity(n),
            runs: Vec::new(),
        };
        let mut from = 0;
        for (_, item) in items {
            match item {
                Item::Shape(i) => {
                    stack.push_shape(self, i);
                    let mine = std::mem::take(&mut attached[i]);
                    stack.push_meshes(&mut from, mine);
                }
                Item::Mesh(j) => stack.push_meshes(&mut from, vec![j]),
            }
        }
        for (_, i) in tail {
            stack.push_shape(self, i);
        }
        stack.close_shapes(&mut from);
        stack
    }
}

#[cfg(test)]
mod tests {
    use super::super::mesh::{GpuMesh, MeshInstance};
    use super::*;
    use crate::camera::Camera;

    fn at(z: f32) -> Mat4 {
        Mat4::translation(Vec3::new(0.0, 0.0, z))
    }

    fn scene<'a>(
        shapes: &'a [Shape],
        models: &'a [Mat4],
        meshes: &'a [MeshInstance<'a>],
        camera: &'a Camera,
        over: usize,
    ) -> Scene<'a> {
        Scene {
            shapes,
            models,
            paths: &[],
            meshes,
            lights: &[],
            camera,
            time: 0.0,
            clocks: &[],
            over,
        }
    }

    fn xs(s: &[Shape]) -> Vec<f32> {
        s.iter().map(|s| s.center()[0]).collect()
    }

    /// Back to front by depth — except the marks drawn over everything,
    /// which keep to the tail, sorted among themselves.
    #[test]
    fn the_marks_over_everything_sort_last() {
        let shapes = [
            Shape::circle([100.0, 100.0], 10.0),
            Shape::circle([200.0, 100.0], 10.0),
            Shape::circle([300.0, 100.0], 10.0),
            Shape::circle([400.0, 100.0], 10.0),
        ];
        // A: on the canvas; B: nearer; C, D: marks, C far behind, D nearest.
        let models = [at(0.0), at(200.0), at(-500.0), at(400.0)];
        let camera = Camera::stage(crate::shapes::CANVAS);
        let s = scene(&shapes, &models, &[], &camera, 2);
        let stack = s.sorted();
        assert_eq!(xs(&stack.shapes), vec![100.0, 200.0, 300.0, 400.0]);
        assert_eq!(stack.models[2], at(-500.0));
        assert_eq!(stack.runs, vec![Run::Shapes(0..4)]);
        // Without the split, the far mark is drawn first of all.
        let plain = Scene { over: 0, ..s };
        assert_eq!(xs(&plain.sorted().shapes), vec![300.0, 100.0, 200.0, 400.0]);
        // More marks than shapes is every shape a mark, not a panic.
        let all = Scene { over: 9, ..s };
        assert_eq!(all.sorted().shapes.len(), 4);
    }

    /// A stand-in the sort can hold: the sort never touches the buffers.
    fn stub_mesh(device: &wgpu::Device, queue: &wgpu::Queue) -> GpuMesh {
        let mut pass = super::super::mesh::MeshPass::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
        pass.upload(
            device,
            queue,
            &super::super::mesh::MeshData {
                positions: vec![[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [0.0, 1.0, 0.0]],
                normals: vec![[0.0, 0.0, 1.0]; 3],
                uvs: vec![],
                indices: vec![0, 1, 2],
            },
            None,
        )
    }

    fn instance<'a>(mesh: &'a GpuMesh, model: Mat4, a: f32, slot: Option<usize>) -> MeshInstance<'a> {
        MeshInstance {
            mesh,
            model,
            color: [1.0, 1.0, 1.0, a],
            unlit: true,
            slot,
        }
    }

    /// A see-through mesh takes its shape's place in the stack: between
    /// the shapes before and after it in list order at one depth, and
    /// wherever its depth puts it otherwise. Opaque and invisible ones
    /// never join the stack; slotless ones sort by their own centre,
    /// after the shapes at that depth.
    #[test]
    fn see_through_meshes_take_their_shapes_places() {
        let Some((device, queue)) = super::super::harness::device() else { return };
        let _held = super::super::harness::exclusive();
        let mesh = stub_mesh(device, queue);
        let shapes = [
            Shape::circle([100.0, 100.0], 10.0),
            Shape::mesh([200.0, 100.0], [10.0, 10.0], 0),
            Shape::circle([300.0, 100.0], 10.0),
            Shape::circle([400.0, 100.0], 10.0),
        ];
        // All on the canvas: list order is the stack.
        let models = [at(0.0); 4];
        let camera = Camera::stage(crate::shapes::CANVAS);
        let meshes = [
            instance(&mesh, at(0.0), 0.5, Some(1)),
            instance(&mesh, at(0.0), 1.0, Some(1)),
            instance(&mesh, at(0.0), 0.0, Some(1)),
        ];
        let s = scene(&shapes, &models, &meshes, &camera, 1);
        let stack = s.sorted();
        assert_eq!(xs(&stack.shapes), vec![100.0, 200.0, 300.0, 400.0]);
        assert_eq!(
            stack.runs,
            vec![Run::Shapes(0..2), Run::Meshes(vec![0]), Run::Shapes(2..4)]
        );
        // Its shape moved nearer than everything: the mesh run is last of
        // the scene, and the mark over everything still after it.
        let models = [at(0.0), at(300.0), at(0.0), at(0.0)];
        let s = scene(&shapes, &models, &meshes, &camera, 1);
        let stack = s.sorted();
        assert_eq!(xs(&stack.shapes), vec![100.0, 300.0, 200.0, 400.0]);
        assert_eq!(
            stack.runs,
            vec![Run::Shapes(0..3), Run::Meshes(vec![0]), Run::Shapes(3..4)]
        );
        // Tied to no shape: far behind, it is drawn first of all; at the
        // canvas's depth it comes after the shapes there — and next to
        // another mesh run, it joins it.
        let meshes = [
            instance(&mesh, at(0.0), 0.5, Some(1)),
            instance(&mesh, at(-500.0), 0.5, None),
            instance(&mesh, at(0.0), 0.5, None),
        ];
        let models = [at(0.0); 4];
        let s = scene(&shapes, &models, &meshes, &camera, 1);
        let stack = s.sorted();
        assert_eq!(
            stack.runs,
            vec![
                Run::Meshes(vec![1]),
                Run::Shapes(0..2),
                Run::Meshes(vec![0]),
                Run::Shapes(2..3),
                Run::Meshes(vec![2]),
                Run::Shapes(3..4),
            ]
        );
        // No shapes at all: the meshes alone, back to front.
        let s = scene(&[], &[], &meshes, &camera, 0);
        assert_eq!(s.sorted().runs, vec![Run::Meshes(vec![1, 0, 2])]);
    }
}
