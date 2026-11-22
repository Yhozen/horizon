//! GLSL atomics (`atomicAdd(buf.count, 1u)`) for naga, whose GLSL front end
//! has none: the host renames each call to an `hz_atomic*` stub that takes the
//! variable by value ([`ATOMIC_STUBS`]), and [`lower`] turns every call of a
//! stub into a naga atomic statement on the loaded pointer, then retypes the
//! member it points to as `atomic<u32>`/`atomic<i32>`. Plain reads and writes
//! of that member stay valid: naga loads and stores an atomic as
//! `atomicLoad`/`atomicStore`.

use naga::{
    AtomicFunction, Block, Expression, Function, Handle, Module, ScalarKind, Statement, Type,
    TypeInner,
};

/// The GLSL atomic functions on buffer and `shared` variables Horizon lowers.
pub const ATOMIC_FUNCTIONS: [&str; 7] = [
    "atomicAdd",
    "atomicMin",
    "atomicMax",
    "atomicAnd",
    "atomicOr",
    "atomicXor",
    "atomicExchange",
];

/// The stubs the host declares for [`ATOMIC_FUNCTIONS`] (renamed `hz_` +
/// name), for `uint` and `int`; [`lower`] replaces their calls.
pub fn atomic_stubs() -> String {
    let mut out = String::new();
    for f in ATOMIC_FUNCTIONS {
        for ty in ["uint", "int"] {
            out.push_str(&format!("{ty} hz_{f}({ty} m, {ty} v) {{ return m; }}\n"));
        }
    }
    out
}

/// `text` with every call of a GLSL atomic function renamed to its stub, and
/// whether there was any. `atomicCompSwap` is refused (no corpus pack uses it).
pub fn rename_calls(text: &str) -> Result<(String, bool), String> {
    let bytes = text.as_bytes();
    let ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut out = String::with_capacity(text.len());
    let mut any = false;
    let mut i = 0;
    while i < bytes.len() {
        if (i == 0 || !ident(bytes[i - 1])) && bytes[i] == b'a' {
            let end = text[i..]
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .map_or(text.len(), |e| i + e);
            let word = &text[i..end];
            if word == "atomicCompSwap" {
                return Err("atomicCompSwap is not supported".into());
            }
            if ATOMIC_FUNCTIONS.contains(&word) && text[end..].trim_start().starts_with('(') {
                out.push_str("hz_");
                any = true;
            }
            out.push_str(word);
            i = end;
            continue;
        }
        let c = text[i..].chars().next().unwrap();
        out.push(c);
        i += c.len_utf8();
    }
    Ok((out, any))
}

/// One step from a global variable to the atomic member.
#[derive(Clone, Copy)]
enum Step {
    Member(u32),
    Element,
}

/// Replaces the calls of the `hz_atomic*` stubs in every function and entry
/// point by atomic statements, and retypes what they point to.
pub fn lower(module: &mut Module) -> Result<(), String> {
    let stubs: Vec<(Handle<Function>, AtomicFunction, Option<Handle<Type>>)> = module
        .functions
        .iter()
        .filter_map(|(h, f)| {
            let name = f.name.as_deref()?.strip_prefix("hz_")?;
            let fun = match name {
                "atomicAdd" => AtomicFunction::Add,
                "atomicMin" => AtomicFunction::Min,
                "atomicMax" => AtomicFunction::Max,
                "atomicAnd" => AtomicFunction::And,
                "atomicOr" => AtomicFunction::InclusiveOr,
                "atomicXor" => AtomicFunction::ExclusiveOr,
                "atomicExchange" => AtomicFunction::Exchange { compare: None },
                _ => return None,
            };
            Some((h, fun, f.result.as_ref().map(|r| r.ty)))
        })
        .collect();
    // Storage globals: `counts.total` loads the whole block in naga's
    // GLSL front end, which cannot be loaded when it ends in an unsized
    // array (and must not be once a member is atomic).
    let storage: Vec<Handle<naga::GlobalVariable>> = module
        .global_variables
        .iter()
        .filter(|(_, g)| matches!(g.space, naga::AddressSpace::Storage { .. }))
        .map(|(h, _)| h)
        .collect();
    if stubs.is_empty() {
        for (_, f) in module.functions.iter_mut() {
            sink_loads(f, &storage);
        }
        for ep in &mut module.entry_points {
            sink_loads(&mut ep.function, &storage);
        }
        return Ok(());
    }
    // The globals the atomics are on: their member reads must load the
    // member, not the whole variable (which would then hold an atomic).
    let mut roots = Vec::new();
    let mut find_roots = |f: &Function| {
        each_call(&f.body, &mut |function, arguments| {
            if stubs.iter().any(|(h, _, _)| *h == function)
                && let Some(g) = value_root(f, arguments[0])
                && !roots.contains(&g)
            {
                roots.push(g);
            }
        });
    };
    for (_, f) in module.functions.iter() {
        find_roots(f);
    }
    for ep in &module.entry_points {
        find_roots(&ep.function);
    }
    roots.extend(storage);
    let mut paths: Vec<(Handle<naga::GlobalVariable>, Vec<Step>)> = Vec::new();
    let mut lower_function = |f: &mut Function| -> Result<(), String> {
        sink_loads(f, &roots);
        let mut body = std::mem::take(&mut f.body);
        let r = lower_block(&mut body, f, &stubs, &mut paths);
        f.body = body;
        r
    };
    for (_, f) in module.functions.iter_mut() {
        lower_function(f)?;
    }
    for ep in &mut module.entry_points {
        lower_function(&mut ep.function)?;
    }
    for (global, path) in paths {
        let ty = module.global_variables[global].ty;
        let new = retype(&mut module.types, ty, &path)?;
        module.global_variables[global].ty = new;
    }
    Ok(())
}

type Stub = (Handle<Function>, AtomicFunction, Option<Handle<Type>>);

fn lower_block(
    block: &mut Block,
    f: &mut Function,
    stubs: &[Stub],
    paths: &mut Vec<(Handle<naga::GlobalVariable>, Vec<Step>)>,
) -> Result<(), String> {
    let old = std::mem::take(block);
    for (mut statement, span) in old.span_into_iter() {
        match &mut statement {
            Statement::Block(b) => lower_block(b, f, stubs, paths)?,
            Statement::If { accept, reject, .. } => {
                lower_block(accept, f, stubs, paths)?;
                lower_block(reject, f, stubs, paths)?;
            }
            Statement::Loop {
                body, continuing, ..
            } => {
                lower_block(body, f, stubs, paths)?;
                lower_block(continuing, f, stubs, paths)?;
            }
            Statement::Switch { cases, .. } => {
                for c in cases {
                    lower_block(&mut c.body, f, stubs, paths)?;
                }
            }
            Statement::Call {
                function,
                arguments,
                result,
            } => {
                if let Some(&(_, fun, ty)) = stubs.iter().find(|(h, _, _)| h == function) {
                    let before = f.expressions.len();
                    let pointer = to_pointer(f, arguments[0])?;
                    let after = f.expressions.len();
                    if after > before {
                        let first = f.expressions.iter().nth(before).map(|(h, _)| h);
                        let last = f.expressions.iter().nth(after - 1).map(|(h, _)| h);
                        if let (Some(first), Some(last)) = (first, last) {
                            block.push(
                                Statement::Emit(naga::Range::new_from_bounds(first, last)),
                                span,
                            );
                        }
                    }
                    paths.push(pointer_path(f, pointer)?);
                    let value = arguments[1];
                    let result = *result;
                    if let Some(r) = result {
                        f.expressions[r] = Expression::AtomicResult {
                            ty: ty.ok_or("an atomic stub without a result")?,
                            comparison: false,
                        };
                    }
                    statement = Statement::Atomic {
                        pointer,
                        fun,
                        value,
                        result,
                    };
                }
            }
            _ => {}
        }
        block.push(statement, span);
    }
    Ok(())
}

/// Calls `visit` with every call statement's function and arguments.
fn each_call(block: &Block, visit: &mut dyn FnMut(Handle<Function>, &[Handle<Expression>])) {
    for statement in block.iter() {
        match statement {
            Statement::Block(b) => each_call(b, visit),
            Statement::If { accept, reject, .. } => {
                each_call(accept, visit);
                each_call(reject, visit);
            }
            Statement::Loop {
                body, continuing, ..
            } => {
                each_call(body, visit);
                each_call(continuing, visit);
            }
            Statement::Switch { cases, .. } => {
                for c in cases {
                    each_call(&c.body, visit);
                }
            }
            Statement::Call {
                function,
                arguments,
                ..
            } => visit(*function, arguments),
            _ => {}
        }
    }
}

/// The global variable a pointer expression points into.
fn pointer_root(f: &Function, mut e: Handle<Expression>) -> Option<Handle<naga::GlobalVariable>> {
    loop {
        match f.expressions[e] {
            Expression::GlobalVariable(g) => return Some(g),
            Expression::AccessIndex { base, .. } | Expression::Access { base, .. } => e = base,
            _ => return None,
        }
    }
}

/// The global variable a value was loaded from (through member and element accesses).
fn value_root(f: &Function, mut e: Handle<Expression>) -> Option<Handle<naga::GlobalVariable>> {
    loop {
        match f.expressions[e] {
            Expression::Load { pointer } => return pointer_root(f, pointer),
            Expression::AccessIndex { base, .. } | Expression::Access { base, .. } => e = base,
            _ => return None,
        }
    }
}

/// `load(p).m` → `load(p.m)` (and `load(p)[i]` → `load(p[i])`) for loads
/// from `roots` used only by that access: the two expressions swap roles in
/// place, so they stay emitted in order. Repeated until nothing changes, so
/// `buf.data[i].x` loads only the component.
fn sink_loads(f: &mut Function, roots: &[Handle<naga::GlobalVariable>]) {
    let refs = |f: &Function, h: Handle<Expression>| {
        let needle = format!("[{}]", h.index());
        f.expressions
            .iter()
            .map(|(_, e)| format!("{e:?}").matches(&needle).count())
            .sum::<usize>()
    };
    loop {
        let mut changed = false;
        let handles: Vec<Handle<Expression>> = f.expressions.iter().map(|(h, _)| h).collect();
        for e in handles {
            let (load, index) = match f.expressions[e] {
                Expression::AccessIndex { base, index } => (base, Err(index)),
                Expression::Access { base, index } => (base, Ok(index)),
                _ => continue,
            };
            let Expression::Load { pointer } = f.expressions[load] else {
                continue;
            };
            if !pointer_root(f, pointer).is_some_and(|g| roots.contains(&g)) || refs(f, load) != 1 {
                continue;
            }
            f.expressions[load] = match index {
                Err(index) => Expression::AccessIndex {
                    base: pointer,
                    index,
                },
                Ok(index) => Expression::Access {
                    base: pointer,
                    index,
                },
            };
            f.expressions[e] = Expression::Load { pointer: load };
            changed = true;
        }
        if !changed {
            break;
        }
    }
}

/// The pointer the value `e` was loaded through: the loaded pointer itself,
/// or new member and element accesses on it (appended, to be emitted).
fn to_pointer(f: &mut Function, e: Handle<Expression>) -> Result<Handle<Expression>, String> {
    let span = f.expressions.get_span(e);
    match f.expressions[e] {
        Expression::Load { pointer } => Ok(pointer),
        Expression::AccessIndex { base, index } => {
            let base = to_pointer(f, base)?;
            Ok(f.expressions
                .append(Expression::AccessIndex { base, index }, span))
        }
        Expression::Access { base, index } => {
            let base = to_pointer(f, base)?;
            Ok(f.expressions
                .append(Expression::Access { base, index }, span))
        }
        _ => Err("an atomic function on something that is not a buffer or shared variable".into()),
    }
}

/// The global variable `pointer` points into, and the path to the member.
fn pointer_path(
    f: &Function,
    mut pointer: Handle<Expression>,
) -> Result<(Handle<naga::GlobalVariable>, Vec<Step>), String> {
    let mut path = Vec::new();
    loop {
        match f.expressions[pointer] {
            Expression::GlobalVariable(g) => {
                path.reverse();
                return Ok((g, path));
            }
            Expression::AccessIndex { base, index } => {
                path.push(Step::Member(index));
                pointer = base;
            }
            Expression::Access { base, .. } => {
                path.push(Step::Element);
                pointer = base;
            }
            _ => {
                return Err(
                    "an atomic function on a local variable or a parameter (only buffer and shared variables can be atomic)"
                        .into(),
                );
            }
        }
    }
}

/// `ty` with the member at `path` made atomic.
fn retype(
    types: &mut naga::UniqueArena<Type>,
    ty: Handle<Type>,
    path: &[Step],
) -> Result<Handle<Type>, String> {
    let t = types[ty].clone();
    let inner = match (&t.inner, path.first()) {
        (TypeInner::Atomic(_), None) => return Ok(ty),
        (TypeInner::Scalar(s), None) if s.width == 4 && s.kind != ScalarKind::Float => {
            if s.kind == ScalarKind::Bool {
                return Err("an atomic function on a bool".into());
            }
            TypeInner::Atomic(*s)
        }
        (TypeInner::Scalar(_), None) => {
            return Err(
                "an atomic function on a float (WebGPU has atomics on int and uint)".into(),
            );
        }
        (TypeInner::Struct { members, span }, Some(Step::Member(i))) => {
            let mut members = members.clone();
            let m = members
                .get_mut(*i as usize)
                .ok_or("an atomic on a missing member")?;
            m.ty = retype(types, m.ty, &path[1..])?;
            TypeInner::Struct {
                members,
                span: *span,
            }
        }
        (TypeInner::Array { base, size, stride }, Some(_)) => TypeInner::Array {
            base: retype(types, *base, &path[1..])?,
            size: *size,
            stride: *stride,
        },
        _ => return Err("an atomic function on a vector or matrix component".into()),
    };
    Ok(types.insert(
        Type {
            name: t.name.clone(),
            inner,
        },
        naga::Span::UNDEFINED,
    ))
}

/// GLSL `barrier()` comes out of naga's front end with every flag, the
/// subgroup and texture ones included, which WebGPU lacks: keep the storage
/// and workgroup ones (`storageBarrier(); workgroupBarrier();`).
pub fn plain_barriers(module: &mut Module) {
    fn walk(block: &mut Block) {
        for statement in block.iter_mut() {
            match statement {
                Statement::Block(b) => walk(b),
                Statement::If { accept, reject, .. } => {
                    walk(accept);
                    walk(reject);
                }
                Statement::Loop {
                    body, continuing, ..
                } => {
                    walk(body);
                    walk(continuing);
                }
                Statement::Switch { cases, .. } => cases.iter_mut().for_each(|c| walk(&mut c.body)),
                Statement::ControlBarrier(b) | Statement::MemoryBarrier(b) => {
                    *b &= naga::Barrier::STORAGE | naga::Barrier::WORK_GROUP;
                }
                _ => {}
            }
        }
    }
    for (_, f) in module.functions.iter_mut() {
        walk(&mut f.body);
    }
    for ep in &mut module.entry_points {
        walk(&mut ep.function.body);
    }
}

/// What a compute program asks of the device, read from its WGSL: the
/// workgroup size and the bytes of workgroup (`shared`) memory, each
/// variable rounded up to 16 bytes as WebGPU counts them.
pub fn compute_facts(wgsl: &str) -> Result<([u32; 3], u32), String> {
    let module = naga::front::wgsl::parse_str(wgsl).map_err(|e| e.emit_to_string(wgsl))?;
    let ep = module
        .entry_points
        .iter()
        .find(|e| e.stage == naga::ShaderStage::Compute)
        .ok_or("no compute entry point")?;
    let mut layouter = naga::proc::Layouter::default();
    layouter
        .update(module.to_ctx())
        .map_err(|e| e.to_string())?;
    let bytes = module
        .global_variables
        .iter()
        .filter(|(_, g)| g.space == naga::AddressSpace::WorkGroup)
        .map(|(_, g)| layouter[g.ty].size.div_ceil(16) * 16)
        .sum();
    Ok((ep.workgroup_size, bytes))
}
