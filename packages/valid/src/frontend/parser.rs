use crate::support::diagnostics::{Diagnostic, DiagnosticSegment, ErrorCode, Span};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedModel {
    pub model_name: String,
    pub state_fields: Vec<ParsedField>,
    pub init_assignments: Vec<ParsedAssignment>,
    pub actions: Vec<ParsedAction>,
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
    pub pre: Option<String>,
    pub posts: Vec<ParsedAssignment>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedProperty {
    pub name: String,
    pub kind: String,
    pub expr: String,
    pub line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    None,
    State,
    Init,
    ActionHeader,
    ActionPost,
    PropertyHeader,
}

pub fn parse_model(source: &str) -> Result<ParsedModel, Vec<Diagnostic>> {
    let mut model_name = None;
    let mut state_fields = Vec::new();
    let mut init_assignments = Vec::new();
    let mut actions = Vec::new();
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

        if let Some(rest) = trimmed.strip_prefix("action ") {
            flush_action(&mut actions, &mut current_action);
            let name = rest.trim_end_matches(':').trim().to_string();
            current_action = Some(ParsedAction {
                name,
                pre: None,
                posts: Vec::new(),
                line: line_no,
            });
            section = Section::ActionHeader;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("property ") {
            flush_action(&mut actions, &mut current_action);
            if let Some((name, inline_definition)) = split_once(rest, ':') {
                let name = name.trim().to_string();
                if inline_definition.trim().is_empty() {
                    properties.push(ParsedProperty {
                        name,
                        kind: String::new(),
                        expr: String::new(),
                        line: line_no,
                    });
                    section = Section::PropertyHeader;
                } else if let Err(error) =
                    parse_property_definition(&name, inline_definition, line_no)
                {
                    errors.push(error);
                    section = Section::None;
                } else {
                    let (kind, expr) =
                        property_kind_and_expr(inline_definition).expect("property parsed");
                    properties.push(ParsedProperty {
                        name,
                        kind,
                        expr,
                        line: line_no,
                    });
                    section = Section::None;
                }
            } else {
                errors.push(parse_error("invalid property declaration", line_no));
                section = Section::None;
            }
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
            Section::ActionHeader | Section::ActionPost => {
                if let Some(action) = current_action.as_mut() {
                    if let Some(rest) = trimmed.strip_prefix("pre:") {
                        action.pre = Some(rest.trim().to_string());
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
                    if let Some((kind, expr)) = property_kind_and_expr(trimmed) {
                        property.kind = kind;
                        property.expr = expr;
                    } else {
                        errors.push(parse_error("invalid property definition", line_no));
                    }
                }
                section = Section::None;
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
}
