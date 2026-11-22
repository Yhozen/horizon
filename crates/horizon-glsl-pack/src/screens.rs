//! The model behind a pack's settings screen: its options (from
//! [`crate::options::scan`]), the screens, sliders and profiles of
//! `shaders.properties`, the labels of `shaders/lang/en_us.lang`, and the
//! option file (`<pack>.txt`, `NAME=VALUE` lines) the values live in.
//!
//! Option values are text: `true`/`false` for booleans, the value as written
//! in the source for the rest ([`Values`]).
//!
//! `shaders.properties` keys read here (OptiFine's documented format):
//! - `screen=` the main screen, `screen.<NAME>=` a sub-screen; entries are an
//!   option name, `[NAME]` (a link to sub-screen NAME), `<profile>` (the
//!   profile button), `<empty>` (a blank slot) and `*` (every option on no
//!   screen). `screen.columns=` / `screen.<NAME>.columns=` (default 2).
//! - `sliders=` the options shown as sliders.
//! - `profile.<NAME>=` entries `OPT` (on), `!OPT` (off), `OPT:v` / `OPT=v`,
//!   `profile.<OTHER>` (copy that profile's settings, in place) and
//!   `!program.<name>` (the profile turns that program off).

use std::collections::{BTreeMap, BTreeSet};

use crate::options::{self, OptionDef, OptionKind};
use crate::{PackFiles, PackOptions, preprocess, shaders_properties};

/// Option values by name.
pub type Values = BTreeMap<String, String>;

/// One option as the screen shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionInfo {
    pub name: String,
    pub kind: OptionKind,
    /// `true`/`false` for a boolean.
    pub default: String,
    /// The values a button cycles through (`false`, `true` for a boolean);
    /// the default is always one of them.
    pub values: Vec<String>,
    /// Listed in `sliders=`.
    pub slider: bool,
    /// The values are `true`/`false`.
    pub boolean: bool,
}

/// One slot of a screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    Option(String),
    /// A link to the sub-screen of this name (`[NAME]`).
    Screen(String),
    /// The profile button (`<profile>`).
    Profile,
    /// A blank slot (`<empty>`).
    Empty,
    /// `*`: every option no screen lists (see [`OptionModel::entries`]).
    Rest,
}

/// A screen: its entries, in order, and how many columns they fill.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Screen {
    pub entries: Vec<Entry>,
    pub columns: u32,
}

/// A profile, resolved: the values it sets and the programs it toggles.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    /// Only the options the profile names.
    pub values: Values,
    /// `program.<name>` → enabled (`!program.x` gives `x → false`).
    pub programs: BTreeMap<String, bool>,
}

/// The `en_us.lang` keys of a pack (`option.X`, `value.X.v`, …), `§` codes
/// kept.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lang(pub BTreeMap<String, String>);

impl Lang {
    /// Reads `key=value` lines (`#` comments ignored; as in Java properties,
    /// spaces after `=` are dropped, trailing ones kept), UTF-8 or else
    /// Latin-1. The last definition of a key wins, as in Java.
    pub fn parse(bytes: &[u8]) -> Self {
        let text = match std::str::from_utf8(bytes) {
            Ok(t) => t.to_string(),
            Err(_) => bytes.iter().map(|&b| char::from(b)).collect(),
        };
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
        let mut map = BTreeMap::new();
        for line in text.lines() {
            let line = line.trim_start();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                map.insert(k.trim().to_string(), v.trim_start().to_string());
            }
        }
        Self(map)
    }

    /// The text of `key`, if the pack has it.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    /// `option.X`, else the name.
    pub fn option(&self, name: &str) -> String {
        self.or_raw(&format!("option.{name}"), name)
    }

    /// `option.X.comment` (the tooltip).
    pub fn option_comment(&self, name: &str) -> Option<&str> {
        self.get(&format!("option.{name}.comment"))
    }

    /// `value.X.v`, else the value with `prefix.X` and `suffix.X` around it
    /// (booleans: `ON` / `OFF`).
    pub fn value(&self, option: &OptionInfo, value: &str) -> String {
        if let Some(label) = self.get(&format!("value.{}.{value}", option.name)) {
            return label.to_string();
        }
        if option.boolean {
            return if value == "true" { "ON" } else { "OFF" }.into();
        }
        let prefix = self.get(&format!("prefix.{}", option.name)).unwrap_or("");
        let suffix = self.get(&format!("suffix.{}", option.name)).unwrap_or("");
        format!("{prefix}{value}{suffix}")
    }

    /// `screen.X`, else the name.
    pub fn screen(&self, name: &str) -> String {
        self.or_raw(&format!("screen.{name}"), name)
    }

    /// `screen.X.comment`.
    pub fn screen_comment(&self, name: &str) -> Option<&str> {
        self.get(&format!("screen.{name}.comment"))
    }

    /// `profile.X`, else the name.
    pub fn profile(&self, name: &str) -> String {
        self.or_raw(&format!("profile.{name}"), name)
    }

    /// The profile button's tooltip: `profile.comments` (or `profile.comment`).
    pub fn profile_comment(&self) -> Option<&str> {
        self.get("profile.comments")
            .or_else(|| self.get("profile.comment"))
    }

    fn or_raw(&self, key: &str, raw: &str) -> String {
        self.get(key).unwrap_or(raw).to_string()
    }
}

/// Everything the settings screen needs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OptionModel {
    /// The options in the order they were found.
    pub options: Vec<OptionInfo>,
    /// `""` is the main screen; a pack without `screen=` gets
    /// `[<profile> if it has profiles, *]`. Sub-screens a link names but no
    /// key defines are empty.
    pub screens: BTreeMap<String, Screen>,
    /// In file order.
    pub profiles: Vec<Profile>,
    pub lang: Lang,
    /// For the report: conflicting defaults, unknown names on screens,
    /// profile cycles, a `shaders.properties` that failed to preprocess.
    pub notes: Vec<String>,
}

/// The model of a pack at its default values.
pub fn option_model(files: &PackFiles) -> OptionModel {
    option_model_with(files, &Values::new())
}

/// The model of a pack whose `shaders.properties` is read with `values` (it
/// may hold `#if` on options); options `values` does not name take their
/// defaults.
pub fn option_model_with(files: &PackFiles, values: &Values) -> OptionModel {
    let scan = options::scan(files);
    let mut model = OptionModel {
        notes: scan.notes.clone(),
        lang: read_lang(files),
        ..OptionModel::default()
    };
    let mut macros = preprocess::predefined(PackOptions::default().mc_version);
    for def in &scan.options {
        let value = values.get(&def.name).unwrap_or(&def.default);
        match def.kind {
            OptionKind::Bool if value == "true" => {
                macros.insert(def.name.clone(), String::new());
            }
            OptionKind::Value => {
                macros.insert(def.name.clone(), value.clone());
            }
            _ => {}
        }
    }
    let entries = match shaders_properties::shaders_properties(files, &macros) {
        Ok(p) => p.entries,
        Err(e) => {
            model
                .notes
                .push(format!("shaders.properties: {}", e.message));
            Vec::new()
        }
    };
    let words = |v: &str| v.split_whitespace().map(str::to_string).collect::<Vec<_>>();

    let mut sliders = BTreeSet::new();
    let mut screen_lists: Vec<(String, Vec<String>)> = Vec::new();
    let mut columns: BTreeMap<String, u32> = BTreeMap::new();
    let mut raw_profiles: Vec<(String, Vec<String>)> = Vec::new();
    for (key, value) in &entries {
        if key == "sliders" {
            sliders.extend(words(value));
        } else if key == "screen" {
            screen_lists.push((String::new(), words(value)));
        } else if key == "screen.columns" {
            columns.insert(String::new(), value.trim().parse().unwrap_or(2));
        } else if let Some(name) = key.strip_prefix("screen.") {
            match name.strip_suffix(".columns") {
                Some(screen) => {
                    columns.insert(screen.to_string(), value.trim().parse().unwrap_or(2));
                }
                None => screen_lists.push((name.to_string(), words(value))),
            }
        } else if let Some(name) = key.strip_prefix("profile.") {
            raw_profiles.push((name.to_string(), words(value)));
        }
    }

    // Options: the scan's, plus candidates a screen or slider names.
    let listed: BTreeSet<&str> = screen_lists
        .iter()
        .flat_map(|(_, l)| l.iter().map(String::as_str))
        .chain(sliders.iter().map(String::as_str))
        .collect();
    let mut defs: Vec<&OptionDef> = scan.options.iter().collect();
    defs.extend(
        scan.candidates
            .iter()
            .filter(|c| listed.contains(c.name.as_str())),
    );
    for def in defs {
        let mut values = if def.boolean && def.allowed.is_empty() {
            vec!["false".to_string(), "true".to_string()]
        } else {
            def.allowed.clone()
        };
        if !values.contains(&def.default) {
            values.insert(0, def.default.clone());
        }
        model.options.push(OptionInfo {
            name: def.name.clone(),
            kind: def.kind,
            default: def.default.clone(),
            values,
            slider: sliders.contains(&def.name),
            boolean: def.boolean,
        });
    }

    // Screens.
    for (name, list) in screen_lists {
        let mut out = Vec::new();
        for item in list {
            let entry = match item.as_str() {
                "<profile>" => Entry::Profile,
                "<empty>" => Entry::Empty,
                "*" => Entry::Rest,
                _ => match item.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                    Some(sub) => Entry::Screen(sub.to_string()),
                    None if model.option(&item).is_some() => Entry::Option(item),
                    None => {
                        model.notes.push(format!(
                            "screen {}: {item} is not an option",
                            screen_key(&name)
                        ));
                        continue;
                    }
                },
            };
            out.push(entry);
        }
        model.screens.insert(
            name.clone(),
            Screen {
                entries: out,
                columns: 2,
            },
        );
    }
    if !model.screens.contains_key("") {
        let mut entries = Vec::new();
        if !raw_profiles.is_empty() {
            entries.push(Entry::Profile);
        }
        entries.push(Entry::Rest);
        model.screens.insert(
            String::new(),
            Screen {
                entries,
                columns: 2,
            },
        );
    }
    let links: Vec<String> = model
        .screens
        .values()
        .flat_map(|s| &s.entries)
        .filter_map(|e| match e {
            Entry::Screen(n) => Some(n.clone()),
            _ => None,
        })
        .collect();
    for link in links {
        model.screens.entry(link).or_insert(Screen {
            entries: Vec::new(),
            columns: 2,
        });
    }
    for (name, n) in columns {
        if let Some(s) = model.screens.get_mut(&name) {
            s.columns = n.max(1);
        }
    }

    // Profiles.
    let raw: BTreeMap<&str, &[String]> = raw_profiles
        .iter()
        .map(|(n, l)| (n.as_str(), l.as_slice()))
        .collect();
    for (name, _) in &raw_profiles {
        let mut profile = Profile {
            name: name.clone(),
            ..Profile::default()
        };
        let mut stack = vec![name.as_str()];
        resolve_profile(&raw, name, &mut stack, &mut profile, &mut model);
        model.profiles.push(profile);
    }
    model
}

fn screen_key(name: &str) -> String {
    if name.is_empty() {
        "screen".into()
    } else {
        format!("screen.{name}")
    }
}

/// Applies the entries of `profile.<name>` to `out`, in order.
fn resolve_profile<'a>(
    raw: &BTreeMap<&'a str, &'a [String]>,
    name: &str,
    stack: &mut Vec<&'a str>,
    out: &mut Profile,
    model: &mut OptionModel,
) {
    let Some(list) = raw.get(name) else {
        model
            .notes
            .push(format!("profile.{}: no profile {name}", out.name));
        return;
    };
    for item in list.iter() {
        if let Some(other) = item.strip_prefix("profile.") {
            match raw.get_key_value(other) {
                Some((&key, _)) if !stack.contains(&key) => {
                    stack.push(key);
                    resolve_profile(raw, key, stack, out, model);
                    stack.pop();
                }
                Some(_) => model
                    .notes
                    .push(format!("profile.{}: profile.{other} is a cycle", out.name)),
                None => model
                    .notes
                    .push(format!("profile.{}: no profile {other}", out.name)),
            }
            continue;
        }
        let (negate, item) = match item.strip_prefix('!') {
            Some(rest) => (true, rest),
            None => (false, item.as_str()),
        };
        if let Some(program) = item.strip_prefix("program.") {
            out.programs.insert(program.to_string(), !negate);
            continue;
        }
        let (option, value) = match item.split_once([':', '=']) {
            Some((o, v)) => (o, v.to_string()),
            None => (item, (!negate).to_string()),
        };
        if model.option(option).is_none() {
            model
                .notes
                .push(format!("profile.{}: {option} is not an option", out.name));
            continue;
        }
        out.values.insert(option.to_string(), value);
    }
}

/// `shaders/lang/en_us.lang`, the file name matched without case.
fn read_lang(files: &PackFiles) -> Lang {
    files
        .0
        .iter()
        .find(|(p, _)| p.eq_ignore_ascii_case("shaders/lang/en_us.lang"))
        .map(|(_, b)| Lang::parse(b))
        .unwrap_or_default()
}

impl OptionModel {
    /// The option called `name`.
    pub fn option(&self, name: &str) -> Option<&OptionInfo> {
        self.options.iter().find(|o| o.name == name)
    }

    /// The profile called `name`.
    pub fn profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.name == name)
    }

    /// Every option at its default.
    pub fn defaults(&self) -> Values {
        self.options
            .iter()
            .map(|o| (o.name.clone(), o.default.clone()))
            .collect()
    }

    /// The defaults with `pairs` (an option file's lines) applied: names
    /// that are not options are dropped, booleans read `true`/`false` (also
    /// `on`/`off`).
    pub fn values_from(&self, pairs: &[(String, String)]) -> Values {
        let mut values = self.defaults();
        for (name, value) in pairs {
            let Some(option) = self.option(name) else {
                continue;
            };
            let value = if option.boolean {
                match value.trim() {
                    "true" | "on" | "ON" => "true",
                    _ => "false",
                }
                .to_string()
            } else {
                value.trim().to_string()
            };
            values.insert(name.clone(), value);
        }
        values
    }

    /// The `(name, value)` pairs of `values` that differ from the defaults,
    /// by name: what [`write_option_file`] saves.
    pub fn changed(&self, values: &Values) -> Vec<(String, String)> {
        self.options
            .iter()
            .filter_map(|o| {
                let v = values.get(&o.name)?;
                (*v != o.default).then(|| (o.name.clone(), v.clone()))
            })
            .collect::<BTreeMap<_, _>>()
            .into_iter()
            .collect()
    }

    /// The entries of a screen with `*` expanded to the options no screen
    /// lists, in the order they were found. Empty for an unknown screen.
    pub fn entries(&self, screen: &str) -> Vec<Entry> {
        let Some(s) = self.screens.get(screen) else {
            return Vec::new();
        };
        let placed: BTreeSet<&str> = self
            .screens
            .values()
            .flat_map(|s| &s.entries)
            .filter_map(|e| match e {
                Entry::Option(n) => Some(n.as_str()),
                _ => None,
            })
            .collect();
        let mut out = Vec::new();
        for entry in &s.entries {
            match entry {
                Entry::Rest => out.extend(
                    self.options
                        .iter()
                        .filter(|o| !placed.contains(o.name.as_str()))
                        .map(|o| Entry::Option(o.name.clone())),
                ),
                e => out.push(e.clone()),
            }
        }
        out
    }

    /// The first profile (in file order) whose every setting equals
    /// `values` (options `values` lacks read their defaults); `None` is
    /// "Custom".
    pub fn current_profile(&self, values: &Values) -> Option<&str> {
        self.profiles
            .iter()
            .find(|p| {
                p.values.iter().all(|(name, v)| {
                    let current = values
                        .get(name)
                        .or_else(|| self.option(name).map(|o| &o.default));
                    current == Some(v)
                })
            })
            .map(|p| p.name.as_str())
    }

    /// The next value of `name` after `current` (wrapping), for a button.
    pub fn next_value(&self, name: &str, current: &str, back: bool) -> Option<String> {
        let o = self.option(name)?;
        let n = o.values.len();
        let i = o.values.iter().position(|v| v == current).unwrap_or(0);
        let j = if back { (i + n - 1) % n } else { (i + 1) % n };
        o.values.get(j).cloned()
    }
}

/// `values` with profile `name`'s settings applied (others unchanged);
/// `values` itself for an unknown profile.
pub fn apply_profile(model: &OptionModel, name: &str, values: &Values) -> Values {
    let mut out = values.clone();
    if let Some(p) = model.profile(name) {
        out.extend(p.values.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
    out
}

/// The lines of an option file (`NAME=VALUE`; blank lines and `#` comments
/// skipped), in order.
pub fn parse_option_file(text: &str) -> Vec<(String, String)> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let (k, v) = l.split_once('=')?;
            Some((k.trim().to_string(), v.trim().to_string()))
        })
        .collect()
}

/// An option file of `values` (pass [`OptionModel::changed`] to write only
/// the non-default values).
pub fn write_option_file(values: &[(String, String)]) -> String {
    values.iter().map(|(k, v)| format!("{k}={v}\n")).collect()
}

/// [`crate::PackOptions::overrides`] for `values`: one entry per option whose
/// value differs from its default (booleans as `Some("true")` /
/// `Some("false")`, which [`options::apply`] uncomments or comments out).
pub fn overrides(model: &OptionModel, values: &Values) -> Vec<(String, Option<String>)> {
    model
        .changed(values)
        .into_iter()
        .map(|(k, v)| (k, Some(v)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(files: &[(&str, &str)]) -> PackFiles {
        PackFiles(
            files
                .iter()
                .map(|(p, t)| (p.to_string(), t.as_bytes().to_vec()))
                .collect(),
        )
    }

    fn vals(pairs: &[(&str, &str)]) -> Values {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    const SETTINGS: &str = "#define SHADOWS\n//#define BLOOM\n#define QUALITY 2 // [1 2 3]\n\
        #define SPEED 1.0 // speed [0.5 1.0 2.0]\n#define LISTED_ONLY\n#define TINT 4\n\
        #define UNUSED 3\n";
    const PROGRAM: &str = "#include \"/lib/settings.glsl\"\n\
        const int shadowMapResolution = 2048; // [1024 2048 4096]\n\
        const float fogDensity = 0.5; // [0.25 0.5]\n\
        #if defined(SHADOWS) && QUALITY > 1\n#endif\n#if defined BLOOM\n#endif\nvoid main() {}\n";

    fn base(properties: &str, lang: &str) -> PackFiles {
        pack(&[
            ("shaders/lib/settings.glsl", SETTINGS),
            ("shaders/composite.fsh", PROGRAM),
            ("shaders/shaders.properties", properties),
            ("shaders/lang/EN_US.lang", lang),
        ])
    }

    #[test]
    fn screens_columns_rest_and_links() {
        let props = "screen=<profile> <empty> [LIGHT] *\nscreen.columns=1\n\
                     screen.LIGHT=SHADOWS shadowMapResolution [DEEP] LISTED_ONLY NOPE\n\
                     screen.LIGHT.columns=3\nsliders=SPEED QUALITY TINT\n";
        let model = option_model(&base(props, ""));
        let names: Vec<_> = model.options.iter().map(|o| o.name.as_str()).collect();
        // Path order: composite.fsh's consts first, then settings.glsl's
        // defines; the listed-only candidates last.
        assert_eq!(
            names,
            [
                "shadowMapResolution",
                "fogDensity",
                "SHADOWS",
                "BLOOM",
                "QUALITY",
                "SPEED",
                "LISTED_ONLY",
                "TINT"
            ]
        );
        let root = &model.screens[""];
        assert_eq!(root.columns, 1);
        assert_eq!(
            model.entries(""),
            [
                Entry::Profile,
                Entry::Empty,
                Entry::Screen("LIGHT".into()),
                Entry::Option("fogDensity".into()),
                Entry::Option("BLOOM".into()),
                Entry::Option("QUALITY".into()),
                Entry::Option("SPEED".into()),
                Entry::Option("TINT".into()),
            ]
        );
        let light = &model.screens["LIGHT"];
        assert_eq!(light.columns, 3);
        assert_eq!(
            light.entries,
            [
                Entry::Option("SHADOWS".into()),
                Entry::Option("shadowMapResolution".into()),
                Entry::Screen("DEEP".into()),
                Entry::Option("LISTED_ONLY".into()),
            ]
        );
        // A linked screen no key defines is empty, with the default columns.
        assert_eq!(model.screens["DEEP"].entries, []);
        assert_eq!(model.screens["DEEP"].columns, 2);
        assert!(
            model.notes.iter().any(|n| n.contains("NOPE")),
            "{:?}",
            model.notes
        );
        let speed = model.option("SPEED").unwrap();
        assert!(speed.slider && !speed.boolean);
        assert_eq!(speed.values, ["0.5", "1.0", "2.0"]);
        // A slider candidate without a list: its value only.
        assert_eq!(model.option("TINT").unwrap().values, ["4"]);
        assert!(model.option("UNUSED").is_none());
        let bloom = model.option("BLOOM").unwrap();
        assert!(bloom.boolean && bloom.default == "false" && !bloom.slider);
        assert_eq!(bloom.values, ["false", "true"]);
        assert_eq!(
            model.next_value("QUALITY", "3", false).as_deref(),
            Some("1")
        );
        assert_eq!(model.next_value("QUALITY", "1", true).as_deref(), Some("3"));
    }

    #[test]
    fn no_screen_is_a_flat_list() {
        let model = option_model(&base("", ""));
        assert_eq!(model.screens.len(), 1);
        assert_eq!(model.screens[""].entries, [Entry::Rest]);
        assert_eq!(model.entries("").len(), model.options.len());
        let with_profiles = option_model(&base("profile.LOW=!SHADOWS\n", ""));
        assert_eq!(
            with_profiles.screens[""].entries,
            [Entry::Profile, Entry::Rest]
        );
    }

    #[test]
    fn profiles_resolve_references_and_match() {
        let props = "profile.LOW=!SHADOWS QUALITY:1 shadowMapResolution=1024 !program.composite1\n\
                     profile.MEDIUM=profile.LOW QUALITY:2 BLOOM\n\
                     profile.HIGH=SHADOWS QUALITY:3 program.world0/composite1\n\
                     profile.LOOP=profile.LOOP2\nprofile.LOOP2=profile.LOOP SPEED:2.0\n";
        let model = option_model(&base(props, ""));
        let names: Vec<_> = model.profiles.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["LOW", "MEDIUM", "HIGH", "LOOP", "LOOP2"]);
        let low = model.profile("LOW").unwrap();
        assert_eq!(
            low.values,
            vals(&[
                ("SHADOWS", "false"),
                ("QUALITY", "1"),
                ("shadowMapResolution", "1024")
            ])
        );
        assert_eq!(low.programs, BTreeMap::from([("composite1".into(), false)]));
        let medium = model.profile("MEDIUM").unwrap();
        assert_eq!(
            medium.values,
            vals(&[
                ("SHADOWS", "false"),
                ("QUALITY", "2"),
                ("shadowMapResolution", "1024"),
                ("BLOOM", "true")
            ])
        );
        assert_eq!(medium.programs.len(), 1);
        assert_eq!(
            model.profile("HIGH").unwrap().programs,
            BTreeMap::from([("world0/composite1".into(), true)])
        );
        assert_eq!(
            model.profile("LOOP").unwrap().values,
            vals(&[("SPEED", "2.0")])
        );
        assert!(model.notes.iter().any(|n| n.contains("cycle")));

        let defaults = model.defaults();
        // Defaults: SHADOWS on, QUALITY 2: nothing matches → Custom.
        assert_eq!(model.current_profile(&defaults), None);
        let high = apply_profile(&model, "HIGH", &defaults);
        assert_eq!(high["QUALITY"], "3");
        assert_eq!(high["BLOOM"], "false");
        assert_eq!(model.current_profile(&high), Some("HIGH"));
        let medium = apply_profile(&model, "MEDIUM", &high);
        assert_eq!(model.current_profile(&medium), Some("MEDIUM"));
        let low = apply_profile(&model, "LOW", &defaults);
        assert_eq!(model.current_profile(&low), Some("LOW"));
        assert_eq!(apply_profile(&model, "NONE", &low), low);
    }

    #[test]
    fn lang_labels_and_values() {
        let lang = "# comment\noption.SHADOWS=§eShadows\noption.SHADOWS.comment=Sun shadows.\n\
                    value.QUALITY.1=Low\nprefix.SPEED=x\nsuffix.SPEED= %\n\
                    screen.LIGHT=Lighting\nscreen.LIGHT.comment=Light things\nprofile.LOW=Potato\n\
                    profile.comments=Pick one\n";
        let model = option_model(&base("", lang));
        let l = &model.lang;
        assert_eq!(l.option("SHADOWS"), "§eShadows");
        assert_eq!(l.option("BLOOM"), "BLOOM");
        assert_eq!(l.option_comment("SHADOWS"), Some("Sun shadows."));
        assert_eq!(l.option_comment("BLOOM"), None);
        let q = model.option("QUALITY").unwrap();
        assert_eq!(
            (l.value(q, "1"), l.value(q, "2")),
            ("Low".into(), "2".into())
        );
        assert_eq!(l.value(model.option("SPEED").unwrap(), "2.0"), "x2.0%");
        let b = model.option("BLOOM").unwrap();
        assert_eq!(
            (l.value(b, "true"), l.value(b, "false")),
            ("ON".into(), "OFF".into())
        );
        assert_eq!(l.screen("LIGHT"), "Lighting");
        assert_eq!(l.screen("OTHER"), "OTHER");
        assert_eq!(l.screen_comment("LIGHT"), Some("Light things"));
        assert_eq!(l.profile("LOW"), "Potato");
        assert_eq!(l.profile_comment(), Some("Pick one"));
        // Latin-1 bytes that are not UTF-8.
        let latin = Lang::parse(b"option.A=caf\xe9 \xa7a\n");
        assert_eq!(latin.get("option.A"), Some("café §a"));
    }

    #[test]
    fn option_files_round_trip_and_overrides() {
        let model = option_model(&base("", ""));
        let text = "# saved by Iris\nQUALITY=3\n\nBLOOM=true\nSHADOWS=false\nGONE=1\nSPEED = 1.0\n";
        let pairs = parse_option_file(text);
        assert_eq!(pairs.len(), 5);
        let values = model.values_from(&pairs);
        assert!(!values.contains_key("GONE"));
        let changed = model.changed(&values);
        assert_eq!(
            changed,
            [
                ("BLOOM".to_string(), "true".to_string()),
                ("QUALITY".into(), "3".into()),
                ("SHADOWS".into(), "false".into()),
            ]
        );
        let written = write_option_file(&changed);
        assert_eq!(written, "BLOOM=true\nQUALITY=3\nSHADOWS=false\n");
        assert_eq!(model.values_from(&parse_option_file(&written)), values);
        let o = overrides(&model, &values);
        assert_eq!(o.len(), 3);
        let applied = options::apply(&base("", ""), &o);
        let settings = applied.text("shaders/lib/settings.glsl").unwrap();
        assert!(settings.starts_with("//#define SHADOWS\n#define BLOOM\n#define QUALITY 3 //"));
        // The applied pack's defaults are the chosen values.
        let again = option_model(&applied);
        assert_eq!(again.defaults(), values);
    }

    #[test]
    fn properties_if_on_options() {
        let props = "#ifdef BLOOM\nscreen=BLOOM\n#else\nscreen=SHADOWS QUALITY\n#endif\n\
                     #if QUALITY > 2\nsliders=QUALITY\n#endif\n";
        let files = base(props, "");
        let model = option_model(&files);
        assert_eq!(model.screens[""].entries.len(), 2);
        assert!(!model.option("QUALITY").unwrap().slider);
        let model = option_model_with(&files, &vals(&[("BLOOM", "true"), ("QUALITY", "3")]));
        assert_eq!(model.screens[""].entries, [Entry::Option("BLOOM".into())]);
        assert!(model.option("QUALITY").unwrap().slider);
    }
}
