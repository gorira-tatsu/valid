use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    Object(BTreeMap<String, JsonValue>),
    Array(Vec<JsonValue>),
    String(String),
    Number(u64),
    Bool(bool),
    Null,
}

pub fn parse_json(body: &str) -> Result<JsonValue, String> {
    let mut parser = Parser::new(body);
    let value = parser.parse_value()?;
    parser.skip_ws();
    if !parser.is_eof() {
        return Err("trailing characters after json value".to_string());
    }
    Ok(value)
}

pub fn require_object<'a>(
    value: &'a JsonValue,
    name: &str,
) -> Result<&'a BTreeMap<String, JsonValue>, String> {
    match value {
        JsonValue::Object(map) => Ok(map),
        _ => Err(format!("{name} must be a JSON object")),
    }
}

pub fn require_string_field<'a>(
    object: &'a BTreeMap<String, JsonValue>,
    field: &str,
) -> Result<&'a str, String> {
    match object.get(field) {
        Some(JsonValue::String(value)) if !value.is_empty() => Ok(value.as_str()),
        _ => Err(format!("{field} must be a non-empty string")),
    }
}

pub fn require_array_field<'a>(
    object: &'a BTreeMap<String, JsonValue>,
    field: &str,
) -> Result<&'a Vec<JsonValue>, String> {
    match object.get(field) {
        Some(JsonValue::Array(items)) => Ok(items),
        _ => Err(format!("{field} must be an array")),
    }
}

pub fn require_number_field(
    object: &BTreeMap<String, JsonValue>,
    field: &str,
) -> Result<(), String> {
    match object.get(field) {
        Some(JsonValue::Number(_)) => Ok(()),
        _ => Err(format!("{field} must be an unsigned integer")),
    }
}

pub fn require_bool_field(object: &BTreeMap<String, JsonValue>, field: &str) -> Result<(), String> {
    match object.get(field) {
        Some(JsonValue::Bool(_)) => Ok(()),
        _ => Err(format!("{field} must be a boolean")),
    }
}

struct Parser<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            pos: 0,
        }
    }

    fn parse_value(&mut self) -> Result<JsonValue, String> {
        self.skip_ws();
        match self.peek() {
            Some(b'{') => self.parse_object(),
            Some(b'[') => self.parse_array(),
            Some(b'"') => self.parse_string().map(JsonValue::String),
            Some(b't') | Some(b'f') => self.parse_bool().map(JsonValue::Bool),
            Some(b'n') => {
                self.expect_bytes(b"null")?;
                Ok(JsonValue::Null)
            }
            Some(b'0'..=b'9') => self.parse_number().map(JsonValue::Number),
            Some(other) => Err(format!("unexpected character `{}`", other as char)),
            None => Err("unexpected end of input".to_string()),
        }
    }

    fn parse_object(&mut self) -> Result<JsonValue, String> {
        self.expect(b'{')?;
        let mut map = BTreeMap::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(JsonValue::Object(map));
        }
        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            self.skip_ws();
            self.expect(b':')?;
            let value = self.parse_value()?;
            map.insert(key, value);
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    break;
                }
                _ => return Err("object must end with `}` or continue with `,`".to_string()),
            }
        }
        Ok(JsonValue::Object(map))
    }

    fn parse_array(&mut self) -> Result<JsonValue, String> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(JsonValue::Array(items));
        }
        loop {
            let value = self.parse_value()?;
            items.push(value);
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    break;
                }
                _ => return Err("array must end with `]` or continue with `,`".to_string()),
            }
        }
        Ok(JsonValue::Array(items))
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = String::new();
        while let Some(byte) = self.peek() {
            self.pos += 1;
            match byte {
                b'"' => return Ok(out),
                b'\\' => {
                    let escaped = self
                        .peek()
                        .ok_or_else(|| "unterminated escape sequence".to_string())?;
                    self.pos += 1;
                    match escaped {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{0008}'),
                        b'f' => out.push('\u{000C}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        _ => return Err("unsupported escape sequence".to_string()),
                    }
                }
                _ => out.push(byte as char),
            }
        }
        Err("unterminated string".to_string())
    }

    fn parse_number(&mut self) -> Result<u64, String> {
        let start = self.pos;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        let text = std::str::from_utf8(&self.input[start..self.pos])
            .map_err(|_| "invalid utf8 in number".to_string())?;
        text.parse::<u64>()
            .map_err(|_| format!("invalid unsigned integer `{text}`"))
    }

    fn parse_bool(&mut self) -> Result<bool, String> {
        if self.remaining_starts_with(b"true") {
            self.pos += 4;
            Ok(true)
        } else if self.remaining_starts_with(b"false") {
            self.pos += 5;
            Ok(false)
        } else {
            Err("invalid boolean literal".to_string())
        }
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), String> {
        match self.peek() {
            Some(current) if current == byte => {
                self.pos += 1;
                Ok(())
            }
            Some(current) => Err(format!(
                "expected `{}` but found `{}`",
                byte as char, current as char
            )),
            None => Err(format!(
                "expected `{}` but reached end of input",
                byte as char
            )),
        }
    }

    fn expect_bytes(&mut self, expected: &[u8]) -> Result<(), String> {
        if self.remaining_starts_with(expected) {
            self.pos += expected.len();
            Ok(())
        } else {
            Err("unexpected token".to_string())
        }
    }

    fn remaining_starts_with(&self, expected: &[u8]) -> bool {
        self.input.get(self.pos..self.pos + expected.len()) == Some(expected)
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.input.len()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        parse_json, require_array_field, require_bool_field, require_number_field, require_object,
        require_string_field, JsonValue,
    };

    #[test]
    fn parses_nested_json() {
        let value = parse_json("{\"a\":1,\"b\":[true,{\"c\":\"x\"}]}").unwrap();
        let object = require_object(&value, "root").unwrap();
        require_number_field(object, "a").unwrap();
        let array = require_array_field(object, "b").unwrap();
        assert_eq!(array.len(), 2);
        match &array[0] {
            JsonValue::Bool(true) => {}
            _ => panic!("expected bool"),
        }
        let nested = require_object(&array[1], "nested").unwrap();
        assert_eq!(require_string_field(nested, "c").unwrap(), "x");
    }

    #[test]
    fn rejects_bad_bool_field() {
        let value = parse_json("{\"ok\":1}").unwrap();
        let object = require_object(&value, "root").unwrap();
        assert!(require_bool_field(object, "ok").is_err());
    }
}
