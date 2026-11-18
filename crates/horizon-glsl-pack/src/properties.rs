//! `.properties` files and the source comments the format reads:
//! `block.properties`, `dimension.properties`, `DRAWBUFFERS`. OWNER: S1.

use std::collections::BTreeMap;

use crate::{PackError, PackFiles};

/// `block.properties`: block names → `mc_Entity.x` ids, compressed to at most
/// 31 material indices (the vertex tag's 5 bits; 31 = none).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlockIds {
    /// Material index → id, ascending ids.
    pub ids: Vec<u32>,
    /// Block name without namespace (`water`) → material index.
    pub by_name: BTreeMap<String, u8>,
}

impl BlockIds {
    /// The material index of a block (`water` or `minecraft:water`), or
    /// [`crate::NO_MATERIAL`].
    pub fn material(&self, block: &str) -> u8 {
        let _ = block;
        todo!("S1")
    }
}

/// Reads `shaders/block.properties` (preprocessed: it may hold `#if` lines).
/// No file is an empty table.
pub fn block_ids(files: &PackFiles, predefined: &BTreeMap<String, String>) -> Result<BlockIds, PackError> {
    let _ = (files, predefined);
    todo!("S1")
}

/// `key=value` lines (`\` continuations joined, `#` comments skipped).
pub fn parse_properties(text: &str) -> Vec<(String, String)> {
    let _ = text;
    todo!("S1")
}

/// `/* DRAWBUFFERS:067 */` → `[0, 6, 7]`, `/* RENDERTARGETS: 0,6,7 */` → same.
/// The last such comment in the source wins; `None` when there is none.
pub fn draw_buffers(source: &str) -> Option<Vec<u8>> {
    let _ = source;
    todo!("S1")
}

/// The folder under `shaders/` whose programs a dimension uses: from
/// `dimension.properties` when present, else `world0` for the overworld,
/// `world-1` for the nether, `world1` for the end.
pub fn dimension_folder(files: &PackFiles, dimension: &str) -> String {
    let _ = (files, dimension);
    todo!("S1")
}
