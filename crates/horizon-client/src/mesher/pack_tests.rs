//! The shader pack's vertex stream (`PackVertex`).
use super::*;
#[allow(unused_imports)]
use horizon_core::fs::PathExt as _;

fn vertex(pos: [f32; 3], uv: [f32; 2]) -> TerrainVertex {
    TerrainVertex::new(pos, uv, 0, 0, 0)
}

#[test]
fn a_quad_s_stream_holds_its_mean_uv_cell_normal_and_light() {
    let uvs = [[0.25, 0.5], [0.75, 0.5], [0.75, 1.0], [0.25, 1.0]];
    let quad = uvs.map(|uv| vertex([0.0; 3], uv));
    let pack = pack_quad(&quad, 7, pack_cell([3, 15, 9], true), [0.0, 0.0, -1.0], 14);
    let mid = TerrainVertex::new([0.0; 3], [0.5, 0.75], 0, 0, 0).uv;
    for p in pack {
        assert_eq!(p.mid_uv, mid);
        assert_eq!(p.block, 7);
        assert_eq!(p.cell, 3 | 15 << 4 | 9 << 8 | CELL_FLUID);
        assert_eq!(p.normal, [0, 0, -127, 14]);
    }
    assert_eq!(std::mem::size_of::<PackVertex>(), 12);
}

#[test]
fn ids_outside_the_stream_s_range_are_unlisted() {
    assert_eq!(pack_block_id(0), 0);
    assert_eq!(pack_block_id(65534), 65534);
    for id in [-1, 65535, 70000] {
        assert_eq!(pack_block_id(id), PACK_UNLISTED);
    }
}

#[test]
fn quad_normals_follow_the_winding_and_tilt_with_the_quad() {
    // An up face as the models wind it (counter-clockwise from above).
    let up = [[0.0, 1.0, 0.0], [0.0, 1.0, 1.0], [1.0, 1.0, 1.0], [1.0, 1.0, 0.0]];
    assert_eq!(quad_normal(up, [0.0; 3]), [0.0, 1.0, 0.0]);
    let mut down = up;
    down.reverse();
    assert_eq!(quad_normal(down, [0.0; 3]), [0.0, -1.0, 0.0]);
    // A cross plant's diagonal: the real normal, not an axis.
    let cross = [[0.0, 1.0, 0.0], [0.0, 0.0, 0.0], [1.0, 0.0, 1.0], [1.0, 1.0, 1.0]];
    let n = quad_normal(cross, [0.0; 3]);
    let h = std::f32::consts::FRAC_1_SQRT_2;
    assert!((n[0] - h).abs() < 1e-6 && n[1] == 0.0 && (n[2] + h).abs() < 1e-6, "{n:?}");
    // A sloped fluid top leans away from its higher side.
    let slope = [[0.0, 0.9, 0.0], [0.0, 0.9, 1.0], [1.0, 0.5, 1.0], [1.0, 0.5, 0.0]];
    let n = quad_normal(slope, [0.0; 3]);
    assert!(n[0] > 0.0 && n[1] > 0.9 && n[2] == 0.0, "{n:?}");
    // A degenerate quad takes the face's direction.
    assert_eq!(quad_normal([[0.0; 3]; 4], [1.0, 0.0, 0.0]), [1.0, 0.0, 0.0]);
}

#[test]
fn sections_carry_the_stream_only_with_a_pack() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.local/horizon-client/26.3");
    if !root.join("horizon-resources.json").exists_vfs() {
        return;
    }
    let assets = Assets::load(&root).unwrap();
    let render = assets.reload(&root).unwrap();
    let facts = BlockFacts::load(&root, assets.registry).unwrap();
    let visibility = crate::models::load_visibility(&root, assets.registry).unwrap();
    let tables = MeshTables::new(&assets, &render, &facts, visibility.clone()).unwrap();
    let biomes = Biomes::load(&root).unwrap();
    let state = |name| assets.registry.default_state(name).unwrap().0;
    let (stone, water, grass, glowstone) = (
        state("minecraft:stone"),
        state("minecraft:water"),
        state("minecraft:short_grass"),
        state("minecraft:glowstone"),
    );
    let mut chunk = Chunk::empty(0, 0);
    chunk.set(1, MIN_Y + 1, 2, stone);
    chunk.set(5, MIN_Y + 1, 5, water);
    chunk.set(9, MIN_Y + 1, 9, grass);
    chunk.set(12, MIN_Y + 3, 4, glowstone);
    let mut region = [[None; 3]; 3];
    region[1][1] = Some(&chunk);
    let mut tags = PackTags {
        states: vec![PACK_UNLISTED; assets.registry.len()],
        water: 9,
        lava: 11,
        old_lighting: true,
        ao_level: 1.0,
    };
    tags.states[stone as usize] = 1;
    tags.states[grass as usize] = 31;
    tags.states[glowstone as usize] = 89;
    let mesh = |pack: Option<&PackTags>| {
        Mesher {
            nether_shading: false,
            assets: &assets,
            render: &render,
            tables: &tables,
            biomes: &biomes,
            visibility: visibility.clone(),
            zoom: BiomeZoom::new(0),
            blend_radius: 0,
            smooth_lighting: true,
            cutout_leaves: true,
            cull_leaves: false,
            pack,
        }
        .mesh(&region, 0)
    };
    let plain = mesh(None);
    assert!(plain.solid_pack.is_empty() && plain.translucent_pack.is_empty());
    let packed = mesh(Some(&tags));
    // The terrain vertices are those of the plain mesh, opaque.
    assert_eq!(packed.solid.len(), plain.solid.len());
    assert!(packed.solid.iter().all(|v| v.color >> 24 == 0xFF));
    assert_eq!(packed.solid_pack.len(), packed.solid.len());
    assert_eq!(packed.translucent_pack.len(), packed.translucent.len());
    for (quads, pack) in [
        (&packed.solid, &packed.solid_pack),
        (&packed.translucent, &packed.translucent_pack),
    ] {
        for (quad, p) in quads.as_chunks::<4>().0.iter().zip(pack.as_chunks::<4>().0) {
            let cell = [p[0].cell & 15, p[0].cell >> 4 & 15, p[0].cell >> 8 & 15];
            let fluid = p[0].cell & CELL_FLUID != 0;
            // Each quad's stream names its block and faces like its corners
            // (grouping moved both together).
            let n = quad_normal(quad.map(|v| v.pos), [0.0; 3]);
            let expected = pack_normal(n, 0);
            assert_eq!(p[0].normal[..3], expected[..3]);
            let (block, light) = match cell {
                [1, 1, 2] => (1, 0),
                [5, 1, 5] => (9, 0),
                [9, 1, 9] => (31, 0),
                [12, 3, 4] => (89, 15),
                other => panic!("{other:?}"),
            };
            assert_eq!((p[0].block, p[0].normal[3], fluid), (block, light, cell == [5, 1, 5]));
            assert!(p.iter().all(|v| *v == p[0]));
        }
    }
}
