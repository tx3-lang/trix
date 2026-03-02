use aiken_lang::{
    ast::{BinOp, ModuleKind, UntypedDefinition, UntypedPattern},
    expr::UntypedExpr,
    parser,
};
use miette::{IntoDiagnostic, Result};
use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use crate::commands::audit::model::{
    ValidatorContextMap, VulnerabilityFinding, VulnerabilitySkill,
};

pub(super) fn collect_findings_for_skill(
    skill: &VulnerabilitySkill,
    source_references: &[String],
    validator_context: &ValidatorContextMap,
    project_root: &Path,
) -> Result<Option<Vec<VulnerabilityFinding>>> {
    let sources = load_sources(project_root, source_references, validator_context)?;

    let findings = match skill.id.as_str() {
        "strict-value-equality-001" => Some(detect_strict_value_equality(skill, &sources)),
        "missing-address-validation-002" => {
            Some(detect_missing_address_validation(skill, &sources))
        }
        "unvalidated-datum-003" => Some(detect_unvalidated_datum(skill, &sources)),
        _ => None,
    };

    Ok(findings)
}

#[derive(Debug, Clone)]
struct SourceDoc {
    file: String,
    content: String,
    lines: Vec<String>,
    module: Option<aiken_lang::ast::UntypedModule>,
}

fn load_sources(
    project_root: &Path,
    source_references: &[String],
    validator_context: &ValidatorContextMap,
) -> Result<Vec<SourceDoc>> {
    let mut candidates = BTreeSet::new();

    for path in source_references {
        if path.ends_with(".ak") {
            candidates.insert(path.clone());
        }
    }

    for validator in &validator_context.validators {
        if validator.source_file.ends_with(".ak") {
            candidates.insert(validator.source_file.clone());
        }
    }

    let mut sources = Vec::new();

    for relative in candidates {
        let full_path = project_root.join(&relative);
        if !full_path.exists() {
            continue;
        }

        let content = std::fs::read_to_string(&full_path).into_diagnostic()?;
        let lines = content
            .lines()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();
        let module = parse_module(&content);

        sources.push(SourceDoc {
            file: relative,
            content,
            lines,
            module,
        });
    }

    Ok(sources)
}

fn parse_module(content: &str) -> Option<aiken_lang::ast::UntypedModule> {
    parser::module(content, ModuleKind::Validator)
        .ok()
        .map(|(module, _)| module)
}

fn detect_strict_value_equality(
    skill: &VulnerabilitySkill,
    sources: &[SourceDoc],
) -> Vec<VulnerabilityFinding> {
    let mut findings = Vec::new();

    for source in sources {
        let Some(module) = &source.module else {
            findings.extend(detect_strict_value_equality_text_fallback(skill, source));
            continue;
        };

        let mut eq_locations = Vec::new();

        for definition in &module.definitions {
            let UntypedDefinition::Validator(validator) = definition else {
                continue;
            };

            for handler in validator
                .handlers
                .iter()
                .chain(std::iter::once(&validator.fallback))
            {
                collect_strict_value_equalities(&handler.body, &mut eq_locations);
            }
        }

        for (byte_offset, snippet) in eq_locations {
            let line = module.lines.line_number(byte_offset).unwrap_or(1);
            findings.push(VulnerabilityFinding {
                title: "Strict value equality in validator logic".to_string(),
                severity: skill.severity.clone(),
                summary: "Detected strict equality over value/ADA-related expression; this can make validators unsatisfiable under ledger changes.".to_string(),
                evidence: vec![format!("{}:{} -> {}", source.file, line, snippet)],
                recommendation:
                    "Prefer minimum/value-shape checks and avoid exact ADA/value equality unless a strong invariant requires it."
                        .to_string(),
                file: Some(source.file.clone()),
                line: Some(line),
            });
        }
    }

    findings
}

fn collect_strict_value_equalities(expr: &UntypedExpr, out: &mut Vec<(usize, String)>) {
    match expr {
        UntypedExpr::BinOp {
            name,
            left,
            right,
            location,
        } => {
            if *name == BinOp::Eq
                && (is_value_or_lovelace_expr(left) || is_value_or_lovelace_expr(right))
                && !contains_without_lovelace(left)
                && !contains_without_lovelace(right)
            {
                out.push((
                    location.start,
                    "strict equality on value/lovelace expression".to_string(),
                ));
            }

            collect_strict_value_equalities(left, out);
            collect_strict_value_equalities(right, out);
        }
        UntypedExpr::Sequence { expressions, .. }
        | UntypedExpr::LogicalOpChain { expressions, .. } => {
            for inner in expressions {
                collect_strict_value_equalities(inner, out);
            }
        }
        UntypedExpr::PipeLine { expressions, .. } => {
            for inner in expressions {
                collect_strict_value_equalities(inner, out);
            }
        }
        UntypedExpr::Call { fun, arguments, .. } => {
            collect_strict_value_equalities(fun, out);
            for arg in arguments {
                collect_strict_value_equalities(&arg.value, out);
            }
        }
        UntypedExpr::Assignment {
            value, patterns, ..
        } => {
            collect_strict_value_equalities(value, out);
            for pattern in patterns {
                collect_pattern_exprs_for_scan(
                    &pattern.pattern,
                    out,
                    collect_strict_value_equalities,
                );
            }
        }
        UntypedExpr::When {
            subject, clauses, ..
        } => {
            collect_strict_value_equalities(subject, out);
            for clause in clauses {
                collect_strict_value_equalities(&clause.then, out);
            }
        }
        UntypedExpr::If {
            branches,
            final_else,
            ..
        } => {
            for branch in branches {
                collect_strict_value_equalities(&branch.condition, out);
                collect_strict_value_equalities(&branch.body, out);
            }
            collect_strict_value_equalities(final_else, out);
        }
        UntypedExpr::Fn { body, .. } => collect_strict_value_equalities(body, out),
        UntypedExpr::Trace {
            then,
            label,
            arguments,
            ..
        } => {
            collect_strict_value_equalities(then, out);
            collect_strict_value_equalities(label, out);
            for arg in arguments {
                collect_strict_value_equalities(arg, out);
            }
        }
        UntypedExpr::TraceIfFalse { value, .. }
        | UntypedExpr::FieldAccess {
            container: value, ..
        }
        | UntypedExpr::TupleIndex { tuple: value, .. }
        | UntypedExpr::UnOp { value, .. } => collect_strict_value_equalities(value, out),
        UntypedExpr::Tuple { elems, .. }
        | UntypedExpr::List {
            elements: elems, ..
        } => {
            for element in elems {
                collect_strict_value_equalities(element, out);
            }
        }
        UntypedExpr::Pair { fst, snd, .. } => {
            collect_strict_value_equalities(fst, out);
            collect_strict_value_equalities(snd, out);
        }
        UntypedExpr::RecordUpdate {
            constructor,
            arguments,
            ..
        } => {
            collect_strict_value_equalities(constructor, out);
            for arg in arguments {
                collect_strict_value_equalities(&arg.value, out);
            }
        }
        _ => {}
    }
}

fn detect_missing_address_validation(
    skill: &VulnerabilitySkill,
    sources: &[SourceDoc],
) -> Vec<VulnerabilityFinding> {
    let mut findings = Vec::new();

    for source in sources {
        let Some(module) = &source.module else {
            findings.extend(detect_missing_address_validation_text_fallback(
                skill, source,
            ));
            continue;
        };

        for definition in &module.definitions {
            let UntypedDefinition::Validator(validator) = definition else {
                continue;
            };

            for handler in validator
                .handlers
                .iter()
                .chain(std::iter::once(&validator.fallback))
            {
                let mut script_hash_vars = Vec::new();
                let mut validated_vars = HashSet::new();

                collect_script_hash_bindings(&handler.body, &mut script_hash_vars);
                collect_equality_validated_vars(&handler.body, &mut validated_vars);

                for (var_name, byte_offset) in script_hash_vars {
                    let line = module.lines.line_number(byte_offset).unwrap_or(1);
                    if var_name != "_" && !validated_vars.contains(&var_name) {
                        findings.push(VulnerabilityFinding {
                            title: "Script credential extracted but not validated".to_string(),
                            severity: skill.severity.clone(),
                            summary: format!(
                                "Output address script credential is extracted as '{}' but never compared against an expected value.",
                                var_name
                            ),
                            evidence: vec![format!(
                                "{}:{} -> extracted script credential '{}' without validation",
                                source.file, line, var_name
                            )],
                            recommendation:
                                "Add explicit validation that extracted script credential matches expected value (e.g. policy_id or known script hash)."
                                    .to_string(),
                            file: Some(source.file.clone()),
                            line: Some(line),
                        });
                    }
                }
            }
        }
    }

    findings
}

fn collect_script_hash_bindings(expr: &UntypedExpr, out: &mut Vec<(String, usize)>) {
    match expr {
        UntypedExpr::Assignment {
            value,
            patterns,
            kind,
            location,
            ..
        } => {
            if kind.is_expect() && has_script_constructor_pattern(patterns) {
                for pattern in patterns {
                    collect_bound_vars_from_script_pattern(&pattern.pattern, out, location.start);
                }
            }

            collect_script_hash_bindings(value, out);
        }
        UntypedExpr::Sequence { expressions, .. }
        | UntypedExpr::LogicalOpChain { expressions, .. } => {
            for inner in expressions {
                collect_script_hash_bindings(inner, out);
            }
        }
        UntypedExpr::PipeLine { expressions, .. } => {
            for inner in expressions {
                collect_script_hash_bindings(inner, out);
            }
        }
        UntypedExpr::Call { fun, arguments, .. } => {
            collect_script_hash_bindings(fun, out);
            for arg in arguments {
                collect_script_hash_bindings(&arg.value, out);
            }
        }
        UntypedExpr::When {
            subject, clauses, ..
        } => {
            collect_script_hash_bindings(subject, out);
            for clause in clauses {
                collect_script_hash_bindings(&clause.then, out);
            }
        }
        UntypedExpr::If {
            branches,
            final_else,
            ..
        } => {
            for branch in branches {
                collect_script_hash_bindings(&branch.condition, out);
                collect_script_hash_bindings(&branch.body, out);
            }
            collect_script_hash_bindings(final_else, out);
        }
        UntypedExpr::Fn { body, .. } => collect_script_hash_bindings(body, out),
        UntypedExpr::Trace {
            then,
            label,
            arguments,
            ..
        } => {
            collect_script_hash_bindings(then, out);
            collect_script_hash_bindings(label, out);
            for arg in arguments {
                collect_script_hash_bindings(arg, out);
            }
        }
        UntypedExpr::TraceIfFalse { value, .. }
        | UntypedExpr::FieldAccess {
            container: value, ..
        }
        | UntypedExpr::TupleIndex { tuple: value, .. }
        | UntypedExpr::UnOp { value, .. } => collect_script_hash_bindings(value, out),
        UntypedExpr::Tuple { elems, .. }
        | UntypedExpr::List {
            elements: elems, ..
        } => {
            for element in elems {
                collect_script_hash_bindings(element, out);
            }
        }
        UntypedExpr::Pair { fst, snd, .. } => {
            collect_script_hash_bindings(fst, out);
            collect_script_hash_bindings(snd, out);
        }
        UntypedExpr::RecordUpdate {
            constructor,
            arguments,
            ..
        } => {
            collect_script_hash_bindings(constructor, out);
            for arg in arguments {
                collect_script_hash_bindings(&arg.value, out);
            }
        }
        _ => {}
    }
}

fn collect_equality_validated_vars(expr: &UntypedExpr, out: &mut HashSet<String>) {
    match expr {
        UntypedExpr::BinOp {
            name, left, right, ..
        } => {
            if matches!(name, BinOp::Eq | BinOp::NotEq) {
                collect_var_names(left, out);
                collect_var_names(right, out);
            }

            collect_equality_validated_vars(left, out);
            collect_equality_validated_vars(right, out);
        }
        UntypedExpr::Sequence { expressions, .. }
        | UntypedExpr::LogicalOpChain { expressions, .. } => {
            for inner in expressions {
                collect_equality_validated_vars(inner, out);
            }
        }
        UntypedExpr::PipeLine { expressions, .. } => {
            for inner in expressions {
                collect_equality_validated_vars(inner, out);
            }
        }
        UntypedExpr::Call { fun, arguments, .. } => {
            collect_equality_validated_vars(fun, out);
            for arg in arguments {
                collect_equality_validated_vars(&arg.value, out);
            }
        }
        UntypedExpr::Assignment { value, .. }
        | UntypedExpr::TraceIfFalse { value, .. }
        | UntypedExpr::FieldAccess {
            container: value, ..
        }
        | UntypedExpr::TupleIndex { tuple: value, .. }
        | UntypedExpr::UnOp { value, .. } => collect_equality_validated_vars(value, out),
        UntypedExpr::When {
            subject, clauses, ..
        } => {
            collect_equality_validated_vars(subject, out);
            for clause in clauses {
                collect_equality_validated_vars(&clause.then, out);
            }
        }
        UntypedExpr::If {
            branches,
            final_else,
            ..
        } => {
            for branch in branches {
                collect_equality_validated_vars(&branch.condition, out);
                collect_equality_validated_vars(&branch.body, out);
            }
            collect_equality_validated_vars(final_else, out);
        }
        UntypedExpr::Fn { body, .. } => collect_equality_validated_vars(body, out),
        UntypedExpr::Trace {
            then,
            label,
            arguments,
            ..
        } => {
            collect_equality_validated_vars(then, out);
            collect_equality_validated_vars(label, out);
            for arg in arguments {
                collect_equality_validated_vars(arg, out);
            }
        }
        UntypedExpr::Tuple { elems, .. }
        | UntypedExpr::List {
            elements: elems, ..
        } => {
            for element in elems {
                collect_equality_validated_vars(element, out);
            }
        }
        UntypedExpr::Pair { fst, snd, .. } => {
            collect_equality_validated_vars(fst, out);
            collect_equality_validated_vars(snd, out);
        }
        UntypedExpr::RecordUpdate {
            constructor,
            arguments,
            ..
        } => {
            collect_equality_validated_vars(constructor, out);
            for arg in arguments {
                collect_equality_validated_vars(&arg.value, out);
            }
        }
        _ => {}
    }
}

fn detect_unvalidated_datum(
    skill: &VulnerabilitySkill,
    sources: &[SourceDoc],
) -> Vec<VulnerabilityFinding> {
    let mut findings = Vec::new();

    for source in sources {
        let Some(module) = &source.module else {
            findings.extend(detect_unvalidated_datum_text_fallback(skill, source));
            continue;
        };

        for definition in &module.definitions {
            let UntypedDefinition::Validator(validator) = definition else {
                continue;
            };

            for handler in validator
                .handlers
                .iter()
                .chain(std::iter::once(&validator.fallback))
            {
                let mut inline_datum_vars = Vec::new();
                collect_inline_datum_bindings(&handler.body, &mut inline_datum_vars);

                for (var_name, byte_offset) in inline_datum_vars {
                    let line = module.lines.line_number(byte_offset).unwrap_or(1);
                    if var_name == "_" {
                        continue;
                    }

                    let tracked_vars = collect_aliases_for_var(&handler.body, &var_name);
                    let has_partial_validation =
                        has_partial_datum_validation_for_vars(&handler.body, &tracked_vars);
                    let has_semantic_validation =
                        has_semantic_validation_for_vars(&handler.body, &tracked_vars);

                    if has_partial_validation || !has_semantic_validation {
                        let summary = if has_partial_validation {
                            format!(
                                "Inline datum '{}' is validated only partially (spread pattern like `Datum {{ ..., .. }}`), which may leave fields unchecked.",
                                var_name
                            )
                        } else {
                            format!(
                                "Inline datum '{}' is extracted from output but not validated by type or field constraints.",
                                var_name
                            )
                        };

                        findings.push(VulnerabilityFinding {
                            title: "Datum extracted but not validated".to_string(),
                            severity: skill.severity.clone(),
                            summary,
                            evidence: vec![format!(
                                "{}:{} -> extracted inline datum '{}' without validation",
                                source.file, line, var_name
                            )],
                            recommendation:
                                "Add explicit datum type validation (`expect <x>: Datum = ...`) and field-level checks or invariant comparisons."
                                    .to_string(),
                            file: Some(source.file.clone()),
                            line: Some(line),
                        });
                    }
                }
            }
        }
    }

    findings
}

fn collect_inline_datum_bindings(expr: &UntypedExpr, out: &mut Vec<(String, usize)>) {
    match expr {
        UntypedExpr::Assignment {
            value,
            patterns,
            kind,
            location,
            ..
        } => {
            if kind.is_expect() && has_inline_datum_constructor_pattern(patterns) {
                for pattern in patterns {
                    collect_bound_vars_from_inline_datum_pattern(
                        &pattern.pattern,
                        out,
                        location.start,
                    );
                }
            }

            collect_inline_datum_bindings(value, out);
        }
        UntypedExpr::Sequence { expressions, .. }
        | UntypedExpr::LogicalOpChain { expressions, .. } => {
            for inner in expressions {
                collect_inline_datum_bindings(inner, out);
            }
        }
        UntypedExpr::PipeLine { expressions, .. } => {
            for inner in expressions {
                collect_inline_datum_bindings(inner, out);
            }
        }
        UntypedExpr::Call { fun, arguments, .. } => {
            collect_inline_datum_bindings(fun, out);
            for arg in arguments {
                collect_inline_datum_bindings(&arg.value, out);
            }
        }
        UntypedExpr::When {
            subject, clauses, ..
        } => {
            collect_inline_datum_bindings(subject, out);
            for clause in clauses {
                collect_inline_datum_bindings(&clause.then, out);
            }
        }
        UntypedExpr::If {
            branches,
            final_else,
            ..
        } => {
            for branch in branches {
                collect_inline_datum_bindings(&branch.condition, out);
                collect_inline_datum_bindings(&branch.body, out);
            }
            collect_inline_datum_bindings(final_else, out);
        }
        UntypedExpr::Fn { body, .. } => collect_inline_datum_bindings(body, out),
        UntypedExpr::Trace {
            then,
            label,
            arguments,
            ..
        } => {
            collect_inline_datum_bindings(then, out);
            collect_inline_datum_bindings(label, out);
            for arg in arguments {
                collect_inline_datum_bindings(arg, out);
            }
        }
        UntypedExpr::TraceIfFalse { value, .. }
        | UntypedExpr::FieldAccess {
            container: value, ..
        }
        | UntypedExpr::TupleIndex { tuple: value, .. }
        | UntypedExpr::UnOp { value, .. } => collect_inline_datum_bindings(value, out),
        UntypedExpr::Tuple { elems, .. }
        | UntypedExpr::List {
            elements: elems, ..
        } => {
            for element in elems {
                collect_inline_datum_bindings(element, out);
            }
        }
        UntypedExpr::Pair { fst, snd, .. } => {
            collect_inline_datum_bindings(fst, out);
            collect_inline_datum_bindings(snd, out);
        }
        UntypedExpr::RecordUpdate {
            constructor,
            arguments,
            ..
        } => {
            collect_inline_datum_bindings(constructor, out);
            for arg in arguments {
                collect_inline_datum_bindings(&arg.value, out);
            }
        }
        _ => {}
    }
}

#[allow(dead_code)]
fn collect_validated_datum_vars(expr: &UntypedExpr, out: &mut HashSet<String>) {
    match expr {
        UntypedExpr::Assignment {
            patterns,
            kind,
            value,
            ..
        } => {
            if kind.is_expect() {
                for pattern in patterns {
                    collect_all_pattern_var_names(&pattern.pattern, out);
                }
            }

            collect_validated_datum_vars(value, out);
        }
        UntypedExpr::BinOp { left, right, .. } => {
            collect_var_names(left, out);
            collect_var_names(right, out);
            collect_validated_datum_vars(left, out);
            collect_validated_datum_vars(right, out);
        }
        UntypedExpr::Sequence { expressions, .. }
        | UntypedExpr::LogicalOpChain { expressions, .. } => {
            for inner in expressions {
                collect_validated_datum_vars(inner, out);
            }
        }
        UntypedExpr::PipeLine { expressions, .. } => {
            for inner in expressions {
                collect_validated_datum_vars(inner, out);
            }
        }
        UntypedExpr::Call { fun, arguments, .. } => {
            collect_validated_datum_vars(fun, out);
            for arg in arguments {
                collect_validated_datum_vars(&arg.value, out);
            }
        }
        UntypedExpr::When {
            subject, clauses, ..
        } => {
            collect_validated_datum_vars(subject, out);
            for clause in clauses {
                collect_validated_datum_vars(&clause.then, out);
            }
        }
        UntypedExpr::If {
            branches,
            final_else,
            ..
        } => {
            for branch in branches {
                collect_validated_datum_vars(&branch.condition, out);
                collect_validated_datum_vars(&branch.body, out);
            }
            collect_validated_datum_vars(final_else, out);
        }
        UntypedExpr::Fn { body, .. } => collect_validated_datum_vars(body, out),
        UntypedExpr::Trace {
            then,
            label,
            arguments,
            ..
        } => {
            collect_validated_datum_vars(then, out);
            collect_validated_datum_vars(label, out);
            for arg in arguments {
                collect_validated_datum_vars(arg, out);
            }
        }
        UntypedExpr::TraceIfFalse { value, .. }
        | UntypedExpr::FieldAccess {
            container: value, ..
        }
        | UntypedExpr::TupleIndex { tuple: value, .. }
        | UntypedExpr::UnOp { value, .. } => collect_validated_datum_vars(value, out),
        UntypedExpr::Tuple { elems, .. }
        | UntypedExpr::List {
            elements: elems, ..
        } => {
            for element in elems {
                collect_validated_datum_vars(element, out);
            }
        }
        UntypedExpr::Pair { fst, snd, .. } => {
            collect_validated_datum_vars(fst, out);
            collect_validated_datum_vars(snd, out);
        }
        UntypedExpr::RecordUpdate {
            constructor,
            arguments,
            ..
        } => {
            collect_validated_datum_vars(constructor, out);
            for arg in arguments {
                collect_validated_datum_vars(&arg.value, out);
            }
        }
        _ => {}
    }
}

fn collect_aliases_for_var(expr: &UntypedExpr, root_var: &str) -> HashSet<String> {
    let mut tracked = HashSet::new();
    tracked.insert(root_var.to_string());

    loop {
        let before = tracked.len();
        collect_aliases_for_vars_once(expr, &mut tracked);
        if tracked.len() == before {
            break;
        }
    }

    tracked
}

fn collect_aliases_for_vars_once(expr: &UntypedExpr, tracked: &mut HashSet<String>) {
    match expr {
        UntypedExpr::Assignment {
            value,
            patterns,
            kind,
            ..
        } => {
            if kind.is_expect() && expr_references_any_var(value, tracked) {
                for assignment in patterns {
                    collect_all_pattern_var_names(&assignment.pattern, tracked);
                }
            }

            collect_aliases_for_vars_once(value, tracked);
        }
        UntypedExpr::Sequence { expressions, .. }
        | UntypedExpr::LogicalOpChain { expressions, .. } => {
            for inner in expressions {
                collect_aliases_for_vars_once(inner, tracked);
            }
        }
        UntypedExpr::PipeLine { expressions, .. } => {
            for inner in expressions {
                collect_aliases_for_vars_once(inner, tracked);
            }
        }
        UntypedExpr::Call { fun, arguments, .. } => {
            collect_aliases_for_vars_once(fun, tracked);
            for arg in arguments {
                collect_aliases_for_vars_once(&arg.value, tracked);
            }
        }
        UntypedExpr::When {
            subject, clauses, ..
        } => {
            collect_aliases_for_vars_once(subject, tracked);
            for clause in clauses {
                collect_aliases_for_vars_once(&clause.then, tracked);
            }
        }
        UntypedExpr::If {
            branches,
            final_else,
            ..
        } => {
            for branch in branches {
                collect_aliases_for_vars_once(&branch.condition, tracked);
                collect_aliases_for_vars_once(&branch.body, tracked);
            }
            collect_aliases_for_vars_once(final_else, tracked);
        }
        UntypedExpr::Fn { body, .. } => collect_aliases_for_vars_once(body, tracked),
        UntypedExpr::Trace {
            then,
            label,
            arguments,
            ..
        } => {
            collect_aliases_for_vars_once(then, tracked);
            collect_aliases_for_vars_once(label, tracked);
            for arg in arguments {
                collect_aliases_for_vars_once(arg, tracked);
            }
        }
        UntypedExpr::TraceIfFalse { value, .. }
        | UntypedExpr::FieldAccess {
            container: value, ..
        }
        | UntypedExpr::TupleIndex { tuple: value, .. }
        | UntypedExpr::UnOp { value, .. } => collect_aliases_for_vars_once(value, tracked),
        UntypedExpr::Tuple { elems, .. }
        | UntypedExpr::List {
            elements: elems, ..
        } => {
            for element in elems {
                collect_aliases_for_vars_once(element, tracked);
            }
        }
        UntypedExpr::Pair { fst, snd, .. } => {
            collect_aliases_for_vars_once(fst, tracked);
            collect_aliases_for_vars_once(snd, tracked);
        }
        UntypedExpr::RecordUpdate {
            constructor,
            arguments,
            ..
        } => {
            collect_aliases_for_vars_once(constructor, tracked);
            for arg in arguments {
                collect_aliases_for_vars_once(&arg.value, tracked);
            }
        }
        _ => {}
    }
}

fn has_partial_datum_validation_for_vars(expr: &UntypedExpr, tracked: &HashSet<String>) -> bool {
    match expr {
        UntypedExpr::Assignment {
            value,
            patterns,
            kind,
            ..
        } => {
            let this_has = kind.is_expect()
                && expr_references_any_var(value, tracked)
                && patterns
                    .iter()
                    .any(|assignment| pattern_has_spread_constructor(&assignment.pattern));

            this_has || has_partial_datum_validation_for_vars(value, tracked)
        }
        UntypedExpr::Sequence { expressions, .. }
        | UntypedExpr::LogicalOpChain { expressions, .. } => expressions
            .iter()
            .any(|inner| has_partial_datum_validation_for_vars(inner, tracked)),
        UntypedExpr::PipeLine { expressions, .. } => expressions
            .iter()
            .any(|inner| has_partial_datum_validation_for_vars(inner, tracked)),
        UntypedExpr::Call { fun, arguments, .. } => {
            has_partial_datum_validation_for_vars(fun, tracked)
                || arguments
                    .iter()
                    .any(|arg| has_partial_datum_validation_for_vars(&arg.value, tracked))
        }
        UntypedExpr::When {
            subject, clauses, ..
        } => {
            has_partial_datum_validation_for_vars(subject, tracked)
                || clauses
                    .iter()
                    .any(|clause| has_partial_datum_validation_for_vars(&clause.then, tracked))
        }
        UntypedExpr::If {
            branches,
            final_else,
            ..
        } => {
            branches.iter().any(|branch| {
                has_partial_datum_validation_for_vars(&branch.condition, tracked)
                    || has_partial_datum_validation_for_vars(&branch.body, tracked)
            }) || has_partial_datum_validation_for_vars(final_else, tracked)
        }
        UntypedExpr::Fn { body, .. } => has_partial_datum_validation_for_vars(body, tracked),
        UntypedExpr::Trace {
            then,
            label,
            arguments,
            ..
        } => {
            has_partial_datum_validation_for_vars(then, tracked)
                || has_partial_datum_validation_for_vars(label, tracked)
                || arguments
                    .iter()
                    .any(|arg| has_partial_datum_validation_for_vars(arg, tracked))
        }
        UntypedExpr::TraceIfFalse { value, .. }
        | UntypedExpr::FieldAccess {
            container: value, ..
        }
        | UntypedExpr::TupleIndex { tuple: value, .. }
        | UntypedExpr::UnOp { value, .. } => has_partial_datum_validation_for_vars(value, tracked),
        UntypedExpr::Tuple { elems, .. }
        | UntypedExpr::List {
            elements: elems, ..
        } => elems
            .iter()
            .any(|element| has_partial_datum_validation_for_vars(element, tracked)),
        UntypedExpr::Pair { fst, snd, .. } => {
            has_partial_datum_validation_for_vars(fst, tracked)
                || has_partial_datum_validation_for_vars(snd, tracked)
        }
        UntypedExpr::RecordUpdate {
            constructor,
            arguments,
            ..
        } => {
            has_partial_datum_validation_for_vars(constructor, tracked)
                || arguments
                    .iter()
                    .any(|arg| has_partial_datum_validation_for_vars(&arg.value, tracked))
        }
        _ => false,
    }
}

fn has_semantic_validation_for_vars(expr: &UntypedExpr, tracked: &HashSet<String>) -> bool {
    match expr {
        UntypedExpr::BinOp {
            left, right, name, ..
        } => {
            let this_has = matches!(name, BinOp::Eq | BinOp::NotEq)
                && (expr_references_any_var(left, tracked)
                    || expr_references_any_var(right, tracked));
            this_has
                || has_semantic_validation_for_vars(left, tracked)
                || has_semantic_validation_for_vars(right, tracked)
        }
        UntypedExpr::Call { fun, arguments, .. } => {
            let this_has = arguments
                .iter()
                .any(|arg| expr_references_any_var(&arg.value, tracked));
            this_has
                || has_semantic_validation_for_vars(fun, tracked)
                || arguments
                    .iter()
                    .any(|arg| has_semantic_validation_for_vars(&arg.value, tracked))
        }
        UntypedExpr::FieldAccess { container, .. } => expr_references_any_var(container, tracked),
        UntypedExpr::Assignment { value, .. }
        | UntypedExpr::TraceIfFalse { value, .. }
        | UntypedExpr::TupleIndex { tuple: value, .. }
        | UntypedExpr::UnOp { value, .. } => has_semantic_validation_for_vars(value, tracked),
        UntypedExpr::Sequence { expressions, .. }
        | UntypedExpr::LogicalOpChain { expressions, .. } => expressions
            .iter()
            .any(|inner| has_semantic_validation_for_vars(inner, tracked)),
        UntypedExpr::PipeLine { expressions, .. } => expressions
            .iter()
            .any(|inner| has_semantic_validation_for_vars(inner, tracked)),
        UntypedExpr::When {
            subject, clauses, ..
        } => {
            has_semantic_validation_for_vars(subject, tracked)
                || clauses
                    .iter()
                    .any(|clause| has_semantic_validation_for_vars(&clause.then, tracked))
        }
        UntypedExpr::If {
            branches,
            final_else,
            ..
        } => {
            branches.iter().any(|branch| {
                has_semantic_validation_for_vars(&branch.condition, tracked)
                    || has_semantic_validation_for_vars(&branch.body, tracked)
            }) || has_semantic_validation_for_vars(final_else, tracked)
        }
        UntypedExpr::Fn { body, .. } => has_semantic_validation_for_vars(body, tracked),
        UntypedExpr::Trace {
            then,
            label,
            arguments,
            ..
        } => {
            has_semantic_validation_for_vars(then, tracked)
                || has_semantic_validation_for_vars(label, tracked)
                || arguments
                    .iter()
                    .any(|arg| has_semantic_validation_for_vars(arg, tracked))
        }
        UntypedExpr::Tuple { elems, .. }
        | UntypedExpr::List {
            elements: elems, ..
        } => elems
            .iter()
            .any(|element| has_semantic_validation_for_vars(element, tracked)),
        UntypedExpr::Pair { fst, snd, .. } => {
            has_semantic_validation_for_vars(fst, tracked)
                || has_semantic_validation_for_vars(snd, tracked)
        }
        UntypedExpr::RecordUpdate {
            constructor,
            arguments,
            ..
        } => {
            has_semantic_validation_for_vars(constructor, tracked)
                || arguments
                    .iter()
                    .any(|arg| has_semantic_validation_for_vars(&arg.value, tracked))
        }
        _ => false,
    }
}

fn expr_references_any_var(expr: &UntypedExpr, tracked: &HashSet<String>) -> bool {
    match expr {
        UntypedExpr::Var { name, .. } => tracked.contains(name),
        UntypedExpr::FieldAccess { container, .. }
        | UntypedExpr::TraceIfFalse {
            value: container, ..
        }
        | UntypedExpr::TupleIndex {
            tuple: container, ..
        }
        | UntypedExpr::UnOp {
            value: container, ..
        } => expr_references_any_var(container, tracked),
        UntypedExpr::Call { fun, arguments, .. } => {
            expr_references_any_var(fun, tracked)
                || arguments
                    .iter()
                    .any(|arg| expr_references_any_var(&arg.value, tracked))
        }
        UntypedExpr::BinOp { left, right, .. } => {
            expr_references_any_var(left, tracked) || expr_references_any_var(right, tracked)
        }
        UntypedExpr::Assignment { value, .. } => expr_references_any_var(value, tracked),
        UntypedExpr::Sequence { expressions, .. }
        | UntypedExpr::LogicalOpChain { expressions, .. } => expressions
            .iter()
            .any(|inner| expr_references_any_var(inner, tracked)),
        UntypedExpr::PipeLine { expressions, .. } => expressions
            .iter()
            .any(|inner| expr_references_any_var(inner, tracked)),
        UntypedExpr::When {
            subject, clauses, ..
        } => {
            expr_references_any_var(subject, tracked)
                || clauses
                    .iter()
                    .any(|clause| expr_references_any_var(&clause.then, tracked))
        }
        UntypedExpr::If {
            branches,
            final_else,
            ..
        } => {
            branches.iter().any(|branch| {
                expr_references_any_var(&branch.condition, tracked)
                    || expr_references_any_var(&branch.body, tracked)
            }) || expr_references_any_var(final_else, tracked)
        }
        UntypedExpr::Fn { body, .. } => expr_references_any_var(body, tracked),
        UntypedExpr::Trace {
            then,
            label,
            arguments,
            ..
        } => {
            expr_references_any_var(then, tracked)
                || expr_references_any_var(label, tracked)
                || arguments
                    .iter()
                    .any(|arg| expr_references_any_var(arg, tracked))
        }
        UntypedExpr::Tuple { elems, .. }
        | UntypedExpr::List {
            elements: elems, ..
        } => elems
            .iter()
            .any(|element| expr_references_any_var(element, tracked)),
        UntypedExpr::Pair { fst, snd, .. } => {
            expr_references_any_var(fst, tracked) || expr_references_any_var(snd, tracked)
        }
        UntypedExpr::RecordUpdate {
            constructor,
            arguments,
            ..
        } => {
            expr_references_any_var(constructor, tracked)
                || arguments
                    .iter()
                    .any(|arg| expr_references_any_var(&arg.value, tracked))
        }
        _ => false,
    }
}

fn pattern_has_spread_constructor(pattern: &UntypedPattern) -> bool {
    match pattern {
        UntypedPattern::Constructor {
            spread_location,
            arguments,
            ..
        } => {
            spread_location.is_some()
                || arguments
                    .iter()
                    .any(|arg| pattern_has_spread_constructor(&arg.value))
        }
        UntypedPattern::List { elements, tail, .. } => {
            elements.iter().any(pattern_has_spread_constructor)
                || tail
                    .as_deref()
                    .map(pattern_has_spread_constructor)
                    .unwrap_or(false)
        }
        UntypedPattern::Pair { fst, snd, .. } => {
            pattern_has_spread_constructor(fst) || pattern_has_spread_constructor(snd)
        }
        UntypedPattern::Tuple { elems, .. } => elems.iter().any(pattern_has_spread_constructor),
        UntypedPattern::Assign { pattern, .. } => pattern_has_spread_constructor(pattern),
        _ => false,
    }
}

fn has_script_constructor_pattern(
    patterns: impl IntoIterator<Item = impl std::borrow::Borrow<aiken_lang::ast::AssignmentPattern>>,
) -> bool {
    patterns
        .into_iter()
        .any(|assignment| pattern_contains_constructor_name(&assignment.borrow().pattern, "Script"))
}

fn has_inline_datum_constructor_pattern(
    patterns: impl IntoIterator<Item = impl std::borrow::Borrow<aiken_lang::ast::AssignmentPattern>>,
) -> bool {
    patterns.into_iter().any(|assignment| {
        pattern_contains_constructor_name(&assignment.borrow().pattern, "InlineDatum")
    })
}

fn pattern_contains_constructor_name(pattern: &UntypedPattern, constructor_name: &str) -> bool {
    match pattern {
        UntypedPattern::Constructor {
            name, arguments, ..
        } => {
            if name == constructor_name {
                return true;
            }

            arguments
                .iter()
                .any(|arg| pattern_contains_constructor_name(&arg.value, constructor_name))
        }
        UntypedPattern::List { elements, tail, .. } => {
            elements
                .iter()
                .any(|element| pattern_contains_constructor_name(element, constructor_name))
                || tail
                    .as_deref()
                    .map(|inner| pattern_contains_constructor_name(inner, constructor_name))
                    .unwrap_or(false)
        }
        UntypedPattern::Pair { fst, snd, .. } => {
            pattern_contains_constructor_name(fst, constructor_name)
                || pattern_contains_constructor_name(snd, constructor_name)
        }
        UntypedPattern::Tuple { elems, .. } => elems
            .iter()
            .any(|element| pattern_contains_constructor_name(element, constructor_name)),
        UntypedPattern::Assign { pattern, .. } => {
            pattern_contains_constructor_name(pattern, constructor_name)
        }
        _ => false,
    }
}

fn collect_bound_vars_from_script_pattern(
    pattern: &UntypedPattern,
    out: &mut Vec<(String, usize)>,
    line: usize,
) {
    match pattern {
        UntypedPattern::Constructor {
            name, arguments, ..
        } => {
            if name == "Script" {
                for arg in arguments {
                    collect_all_pattern_var_names_with_line(&arg.value, out, line);
                }
            }

            for arg in arguments {
                collect_bound_vars_from_script_pattern(&arg.value, out, line);
            }
        }
        UntypedPattern::List { elements, tail, .. } => {
            for element in elements {
                collect_bound_vars_from_script_pattern(element, out, line);
            }

            if let Some(inner) = tail {
                collect_bound_vars_from_script_pattern(inner, out, line);
            }
        }
        UntypedPattern::Pair { fst, snd, .. } => {
            collect_bound_vars_from_script_pattern(fst, out, line);
            collect_bound_vars_from_script_pattern(snd, out, line);
        }
        UntypedPattern::Tuple { elems, .. } => {
            for element in elems {
                collect_bound_vars_from_script_pattern(element, out, line);
            }
        }
        UntypedPattern::Assign { pattern, .. } => {
            collect_bound_vars_from_script_pattern(pattern, out, line)
        }
        _ => {}
    }
}

fn collect_bound_vars_from_inline_datum_pattern(
    pattern: &UntypedPattern,
    out: &mut Vec<(String, usize)>,
    line: usize,
) {
    match pattern {
        UntypedPattern::Constructor {
            name, arguments, ..
        } => {
            if name == "InlineDatum" {
                for arg in arguments {
                    collect_all_pattern_var_names_with_line(&arg.value, out, line);
                }
            }

            for arg in arguments {
                collect_bound_vars_from_inline_datum_pattern(&arg.value, out, line);
            }
        }
        UntypedPattern::List { elements, tail, .. } => {
            for element in elements {
                collect_bound_vars_from_inline_datum_pattern(element, out, line);
            }

            if let Some(inner) = tail {
                collect_bound_vars_from_inline_datum_pattern(inner, out, line);
            }
        }
        UntypedPattern::Pair { fst, snd, .. } => {
            collect_bound_vars_from_inline_datum_pattern(fst, out, line);
            collect_bound_vars_from_inline_datum_pattern(snd, out, line);
        }
        UntypedPattern::Tuple { elems, .. } => {
            for element in elems {
                collect_bound_vars_from_inline_datum_pattern(element, out, line);
            }
        }
        UntypedPattern::Assign { pattern, .. } => {
            collect_bound_vars_from_inline_datum_pattern(pattern, out, line)
        }
        _ => {}
    }
}

fn collect_all_pattern_var_names_with_line(
    pattern: &UntypedPattern,
    out: &mut Vec<(String, usize)>,
    line: usize,
) {
    match pattern {
        UntypedPattern::Var { name, .. } => out.push((name.clone(), line)),
        UntypedPattern::Assign { name, pattern, .. } => {
            out.push((name.clone(), line));
            collect_all_pattern_var_names_with_line(pattern, out, line);
        }
        UntypedPattern::List { elements, tail, .. } => {
            for element in elements {
                collect_all_pattern_var_names_with_line(element, out, line);
            }
            if let Some(inner) = tail {
                collect_all_pattern_var_names_with_line(inner, out, line);
            }
        }
        UntypedPattern::Pair { fst, snd, .. } => {
            collect_all_pattern_var_names_with_line(fst, out, line);
            collect_all_pattern_var_names_with_line(snd, out, line);
        }
        UntypedPattern::Tuple { elems, .. } => {
            for element in elems {
                collect_all_pattern_var_names_with_line(element, out, line);
            }
        }
        UntypedPattern::Constructor { arguments, .. } => {
            for arg in arguments {
                collect_all_pattern_var_names_with_line(&arg.value, out, line);
            }
        }
        _ => {}
    }
}

fn collect_all_pattern_var_names(pattern: &UntypedPattern, out: &mut HashSet<String>) {
    match pattern {
        UntypedPattern::Var { name, .. } => {
            out.insert(name.clone());
        }
        UntypedPattern::Assign { name, pattern, .. } => {
            out.insert(name.clone());
            collect_all_pattern_var_names(pattern, out);
        }
        UntypedPattern::List { elements, tail, .. } => {
            for element in elements {
                collect_all_pattern_var_names(element, out);
            }
            if let Some(inner) = tail {
                collect_all_pattern_var_names(inner, out);
            }
        }
        UntypedPattern::Pair { fst, snd, .. } => {
            collect_all_pattern_var_names(fst, out);
            collect_all_pattern_var_names(snd, out);
        }
        UntypedPattern::Tuple { elems, .. } => {
            for element in elems {
                collect_all_pattern_var_names(element, out);
            }
        }
        UntypedPattern::Constructor { arguments, .. } => {
            for arg in arguments {
                collect_all_pattern_var_names(&arg.value, out);
            }
        }
        _ => {}
    }
}

fn collect_var_names(expr: &UntypedExpr, out: &mut HashSet<String>) {
    match expr {
        UntypedExpr::Var { name, .. } => {
            out.insert(name.clone());
        }
        UntypedExpr::FieldAccess { container, .. }
        | UntypedExpr::TraceIfFalse {
            value: container, ..
        }
        | UntypedExpr::UnOp {
            value: container, ..
        }
        | UntypedExpr::TupleIndex {
            tuple: container, ..
        } => collect_var_names(container, out),
        UntypedExpr::Call { fun, arguments, .. } => {
            collect_var_names(fun, out);
            for arg in arguments {
                collect_var_names(&arg.value, out);
            }
        }
        UntypedExpr::BinOp { left, right, .. } => {
            collect_var_names(left, out);
            collect_var_names(right, out);
        }
        _ => {}
    }
}

fn is_value_or_lovelace_expr(expr: &UntypedExpr) -> bool {
    match expr {
        UntypedExpr::Var { name, .. } => name.contains("value") || name.contains("lovelace"),
        UntypedExpr::FieldAccess {
            label, container, ..
        } => {
            label.contains("value")
                || label.contains("lovelace")
                || is_value_or_lovelace_expr(container)
        }
        UntypedExpr::Call { fun, arguments, .. } => {
            let fn_name_has_signal = matches!(&**fun, UntypedExpr::Var { name, .. } if name.contains("value") || name.contains("lovelace") || name.contains("from_lovelace") || name.contains("lovelace_of"));
            fn_name_has_signal
                || arguments
                    .iter()
                    .any(|arg| is_value_or_lovelace_expr(&arg.value))
        }
        UntypedExpr::BinOp { left, right, .. } => {
            is_value_or_lovelace_expr(left) || is_value_or_lovelace_expr(right)
        }
        UntypedExpr::Tuple { elems, .. }
        | UntypedExpr::List {
            elements: elems, ..
        } => elems.iter().any(is_value_or_lovelace_expr),
        UntypedExpr::Pair { fst, snd, .. } => {
            is_value_or_lovelace_expr(fst) || is_value_or_lovelace_expr(snd)
        }
        _ => false,
    }
}

fn contains_without_lovelace(expr: &UntypedExpr) -> bool {
    match expr {
        UntypedExpr::Call { fun, arguments, .. } => {
            let this_has = matches!(&**fun, UntypedExpr::Var { name, .. } if name.contains("without_lovelace"))
                || matches!(&**fun, UntypedExpr::FieldAccess { label, .. } if label.contains("without_lovelace"));

            this_has
                || contains_without_lovelace(fun)
                || arguments
                    .iter()
                    .any(|arg| contains_without_lovelace(&arg.value))
        }
        UntypedExpr::FieldAccess { container, .. }
        | UntypedExpr::TraceIfFalse {
            value: container, ..
        }
        | UntypedExpr::UnOp {
            value: container, ..
        }
        | UntypedExpr::TupleIndex {
            tuple: container, ..
        } => contains_without_lovelace(container),
        UntypedExpr::BinOp { left, right, .. } => {
            contains_without_lovelace(left) || contains_without_lovelace(right)
        }
        UntypedExpr::Tuple { elems, .. }
        | UntypedExpr::List {
            elements: elems, ..
        } => elems.iter().any(contains_without_lovelace),
        UntypedExpr::Pair { fst, snd, .. } => {
            contains_without_lovelace(fst) || contains_without_lovelace(snd)
        }
        _ => false,
    }
}

fn collect_pattern_exprs_for_scan<F>(
    pattern: &UntypedPattern,
    out: &mut Vec<(usize, String)>,
    scan: F,
) where
    F: Fn(&UntypedExpr, &mut Vec<(usize, String)>),
{
    let _ = pattern;
    let _ = out;
    let _ = scan;
}

fn detect_strict_value_equality_text_fallback(
    skill: &VulnerabilitySkill,
    source: &SourceDoc,
) -> Vec<VulnerabilityFinding> {
    source
        .lines
        .iter()
        .enumerate()
        .filter_map(|(idx, line)| {
            let normalized = line.trim();
            if normalized.starts_with("//") {
                return None;
            }

            let has_strict_eq = normalized.contains("==");
            let has_value_signal = contains_any(normalized, &["value", "lovelace", "from_lovelace"]);
            let safe_pattern = contains_any(normalized, &["without_lovelace", ">="]);

            (has_strict_eq && has_value_signal && !safe_pattern).then_some(VulnerabilityFinding {
                title: "Strict value equality in validator logic".to_string(),
                severity: skill.severity.clone(),
                summary: "Detected strict equality over value/ADA-related expression; this can make validators unsatisfiable under ledger changes.".to_string(),
                evidence: vec![format!("{}:{} -> {}", source.file, idx + 1, normalized)],
                recommendation:
                    "Prefer minimum/value-shape checks and avoid exact ADA/value equality unless a strong invariant requires it."
                        .to_string(),
                file: Some(source.file.clone()),
                line: Some(idx + 1),
            })
        })
        .collect()
}

fn detect_missing_address_validation_text_fallback(
    skill: &VulnerabilitySkill,
    source: &SourceDoc,
) -> Vec<VulnerabilityFinding> {
    let content = source.content.as_str();
    let lines = source
        .lines
        .iter()
        .map(|line| line.as_str())
        .collect::<Vec<_>>();

    for (idx, line) in lines.iter().enumerate() {
        let normalized = line.trim();

        if normalized.contains("Script(") && normalized.contains("payment_credential") {
            let hash_var = if let Some(start) = normalized.find("Script(") {
                let after = &normalized[start + 7..];
                after.find(')').map(|end| after[..end].trim().to_string())
            } else {
                None
            };

            if let Some(var_name) = hash_var {
                if var_name == "_" {
                    continue;
                }

                let search_end = (idx + 30).min(lines.len());
                let found_validation = lines[idx + 1..search_end]
                    .iter()
                    .map(|value| value.trim())
                    .any(|check| {
                        (check.contains("==") || check.contains("!=")) && check.contains(&var_name)
                    });

                if !found_validation {
                    return vec![VulnerabilityFinding {
                        title: "Script credential extracted but not validated".to_string(),
                        severity: skill.severity.clone(),
                        summary: format!(
                            "Output address script credential is extracted as '{}' but never compared or validated against expected value.",
                            var_name
                        ),
                        evidence: vec![format!(
                            "{}:{} -> Script credential extracted: {}",
                            source.file,
                            idx + 1,
                            normalized
                        )],
                        recommendation:
                            "Add explicit validation that the extracted script credential matches the expected value (e.g., compare with policy_id or known script hash)."
                                .to_string(),
                        file: Some(source.file.clone()),
                        line: Some(idx + 1),
                    }];
                }
            }
        }
    }

    let _ = content;
    vec![]
}

fn detect_unvalidated_datum_text_fallback(
    skill: &VulnerabilitySkill,
    source: &SourceDoc,
) -> Vec<VulnerabilityFinding> {
    let lines = source
        .lines
        .iter()
        .map(|line| line.as_str())
        .collect::<Vec<_>>();

    for (idx, line) in lines.iter().enumerate() {
        let normalized = line.trim();

        if normalized.contains("InlineDatum(") && normalized.contains("datum:") {
            let datum_var = if let Some(start) = normalized.find("InlineDatum(") {
                let after = &normalized[start + 12..];
                after.find(')').map(|end| after[..end].trim().to_string())
            } else {
                None
            };

            if let Some(var_name) = datum_var {
                if var_name == "_" {
                    continue;
                }

                let search_end = (idx + 40).min(lines.len());
                let found_validation = lines[idx + 1..search_end]
                    .iter()
                    .map(|value| value.trim())
                    .any(|check| {
                        (check.contains("expect") && check.contains(&var_name))
                            || (check.contains(&format!("{}.", var_name)))
                            || (check.contains("Datum {") && check.contains(&var_name))
                    });

                if !found_validation {
                    return vec![VulnerabilityFinding {
                        title: "Datum extracted but not validated".to_string(),
                        severity: skill.severity.clone(),
                        summary:
                            "InlineDatum is extracted from output but its type and fields are never validated."
                                .to_string(),
                        evidence: vec![format!(
                            "{}:{} -> Datum extracted as '{}' but not validated",
                            source.file,
                            idx + 1,
                            var_name
                        )],
                        recommendation:
                            "Add explicit datum type validation (expect Type = datum) and validate all relevant fields."
                                .to_string(),
                        file: Some(source.file.clone()),
                        line: Some(idx + 1),
                    }];
                }
            }
        }
    }

    vec![]
}

fn contains_any(content: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| content.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::audit::model::ValidatorContextMap;
    use tempfile::tempdir;

    fn skill(id: &str) -> VulnerabilitySkill {
        VulnerabilitySkill {
            id: id.to_string(),
            name: id.to_string(),
            severity: "high".to_string(),
            description: "desc".to_string(),
            prompt_fragment: "prompt".to_string(),
            examples: vec![],
            false_positives: vec![],
            references: vec![],
            tags: vec![],
            confidence_hint: None,
            guidance_markdown: String::new(),
        }
    }

    #[test]
    fn returns_unsupported_skill_for_unknown_id() {
        let tmp = tempdir().expect("tempdir");

        let findings = collect_findings_for_skill(
            &skill("custom-skill-999"),
            &[],
            &ValidatorContextMap::default(),
            tmp.path(),
        )
        .expect("analysis should succeed");

        assert!(findings.is_none());
    }

    #[test]
    fn detects_strict_value_equality() {
        let tmp = tempdir().expect("tempdir");
        std::fs::write(
            tmp.path().join("validator.ak"),
            r#"validator test {
  spend(_datum, _redeemer, _utxo, transaction) {
    expect [output, ..] = transaction.outputs
    lovelace_of(output.value) == 2_000_000
  }
}"#,
        )
        .expect("write source");

        let findings = collect_findings_for_skill(
            &skill("strict-value-equality-001"),
            &["validator.ak".to_string()],
            &ValidatorContextMap::default(),
            tmp.path(),
        )
        .expect("analysis should succeed");

        assert!(findings.is_some());
        assert!(!findings.unwrap_or_default().is_empty());
    }

    #[test]
    fn ignores_safe_value_pattern() {
        let tmp = tempdir().expect("tempdir");
        std::fs::write(
            tmp.path().join("validator.ak"),
            r#"validator test {
  spend(_datum, _redeemer, _utxo, transaction) {
    expect [output, ..] = transaction.outputs
    output.value.without_lovelace() == expected_value
  }
}"#,
        )
        .expect("write source");

        let findings = collect_findings_for_skill(
            &skill("strict-value-equality-001"),
            &["validator.ak".to_string()],
            &ValidatorContextMap::default(),
            tmp.path(),
        )
        .expect("analysis should succeed");

        assert!(findings.unwrap_or_default().is_empty());
    }

    #[test]
    fn detects_missing_address_validation() {
        let tmp = tempdir().expect("tempdir");
        std::fs::write(
            tmp.path().join("validator.ak"),
            r#"use cardano/address.{Address, Script}

validator test {
  mint(_redeemer: Data, policy_id: PolicyId, self: Transaction) {
    expect [first_output, ..] = self.outputs
    expect Output {
      address: Address {
        payment_credential: Script(some_hash),
        stake_credential: None,
      },
      value: val,
      datum: NoDatum,
      reference_script: None,
    } = first_output

    quantity_of(val, policy_id, "token") == 1
  }
}"#,
        )
        .expect("write source");

        let findings = collect_findings_for_skill(
            &skill("missing-address-validation-002"),
            &["validator.ak".to_string()],
            &ValidatorContextMap::default(),
            tmp.path(),
        )
        .expect("analysis should succeed");

        assert!(!findings.unwrap_or_default().is_empty());
    }

    #[test]
    fn ignores_when_address_validation_exists() {
        let tmp = tempdir().expect("tempdir");
        std::fs::write(
            tmp.path().join("validator.ak"),
            r#"use cardano/address.{Address, Script}

validator test {
  mint(_redeemer: Data, policy_id: PolicyId, self: Transaction) {
    expect [first_output, ..] = self.outputs
    expect Output {
      address: Address {
        payment_credential: Script(some_hash),
        stake_credential: None,
      },
      value: val,
      datum: NoDatum,
      reference_script: None,
    } = first_output

    and {
      quantity_of(val, policy_id, "token") == 1,
      some_hash == policy_id
    }
  }
}"#,
        )
        .expect("write source");

        let findings = collect_findings_for_skill(
            &skill("missing-address-validation-002"),
            &["validator.ak".to_string()],
            &ValidatorContextMap::default(),
            tmp.path(),
        )
        .expect("analysis should succeed");

        assert!(findings.unwrap_or_default().is_empty());
    }

    #[test]
    fn detects_unvalidated_datum() {
        let tmp = tempdir().expect("tempdir");
        std::fs::write(
            tmp.path().join("validator.ak"),
            r#"use cardano/transaction.{InlineDatum, Output}

validator test {
  spend(_datum, _redeemer, _utxo, transaction) {
    expect [script_output] = list.filter(
      transaction.outputs,
      fn(output) { output.address == script_address }
    )

    expect Output {
      address: o_address,
      value: _value,
      datum: InlineDatum(script_datum),
      reference_script: None,
    } = script_output

    o_address == script_address
  }
}"#,
        )
        .expect("write source");

        let findings = collect_findings_for_skill(
            &skill("unvalidated-datum-003"),
            &["validator.ak".to_string()],
            &ValidatorContextMap::default(),
            tmp.path(),
        )
        .expect("analysis should succeed");

        assert!(!findings.unwrap_or_default().is_empty());
    }

    #[test]
    fn ignores_when_datum_is_validated() {
        let tmp = tempdir().expect("tempdir");
        std::fs::write(
            tmp.path().join("validator.ak"),
            r#"use cardano/transaction.{InlineDatum, Output}

pub type Datum {
  owner: ByteArray,
}

validator test {
  spend(_datum, _redeemer, _utxo, transaction) {
    expect [script_output] = list.filter(
      transaction.outputs,
      fn(output) { output.address == script_address }
    )

    expect Output {
      address: o_address,
      value: _value,
      datum: InlineDatum(script_datum),
      reference_script: None,
    } = script_output

    expect expected_datum: Datum = script_datum
    expected_datum.owner == some_owner
    o_address == script_address
  }
}"#,
        )
        .expect("write source");

        let findings = collect_findings_for_skill(
            &skill("unvalidated-datum-003"),
            &["validator.ak".to_string()],
            &ValidatorContextMap::default(),
            tmp.path(),
        )
        .expect("analysis should succeed");

        assert!(findings.unwrap_or_default().is_empty());
    }
}
