//! GLSL 450 (Vulkan flavour) → validated naga module → WGSL text.

use naga::back::wgsl;
use naga::front::glsl;
use naga::valid::{Capabilities, ValidationFlags, Validator};

/// Which pipeline stage a GLSL source is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Vertex,
    Fragment,
}

/// Parses `source` as one stage, validates it, and writes it as WGSL with the
/// derivative-uniformity diagnostic switched off (packs sample textures in
/// non-uniform control flow everywhere).
pub fn glsl_to_wgsl(source: &str, stage: Stage) -> Result<String, String> {
    let stage = match stage {
        Stage::Vertex => naga::ShaderStage::Vertex,
        Stage::Fragment => naga::ShaderStage::Fragment,
    };
    let (mut module, floated) = parse_floating_ambiguous(source, stage)?;
    let source = floated.as_deref().unwrap_or(source);
    // `vector % scalar`: wrap the scalar in the source and parse again.
    let mut patched = String::new();
    for _ in 0..4 {
        let splats = modulo_splats(&module, if patched.is_empty() { source } else { &patched });
        if splats.is_empty() {
            break;
        }
        patched = wrap_ranges(if patched.is_empty() { source } else { &patched }, splats);
        module = glsl::Frontend::default()
            .parse(&glsl::Options::from(stage), &patched)
            .map_err(|e| e.emit_to_string(&patched))?;
    }
    let source = if patched.is_empty() { source } else { &patched };
    // Float `!=` as `!(a == b)`: true when either side is NaN, as in GLSL.
    let mut unordered = String::new();
    for _ in 0..4 {
        let current = if unordered.is_empty() {
            source
        } else {
            &unordered
        };
        let patches = float_not_equals(&module, current);
        if patches.is_empty() {
            break;
        }
        let next = replace_ranges(current, patches);
        match glsl::Frontend::default().parse(&glsl::Options::from(stage), &next) {
            Ok(m) => {
                module = m;
                unordered = next;
            }
            // A span naga reported wrongly: keep the ordered comparison.
            Err(_) => break,
        }
    }
    let source = if unordered.is_empty() {
        source
    } else {
        &unordered
    };
    spill_pointer_arguments(&mut module);
    end_terminated_cases(&mut module);
    flatten_else_if_chains(&mut module);
    unsigned_literals(&mut module);
    let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
        .validate(&module)
        .map_err(|e| e.emit_to_string(source))?;
    let text = wgsl::write_string(&module, &info, wgsl::WriterFlags::empty())
        .map_err(|e| e.to_string())?;
    // naga writes `f32::MAX` as its shortest round trip, 340282350…0f, which
    // is above the largest f32 as written: Chrome's compiler refuses it
    // ("value cannot be represented as 'f32'"). The exact value is the same float.
    let text = text.replace(
        "340282350000000000000000000000000000000f",
        "340282346638528859811704183484516925440f",
    );
    let out = format!("diagnostic(off, derivative_uniformity);\n{text}");
    // The browser compiles this text again: make sure naga's own WGSL front end takes it.
    let reparsed = naga::front::wgsl::parse_str(&out).map_err(|e| e.emit_to_string(&out))?;
    Validator::new(ValidationFlags::all(), Capabilities::empty())
        .validate(&reparsed)
        .map_err(|e| e.emit_to_string(&out))?;
    Ok(out)
}

/// Passes a temporary instead of a pointer into a struct, array or vector
/// (`f(s.field)` with an `inout` parameter): naga's SPIR-V writer, which
/// wgpu uses on Vulkan, cannot pass such pointers ("Expression is not
/// cached"). The temporary is loaded before the call and stored back after.
fn spill_pointer_arguments(module: &mut naga::Module) {
    use naga::{Expression, Span, Statement, TypeInner};
    // Each function's parameter types that are pointers: the pointee.
    let pointee: Vec<Vec<Option<naga::Handle<naga::Type>>>> = module
        .functions
        .iter()
        .map(|(_, f)| {
            f.arguments
                .iter()
                .map(|a| match module.types[a.ty].inner {
                    TypeInner::Pointer { base, .. } => Some(base),
                    _ => None,
                })
                .collect()
        })
        .collect();
    let handles: Vec<_> = module.functions.iter().map(|(h, _)| h).collect();
    let index = |h: naga::Handle<naga::Function>| handles.iter().position(|&x| x == h);
    fn rewrite(
        block: &mut naga::Block,
        f: &mut naga::Function,
        pointee: &[Vec<Option<naga::Handle<naga::Type>>>],
        index: &dyn Fn(naga::Handle<naga::Function>) -> Option<usize>,
    ) {
        let old = std::mem::take(block);
        for (mut statement, span) in old.span_into_iter() {
            match &mut statement {
                Statement::Block(b) => rewrite(b, f, pointee, index),
                Statement::If { accept, reject, .. } => {
                    rewrite(accept, f, pointee, index);
                    rewrite(reject, f, pointee, index);
                }
                Statement::Loop {
                    body, continuing, ..
                } => {
                    rewrite(body, f, pointee, index);
                    rewrite(continuing, f, pointee, index);
                }
                Statement::Switch { cases, .. } => {
                    for c in cases {
                        rewrite(&mut c.body, f, pointee, index);
                    }
                }
                _ => {}
            }
            let mut after = Vec::new();
            if let Statement::Call {
                function,
                arguments,
                ..
            } = &mut statement
                && let Some(params) = index(*function).map(|i| &pointee[i])
            {
                for (arg, ty) in arguments.iter_mut().zip(params) {
                    let Some(ty) = *ty else { continue };
                    if !matches!(
                        f.expressions[*arg],
                        Expression::Access { .. } | Expression::AccessIndex { .. }
                    ) {
                        continue;
                    }
                    let local = f.local_variables.append(
                        naga::LocalVariable {
                            name: Some("hz_spill".into()),
                            ty,
                            init: None,
                        },
                        Span::UNDEFINED,
                    );
                    let temp = f
                        .expressions
                        .append(Expression::LocalVariable(local), Span::UNDEFINED);
                    let load = f
                        .expressions
                        .append(Expression::Load { pointer: *arg }, Span::UNDEFINED);
                    block.push(
                        Statement::Emit(naga::Range::new_from_bounds(load, load)),
                        Span::UNDEFINED,
                    );
                    block.push(
                        Statement::Store {
                            pointer: temp,
                            value: load,
                        },
                        Span::UNDEFINED,
                    );
                    after.push((*arg, temp));
                    *arg = temp;
                }
            }
            block.push(statement, span);
            for (target, temp) in after {
                let load = f
                    .expressions
                    .append(Expression::Load { pointer: temp }, Span::UNDEFINED);
                block.push(
                    Statement::Emit(naga::Range::new_from_bounds(load, load)),
                    Span::UNDEFINED,
                );
                block.push(
                    Statement::Store {
                        pointer: target,
                        value: load,
                    },
                    Span::UNDEFINED,
                );
            }
        }
    }
    for (_, f) in module.functions.iter_mut() {
        let mut body = std::mem::take(&mut f.body);
        rewrite(&mut body, f, &pointee, &index);
        f.body = body;
    }
    for e in module.entry_points.iter_mut() {
        let f = &mut e.function;
        let mut body = std::mem::take(&mut f.body);
        rewrite(&mut body, f, &pointee, &index);
        f.body = body;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inout_struct_fields_go_through_a_temporary() {
        let source = "#version 450
struct G { vec3 normal; float depth; };
layout(location = 0) out vec4 color;
void fix(inout vec3 n) { n = normalize(n); }
void main() {
    G g = G(vec3(1.0, 2.0, 2.0), 0.5);
    vec3 v[2] = vec3[](vec3(0.0), vec3(3.0));
    fix(g.normal);
    fix(v[1]);
    color = vec4(g.normal + v[1], g.depth);
}
";
        let wgsl = glsl_to_wgsl(source, Stage::Fragment).unwrap();
        assert!(wgsl.contains("hz_spill"), "{wgsl}");
        assert!(!wgsl.contains("(&g.") && !wgsl.contains("(&v["), "{wgsl}");
    }
}

/// Marks `switch` cases that end in `return`, `discard` or `continue` as not
/// falling through: naga's GLSL front end only counts a final `break`, and
/// its WGSL writer refuses fall-through cases with statements.
fn end_terminated_cases(module: &mut naga::Module) {
    use naga::Statement;
    fn walk(block: &mut naga::Block) {
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
                Statement::Switch { cases, .. } => {
                    for c in cases {
                        walk(&mut c.body);
                        if c.fall_through
                            && matches!(
                                c.body.last(),
                                Some(
                                    Statement::Return { .. }
                                        | Statement::Kill
                                        | Statement::Continue
                                        | Statement::Break
                                )
                            )
                        {
                            c.fall_through = false;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    for (_, f) in module.functions.iter_mut() {
        walk(&mut f.body);
    }
    for e in &mut module.entry_points {
        walk(&mut e.function.body);
    }
}

/// Int literals beside a `uint` (`min(n, 32)` with a `uint n`, `n + 1u`'s
/// GLSL 1.x spelling `n + 1`) become `uint` literals: GLSL converts them,
/// naga's GLSL front end leaves an `int` that validation then refuses.
fn unsigned_literals(module: &mut naga::Module) {
    let handles: Vec<_> = module.functions.iter().map(|(h, _)| h).collect();
    for h in handles {
        let mut f = std::mem::take(&mut module.functions[h]);
        unsigned_literals_in(module, &mut f);
        module.functions[h] = f;
    }
    for i in 0..module.entry_points.len() {
        let mut f = std::mem::take(&mut module.entry_points[i].function);
        unsigned_literals_in(module, &mut f);
        module.entry_points[i].function = f;
    }
}

fn unsigned_literals_in(module: &naga::Module, f: &mut naga::Function) {
    use naga::{Expression, Literal, MathFunction as M, ScalarKind, TypeInner};
    let Some(last) = f.expressions.iter().map(|(h, _)| h).next_back() else {
        return;
    };
    let ctx = naga::proc::ResolveContext::with_locals(module, &f.local_variables, &f.arguments);
    let mut types = naga::front::Typifier::new();
    if types.grow(last, &f.expressions, &ctx).is_err() {
        return;
    }
    let is_uint = |h| matches!(types.get(h, &module.types), TypeInner::Scalar(s) | TypeInner::Vector { scalar: s, .. } if s.kind == ScalarKind::Uint);
    let mut fix = Vec::new();
    for (_, e) in f.expressions.iter() {
        let operands: Vec<_> = match *e {
            Expression::Math {
                fun: M::Min | M::Max | M::Clamp,
                arg,
                arg1,
                arg2,
                ..
            } => [Some(arg), arg1, arg2].into_iter().flatten().collect(),
            Expression::Binary { left, right, .. } => vec![left, right],
            _ => continue,
        };
        if !operands.iter().any(|&o| is_uint(o)) {
            continue;
        }
        fix.extend(operands.into_iter().filter(
            |&o| matches!(f.expressions[o], Expression::Literal(Literal::I32(n)) if n >= 0),
        ));
    }
    for h in fix {
        if let Expression::Literal(Literal::I32(n)) = f.expressions[h] {
            f.expressions[h] = Expression::Literal(Literal::U32(n as u32));
        }
    }
}

/// `else if` chains longer than this become flat (see [`flatten_else_if_chains`]).
const LONG_CHAIN: usize = 8;

/// Rewrites long `if … else if … else if …` chains (block light by id: Solas
/// has hundreds) as a flat sequence guarded by a `done` flag: WGSL writes
/// each `else if` as a nested `else { if }`, and Chrome refuses nesting
/// deeper than 127.
///
/// `if (a) A else if (b) B else C` becomes `done = false; if (a) { done =
/// true; A } if (!done) { if (b) { done = true; B } } if (!done) { C }`;
/// each condition is still only evaluated when the ones before it failed.
fn flatten_else_if_chains(module: &mut naga::Module) {
    let ty = module.types.insert(
        naga::Type {
            name: None,
            inner: naga::TypeInner::Scalar(naga::Scalar::BOOL),
        },
        naga::Span::UNDEFINED,
    );
    for (_, f) in module.functions.iter_mut() {
        flatten_function(f, ty);
    }
    for e in &mut module.entry_points {
        flatten_function(&mut e.function, ty);
    }
}

fn flatten_function(f: &mut naga::Function, ty: naga::Handle<naga::Type>) {
    let mut body = std::mem::take(&mut f.body);
    let mut flag = None;
    flatten_block(&mut body, f, ty, &mut flag);
    f.body = body;
}

/// The links of an `else if` chain: (statements before the condition is
/// tested, the condition, its block), and the final `else`.
type Chain = (
    Vec<(naga::Block, naga::Handle<naga::Expression>, naga::Block)>,
    naga::Block,
);

/// Splits `if (c) accept else reject` into its chain when `reject` is only
/// emits and one more `if`.
fn chain(
    condition: naga::Handle<naga::Expression>,
    accept: naga::Block,
    reject: naga::Block,
) -> Chain {
    use naga::Statement;
    let mut links = vec![(naga::Block::new(), condition, accept)];
    let mut rest = reject;
    loop {
        let n = rest.len();
        let is_link = n > 0
            && matches!(rest.last(), Some(Statement::If { .. }))
            && rest
                .iter()
                .take(n - 1)
                .all(|s| matches!(s, Statement::Emit(_)));
        if !is_link {
            return (links, rest);
        }
        let mut pre = naga::Block::new();
        let mut last = None;
        for (s, span) in rest.span_into_iter() {
            match s {
                Statement::If { .. } => last = Some(s),
                _ => pre.push(s, span),
            }
        }
        let Some(Statement::If {
            condition,
            accept,
            reject,
        }) = last
        else {
            unreachable!("checked above");
        };
        links.push((pre, condition, accept));
        rest = reject;
    }
}

fn flatten_block(
    block: &mut naga::Block,
    f: &mut naga::Function,
    ty: naga::Handle<naga::Type>,
    flag: &mut Option<naga::Handle<naga::Expression>>,
) {
    use naga::{Expression, Literal, Span, Statement, UnaryOperator};
    let old = std::mem::take(block);
    for (statement, span) in old.span_into_iter() {
        let statement = match statement {
            Statement::Block(mut b) => {
                flatten_block(&mut b, f, ty, flag);
                Statement::Block(b)
            }
            Statement::Loop {
                mut body,
                mut continuing,
                break_if,
            } => {
                flatten_block(&mut body, f, ty, flag);
                flatten_block(&mut continuing, f, ty, flag);
                Statement::Loop {
                    body,
                    continuing,
                    break_if,
                }
            }
            Statement::Switch {
                selector,
                mut cases,
            } => {
                for c in &mut cases {
                    flatten_block(&mut c.body, f, ty, flag);
                }
                Statement::Switch { selector, cases }
            }
            Statement::If {
                condition,
                accept,
                reject,
            } => {
                let (mut links, mut otherwise) = chain(condition, accept, reject);
                for (_, _, b) in &mut links {
                    flatten_block(b, f, ty, flag);
                }
                flatten_block(&mut otherwise, f, ty, flag);
                if links.len() <= LONG_CHAIN {
                    // Rebuild the chain as it was.
                    let mut tail = otherwise;
                    for (pre, condition, accept) in links.into_iter().rev() {
                        let mut b = pre;
                        b.push(
                            Statement::If {
                                condition,
                                accept,
                                reject: tail,
                            },
                            span,
                        );
                        tail = b;
                    }
                    for (s, sp) in tail.span_into_iter() {
                        block.push(s, sp);
                    }
                    continue;
                }
                let done = *flag.get_or_insert_with(|| {
                    let local = f.local_variables.append(
                        naga::LocalVariable {
                            name: Some("hz_done".into()),
                            ty,
                            init: None,
                        },
                        Span::UNDEFINED,
                    );
                    f.expressions
                        .append(Expression::LocalVariable(local), Span::UNDEFINED)
                });
                let yes = f
                    .expressions
                    .append(Expression::Literal(Literal::Bool(true)), Span::UNDEFINED);
                let no = f
                    .expressions
                    .append(Expression::Literal(Literal::Bool(false)), Span::UNDEFINED);
                let set = |value| Statement::Store {
                    pointer: done,
                    value,
                };
                block.push(set(no), span);
                let not_done = |f: &mut naga::Function| {
                    let load = f
                        .expressions
                        .append(Expression::Load { pointer: done }, Span::UNDEFINED);
                    let not = f.expressions.append(
                        Expression::Unary {
                            op: UnaryOperator::LogicalNot,
                            expr: load,
                        },
                        Span::UNDEFINED,
                    );
                    (naga::Range::new_from_bounds(load, not), not)
                };
                for (k, (pre, condition, accept)) in links.into_iter().enumerate() {
                    let mut guarded = naga::Block::new();
                    guarded.push(set(yes), span);
                    for (s, sp) in accept.span_into_iter() {
                        guarded.push(s, sp);
                    }
                    let mut link = pre;
                    link.push(
                        Statement::If {
                            condition,
                            accept: guarded,
                            reject: naga::Block::new(),
                        },
                        span,
                    );
                    if k == 0 {
                        for (s, sp) in link.span_into_iter() {
                            block.push(s, sp);
                        }
                        continue;
                    }
                    let (range, not) = not_done(f);
                    block.push(Statement::Emit(range), span);
                    block.push(
                        Statement::If {
                            condition: not,
                            accept: link,
                            reject: naga::Block::new(),
                        },
                        span,
                    );
                }
                if !otherwise.is_empty() {
                    let (range, not) = not_done(f);
                    block.push(Statement::Emit(range), span);
                    block.push(
                        Statement::If {
                            condition: not,
                            accept: otherwise,
                            reject: naga::Block::new(),
                        },
                        span,
                    );
                }
                continue;
            }
            other => other,
        };
        block.push(statement, span);
    }
}

/// Where `vector % scalar` (or `scalar % vector`) sits in `source`: the
/// scalar operand's byte range and the vector constructor to wrap it in.
/// GLSL allows the mix; naga's GLSL front end keeps it as is and its
/// validator rejects it.
fn modulo_splats(module: &naga::Module, source: &str) -> Vec<(std::ops::Range<usize>, String)> {
    use naga::{BinaryOperator, Expression, ScalarKind, TypeInner};
    let functions = module
        .functions
        .iter()
        .map(|(_, f)| f)
        .chain(module.entry_points.iter().map(|e| &e.function));
    let mut out = Vec::new();
    for f in functions {
        let Some(last) = f.expressions.iter().map(|(h, _)| h).next_back() else {
            continue;
        };
        let ctx = naga::proc::ResolveContext::with_locals(module, &f.local_variables, &f.arguments);
        let mut types = naga::front::Typifier::new();
        if types.grow(last, &f.expressions, &ctx).is_err() {
            continue;
        }
        for (h, e) in f.expressions.iter() {
            let Expression::Binary {
                op: BinaryOperator::Modulo,
                left,
                right,
            } = *e
            else {
                continue;
            };
            let (l, r) = (
                types.get(left, &module.types),
                types.get(right, &module.types),
            );
            let (is_right, size, scalar) = match (l, r) {
                (TypeInner::Vector { size, scalar }, TypeInner::Scalar(_)) => (true, size, scalar),
                (TypeInner::Scalar(_), TypeInner::Vector { size, scalar }) => (false, size, scalar),
                _ => continue,
            };
            let prefix = match scalar.kind {
                ScalarKind::Sint => "ivec",
                ScalarKind::Uint => "uvec",
                _ => "vec",
            };
            // The operand's text is the `%`'s, split at its top-level `%`: an
            // operand's own span may be elsewhere (a constant's declaration).
            let Some(range) = f.expressions.get_span(h).to_range() else {
                continue;
            };
            let Some(text) = source.get(range.clone()) else {
                continue;
            };
            let Some(at) = top_level_percent(text) else {
                continue;
            };
            let side = if is_right {
                range.start + at + 1..range.end
            } else {
                range.start..range.start + at
            };
            let lead = source[side.clone()].len() - source[side.clone()].trim_start().len();
            let trail = source[side.clone()].len() - source[side.clone()].trim_end().len();
            let side = side.start + lead..side.end - trail;
            if !side.is_empty() {
                out.push((side, format!("{prefix}{}", *size as u8)));
            }
        }
    }
    out
}

/// Where float `a != b` sits in `source`, with its text written as
/// `!(a == b)`. GLSL's `!=` is true when either side is NaN (packs test
/// `clamp(x, 0, 1) != x` to catch NaN), but naga's SPIR-V writer emits the
/// ordered comparison, which is false. The negated `==` keeps GLSL's answer.
/// (`notEqual`, which naga gives no span, is a rewrite polyfill.)
fn float_not_equals(module: &naga::Module, source: &str) -> Vec<(std::ops::Range<usize>, String)> {
    use naga::{BinaryOperator, Expression, ScalarKind};
    let functions = module
        .functions
        .iter()
        .map(|(_, f)| f)
        .chain(module.entry_points.iter().map(|e| &e.function));
    let mut out = Vec::new();
    for f in functions {
        let Some(last) = f.expressions.iter().map(|(h, _)| h).next_back() else {
            continue;
        };
        let ctx = naga::proc::ResolveContext::with_locals(module, &f.local_variables, &f.arguments);
        let mut types = naga::front::Typifier::new();
        if types.grow(last, &f.expressions, &ctx).is_err() {
            continue;
        }
        for (h, e) in f.expressions.iter() {
            let Expression::Binary {
                op: BinaryOperator::NotEqual,
                left,
                ..
            } = *e
            else {
                continue;
            };
            if types.get(left, &module.types).scalar_kind() != Some(ScalarKind::Float) {
                continue;
            }
            let Some(range) = f.expressions.get_span(h).to_range() else {
                continue;
            };
            let Some(text) = source.get(range.clone()) else {
                continue;
            };
            if let Some(at) = top_level_not_equal(text) {
                out.push((range, format!("!({}=={})", &text[..at], &text[at + 2..])));
            }
        }
    }
    out
}

/// The byte offset of the `!=` of `text` outside parentheses and brackets.
fn top_level_not_equal(text: &str) -> Option<usize> {
    let mut depth = 0i32;
    let b = text.as_bytes();
    for (i, &c) in b.iter().enumerate() {
        match c {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth -= 1,
            b'!' if depth == 0 && b.get(i + 1) == Some(&b'=') => return Some(i),
            _ => {}
        }
    }
    None
}

/// `source` with each range replaced by its text; a range overlapping one
/// already replaced is left for the next round.
fn replace_ranges(source: &str, mut patches: Vec<(std::ops::Range<usize>, String)>) -> String {
    patches.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    let mut out = source.to_string();
    let mut limit = usize::MAX;
    for (r, text) in patches {
        if r.end > limit || r.end > out.len() || r.start >= r.end {
            continue;
        }
        out.replace_range(r.clone(), &text);
        limit = r.start;
    }
    out
}

/// Parses `source`; a call naga finds ambiguous (`max(x, 1)` with a float
/// or uint `x`: GLSL converts the `1`, naga's overload resolution does not
/// choose) gets its integer literals written as `uint`s, which convert to
/// either, and the source is parsed again. `dot` on two scalars (GLSL's
/// `a * b`, which naga refuses) is written as the product, and a swizzle
/// store into a matrix column (`m[0].xyz = v`, which naga cannot look up) as
/// stores of its components. Returns the
/// patched source when it changed.
fn parse_floating_ambiguous(
    source: &str,
    stage: naga::ShaderStage,
) -> Result<(naga::Module, Option<String>), String> {
    let mut text: Option<String> = None;
    for _ in 0..16 {
        let current = text.as_deref().unwrap_or(source);
        let errors = match glsl::Frontend::default().parse(&glsl::Options::from(stage), current) {
            Ok(module) => return Ok((module, text)),
            Err(e) => e,
        };
        let ambiguous: Vec<_> = errors
            .errors
            .iter()
            .filter(|e| e.kind.to_string().starts_with("Ambiguous best function"))
            .filter_map(|e| e.meta.to_range())
            .collect();
        let dots: Vec<_> = errors
            .errors
            .iter()
            .filter(|e| {
                e.kind
                    .to_string()
                    .contains("BuiltinArgumentsInvalid(\"Dot\")")
            })
            .filter_map(|e| e.meta.to_range())
            .collect();
        let fields: Vec<_> = errors
            .errors
            .iter()
            .filter(|e| {
                e.kind
                    .to_string()
                    .starts_with("Can't lookup field on this type")
            })
            .filter_map(|e| e.meta.to_range())
            .collect();
        let patched = if !dots.is_empty() {
            scalar_dots(current, dots)
        } else if !fields.is_empty() {
            column_swizzle_stores(current, fields)
        } else {
            float_literals(current, &ambiguous)
        };
        if patched == current {
            return Err(errors.emit_to_string(current));
        }
        text = Some(patched);
    }
    let current = text.as_deref().unwrap_or(source);
    glsl::Frontend::default()
        .parse(&glsl::Options::from(stage), current)
        .map(|m| (m, text.clone()))
        .map_err(|e| e.emit_to_string(current))
}

/// `source` with the integer literals inside `ranges` (outside `[…]`) as
/// `uint`s (floats when negated).
fn float_literals(source: &str, ranges: &[std::ops::Range<usize>]) -> String {
    let mut out = String::with_capacity(source.len() + 16);
    let bytes = source.as_bytes();
    let mut i = 0;
    let mut brackets = 0i32;
    while i < bytes.len() {
        let inside = ranges.iter().any(|r| r.contains(&i));
        let c = bytes[i];
        if !inside {
            brackets = 0;
            out.push(c as char);
            i += 1;
            continue;
        }
        match c {
            b'[' => brackets += 1,
            b']' => brackets -= 1,
            _ => {}
        }
        let starts_number = c.is_ascii_digit()
            && (i == 0
                || !(bytes[i - 1].is_ascii_alphanumeric()
                    || bytes[i - 1] == b'_'
                    || bytes[i - 1] == b'.'));
        if !starts_number || brackets > 0 {
            out.push(c as char);
            i += 1;
            continue;
        }
        let end = (i..bytes.len())
            .find(|&j| !(bytes[j].is_ascii_alphanumeric() || bytes[j] == b'.' || bytes[j] == b'_'))
            .unwrap_or(bytes.len());
        let literal = &source[i..end];
        out.push_str(literal);
        if literal.bytes().all(|b| b.is_ascii_digit()) {
            // `u` fits both a `uint` and a `float` neighbour (GLSL converts
            // uint to float); a negated one can only be a float.
            let negated = source[..i].trim_end().ends_with('-');
            out.push_str(if negated { ".0" } else { "u" });
        }
        i = end;
    }
    out
}

/// `source` with each swizzle store whose field is at `ranges`
/// (`m[0].xyz = v;`) written as one store per component:
/// `{ vec3 hz_sw = vec3(v); m[0][0] = hz_sw.x; … }`. Fields that are not
/// the target of a whole-statement store are left alone.
fn column_swizzle_stores(source: &str, mut ranges: Vec<std::ops::Range<usize>>) -> String {
    ranges.sort_by_key(|r| std::cmp::Reverse(r.start));
    ranges.dedup();
    let mut out = source.to_string();
    for r in ranges {
        if let Some((range, text)) = column_swizzle_store(&out, r) {
            out.replace_range(range, &text);
        }
    }
    out
}

fn column_swizzle_store(
    s: &str,
    field: std::ops::Range<usize>,
) -> Option<(std::ops::Range<usize>, String)> {
    let b = s.as_bytes();
    // naga's span starts at the `.`.
    let text = s.get(field.clone())?;
    let field = field.start + (text.len() - text.trim_start_matches(['.', ' ']).len())..field.end;
    let name = s.get(field.clone())?.trim_end();
    let field = field.start..field.start + name.len();
    let set = ["xyzw", "rgba", "stpq"]
        .into_iter()
        .find(|set| name.chars().all(|c| set.contains(c)))?;
    if !(2..=4).contains(&name.len()) {
        return None;
    }
    // Back over the `.` and the postfix chain before it: `cols[i].m[1]`.
    let mut i = s[..field.start].trim_end().len();
    if i == 0 || b[i - 1] != b'.' {
        return None;
    }
    i = s[..i - 1].trim_end().len();
    let dot_end = i;
    loop {
        if i > 0 && b[i - 1] == b']' {
            let mut depth = 0;
            loop {
                i = i.checked_sub(1)?;
                match b[i] {
                    b']' => depth += 1,
                    b'[' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
            }
        } else if i > 0
            && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_' || b[i - 1] == b'.')
        {
            i -= 1;
        } else {
            break;
        }
    }
    let start = i;
    let column = s[start..dot_end].trim();
    if column.is_empty() || !column.ends_with(']') {
        return None;
    }
    // The statement must begin here.
    let before = s[..start].trim_end();
    if !(before.is_empty() || before.ends_with([';', '{', '}', ')']) || before.ends_with("else")) {
        return None;
    }
    let rest = &s[field.end..];
    let after = rest.trim_start();
    let op = ["+=", "-=", "*=", "/=", "="]
        .into_iter()
        .find(|op| after.starts_with(op) && !after[op.len()..].starts_with('='))?;
    let rhs_start = field.end + (rest.len() - after.len()) + op.len();
    let mut depth = 0i32;
    let semi = s[rhs_start..].char_indices().find(|&(_, c)| {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
        c == ';' && depth == 0
    })?;
    let rhs = s[rhs_start..rhs_start + semi.0].trim();
    let ty = format!("vec{}", name.len());
    let mut text = format!("{{ {ty} hz_sw = {ty}({rhs});");
    for (k, c) in name.chars().enumerate() {
        let index = set.find(c)?;
        let lane = ['x', 'y', 'z', 'w'][k];
        text.push_str(&format!(" {column}[{index}] {op} hz_sw.{lane};"));
    }
    text.push_str(" }");
    Some((start..rhs_start + semi.0 + 1, text))
}

/// `source` with each `dot(a, b)` call at `ranges` written `((a) * (b))`.
fn scalar_dots(source: &str, mut ranges: Vec<std::ops::Range<usize>>) -> String {
    ranges.sort_by_key(|r| std::cmp::Reverse(r.start));
    ranges.dedup();
    let mut out = source.to_string();
    for r in ranges {
        let Some(call) = out.get(r.clone()) else {
            continue;
        };
        let Some(inner) = call
            .strip_prefix("dot")
            .map(str::trim_start)
            .and_then(|c| c.strip_prefix('('))
            .and_then(|c| c.strip_suffix(')'))
        else {
            continue;
        };
        let mut depth = 0i32;
        let comma = inner.char_indices().find(|&(_, c)| {
            match c {
                '(' | '[' => depth += 1,
                ')' | ']' => depth -= 1,
                _ => {}
            }
            c == ',' && depth == 0
        });
        let Some((k, _)) = comma else {
            continue;
        };
        let product = format!("(({}) * ({}))", inner[..k].trim(), inner[k + 1..].trim());
        out.replace_range(r, &product);
    }
    out
}

/// The byte offset of the last `%` outside parentheses and brackets (the
/// top operator of a left-associative `a % b % c`), not `%=`.
fn top_level_percent(text: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut found = None;
    for (i, c) in text.char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            '%' if depth == 0 && !text[i + 1..].starts_with('=') => found = Some(i),
            _ => {}
        }
    }
    found
}

/// `source` with each range wrapped in its constructor (`ivec2(…)`);
/// overlapping ranges after the first are left for the next round.
fn wrap_ranges(source: &str, mut patches: Vec<(std::ops::Range<usize>, String)>) -> String {
    patches.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    let mut out = source.to_string();
    let mut limit = usize::MAX;
    for (r, ctor) in patches {
        if r.end > limit || r.end > out.len() || r.start >= r.end {
            continue;
        }
        out.insert(r.end, ')');
        out.insert_str(r.start, &format!("{ctor}("));
        limit = r.start;
    }
    out
}

#[cfg(test)]
mod modulo_tests {
    use super::*;

    #[test]
    fn swizzle_stores_into_matrix_columns() {
        let src = "#version 450\nlayout(location = 0) out vec4 o;\nvoid main() { mat4 m = mat4(1.0); vec4 a[2]; a[1] = vec4(0.0); a[1].xy = vec2(2.0); if (o.x > 0.0) m[0].xyz = vec3(o.x, 2.0, 3.0); m[3].wx += 1.0; o = m[0] + m[3] + a[1]; }\n";
        let wgsl = glsl_to_wgsl(src, Stage::Fragment).unwrap();
        assert!(wgsl.contains("hz_sw"), "{wgsl}");
        let patched = column_swizzle_stores("m[3].wx += 1.0;", std::iter::once(4..7).collect());
        assert_eq!(
            patched,
            "{ vec2 hz_sw = vec2(1.0); m[3][3] += hz_sw.x; m[3][0] += hz_sw.y; }"
        );
    }

    #[test]
    fn float_not_equal_is_true_for_nan() {
        // Photon tests `clamp01(uv) != uv` to catch a NaN reprojection: the
        // ordered `!=` naga's SPIR-V writer emits misses it and the NaN
        // reaches the clouds' history, which goes black.
        let src = "#version 450\nlayout(location = 0) out vec4 o;\n\
                   void main() { vec2 p = gl_FragCoord.xy / 0.0; float x = p.x; int i = int(o.w);\n\
                   bool d = clamp(p, 0.0, 1.0) != p || (x != 1.0 && i != 2);\n\
                   o = vec4(d ? 1.0 : 0.0, float(x != x), 0.0, 0.0); }\n";
        let wgsl = glsl_to_wgsl(src, Stage::Fragment).unwrap();
        // Integers keep `!=`; every float comparison is a negated `==`.
        assert_eq!(wgsl.matches("!=").count(), 1, "{wgsl}");
        assert!(wgsl.matches("!(").count() >= 3, "{wgsl}");
        let patches = float_not_equals(
            &glsl::Frontend::default()
                .parse(&glsl::Options::from(naga::ShaderStage::Fragment), src)
                .unwrap(),
            src,
        );
        let texts: Vec<_> = patches.iter().map(|(_, t)| t.as_str()).collect();
        assert!(texts.contains(&"!(clamp(p, 0.0, 1.0) == p)"), "{texts:?}");
        assert!(texts.contains(&"!(x == 1.0)"), "{texts:?}");
    }

    #[test]
    fn largest_float_is_written_exactly() {
        let src = "#version 450\nlayout(location = 0) out vec4 o;\nvoid main() { float m = 3.4028235e38; o = vec4(min(gl_FragCoord.x * m, m)); }\n";
        let wgsl = glsl_to_wgsl(src, Stage::Fragment).unwrap();
        assert!(
            wgsl.contains("340282346638528859811704183484516925440f"),
            "{wgsl}"
        );
        assert_eq!(
            "340282346638528859811704183484516925440".parse::<f32>(),
            Ok(f32::MAX)
        );
    }

    #[test]
    fn scalar_dot_is_a_product() {
        let src = "#version 450\nlayout(location = 0) out vec4 o;\nfloat h(float x) { x = fract(x); x += dot(x, x + 19.19); return x + dot(vec2(x), vec2(1.0)); }\nvoid main() { o = vec4(h(gl_FragCoord.x)); }\n";
        let wgsl = glsl_to_wgsl(src, Stage::Fragment).unwrap();
        assert!(wgsl.contains("dot("), "{wgsl}");
    }

    #[test]
    fn ambiguous_calls_take_float_literals() {
        let src = "#version 450\nlayout(location = 0) in vec2 v;\nlayout(location = 0) out vec4 c;\n\
                   const int n = 4;\nvoid main() { float s = v.x; s /= max(s, 1); vec2 w[2]; \
                   c = vec4(clamp(s, 0, 1), min(v.y, 2), w[1]); ivec2 p = ivec2(v) % n; c.x += float(p.x); }\n";
        let wgsl = glsl_to_wgsl(src, Stage::Fragment).unwrap();
        assert!(wgsl.contains("max("), "{wgsl}");
        assert_eq!(
            float_literals("f(a, 12, -2, x2, 3u, v[1])", std::slice::from_ref(&(0..25))),
            "f(a, 12u, -2.0, x2, 3u, v[1])"
        );
    }

    #[test]
    fn int_literals_beside_uints() {
        let src = "#version 450\nlayout(location = 0) flat in uint n;\nlayout(location = 0) out vec4 c;\n\
                   void main() { uint s = min(n, 32); s = clamp(s, 1, 8); bool b = s > 2; c = vec4(float(s), b, 0.0, 1.0); }\n";
        glsl_to_wgsl(src, Stage::Fragment).unwrap();
    }

    #[test]
    fn long_else_if_chains_are_flat() {
        // naga's parser recurses per `else if`: more stack than a test thread has.
        std::thread::Builder::new()
            .stack_size(64 << 20)
            .spawn(long_chains)
            .unwrap()
            .join()
            .unwrap();
    }

    fn long_chains() {
        let mut chain = String::from("if (id == 0) { c = vec3(0.0); }");
        for i in 1..200 {
            chain += &format!(" else if (id == {i}) {{ c = vec3({i}.0); }}");
        }
        chain += " else { c = vec3(-1.0); }";
        let src = format!(
            "#version 450\nlayout(location = 0) flat in int id;\nlayout(location = 0) out vec4 o;\n\
             void main() {{ vec3 c; for (int k = 0; k < 2; k++) {{ {chain} }} o = vec4(c, 1.0); }}\n"
        );
        let wgsl = glsl_to_wgsl(&src, Stage::Fragment).unwrap();
        let depth = wgsl
            .lines()
            .map(|l| l.len() - l.trim_start().len())
            .max()
            .unwrap();
        assert!(depth < 40, "{depth}");
        assert!(wgsl.contains("hz_done"), "{wgsl}");
        // Short chains stay as they are.
        let short = src.replace(
            &chain,
            "if (id == 0) { c = vec3(0.0); } else if (id == 1) { c = vec3(1.0); }",
        );
        assert!(
            !glsl_to_wgsl(&short, Stage::Fragment)
                .unwrap()
                .contains("hz_done")
        );
    }

    #[test]
    fn cases_ending_in_return_do_not_fall_through() {
        let src = "#version 450\nlayout(location = 0) flat in int m;\nlayout(location = 0) out vec4 c;\n\
                   float f(float x) { switch (m) { case 1: return x; case 2: case 3: \
                   if (x > 0.0) { return 1.0; } return max(x, 2.0); default: return 0.0; } }\n\
                   void main() { c = vec4(f(1.0)); }\n";
        glsl_to_wgsl(src, Stage::Fragment).unwrap();
    }

    #[test]
    fn vector_modulo_scalar() {
        let src = "#version 450\nlayout(location = 0) flat in ivec2 p;\nlayout(location = 0) out vec4 c;\n\
                   void main() { ivec2 a = p % 4; ivec2 b = 3 % p; if (p.x > 0) { c = vec4(a + b, p % 2); } }\n";
        let wgsl = glsl_to_wgsl(src, Stage::Fragment).unwrap();
        assert!(
            wgsl.contains("% vec2(4i)") || wgsl.contains("vec2<i32>(4i)"),
            "{wgsl}"
        );
    }
}
