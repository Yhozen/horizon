//! The shape library: block shapes built from boxes with the shared engine's
//! own `VoxelShape` (Minecraft's `Shapes.box`, `Shapes.or`), the per-state
//! shapes of the blocks that are not full cubes, the facts that follow from a
//! shape (full block, sturdy faces, light), and the writer of the cached
//! shape tables (`cached-collision.nbt`, `occlusion.nbt`).
//!
//! A block with [`Form::Shaped`] gets its shapes from [`of`], which picks
//! the family by class; a family module gives the boxes of each state.
use crate::blocks::{Block, Form};
use horizon_core::geometry::{Aabb, Axis, Direction, ShapeOp, Vec3, VoxelShape};
use horizon_world::nbt::Tag;
use std::collections::HashMap;

pub type Shape = VoxelShape;

/// A box in sixteenths of a block, as Minecraft's `Block.box` takes it.
pub fn cuboid(x0: f64, y0: f64, z0: f64, x1: f64, y1: f64, z1: f64) -> Shape {
    let b = Aabb::new(
        Vec3::new(x0 / 16.0, y0 / 16.0, z0 / 16.0),
        Vec3::new(x1 / 16.0, y1 / 16.0, z1 / 16.0),
    );
    Shape::from_box(b).expect("a box inside the block")
}

/// The union of shapes (`Shapes.or`), optimized as Minecraft's is.
pub fn union(shapes: &[Shape]) -> Shape {
    let mut out = Shape::empty();
    for shape in shapes {
        out = out
            .join_unoptimized(shape, ShapeOp::OR)
            .expect("a bounded join");
    }
    out.optimized().expect("an optimized shape")
}

pub fn full() -> Shape {
    Shape::block()
}

pub fn empty() -> Shape {
    Shape::empty()
}

fn from_boxes(boxes: impl IntoIterator<Item = Aabb>) -> Shape {
    let parts: Vec<Shape> = boxes
        .into_iter()
        .map(|b| Shape::from_box(b).expect("a box inside the block"))
        .collect();
    union(&parts)
}

/// Turned `turns` quarter turns clockwise seen from above (north to east)
/// about the block's centre.
pub fn rotate_y(shape: &Shape, turns: u8) -> Shape {
    let mut boxes: Vec<Aabb> = shape.boxes().to_vec();
    for _ in 0..turns % 4 {
        boxes = boxes
            .into_iter()
            .map(|b| {
                Aabb::new(
                    Vec3::new(1.0 - b.max(Axis::Z), b.min(Axis::Y), b.min(Axis::X)),
                    Vec3::new(1.0 - b.min(Axis::Z), b.max(Axis::Y), b.max(Axis::X)),
                )
            })
            .collect();
    }
    from_boxes(boxes)
}

/// Upside down (Minecraft's `OctahedralGroup.INVERT_Y`).
pub fn flip_y(shape: &Shape) -> Shape {
    from_boxes(shape.boxes().iter().map(|b| {
        Aabb::new(
            Vec3::new(b.min(Axis::X), 1.0 - b.max(Axis::Y), b.min(Axis::Z)),
            Vec3::new(b.max(Axis::X), 1.0 - b.min(Axis::Y), b.max(Axis::Z)),
        )
    }))
}

/// The quarter turns from north to a horizontal direction's name.
pub fn turns(facing: &str) -> u8 {
    match facing {
        "east" => 1,
        "south" => 2,
        "west" => 3,
        _ => 0,
    }
}

/// Its boxes as `[min x, min y, min z, max x, max y, max z]`, the order
/// `VoxelShape.toAabbs` gives (the outline field of `gameplay-facts.json`).
pub fn boxes(shape: &Shape) -> Vec<[f64; 6]> {
    shape
        .boxes()
        .iter()
        .map(|b| {
            [
                b.min(Axis::X),
                b.min(Axis::Y),
                b.min(Axis::Z),
                b.max(Axis::X),
                b.max(Axis::Y),
                b.max(Axis::Z),
            ]
        })
        .collect()
}

/// The edges the outline renderer draws (`VoxelShape.forAllEdges`): the
/// grid lines where the cells around them are not a flat face, merged
/// along their axis.
pub fn edges(shape: &Shape) -> Vec<[f64; 6]> {
    let sizes = shape.sizes().map(|s| s as i32);
    let coords = |axis: usize, i: i32| shape.coordinates(Axis::ALL[axis])[i as usize];
    let mut out = Vec::new();
    for axis in 0..3 {
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        let cell = |a: i32, i: i32, j: i32| {
            let mut p = [0; 3];
            p[axis] = a;
            p[u] = i;
            p[v] = j;
            shape.is_full(p[0], p[1], p[2])
        };
        for i in 0..=sizes[u] {
            for j in 0..=sizes[v] {
                let mut start = None;
                for a in 0..=sizes[axis] {
                    let edge = a < sizes[axis] && {
                        let c = [
                            cell(a, i - 1, j - 1),
                            cell(a, i, j - 1),
                            cell(a, i - 1, j),
                            cell(a, i, j),
                        ];
                        let n = c.iter().filter(|x| **x).count();
                        n == 1 || n == 3 || (n == 2 && c[0] == c[3])
                    };
                    match (edge, start) {
                        (true, None) => start = Some(a),
                        (false, Some(s)) => {
                            let mut from = [0.0; 3];
                            from[axis] = coords(axis, s);
                            from[u] = coords(u, i);
                            from[v] = coords(v, j);
                            let mut to = from;
                            to[axis] = coords(axis, a);
                            out.push([from[0], from[1], from[2], to[0], to[1], to[2]]);
                            start = None;
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    out
}

/// `Block.isShapeFullBlock`.
pub fn is_full_block(shape: &Shape) -> bool {
    !shape
        .join_is_not_empty(&Shape::block(), ShapeOp::XOR)
        .expect("a bounded join")
}

/// `BlockState.isFaceSturdy` for the three support types (full, centre,
/// rigid) and six directions, as `block-behaviours.json` packs it: bit
/// `type * 6 + direction`.
pub fn sturdy(support: &Shape) -> u32 {
    let center = cuboid(6.0, 0.0, 6.0, 10.0, 10.0, 10.0);
    let rigid = Shape::block()
        .join(
            &cuboid(2.0, 0.0, 2.0, 14.0, 16.0, 14.0),
            ShapeOp::ONLY_FIRST,
        )
        .expect("a bounded join");
    let mut bits = 0;
    for (d, direction) in Direction::ALL.into_iter().enumerate() {
        let face = support.face_shape(direction).expect("a face");
        let covers = |test: &Shape| {
            !face
                .join_is_not_empty(test, ShapeOp::ONLY_SECOND)
                .expect("a bounded join")
        };
        for (t, test) in [Shape::block(), center.clone(), rigid.clone()]
            .iter()
            .enumerate()
        {
            if covers(test) {
                bits |= 1 << (t * 6 + d);
            }
        }
    }
    bits
}

/// The legacy `isSolid` flag (`BlockBehaviour.calculateSolid`): the
/// collision box is at least 0.73 on average, or a block tall.
pub fn legacy_solid(collision: &Shape) -> bool {
    collision
        .bounds()
        .is_some_and(|b| b.average_size() >= 0.7291666666666666 || b.size().y >= 1.0)
}

/// One state's shapes and the facts that follow from them.
#[derive(Clone)]
pub struct StateShape {
    pub collision: Shape,
    /// What the outline shows and the ray targets (`getShape`).
    pub outline: Shape,
    /// Its light occlusion (`getOcclusionShape`; empty when it cannot occlude).
    pub occlusion: Shape,
    /// `canOcclude`: neighbours' faces against it may be hidden.
    pub can_occlude: bool,
    /// `useShapeForLightOcclusion`.
    pub shape_occlusion: bool,
    /// Holds a water source (`waterlogged=true`).
    pub waterlogged: bool,
}

impl StateShape {
    /// A solid shape (collision, outline and occlusion the same), which may
    /// hold water.
    pub fn solid(shape: Shape, shape_occlusion: bool, waterlogged: bool) -> Self {
        Self {
            collision: shape.clone(),
            outline: shape.clone(),
            occlusion: shape,
            can_occlude: true,
            shape_occlusion,
            waterlogged,
        }
    }
    pub fn full_collision(&self) -> bool {
        is_full_block(&self.collision)
    }
    /// `isSolidRender`.
    pub fn solid_render(&self) -> bool {
        self.can_occlude && is_full_block(&self.occlusion)
    }
    /// `propagatesSkylightDown`.
    pub fn skylight_down(&self) -> bool {
        !is_full_block(&self.outline) && !self.waterlogged
    }
    /// `getLightDampening`.
    pub fn dampening(&self) -> u8 {
        if self.solid_render() {
            15
        } else if self.skylight_down() {
            0
        } else {
            1
        }
    }
}

/// The shapes of a state of a [`Form::Shaped`] block, by its family.
pub fn of(block: &Block, properties: &[(&str, &str)]) -> Option<StateShape> {
    if block.form != Form::Shaped {
        return None;
    }
    let class = |name: &str| block.classes.contains(&name);
    if class("SlabBlock") {
        Some(crate::slabs::slab(properties))
    } else if class("StairBlock") {
        Some(crate::slabs::stairs(properties))
    } else {
        None
    }
}

/// A property's value in a state.
pub fn value<'a>(properties: &[(&str, &'a str)], key: &str) -> &'a str {
    properties
        .iter()
        .find(|(k, _)| *k == key)
        .map_or("", |(_, v)| v)
}

/// The shapes of a cached shape table (`cached-collision.nbt`,
/// `occlusion.nbt`), each written once: the empty shape and the full block
/// first, as Minecraft's canonical shapes 1 and 2.
pub struct Table {
    shapes: Vec<Tag>,
    index: HashMap<String, i32>,
    cached: Vec<i32>,
}

impl Default for Table {
    fn default() -> Self {
        Self::new()
    }
}

impl Table {
    pub fn new() -> Self {
        let mut table = Self {
            shapes: Vec::new(),
            index: HashMap::new(),
            cached: Vec::new(),
        };
        table.intern(&Shape::empty(), Some(1));
        table.intern(&Shape::block(), Some(2));
        table
    }
    fn intern(&mut self, shape: &Shape, canonical: Option<i8>) -> i32 {
        let tag = shape_tag(shape, canonical);
        let key = format!("{:?}", strip_canonical(&tag));
        if let Some(i) = self.index.get(&key) {
            return *i;
        }
        let i = self.shapes.len() as i32;
        self.shapes.push(tag);
        self.index.insert(key, i);
        i
    }
    /// The next state's shape.
    pub fn push(&mut self, shape: &Shape) {
        let i = self.intern(shape, None);
        self.cached.push(i);
    }
    /// The table's tag: `DataVersion`, the states, the shapes and each
    /// state's shape.
    pub fn tag(self, states: Vec<Tag>) -> Tag {
        Tag::compound([
            ("DataVersion", Tag::Int(horizon_world::DATA_VERSION)),
            ("states", Tag::List(states)),
            ("shapes", Tag::List(self.shapes)),
            ("cached_shapes", Tag::IntArray(self.cached)),
        ])
    }
}

fn strip_canonical(tag: &Tag) -> Vec<String> {
    let mut parts = Vec::new();
    for key in [
        "x",
        "y",
        "z",
        "occupied",
        "cube",
        "cube_axes",
        "discrete_min",
        "discrete_max",
    ] {
        parts.push(format!("{key}={:?}", tag.get(key)));
    }
    parts
}

/// A shape as the cached tables write it: the coordinates' bits per axis,
/// the occupied cells (x, then y, then z fastest), whether it is a cube
/// grid, which axes have cube coordinates, and its discrete bounds.
fn shape_tag(shape: &Shape, canonical: Option<i8>) -> Tag {
    let sizes = shape.sizes();
    let mut occupied = Vec::with_capacity(sizes[0] * sizes[1] * sizes[2]);
    for x in 0..sizes[0] as i32 {
        for y in 0..sizes[1] as i32 {
            for z in 0..sizes[2] as i32 {
                occupied.push(u8::from(shape.is_full(x, y, z)));
            }
        }
    }
    let coords = |axis: Axis| {
        Tag::LongArray(
            shape
                .coordinates(axis)
                .iter()
                .map(|v| v.to_bits() as i64)
                .collect(),
        )
    };
    let axes = shape.cube_coordinate_axes();
    let (min, max) = shape.discrete_bounds();
    let mut fields = Vec::new();
    if let Some(c) = canonical {
        fields.push(("canonical", Tag::Byte(c)));
    }
    fields.extend([
        ("x", coords(Axis::X)),
        ("y", coords(Axis::Y)),
        ("z", coords(Axis::Z)),
        ("occupied", Tag::ByteArray(occupied)),
        ("cube", Tag::Byte(i8::from(shape.is_cube()))),
        (
            "cube_axes",
            Tag::Byte(axes[0] as i8 | (axes[1] as i8) << 1 | (axes[2] as i8) << 2),
        ),
        ("discrete_min", Tag::IntArray(min.to_vec())),
        ("discrete_max", Tag::IntArray(max.to_vec())),
    ]);
    Tag::compound(fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boxes_unions_and_turns_are_minecraft_shapes() {
        let slab = cuboid(0.0, 0.0, 0.0, 16.0, 8.0, 16.0);
        assert_eq!(boxes(&slab), vec![[0.0, 0.0, 0.0, 1.0, 0.5, 1.0]]);
        assert!(is_full_block(&union(&[
            slab.clone(),
            cuboid(0.0, 8.0, 0.0, 16.0, 16.0, 16.0)
        ])));
        let north = cuboid(0.0, 8.0, 0.0, 16.0, 16.0, 8.0);
        assert_eq!(
            boxes(&rotate_y(&north, 1)),
            vec![[0.5, 0.5, 0.0, 1.0, 1.0, 1.0]]
        );
        assert_eq!(boxes(&flip_y(&slab)), vec![[0.0, 0.5, 0.0, 1.0, 1.0, 1.0]]);
        // A bottom slab is sturdy below only; a slab is solid.
        assert_eq!(sturdy(&slab), 4161);
        assert_eq!(sturdy(&full()), (1 << 18) - 1);
        assert!(legacy_solid(&slab));
        assert_eq!(edges(&slab).len(), 12);
    }

    #[test]
    fn tables_write_each_shape_once() {
        let mut table = Table::new();
        let slab = cuboid(0.0, 0.0, 0.0, 16.0, 8.0, 16.0);
        for shape in [&empty(), &full(), &slab, &slab.clone()] {
            table.push(shape);
        }
        assert_eq!(table.cached, vec![0, 1, 2, 2]);
        assert_eq!(table.shapes.len(), 3);
    }
}
