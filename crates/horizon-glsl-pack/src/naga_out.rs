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
    let mut frontend = glsl::Frontend::default();
    let mut module = frontend
        .parse(&glsl::Options::from(stage), source)
        .map_err(|e| e.emit_to_string(source))?;
    // `vector % scalar`: wrap the scalar in the source and parse again.
    let mut patched = String::new();
    for _ in 0..4 {
        let splats = modulo_splats(&module);
        if splats.is_empty() {
            break;
        }
        patched = wrap_ranges(if patched.is_empty() { source } else { &patched }, splats);
        module = glsl::Frontend::default()
            .parse(&glsl::Options::from(stage), &patched)
            .map_err(|e| e.emit_to_string(&patched))?;
    }
    let source = if patched.is_empty() { source } else { &patched };
    spill_pointer_arguments(&mut module);
    end_terminated_cases(&mut module);
    let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
        .validate(&module)
        .map_err(|e| e.emit_to_string(source))?;
    let text = wgsl::write_string(&module, &info, wgsl::WriterFlags::empty())
        .map_err(|e| e.to_string())?;
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

/// Where `vector % scalar` (or `scalar % vector`) sits in `source`: the
/// scalar operand's byte range and the vector constructor to wrap it in.
/// GLSL allows the mix; naga's GLSL front end keeps it as is and its
/// validator rejects it.
fn modulo_splats(module: &naga::Module) -> Vec<(std::ops::Range<usize>, String)> {
    use naga::{BinaryOperator, Expression, ScalarKind, TypeInner};
    let functions = module
        .functions
        .iter()
        .map(|(_, f)| f)
        .chain(module.entry_points.iter().map(|e| &e.function));
    let mut out = Vec::new();
    for f in functions {
        let Some(last) = f.expressions.iter().map(|(h, _)| h).last() else {
            continue;
        };
        let ctx = naga::proc::ResolveContext::with_locals(module, &f.local_variables, &f.arguments);
        let mut types = naga::front::Typifier::new();
        if types.grow(last, &f.expressions, &ctx).is_err() {
            continue;
        }
        for (_, e) in f.expressions.iter() {
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
            let (operand, size, scalar) = match (l, r) {
                (TypeInner::Vector { size, scalar }, TypeInner::Scalar(_)) => (right, size, scalar),
                (TypeInner::Scalar(_), TypeInner::Vector { size, scalar }) => (left, size, scalar),
                _ => continue,
            };
            let prefix = match scalar.kind {
                ScalarKind::Sint => "ivec",
                ScalarKind::Uint => "uvec",
                _ => "vec",
            };
            if let Some(range) = f.expressions.get_span(operand).to_range() {
                out.push((range, format!("{prefix}{}", *size as u8)));
            }
        }
    }
    out
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
