//! A minimal, std-only JSON implementation: a recursive-descent parser and a
//! pretty-printing serializer. Used by `export`/`import` to interoperate with
//! GitHub Issues JSON. Per ADR 0002 the project depends on no external crates,
//! so this is hand-rolled rather than using `serde_json`.

use std::str::FromStr;

/// A parsed JSON value. Objects preserve insertion order (a `Vec` of pairs)
/// rather than using a map, so serialization is deterministic and key order is
/// retained.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    /// Borrows the inner string, or `None` for non-strings.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }

    /// Borrows the inner array, or `None` for non-arrays.
    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Arr(a) => Some(a),
            _ => None,
        }
    }

    /// Returns the number as an `i64` when it is integral and in range.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Json::Num(n) if n.fract() == 0.0 && *n >= i64::MIN as f64 && *n <= i64::MAX as f64 => {
                Some(*n as i64)
            }
            _ => None,
        }
    }

    /// Returns the inner boolean, or `None` for non-booleans. Part of the
    /// accessor API (used by tests and available to callers); the binary's
    /// command paths happen not to need booleans today.
    #[allow(dead_code)]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Looks up `key` in an object value. Returns `None` for non-objects or a
    /// missing key. Callers needing case- or alias-insensitive lookup try
    /// several keys in turn (e.g. `created_at` then `createdAt`).
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Parser (recursive descent over UTF-8 bytes)
// ---------------------------------------------------------------------------

/// Parses a complete JSON document. Leading/trailing whitespace is allowed;
/// any non-whitespace after the top-level value is an error. On malformed
/// input returns `Err` with a human-readable message including a byte offset.
pub fn parse(input: &str) -> Result<Json, String> {
    let mut p = Parser {
        bytes: input.as_bytes(),
        pos: 0,
    };
    p.skip_ws();
    let value = p.parse_value()?;
    p.skip_ws();
    if p.pos != p.bytes.len() {
        return Err(format!("trailing characters at byte {}", p.pos));
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn skip_ws(&mut self) {
        while let Some(&b) = self.bytes.get(self.pos) {
            if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn parse_value(&mut self) -> Result<Json, String> {
        match self.peek() {
            Some(b'{') => self.parse_object(),
            Some(b'[') => self.parse_array(),
            Some(b'"') => self.parse_string().map(Json::Str),
            Some(b't') | Some(b'f') => self.parse_bool(),
            Some(b'n') => self.parse_null(),
            Some(c) if c == b'-' || c.is_ascii_digit() => self.parse_number(),
            Some(c) => Err(format!("unexpected character '{}' at byte {}", c as char, self.pos)),
            None => Err("unexpected end of input".to_string()),
        }
    }

    fn parse_object(&mut self) -> Result<Json, String> {
        self.pos += 1; // consume '{'
        let mut pairs = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Json::Obj(pairs));
        }
        loop {
            self.skip_ws();
            if self.peek() != Some(b'"') {
                return Err(format!("expected object key string at byte {}", self.pos));
            }
            let key = self.parse_string()?;
            self.skip_ws();
            if self.peek() != Some(b':') {
                return Err(format!("expected ':' at byte {}", self.pos));
            }
            self.pos += 1; // consume ':'
            self.skip_ws();
            let value = self.parse_value()?;
            pairs.push((key, value));
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Json::Obj(pairs));
                }
                _ => return Err(format!("expected ',' or '}}' at byte {}", self.pos)),
            }
        }
    }

    fn parse_array(&mut self) -> Result<Json, String> {
        self.pos += 1; // consume '['
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Json::Arr(items));
        }
        loop {
            self.skip_ws();
            let value = self.parse_value()?;
            items.push(value);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Json::Arr(items));
                }
                _ => return Err(format!("expected ',' or ']' at byte {}", self.pos)),
            }
        }
    }

    /// Parses a string literal (the opening `"` is at the current position),
    /// decoding escapes. String contents are built into a `Vec<u8>`: raw bytes
    /// (including multibyte UTF-8) are pushed directly, and `\u` escapes are
    /// decoded to chars and pushed as their UTF-8 encoding.
    fn parse_string(&mut self) -> Result<String, String> {
        let start = self.pos;
        self.pos += 1; // consume opening '"'
        let mut out: Vec<u8> = Vec::new();
        loop {
            let Some(b) = self.peek() else {
                return Err(format!("unterminated string starting at byte {start}"));
            };
            match b {
                b'"' => {
                    self.pos += 1;
                    return String::from_utf8(out)
                        .map_err(|_| format!("invalid UTF-8 in string at byte {start}"));
                }
                b'\\' => {
                    self.pos += 1;
                    let Some(esc) = self.peek() else {
                        return Err(format!("unterminated escape at byte {}", self.pos));
                    };
                    match esc {
                        b'"' => out.push(b'"'),
                        b'\\' => out.push(b'\\'),
                        b'/' => out.push(b'/'),
                        b'b' => out.push(0x08),
                        b'f' => out.push(0x0c),
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'u' => {
                            self.pos += 1; // consume 'u' (loop tail re-adds 1, so step back)
                            let ch = self.parse_unicode_escape()?;
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                            // parse_unicode_escape left pos at the last hex digit;
                            // fall through so the `self.pos += 1` below advances past it.
                            self.pos -= 1;
                        }
                        other => {
                            return Err(format!(
                                "invalid escape '\\{}' at byte {}",
                                other as char, self.pos
                            ))
                        }
                    }
                    self.pos += 1;
                }
                _ => {
                    out.push(b);
                    self.pos += 1;
                }
            }
        }
    }

    /// Decodes a `\uXXXX` escape (the `u` has just been consumed, so the four
    /// hex digits start at the current position). Handles surrogate pairs; a
    /// lone surrogate decodes to U+FFFD. On return, `self.pos` points at the
    /// last consumed hex digit.
    fn parse_unicode_escape(&mut self) -> Result<char, String> {
        let hi = self.read_hex4()?;
        if (0xD800..=0xDBFF).contains(&hi) {
            // High surrogate: expect a following \uXXXX low surrogate.
            if self.bytes.get(self.pos) == Some(&b'\\')
                && self.bytes.get(self.pos + 1) == Some(&b'u')
            {
                self.pos += 2; // consume "\u"
                let lo = self.read_hex4()?;
                if (0xDC00..=0xDFFF).contains(&lo) {
                    let c = 0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
                    return Ok(char::from_u32(c).unwrap_or('\u{FFFD}'));
                }
                // Not a valid low surrogate: lone high surrogate.
                return Ok('\u{FFFD}');
            }
            return Ok('\u{FFFD}');
        }
        if (0xDC00..=0xDFFF).contains(&hi) {
            // Lone low surrogate.
            return Ok('\u{FFFD}');
        }
        Ok(char::from_u32(hi).unwrap_or('\u{FFFD}'))
    }

    /// Reads exactly four hex digits and returns their value. On return,
    /// `self.pos` points just past the four digits.
    fn read_hex4(&mut self) -> Result<u32, String> {
        let mut val: u32 = 0;
        for _ in 0..4 {
            let Some(&b) = self.bytes.get(self.pos) else {
                return Err(format!("truncated \\u escape at byte {}", self.pos));
            };
            let digit = match b {
                b'0'..=b'9' => (b - b'0') as u32,
                b'a'..=b'f' => (b - b'a' + 10) as u32,
                b'A'..=b'F' => (b - b'A' + 10) as u32,
                _ => return Err(format!("invalid hex digit at byte {}", self.pos)),
            };
            val = val * 16 + digit;
            self.pos += 1;
        }
        Ok(val)
    }

    fn parse_number(&mut self) -> Result<Json, String> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while let Some(b) = self.peek() {
            if b.is_ascii_digit() || b == b'.' || b == b'e' || b == b'E' || b == b'+' || b == b'-' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let slice = &self.bytes[start..self.pos];
        let s = std::str::from_utf8(slice)
            .map_err(|_| format!("invalid number at byte {start}"))?;
        f64::from_str(s)
            .map(Json::Num)
            .map_err(|_| format!("invalid number '{s}' at byte {start}"))
    }

    fn parse_bool(&mut self) -> Result<Json, String> {
        if self.bytes[self.pos..].starts_with(b"true") {
            self.pos += 4;
            Ok(Json::Bool(true))
        } else if self.bytes[self.pos..].starts_with(b"false") {
            self.pos += 5;
            Ok(Json::Bool(false))
        } else {
            Err(format!("invalid literal at byte {}", self.pos))
        }
    }

    fn parse_null(&mut self) -> Result<Json, String> {
        if self.bytes[self.pos..].starts_with(b"null") {
            self.pos += 4;
            Ok(Json::Null)
        } else {
            Err(format!("invalid literal at byte {}", self.pos))
        }
    }
}

// ---------------------------------------------------------------------------
// Serializer
// ---------------------------------------------------------------------------

/// Escapes a string per the JSON spec and wraps it in double quotes is NOT
/// done here — this returns only the inner escaped contents (no surrounding
/// quotes), so callers can place it inside their own quoting.
pub fn escape_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Serializes a value as 2-space-indented (pretty) JSON.
pub fn to_pretty(value: &Json) -> String {
    let mut out = String::new();
    write_pretty(value, 0, &mut out);
    out
}

fn write_indent(level: usize, out: &mut String) {
    for _ in 0..level {
        out.push_str("  ");
    }
}

fn write_pretty(value: &Json, level: usize, out: &mut String) {
    match value {
        Json::Null => out.push_str("null"),
        Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Json::Num(n) => out.push_str(&format_number(*n)),
        Json::Str(s) => {
            out.push('"');
            out.push_str(&escape_str(s));
            out.push('"');
        }
        Json::Arr(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push_str("[\n");
            for (i, item) in items.iter().enumerate() {
                write_indent(level + 1, out);
                write_pretty(item, level + 1, out);
                if i + 1 < items.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            write_indent(level, out);
            out.push(']');
        }
        Json::Obj(pairs) => {
            if pairs.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push_str("{\n");
            for (i, (k, v)) in pairs.iter().enumerate() {
                write_indent(level + 1, out);
                out.push('"');
                out.push_str(&escape_str(k));
                out.push_str("\": ");
                write_pretty(v, level + 1, out);
                if i + 1 < pairs.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            write_indent(level, out);
            out.push('}');
        }
    }
}

/// Formats a number, rendering integral values without a trailing `.0`.
fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.is_finite() && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_primitives() {
        assert_eq!(parse("true").unwrap(), Json::Bool(true));
        assert_eq!(parse("false").unwrap(), Json::Bool(false));
        assert_eq!(parse("null").unwrap(), Json::Null);
        assert_eq!(parse("42").unwrap(), Json::Num(42.0));
        assert_eq!(parse("-3.5").unwrap(), Json::Num(-3.5));
        assert_eq!(parse("1e3").unwrap(), Json::Num(1000.0));
        assert_eq!(parse("  \"hi\"  ").unwrap(), Json::Str("hi".to_string()));
    }

    #[test]
    fn parse_object_and_array() {
        let v = parse(r#"{"a": 1, "b": [true, null, "x"]}"#).unwrap();
        assert_eq!(v.get("a").unwrap().as_i64(), Some(1));
        let arr = v.get("b").unwrap().as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0].as_bool(), Some(true));
        assert_eq!(arr[1], Json::Null);
        assert_eq!(arr[2].as_str(), Some("x"));
    }

    #[test]
    fn parse_nested() {
        let v = parse(r#"{"outer": {"inner": [{"k": "v"}]}}"#).unwrap();
        let inner = v.get("outer").unwrap().get("inner").unwrap().as_array().unwrap();
        assert_eq!(inner[0].get("k").unwrap().as_str(), Some("v"));
    }

    #[test]
    fn parse_escapes() {
        let v = parse(r#""a\"b\\c\/d\n\t\r\b\f""#).unwrap();
        assert_eq!(v.as_str(), Some("a\"b\\c/d\n\t\r\u{08}\u{0c}"));
    }

    #[test]
    fn parse_unicode_bmp() {
        let v = parse(r#""é""#).unwrap();
        assert_eq!(v.as_str(), Some("é"));
    }

    #[test]
    fn parse_unicode_surrogate_pair() {
        // U+1F600 GRINNING FACE encoded as a surrogate pair.
        let v = parse(r#""😀""#).unwrap();
        assert_eq!(v.as_str(), Some("\u{1F600}"));
    }

    #[test]
    fn parse_lone_surrogate_is_replacement() {
        let v = parse(r#""\uD83D""#).unwrap();
        assert_eq!(v.as_str(), Some("\u{FFFD}"));
    }

    #[test]
    fn parse_multibyte_raw() {
        let v = parse("\"caf\u{e9} \u{1f600}\"").unwrap();
        assert_eq!(v.as_str(), Some("café \u{1f600}"));
    }

    #[test]
    fn parse_empty_containers() {
        assert_eq!(parse("[]").unwrap(), Json::Arr(vec![]));
        assert_eq!(parse("{}").unwrap(), Json::Obj(vec![]));
    }

    #[test]
    fn parse_error_unterminated_string() {
        assert!(parse(r#""abc"#).is_err());
    }

    #[test]
    fn parse_error_trailing_junk() {
        let e = parse("42 garbage").unwrap_err();
        assert!(e.contains("trailing"));
    }

    #[test]
    fn parse_error_empty() {
        assert!(parse("   ").is_err());
    }

    #[test]
    fn as_i64_rejects_fractional() {
        assert_eq!(Json::Num(1.5).as_i64(), None);
        assert_eq!(Json::Num(7.0).as_i64(), Some(7));
    }

    #[test]
    fn get_on_non_object_is_none() {
        assert!(Json::Num(1.0).get("k").is_none());
        assert!(Json::Arr(vec![]).get("k").is_none());
    }

    #[test]
    fn to_pretty_basic_shape() {
        let v = Json::Obj(vec![
            ("n".to_string(), Json::Num(1.0)),
            ("a".to_string(), Json::Arr(vec![Json::Str("x".to_string())])),
        ]);
        let s = to_pretty(&v);
        assert_eq!(s, "{\n  \"n\": 1,\n  \"a\": [\n    \"x\"\n  ]\n}");
    }

    #[test]
    fn to_pretty_then_parse_roundtrip() {
        let v = Json::Arr(vec![
            Json::Obj(vec![
                ("number".to_string(), Json::Num(3.0)),
                ("title".to_string(), Json::Str("a \"q\" title\nline".to_string())),
                ("state".to_string(), Json::Str("open".to_string())),
                ("done".to_string(), Json::Bool(false)),
                ("reason".to_string(), Json::Null),
                (
                    "labels".to_string(),
                    Json::Arr(vec![Json::Obj(vec![(
                        "name".to_string(),
                        Json::Str("bug".to_string()),
                    )])]),
                ),
            ]),
            Json::Obj(vec![]),
        ]);
        let pretty = to_pretty(&v);
        let reparsed = parse(&pretty).unwrap();
        assert_eq!(reparsed, v);
    }

    #[test]
    fn escape_str_handles_controls() {
        assert_eq!(escape_str("a\nb"), "a\\nb");
        assert_eq!(escape_str("\u{01}"), "\\u0001");
        assert_eq!(escape_str("\"\\"), "\\\"\\\\");
    }
}
