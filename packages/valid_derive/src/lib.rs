use proc_macro::{Delimiter, Group, TokenStream, TokenTree};

#[proc_macro]
pub fn valid_model(input: TokenStream) -> TokenStream {
    let tokens = input.clone().into_iter().collect::<Vec<_>>();
    if let Err(message) = validate_valid_model_header(&tokens) {
        return compile_error_tokens(&message);
    }
    if let Err(message) = validate_valid_model_shape(&tokens) {
        return compile_error_tokens(&message);
    }
    wrap_valid_model_tokens(input)
}

#[proc_macro_derive(ValidState, attributes(valid))]
pub fn derive_valid_state(input: TokenStream) -> TokenStream {
    let parsed = parse_struct(input);
    let mut snapshot_entries = String::new();
    let mut descriptors = String::new();
    for field in &parsed.fields {
        if !snapshot_entries.is_empty() {
            snapshot_entries.push(',');
        }
        snapshot_entries.push_str(&format!(
            "(\"{name}\".to_string(), ::valid::modeling::IntoModelValue::into_model_value(self.{name}.clone()))",
            name = field.name
        ));
        if !descriptors.is_empty() {
            descriptors.push(',');
        }
        let range = field
            .range
            .as_ref()
            .map(|value| format!("Some({value:?})"))
            .unwrap_or_else(|| "None".to_string());
        let variants = if field.is_enum {
            format!(
                "Some(<{} as ::valid::modeling::FiniteValueSpec>::variant_labels().to_vec())",
                field.ty
            )
        } else if field.is_set {
            format!(
                "Some(<{} as ::valid::modeling::FiniteSetSpec>::variant_labels().to_vec())",
                field.ty
            )
        } else {
            "None".to_string()
        };
        descriptors.push_str(&format!(
            "::valid::modeling::StateFieldDescriptor {{ name: \"{name}\", rust_type: {ty:?}, range: {range}, variants: {variants}, is_set: {is_set} }}",
            name = field.name,
            ty = field.ty,
            range = range,
            variants = variants,
            is_set = field.is_set
        ));
    }
    format!(
        r#"
impl ::valid::modeling::ModelingState for {name} {{
    fn snapshot(&self) -> ::std::collections::BTreeMap<String, ::valid::ir::Value> {{
        ::std::collections::BTreeMap::from([{snapshot_entries}])
    }}
}}

impl ::valid::modeling::StateSpec for {name} {{
    fn state_fields() -> ::std::vec::Vec<::valid::modeling::StateFieldDescriptor> {{
        vec![{descriptors}]
    }}
}}
"#,
        name = parsed.name,
        snapshot_entries = snapshot_entries,
        descriptors = descriptors
    )
    .parse()
    .expect("ValidState derive must emit valid tokens")
}

#[proc_macro_derive(ValidEnum)]
pub fn derive_valid_enum(input: TokenStream) -> TokenStream {
    let parsed = parse_enum(input);
    let labels = parsed
        .variants
        .iter()
        .map(|variant| format!("{:?}", variant.name))
        .collect::<Vec<_>>()
        .join(",");
    let index_match = parsed
        .variants
        .iter()
        .enumerate()
        .map(|(index, variant)| format!("Self::{} => {}", variant.name, index))
        .collect::<Vec<_>>()
        .join(",");
    let label_match = parsed
        .variants
        .iter()
        .map(|variant| format!("Self::{} => {:?}", variant.name, variant.name))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"
impl ::valid::modeling::FiniteValueSpec for {name} {{
    fn variant_labels() -> &'static [&'static str] {{
        &[{labels}]
    }}

    fn variant_index(&self) -> u64 {{
        (match self {{
            {index_match}
        }}) as u64
    }}

    fn variant_label(&self) -> &'static str {{
        match self {{
            {label_match}
        }}
    }}
}}
"#,
        name = parsed.name,
        labels = labels,
        index_match = index_match,
        label_match = label_match,
    )
    .parse()
    .expect("ValidEnum derive must emit valid tokens")
}

#[proc_macro_derive(ValidAction, attributes(valid))]
pub fn derive_valid_action(input: TokenStream) -> TokenStream {
    let parsed = parse_enum(input);
    let all = parsed
        .variants
        .iter()
        .map(|variant| format!("Self::{}", variant.name))
        .collect::<Vec<_>>()
        .join(",");
    let action_id_match = parsed
        .variants
        .iter()
        .map(|variant| {
            format!(
                "Self::{name} => {action_id:?}.to_string()",
                name = variant.name,
                action_id = variant.action_id
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let descriptors = parsed
        .variants
        .iter()
        .map(|variant| {
            let reads = render_literal_slice(&variant.reads);
            let writes = render_literal_slice(&variant.writes);
            format!(
                "::valid::modeling::ActionDescriptor {{ variant: {variant:?}, action_id: {action_id:?}, reads: {reads}, writes: {writes} }}",
                variant = variant.name,
                action_id = variant.action_id,
                reads = reads,
                writes = writes
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"
impl ::valid::modeling::Finite for {name} {{
    fn all() -> ::std::vec::Vec<Self> {{
        vec![{all}]
    }}
}}

impl ::valid::modeling::ModelingAction for {name} {{
    fn action_id(&self) -> String {{
        match self {{
            {action_id_match}
        }}
    }}
}}

impl ::valid::modeling::ActionSpec for {name} {{
    fn action_descriptors() -> ::std::vec::Vec<::valid::modeling::ActionDescriptor> {{
        vec![{descriptors}]
    }}
}}
"#,
        name = parsed.name,
        all = all,
        action_id_match = action_id_match,
        descriptors = descriptors
    )
    .parse()
    .expect("ValidAction derive must emit valid tokens")
}

struct ParsedStruct {
    name: String,
    fields: Vec<ParsedField>,
}

struct ParsedField {
    name: String,
    ty: String,
    range: Option<String>,
    is_enum: bool,
    is_set: bool,
}

struct ParsedEnum {
    name: String,
    variants: Vec<ParsedVariant>,
}

struct ParsedVariant {
    name: String,
    action_id: String,
    reads: Vec<String>,
    writes: Vec<String>,
}

fn validate_valid_model_header(tokens: &[TokenTree]) -> Result<(), String> {
    let mut iter = tokens.iter();
    match iter.next() {
        Some(TokenTree::Ident(ident)) if ident.to_string() == "model" => {}
        _ => return Err("valid_model! expects a `model Name<State, Action>;` header".to_string()),
    }

    let Some(TokenTree::Ident(_)) = iter.next() else {
        return Err("valid_model! expected a model name after `model`".to_string());
    };

    let mut saw_generics = false;
    let mut depth = 0usize;
    let mut saw_header_end = false;
    for token in iter {
        match token {
            TokenTree::Punct(punct) if punct.as_char() == '<' => {
                saw_generics = true;
                depth += 1;
            }
            TokenTree::Punct(punct) if punct.as_char() == '>' => {
                depth = depth.saturating_sub(1);
            }
            TokenTree::Punct(punct) if punct.as_char() == ';' && depth == 0 => {
                saw_header_end = true;
                break;
            }
            _ => {}
        }
    }

    if !saw_header_end {
        return Err("valid_model! expected `;` after `model Name<State, Action>`".to_string());
    }
    if !saw_generics {
        return Err(
            "valid_model! requires explicit state/action types. Use `model Name<State, Action>;`."
                .to_string(),
        );
    }
    Ok(())
}

fn validate_valid_model_shape(tokens: &[TokenTree]) -> Result<(), String> {
    let mut top_level_idents = Vec::new();
    let mut after_header = false;
    let mut generic_depth = 0usize;
    for token in tokens {
        match token {
            TokenTree::Punct(punct) if punct.as_char() == '<' && !after_header => {
                generic_depth += 1;
            }
            TokenTree::Punct(punct) if punct.as_char() == '>' && !after_header => {
                generic_depth = generic_depth.saturating_sub(1);
            }
            TokenTree::Punct(punct)
                if punct.as_char() == ';' && !after_header && generic_depth == 0 =>
            {
                after_header = true;
            }
            TokenTree::Ident(ident) if after_header => {
                top_level_idents.push(ident.to_string());
            }
            _ => {}
        }
    }

    let count = |needle: &str| {
        top_level_idents
            .iter()
            .filter(|ident| ident.as_str() == needle)
            .count()
    };
    let init_count = count("init");
    let step_count = count("step");
    let transitions_count = count("transitions");
    let properties_count = count("properties");
    let legacy_property_count = count("property");
    let invariant_count = count_ident_recursive(tokens, "invariant");
    let transition_item_count = count_ident_recursive(tokens, "transition");
    let grouped_on_count = count_ident_recursive(tokens, "on");

    let has_init = init_count > 0;
    let has_step = step_count > 0;
    let has_transitions = transitions_count > 0;
    let has_properties = properties_count > 0;
    let has_legacy_property = legacy_property_count > 0;
    let has_legacy_invariant = invariant_count > 0;

    if !has_init {
        return Err("valid_model! requires `init [...]` after the model header".to_string());
    }
    if init_count > 1 {
        return Err("valid_model! supports exactly one `init [...]` section".to_string());
    }
    if has_step && has_transitions {
        return Err(
            "valid_model! must choose either `step |state, action| { ... }` or `transitions { ... }`, not both"
                .to_string(),
        );
    }
    if step_count > 1 {
        return Err(
            "valid_model! supports exactly one `step |state, action| { ... }` section".to_string(),
        );
    }
    if transitions_count > 1 {
        return Err("valid_model! supports exactly one `transitions { ... }` block".to_string());
    }
    if !has_step && !has_transitions {
        return Err(
            "valid_model! requires either `step |state, action| { ... }` or `transitions { ... }`"
                .to_string(),
        );
    }
    if has_transitions && transition_item_count == 0 && grouped_on_count == 0 {
        return Err(
            "valid_model! requires at least one `transition ...` or `on Action { ... }` entry inside `transitions { ... }`"
                .to_string(),
        );
    }
    if !has_properties && !(has_legacy_property && has_legacy_invariant) {
        return Err(
            "valid_model! requires `properties { ... }` or legacy `property ...; invariant ...;`"
                .to_string(),
        );
    }
    if properties_count > 1 {
        return Err("valid_model! supports exactly one `properties { ... }` block".to_string());
    }
    if has_properties && invariant_count == 0 {
        return Err(
            "valid_model! requires at least one `invariant ...;` inside `properties { ... }`"
                .to_string(),
        );
    }
    if has_properties && has_legacy_property {
        return Err("valid_model! cannot mix `properties { ... }` with legacy `property ...; invariant ...;` syntax".to_string());
    }
    Ok(())
}

fn count_ident_recursive(tokens: &[TokenTree], needle: &str) -> usize {
    let mut count = 0usize;
    for token in tokens {
        match token {
            TokenTree::Ident(ident) if ident.to_string() == needle => {
                count += 1;
            }
            TokenTree::Group(group) => {
                let nested = group.stream().into_iter().collect::<Vec<_>>();
                count += count_ident_recursive(&nested, needle);
            }
            _ => {}
        }
    }
    count
}

fn wrap_valid_model_tokens(input: TokenStream) -> TokenStream {
    let mut output = "::valid::__valid_model_internal!"
        .parse::<TokenStream>()
        .expect("valid_model proc-macro prelude should parse");
    output.extend([TokenTree::Group(Group::new(Delimiter::Brace, input))]);
    output
}

fn compile_error_tokens(message: &str) -> TokenStream {
    format!("compile_error!({message:?});")
        .parse()
        .expect("compile_error! token emission must succeed")
}

fn parse_struct(input: TokenStream) -> ParsedStruct {
    let tokens = input.into_iter().collect::<Vec<_>>();
    let mut iter = tokens.iter();
    let mut name = None;
    let mut body = None;
    while let Some(token) = iter.next() {
        match token {
            TokenTree::Ident(ident) if ident.to_string() == "struct" => {
                name = iter.next().and_then(as_ident_name);
                body = iter.next().and_then(as_group);
                break;
            }
            _ => {}
        }
    }
    let name = name.expect("ValidState requires a struct");
    let body = body.expect("ValidState requires named fields");
    let fields = split_comma_tokens(body.stream())
        .into_iter()
        .filter(|entry| !entry.is_empty())
        .map(parse_struct_field)
        .collect();
    ParsedStruct { name, fields }
}

fn parse_struct_field(tokens: Vec<TokenTree>) -> ParsedField {
    let attrs = collect_valid_attrs(&tokens);
    let filtered = strip_attributes(tokens);
    let colon = filtered
        .iter()
        .position(|token| matches!(token, TokenTree::Punct(p) if p.as_char() == ':'))
        .expect("field must contain `:`");
    let name = filtered[..colon]
        .iter()
        .rev()
        .find_map(as_ident_name)
        .expect("field must have a name");
    let ty = normalize_type_tokens(&filtered[colon + 1..]);
    ParsedField {
        name,
        ty,
        range: attrs.get("range").cloned(),
        is_enum: attrs.contains_key("enum"),
        is_set: attrs.contains_key("set"),
    }
}

fn normalize_type_tokens(tokens: &[TokenTree]) -> String {
    tokens
        .iter()
        .map(TokenTree::to_string)
        .collect::<Vec<_>>()
        .join(" ")
        .replace(" < ", "<")
        .replace("< ", "<")
        .replace(" >", ">")
        .replace(" , ", ", ")
}

fn parse_enum(input: TokenStream) -> ParsedEnum {
    let tokens = input.into_iter().collect::<Vec<_>>();
    let mut iter = tokens.iter();
    let mut name = None;
    let mut body = None;
    while let Some(token) = iter.next() {
        match token {
            TokenTree::Ident(ident) if ident.to_string() == "enum" => {
                name = iter.next().and_then(as_ident_name);
                body = iter.next().and_then(as_group);
                break;
            }
            _ => {}
        }
    }
    let name = name.expect("ValidAction requires an enum");
    let body = body.expect("ValidAction requires enum body");
    let variants = split_comma_tokens(body.stream())
        .into_iter()
        .filter(|entry| !entry.is_empty())
        .map(parse_enum_variant)
        .collect();
    ParsedEnum { name, variants }
}

fn parse_enum_variant(tokens: Vec<TokenTree>) -> ParsedVariant {
    let attrs = collect_valid_attrs(&tokens);
    let filtered = strip_attributes(tokens);
    let name = filtered
        .iter()
        .find_map(as_ident_name)
        .expect("variant must have a name");
    ParsedVariant {
        action_id: attrs
            .get("action_id")
            .cloned()
            .unwrap_or_else(|| name.clone()),
        reads: parse_array_values(attrs.get("reads")),
        writes: parse_array_values(attrs.get("writes")),
        name,
    }
}

fn split_comma_tokens(stream: TokenStream) -> Vec<Vec<TokenTree>> {
    let mut entries = Vec::new();
    let mut current = Vec::new();
    for token in stream {
        match &token {
            TokenTree::Punct(p) if p.as_char() == ',' => {
                entries.push(current);
                current = Vec::new();
            }
            _ => current.push(token),
        }
    }
    if !current.is_empty() {
        entries.push(current);
    }
    entries
}

fn collect_valid_attrs(tokens: &[TokenTree]) -> std::collections::BTreeMap<String, String> {
    let mut values = std::collections::BTreeMap::new();
    let mut index = 0;
    while index + 1 < tokens.len() {
        if matches!(&tokens[index], TokenTree::Punct(p) if p.as_char() == '#') {
            if let TokenTree::Group(group) = &tokens[index + 1] {
                if group.delimiter() == Delimiter::Bracket {
                    parse_valid_attribute_group(group, &mut values);
                }
            }
            index += 2;
        } else {
            index += 1;
        }
    }
    values
}

fn parse_valid_attribute_group(
    group: &Group,
    values: &mut std::collections::BTreeMap<String, String>,
) {
    let mut iter = group.stream().into_iter();
    let Some(TokenTree::Ident(ident)) = iter.next() else {
        return;
    };
    if ident.to_string() != "valid" {
        return;
    }
    let Some(TokenTree::Group(args)) = iter.next() else {
        return;
    };
    for entry in split_comma_tokens(args.stream()) {
        let eq = entry
            .iter()
            .position(|token| matches!(token, TokenTree::Punct(p) if p.as_char() == '='));
        let Some(eq) = eq else {
            if let Some(key) = entry.iter().find_map(as_ident_name) {
                values.insert(key, "true".to_string());
            }
            continue;
        };
        let key = entry[..eq]
            .iter()
            .find_map(as_ident_name)
            .expect("valid attribute key");
        let value = entry[eq + 1..]
            .iter()
            .map(TokenTree::to_string)
            .collect::<Vec<_>>()
            .join(" ");
        values.insert(key, trim_quotes(&value));
    }
}

fn strip_attributes(tokens: Vec<TokenTree>) -> Vec<TokenTree> {
    let mut filtered = Vec::new();
    let mut skip_next_group = false;
    for token in tokens {
        if skip_next_group {
            skip_next_group = false;
            continue;
        }
        match &token {
            TokenTree::Punct(p) if p.as_char() == '#' => {
                skip_next_group = true;
            }
            _ => filtered.push(token),
        }
    }
    filtered
}

fn parse_array_values(value: Option<&String>) -> Vec<String> {
    let Some(value) = value else {
        return Vec::new();
    };
    let trimmed = value.trim();
    let inner = trimmed
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(trimmed);
    inner
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(trim_quotes)
        .collect()
}

fn render_literal_slice(values: &[String]) -> String {
    let body = values
        .iter()
        .map(|value| format!("{value:?}"))
        .collect::<Vec<_>>()
        .join(",");
    format!("&[{body}]")
}

fn trim_quotes(value: &str) -> String {
    value.trim().trim_matches('"').to_string()
}

fn as_ident_name(token: &TokenTree) -> Option<String> {
    match token {
        TokenTree::Ident(ident) => Some(ident.to_string()),
        _ => None,
    }
}

fn as_group(token: &TokenTree) -> Option<Group> {
    match token {
        TokenTree::Group(group) => Some(group.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{validate_valid_model_header, validate_valid_model_shape};

    #[test]
    fn valid_model_header_requires_explicit_state_and_action_types() {
        let tokens =
            "model CounterModel<State, Action>; init []; properties { invariant P |state| true; }"
                .parse()
                .unwrap();
        assert!(validate_valid_model_header(&tokens.into_iter().collect::<Vec<_>>()).is_ok());
    }

    #[test]
    fn valid_model_header_rejects_shorthand_form() {
        let tokens = "model CounterModel; init []; properties { invariant P |state| true; }"
            .parse()
            .unwrap();
        let error = validate_valid_model_header(&tokens.into_iter().collect::<Vec<_>>())
            .expect_err("shorthand form should be rejected");
        assert!(error.contains("explicit state/action types"));
    }

    #[test]
    fn valid_model_shape_requires_init_and_properties() {
        let tokens = "model CounterModel<State, Action>; step |state, action| { Vec::new() }"
            .parse()
            .unwrap();
        let error = validate_valid_model_shape(&tokens.into_iter().collect::<Vec<_>>())
            .expect_err("missing init/properties should be rejected");
        assert!(error.contains("init ["));
    }

    #[test]
    fn valid_model_shape_rejects_step_and_transitions_together() {
        let tokens = "model CounterModel<State, Action>; init []; step |state, action| { Vec::new() } transitions { transition Go when |state| true => []; } properties { invariant P |state| true; }"
            .parse()
            .unwrap();
        let error = validate_valid_model_shape(&tokens.into_iter().collect::<Vec<_>>())
            .expect_err("step and transitions should not coexist");
        assert!(error.contains("not both"));
    }

    #[test]
    fn valid_model_shape_accepts_transitions_and_properties() {
        let tokens = "model CounterModel<State, Action>; init []; transitions { transition Go when |state| true => []; } properties { invariant P |state| true; }"
            .parse()
            .unwrap();
        assert!(validate_valid_model_shape(&tokens.into_iter().collect::<Vec<_>>()).is_ok());
    }

    #[test]
    fn valid_model_shape_accepts_grouped_on_transitions() {
        let tokens = "model CounterModel<State, Action>; init []; transitions { on Go { when |state| true => []; } } properties { invariant P |state| true; }"
            .parse()
            .unwrap();
        assert!(validate_valid_model_shape(&tokens.into_iter().collect::<Vec<_>>()).is_ok());
    }

    #[test]
    fn valid_model_shape_requires_transition_entries() {
        let tokens = "model CounterModel<State, Action>; init []; transitions { } properties { invariant P |state| true; }"
            .parse()
            .unwrap();
        let error = validate_valid_model_shape(&tokens.into_iter().collect::<Vec<_>>())
            .expect_err("empty transitions block should be rejected");
        assert!(error.contains("at least one `transition"));
    }

    #[test]
    fn valid_model_shape_rejects_mixed_property_styles() {
        let tokens = "model CounterModel<State, Action>; init []; transitions { transition Go when |state| true => []; } property P; properties { invariant Q |state| true; } invariant |state| true;"
            .parse()
            .unwrap();
        let error = validate_valid_model_shape(&tokens.into_iter().collect::<Vec<_>>())
            .expect_err("mixed property styles should be rejected");
        assert!(error.contains("cannot mix"));
    }
}
