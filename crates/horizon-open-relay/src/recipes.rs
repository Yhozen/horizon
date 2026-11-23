//! Recipe book display ids between a Java server and an open player.
//!
//! A server numbers its recipe displays in its own recipe order; the open
//! client knows only the bundle's recipes and finds a display by the id
//! `RecipeIndex` gives it (`data/minecraft/recipe`, sorted by name). So the
//! relay matches each display the server sends to the bundle's recipe that
//! draws the same (shape, ingredients, result) and renames it; a display the
//! bundle has no recipe for is left out, so the book never shows one recipe
//! and places another. The ids learnt are shared with the serverbound side,
//! which renames the player's picks back.
use horizon_protocol::packets::recipes::{RecipeDisplay, SlotDisplay};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// How a recipe draws in the book: what a server display is matched on.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Drawing {
    /// `Some((width, height))` for a shaped recipe, `None` for a shapeless one.
    shape: Option<(i32, i32)>,
    /// Each cell's choices (item names, `#tag`), sorted; empty for an empty
    /// cell. A shapeless recipe's cells are sorted too.
    cells: Vec<Vec<String>>,
    /// The result's item and count.
    result: (String, i32),
}

/// The bundle's recipes that the book can show, by drawing: their display ids.
#[derive(Clone, Debug, Default)]
pub struct OpenBook {
    by_drawing: HashMap<Drawing, i32>,
}

impl OpenBook {
    /// Reads `data/minecraft/recipe/*.json` under `root` (none: an empty book).
    pub fn load(root: &Path) -> Result<Self, String> {
        let dir = root.join("data/minecraft/recipe");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Ok(Self::default());
        };
        let mut recipes = BTreeMap::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(stem) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .filter(|_| path.extension().is_some_and(|e| e == "json"))
            else {
                continue;
            };
            let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let json =
                serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
            recipes.insert(format!("minecraft:{stem}"), json);
        }
        Ok(Self::from_recipes(&recipes))
    }

    /// The book of these recipes, numbered as `RecipeIndex::load` numbers
    /// them: by name, one display each, none for a special recipe, one per
    /// material count for a transmute recipe.
    pub fn from_recipes(recipes: &BTreeMap<String, serde_json::Value>) -> Self {
        let mut by_drawing = HashMap::new();
        let mut next = 0;
        for v in recipes.values() {
            let kind = v["type"].as_str().unwrap_or_default();
            let displays = if kind.starts_with("minecraft:crafting_special_")
                || kind == "minecraft:crafting_decorated_pot"
            {
                0
            } else if kind == "minecraft:crafting_transmute" {
                match &v["material_count"] {
                    serde_json::Value::Object(o) => {
                        let min = o.get("min").and_then(|m| m.as_i64()).unwrap_or(1);
                        let max = o.get("max").and_then(|m| m.as_i64()).unwrap_or(8);
                        (max - min).max(0) + 1
                    }
                    _ => 1,
                }
            } else {
                1
            };
            if displays == 1
                && let Some(d) = drawing_of_json(v)
            {
                by_drawing.entry(d).or_insert(next);
            }
            next += displays as i32;
        }
        Self { by_drawing }
    }

    /// The bundle's display that draws like a server's, if any.
    pub fn display(
        &self,
        d: &RecipeDisplay,
        item_name: &dyn Fn(i32) -> Option<String>,
    ) -> Option<i32> {
        self.by_drawing
            .get(&drawing_of_display(d, item_name)?)
            .copied()
    }

    pub fn len(&self) -> usize {
        self.by_drawing.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_drawing.is_empty()
    }
}

/// One connection's display ids, server's to open and back.
#[derive(Debug, Default)]
pub struct DisplayMap {
    to_open: HashMap<i32, i32>,
    to_server: HashMap<i32, i32>,
}
impl DisplayMap {
    pub fn learn(&mut self, server: i32, open: i32) {
        self.to_open.insert(server, open);
        self.to_server.insert(open, server);
    }
    pub fn forget(&mut self, server: i32) -> Option<i32> {
        let open = self.to_open.remove(&server)?;
        self.to_server.remove(&open);
        Some(open)
    }
    pub fn clear(&mut self) {
        self.to_open.clear();
        self.to_server.clear();
    }
    pub fn to_server(&self, open: i32) -> Option<i32> {
        self.to_server.get(&open).copied()
    }
}

/// The map both directions of one connection share.
pub type Displays = Arc<Mutex<DisplayMap>>;

/// A recipe file's drawing (shaped and shapeless crafting only).
fn drawing_of_json(v: &serde_json::Value) -> Option<Drawing> {
    let result = (
        v["result"]["id"].as_str()?.to_owned(),
        v["result"]["count"].as_i64().unwrap_or(1) as i32,
    );
    match v["type"].as_str()? {
        "minecraft:crafting_shaped" => {
            let rows: Vec<Vec<char>> = v["pattern"]
                .as_array()?
                .iter()
                .map(|r| r.as_str().map(|s| s.chars().collect()))
                .collect::<Option<_>>()?;
            // ShapedRecipePattern.shrink: empty rows and columns at the edges go.
            let filled = |c: &char| *c != ' ';
            let used_rows: Vec<usize> = (0..rows.len())
                .filter(|&y| rows[y].iter().any(filled))
                .collect();
            let width = rows.iter().map(Vec::len).max()?;
            let used_cols: Vec<usize> = (0..width)
                .filter(|&x| rows.iter().any(|r| r.get(x).is_some_and(filled)))
                .collect();
            let (y0, y1) = (*used_rows.first()?, *used_rows.last()?);
            let (x0, x1) = (*used_cols.first()?, *used_cols.last()?);
            let mut cells = Vec::new();
            for row in &rows[y0..=y1] {
                for x in x0..=x1 {
                    cells.push(match row.get(x).copied().unwrap_or(' ') {
                        ' ' => Vec::new(),
                        c => ingredient(&v["key"][c.to_string()])?,
                    });
                }
            }
            Some(Drawing {
                shape: Some(((x1 - x0 + 1) as i32, (y1 - y0 + 1) as i32)),
                cells,
                result,
            })
        }
        "minecraft:crafting_shapeless" => {
            let mut cells: Vec<Vec<String>> = v["ingredients"]
                .as_array()?
                .iter()
                .map(ingredient)
                .collect::<Option<_>>()?;
            cells.sort();
            Some(Drawing {
                shape: None,
                cells,
                result,
            })
        }
        _ => None,
    }
}

/// A recipe file's ingredient: an item, `#tag`, or a list of items.
fn ingredient(v: &serde_json::Value) -> Option<Vec<String>> {
    let mut names = match v {
        serde_json::Value::String(s) => vec![s.clone()],
        serde_json::Value::Array(list) => list
            .iter()
            .map(|i| i.as_str().map(String::from))
            .collect::<Option<_>>()?,
        _ => return None,
    };
    names.sort();
    names.dedup();
    (!names.is_empty()).then_some(names)
}

/// A server display's drawing (shaped and shapeless crafting only).
fn drawing_of_display(d: &RecipeDisplay, name: &dyn Fn(i32) -> Option<String>) -> Option<Drawing> {
    let cells = |list: &[SlotDisplay]| -> Option<Vec<Vec<String>>> {
        list.iter().map(|s| choices(s, name)).collect()
    };
    let result = |s: &SlotDisplay| -> Option<(String, i32)> {
        match s {
            SlotDisplay::ItemStack(stack) => Some((name(stack.item)?, stack.count)),
            SlotDisplay::Item(id) => Some((name(*id)?, 1)),
            _ => None,
        }
    };
    match d {
        RecipeDisplay::Shaped {
            width,
            height,
            ingredients,
            result: r,
            ..
        } => Some(Drawing {
            shape: Some((*width, *height)),
            cells: cells(ingredients)?,
            result: result(r)?,
        }),
        RecipeDisplay::Shapeless {
            ingredients,
            result: r,
            ..
        } => {
            let mut cells = cells(ingredients)?;
            cells.sort();
            Some(Drawing {
                shape: None,
                cells,
                result: result(r)?,
            })
        }
        _ => None,
    }
}

/// What one ingredient slot accepts (`Ingredient.display`: a tag, or the
/// items, each maybe with its crafting remainder), sorted.
fn choices(s: &SlotDisplay, name: &dyn Fn(i32) -> Option<String>) -> Option<Vec<String>> {
    use horizon_protocol::packets::recipes::HolderSet;
    let mut out = match s {
        SlotDisplay::Empty => Vec::new(),
        SlotDisplay::Item(id) => vec![name(*id)?],
        SlotDisplay::Tag(HolderSet::Tag(tag)) => vec![format!("#{tag}")],
        SlotDisplay::Tag(HolderSet::Items(ids)) => {
            ids.iter().map(|i| name(*i)).collect::<Option<_>>()?
        }
        SlotDisplay::WithRemainder { input, .. } => choices(input, name)?,
        SlotDisplay::Composite(list) => {
            let mut all = Vec::new();
            for s in list {
                all.extend(choices(s, name)?);
            }
            all
        }
        _ => return None,
    };
    out.sort();
    out.dedup();
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_protocol::item::Stack;
    use horizon_protocol::packets::recipes::HolderSet;

    fn names(id: i32) -> Option<String> {
        [
            "minecraft:air",
            "minecraft:cobblestone",
            "minecraft:furnace",
            "minecraft:crafting_table",
        ]
        .get(id as usize)
        .map(|s| s.to_string())
    }

    fn book() -> OpenBook {
        let recipes: BTreeMap<String, serde_json::Value> = [
            // Display 0: a special recipe has none, so the furnace is 0.
            (
                "minecraft:a_special",
                serde_json::json!({"type": "minecraft:crafting_special_mapcloning"}),
            ),
            (
                "minecraft:furnace",
                serde_json::json!({
                    "type": "minecraft:crafting_shaped",
                    "pattern": ["###", "# #", "###", "   "],
                    "key": {"#": "minecraft:cobblestone"},
                    "result": {"id": "minecraft:furnace"},
                }),
            ),
            (
                "minecraft:table",
                serde_json::json!({
                    "type": "minecraft:crafting_shapeless",
                    "ingredients": ["#minecraft:planks", "minecraft:cobblestone"],
                    "result": {"id": "minecraft:crafting_table", "count": 2},
                }),
            ),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect();
        OpenBook::from_recipes(&recipes)
    }

    fn stack(item: i32, count: i32) -> SlotDisplay {
        SlotDisplay::ItemStack(Stack {
            item,
            count,
            added: vec![],
            removed: vec![],
        })
    }

    #[test]
    fn displays_match_the_recipe_that_draws_the_same() {
        let book = book();
        assert_eq!(book.len(), 2);
        let c = || SlotDisplay::Item(1);
        let furnace = RecipeDisplay::Shaped {
            width: 3,
            height: 3,
            ingredients: vec![c(), c(), c(), c(), SlotDisplay::Empty, c(), c(), c(), c()],
            result: stack(2, 1),
            station: SlotDisplay::Item(3),
        };
        assert_eq!(book.display(&furnace, &names), Some(0));
        // Shapeless cells in another order, the tag as the server sends it.
        let table = RecipeDisplay::Shapeless {
            ingredients: vec![
                SlotDisplay::Composite(vec![SlotDisplay::Item(1)]),
                SlotDisplay::Tag(HolderSet::Tag("minecraft:planks".into())),
            ],
            result: stack(3, 2),
            station: SlotDisplay::Item(3),
        };
        assert_eq!(book.display(&table, &names), Some(1));
        // Another count or another ingredient is another recipe.
        let more = RecipeDisplay::Shapeless {
            ingredients: vec![
                SlotDisplay::Item(1),
                SlotDisplay::Tag(HolderSet::Tag("minecraft:planks".into())),
            ],
            result: stack(3, 4),
            station: SlotDisplay::Item(3),
        };
        assert_eq!(book.display(&more, &names), None);
    }
}
