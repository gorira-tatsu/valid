use crate::support::diagnostics::{Diagnostic, DiagnosticSegment, ErrorCode, Span};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedModel {
    pub model_name: String,
    pub state_fields: Vec<ParsedField>,
    pub init_assignments: Vec<ParsedAssignment>,
    pub actions: Vec<ParsedAction>,
    pub predicates: Vec<ParsedNamedExpr>,
    pub scenarios: Vec<ParsedNamedExpr>,
    pub properties: Vec<ParsedProperty>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedField {
    pub name: String,
    pub ty: String,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedAssignment {
    pub field: String,
    pub expr: String,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedAction {
    pub name: String,
    pub role: String,
    pub choices: Vec<ParsedChoice>,
    pub pre: Option<String>,
    pub posts: Vec<ParsedAssignment>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedChoice {
    pub name: String,
    pub values: Vec<String>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedProperty {
    pub name: String,
    pub layer: String,
    pub kind: String,
    pub expr: String,
    pub scope_expr: Option<String>,
    pub action_filter: Option<String>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedNamedExpr {
    pub name: String,
    pub expr: String,
    pub line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    None,
    State,
    Init,
    Predicates,
    Scenarios,
    ActionHeader,
    ActionPost,
    PropertyHeader,
}

pub fn parse_model(source: &str) -> Result<ParsedModel, Vec<Diagnostic>> {
    let mut model_name = None;
    let mut state_fields = Vec::new();
    let mut init_assignments = Vec::new();
    let mut actions = Vec::new();
    let mut predicates = Vec::new();
    let mut scenarios = Vec::new();
    let mut properties = Vec::new();
    let mut errors = Vec::new();

    let mut section = Section::None;
    let mut current_action: Option<ParsedAction> = None;

    for (index, raw_line) in source.lines().enumerate() {
        let line_no = index + 1;
        let line = raw_line.trim_end();
        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("model ") {
            model_name = Some(rest.trim().to_string());
            section = Section::None;
            continue;
        }

        if trimmed == "state:" {
            flush_action(&mut actions, &mut current_action);
            section = Section::State;
            continue;
        }

        if trimmed == "init:" {
            flush_action(&mut actions, &mut current_action);
            section = Section::Init;
            continue;
        }

        if trimmed == "predicates:" {
            flush_action(&mut actions, &mut current_action);
            section = Section::Predicates;
            continue;
        }

        if trimmed == "scenarios:" {
            flush_action(&mut actions, &mut current_action);
            section = Section::Scenarios;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("action ") {
            flush_action(&mut actions, &mut current_action);
            let name = rest.trim_end_matches(':').trim().to_string();
            current_action = Some(ParsedAction {
                name,
                role: "business".to_string(),
                choices: Vec::new(),
                pre: None,
                posts: Vec::new(),
                line: line_no,
            });
            section = Section::ActionHeader;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("property ") {
            start_property_declaration(
                &mut actions,
                &mut current_action,
                &mut properties,
                &mut errors,
                rest,
                "assert",
                line_no,
                &mut section,
            );
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("assume ") {
            start_property_declaration(
                &mut actions,
                &mut current_action,
                &mut properties,
                &mut errors,
                rest,
                "assume",
                line_no,
                &mut section,
            );
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("assert ") {
            start_property_declaration(
                &mut actions,
                &mut current_action,
                &mut properties,
                &mut errors,
                rest,
                "assert",
                line_no,
                &mut section,
            );
            continue;
        }

        match section {
            Section::State => {
                if let Some((name, ty)) = split_once(trimmed, ':') {
                    state_fields.push(ParsedField {
                        name: name.trim().to_string(),
                        ty: ty.trim().to_string(),
                        line: line_no,
                    });
                } else {
                    errors.push(parse_error("invalid state field declaration", line_no));
                }
            }
            Section::Init => {
                if let Some(assignment) = parse_assignment(trimmed, line_no) {
                    init_assignments.push(assignment);
                } else {
                    errors.push(parse_error("invalid init assignment", line_no));
                }
            }
            Section::Predicates => {
                if let Some((name, expr)) = split_once(trimmed, ':') {
                    predicates.push(ParsedNamedExpr {
                        name: name.trim().to_string(),
                        expr: expr.trim().to_string(),
                        line: line_no,
                    });
                } else {
                    errors.push(parse_error("invalid predicate declaration", line_no));
                }
            }
            Section::Scenarios => {
                if let Some((name, expr)) = split_once(trimmed, ':') {
                    scenarios.push(ParsedNamedExpr {
                        name: name.trim().to_string(),
                        expr: expr.trim().to_string(),
                        line: line_no,
                    });
                } else {
                    errors.push(parse_error("invalid scenario declaration", line_no));
                }
            }
            Section::ActionHeader | Section::ActionPost => {
                if let Some(action) = current_action.as_mut() {
                    if let Some(rest) = trimmed.strip_prefix("pre:") {
                        action.pre = Some(rest.trim().to_string());
                        section = Section::ActionHeader;
                    } else if let Some(rest) = trimmed.strip_prefix("choose ") {
                        if let Some((name, values)) = split_once(rest, ':') {
                            let parsed_values = values
                                .split(',')
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .map(ToString::to_string)
                                .collect::<Vec<_>>();
                            if parsed_values.is_empty() {
                                errors.push(parse_error(
                                    "bounded choice requires at least one value",
                                    line_no,
                                ));
                            } else {
                                action.choices.push(ParsedChoice {
                                    name: name.trim().to_string(),
                                    values: parsed_values,
                                    line: line_no,
                                });
                            }
                        } else {
                            errors.push(parse_error("invalid bounded choice declaration", line_no));
                        }
                    } else if let Some(rest) = trimmed.strip_prefix("role:") {
                        action.role = rest.trim().to_string();
                        section = Section::ActionHeader;
                    } else if trimmed == "post:" {
                        section = Section::ActionPost;
                    } else if section == Section::ActionPost {
                        if let Some(assignment) = parse_assignment(trimmed, line_no) {
                            action.posts.push(assignment);
                        } else {
                            errors.push(parse_error("invalid action post assignment", line_no));
                        }
                    } else {
                        errors.push(parse_error("unexpected token in action block", line_no));
                    }
                }
            }
            Section::PropertyHeader => {
                if let Some(property) = properties.last_mut() {
                    if let Some(expr) = trimmed.strip_prefix("when:") {
                        property.scope_expr = Some(expr.trim().to_string());
                    } else if let Some(action_id) = trimmed.strip_prefix("on:") {
                        property.action_filter = Some(action_id.trim().to_string());
                    } else if let Some((kind, expr)) = property_kind_and_expr(trimmed) {
                        property.kind = kind;
                        property.expr = expr;
                    } else {
                        errors.push(parse_error("invalid property definition", line_no));
                    }
                }
            }
            Section::None => {
                errors.push(parse_error("unexpected top-level token", line_no));
            }
        }
    }

    flush_action(&mut actions, &mut current_action);

    if model_name.is_none() {
        errors.push(parse_error("missing model declaration", 1));
    }

    if errors.is_empty() {
        Ok(ParsedModel {
            model_name: model_name.unwrap_or_default(),
            state_fields,
            init_assignments,
            actions,
            predicates,
            scenarios,
            properties,
        })
    } else {
        Err(errors)
    }
}

fn flush_action(actions: &mut Vec<ParsedAction>, current_action: &mut Option<ParsedAction>) {
    if let Some(action) = current_action.take() {
        actions.push(action);
    }
}

fn start_property_declaration(
    actions: &mut Vec<ParsedAction>,
    current_action: &mut Option<ParsedAction>,
    properties: &mut Vec<ParsedProperty>,
    errors: &mut Vec<Diagnostic>,
    rest: &str,
    layer: &str,
    line_no: usize,
    section: &mut Section,
) {
    flush_action(actions, current_action);
    if let Some((name, inline_definition)) = split_once(rest, ':') {
        let name = name.trim().to_string();
        if inline_definition.trim().is_empty() {
            properties.push(ParsedProperty {
                name,
                layer: layer.to_string(),
                kind: String::new(),
                expr: String::new(),
                scope_expr: None,
                action_filter: None,
                line: line_no,
            });
            *section = Section::PropertyHeader;
        } else if let Err(error) = parse_property_definition(&name, inline_definition, line_no) {
            errors.push(error);
            *section = Section::None;
        } else {
            let (kind, expr) = property_kind_and_expr(inline_definition).expect("property parsed");
            properties.push(ParsedProperty {
                name,
                layer: layer.to_string(),
                kind,
                expr,
                scope_expr: None,
                action_filter: None,
                line: line_no,
            });
            *section = Section::None;
        }
    } else {
        errors.push(parse_error("invalid property declaration", line_no));
        *section = Section::None;
    }
}

fn parse_assignment(input: &str, line: usize) -> Option<ParsedAssignment> {
    let (field, expr) = split_once(input, '=')?;
    Some(ParsedAssignment {
        field: field.trim().to_string(),
        expr: expr.trim().to_string(),
        line,
    })
}

fn split_once(input: &str, needle: char) -> Option<(&str, &str)> {
    let idx = input.find(needle)?;
    Some((&input[..idx], &input[idx + 1..]))
}

fn property_kind_and_expr(input: &str) -> Option<(String, String)> {
    if let Some((kind, expr)) = split_once(input, ':') {
        Some((kind.trim().to_string(), expr.trim().to_string()))
    } else {
        let kind = input.trim();
        if kind.is_empty() {
            None
        } else {
            Some((kind.to_string(), String::new()))
        }
    }
}

fn parse_property_definition(
    _name: &str,
    input: &str,
    line: usize,
) -> Result<(String, String), Diagnostic> {
    property_kind_and_expr(input).ok_or_else(|| parse_error("invalid property definition", line))
}

fn parse_error(message: &str, line: usize) -> Diagnostic {
    Diagnostic::new(
        ErrorCode::ParseError,
        DiagnosticSegment::FrontendParse,
        message,
    )
    .with_span(Span::new(line, 1))
    .with_help("check the surrounding block structure and separators")
    .with_best_practice("keep one declaration or assignment per line")
}

#[cfg(test)]
mod tests {
    use super::parse_model;

    #[test]
    fn parses_minimal_model() {
        let source = r#"
model CounterLock
state:
  x: u8[0..7]
  locked: bool
init:
  x = 0
  locked = false
action Inc:
  pre: !locked
  post:
    x = x + 1
property P_SAFE:
  invariant: x <= 7
"#;

        let parsed = parse_model(source).expect("should parse");
        assert_eq!(parsed.model_name, "CounterLock");
        assert_eq!(parsed.state_fields.len(), 2);
        assert_eq!(parsed.actions.len(), 1);
        assert_eq!(parsed.predicates.len(), 0);
        assert_eq!(parsed.scenarios.len(), 0);
        assert_eq!(parsed.properties.len(), 1);
    }

    #[test]
    fn parses_inline_deadlock_freedom_property() {
        let source = r#"
model CounterLock
state:
  x: u8[0..7]
init:
  x = 0
property P_LIVE: deadlock_freedom
"#;

        let parsed = parse_model(source).expect("should parse");
        assert_eq!(parsed.properties.len(), 1);
        assert_eq!(parsed.properties[0].name, "P_LIVE");
        assert_eq!(parsed.properties[0].kind, "deadlock_freedom");
        assert!(parsed.properties[0].expr.is_empty());
    }

    #[test]
    fn parses_predicates_scenarios_and_transition_property() {
        let source = r#"
model PostFlow
state:
  visible: bool
  deleted: bool
init:
  visible = true
  deleted = false
predicates:
  deleted_view: visible == false && deleted == true
scenarios:
  DeletedPost: deleted == true
action Delete:
  pre: visible == true
  post:
    visible = false
    deleted = true
        property P_DELETE_TRANSITION:
  transition: next.deleted == true && prev.visible == true
  on: Delete
  when: prev.visible == true
assume ENV_PRECONDITION:
  invariant: visible == true || deleted == false
"#;

        let parsed = parse_model(source).expect("should parse");
        assert_eq!(parsed.predicates[0].name, "deleted_view");
        assert_eq!(parsed.scenarios[0].name, "DeletedPost");
        assert_eq!(parsed.properties[0].kind, "transition");
        assert_eq!(
            parsed.properties[0].action_filter.as_deref(),
            Some("Delete")
        );
        assert_eq!(
            parsed.properties[0].scope_expr.as_deref(),
            Some("prev.visible == true")
        );
        assert_eq!(parsed.properties[1].layer, "assume");
    }

    #[test]
    fn parses_bounded_action_choices() {
        let source = r#"
model Counter
state:
  x: u8[0..2]
init:
  x = 0
action Add:
  choose delta: 1, 2
  pre: x + {{delta}} <= 2
  post:
    x = x + {{delta}}
"#;

        let parsed = parse_model(source).expect("parse");
        assert_eq!(parsed.actions.len(), 1);
        assert_eq!(parsed.actions[0].choices.len(), 1);
        assert_eq!(parsed.actions[0].choices[0].name, "delta");
        assert_eq!(parsed.actions[0].choices[0].values, vec!["1", "2"]);
    }
}
