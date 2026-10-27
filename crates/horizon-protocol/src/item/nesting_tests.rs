use super::*;
use crate::{buf::MAX_NESTING_DEPTH, ids, packets::play};

/// Build wire data iteratively so the test itself never recurses. Each layer
/// contains one more component; `leaf` includes its component id and value.
fn nested_slot(layers: &[i32], leaf: &[u8]) -> Vec<u8> {
    let mut w = Writer::new();
    w.bytes(&[1, 5, 1, 0]); // Slot: count, item, added, removed.
    let mut predicates = 0;
    for &kind in layers {
        w.varint(kind);
        match kind {
            25 => w.bytes(&[5, 1, 1, 0]),    // use_remainder: a template.
            51 => w.bytes(&[1, 5, 1, 1, 0]), // charged_projectiles: one template.
            14 => {
                // can_place_on: one predicate with no blocks/state/NBT and
                // one exact component matcher. Partial matchers follow it.
                w.bytes(&[1, 0, 0, 0, 1]);
                predicates += 1;
            }
            _ => unreachable!(),
        }
    }
    w.bytes(leaf);
    for _ in 0..predicates {
        w.varint(0); // Empty partial-matcher list as each predicate unwinds.
    }
    w.data
}

fn potion_component(effects: usize) -> Vec<u8> {
    assert!(effects > 0);
    // potion_contents: no potion or colour, one custom effect (id 1).
    let mut bytes = vec![53, 0, 0, 1, 1];
    for i in 0..effects {
        // Amplifier, duration, ambient, particles, icon, hidden-effect flag.
        bytes.extend_from_slice(&[0, 1, 0, 1, 1, u8::from(i + 1 < effects)]);
    }
    bytes.push(0); // No custom name.
    bytes
}

fn assert_round_trip(bytes: &[u8]) {
    let mut r = Reader::new(bytes);
    let stack = Slot::read(&mut r).unwrap();
    assert_eq!(r.remaining(), 0);
    let mut w = Writer::new();
    Slot::write(&stack, &mut w);
    assert_eq!(w.data, bytes);
}

fn assert_depth_error(bytes: &[u8]) {
    let error = Slot::read(&mut Reader::new(bytes)).unwrap_err();
    assert_eq!(error.0, "protocol values nest too deep");
}

#[test]
fn templates_and_predicates_share_a_component_depth_limit() {
    for pattern in [&[25][..], &[51], &[14], &[25, 51, 14]] {
        let layers: Vec<_> = pattern
            .iter()
            .copied()
            .cycle()
            .take(MAX_NESTING_DEPTH - 1)
            .collect();
        // The innermost damage component occupies the last allowed level.
        assert_round_trip(&nested_slot(&layers, &[3, 1]));
        let mut deeper = layers;
        deeper.push(25);
        assert_depth_error(&nested_slot(&deeper, &[3, 1]));
    }
}

#[test]
fn hidden_effects_share_the_component_depth_limit() {
    for layers in [vec![], [25, 51, 14].repeat(5)] {
        // The potion component and each hidden effect use the same budget
        // as their enclosing templates and predicates.
        let effects = MAX_NESTING_DEPTH - layers.len() - 1;
        assert_round_trip(&nested_slot(&layers, &potion_component(effects)));
        assert_depth_error(&nested_slot(&layers, &potion_component(effects + 1)));
    }
}

#[test]
fn creative_slot_rejects_stack_overflow_inputs() {
    // Both inputs previously aborted a network reader thread (and therefore
    // the process), before the server could check creative-mode permissions.
    let items = [
        nested_slot(&[25; 4096], &[25, 5, 1, 0, 0]),
        nested_slot(&[], &potion_component(65536)),
    ];
    assert_eq!(items[0].len() + 2, 20_491);
    for item in items {
        let mut w = Writer::new();
        w.i16(0);
        w.bytes(&item);
        let error =
            play::Serverbound::decode(ids::play::serverbound::SET_CREATIVE_MODE_SLOT, &w.data)
                .unwrap_err();
        assert_eq!(error.0, "protocol values nest too deep");
    }
}

#[test]
fn sibling_templates_do_not_accumulate_depth() {
    // Each child reaches the limit while inside charged_projectiles, but
    // reading one sibling must release its budget before the next begins.
    let child = nested_slot(&[25; MAX_NESTING_DEPTH - 2], &[3, 1]);
    let mut w = Writer::new();
    w.bytes(&[1, 5, 1, 0, 51]);
    w.varint(64);
    for _ in 0..64 {
        w.bytes(&[5, 1]); // Template has item before count, unlike Slot.
        w.bytes(&child[2..]);
    }
    assert_round_trip(&w.data);
}

#[test]
fn failed_component_reads_release_the_depth_budget() {
    let bad = nested_slot(&[25; MAX_NESTING_DEPTH], &[3, 1]);
    let good = nested_slot(&[25; MAX_NESTING_DEPTH - 1], &[3, 1]);
    let bytes = [bad.as_slice(), good.as_slice()].concat();
    let mut r = Reader::new(&bytes);
    assert!(Slot::read(&mut r).is_err());
    r.bytes(bad.len() - r.position()).unwrap();
    assert!(Slot::read(&mut r).unwrap().is_some());
    assert_eq!(r.remaining(), 0);
}
