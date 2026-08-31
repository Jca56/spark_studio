//! A JSON reader, ours. glTF is JSON wrapped around a binary blob, and
//! nothing else in Spark speaks it, so this is the whole of what a glTF
//! file needs: the six value kinds, string escapes, numbers as `f64`, and
//! a few typed accessors that let the loader read like the spec.
//!
//! Objects keep their keys in file order in a `Vec`; lookups are linear,
//! which for glTF's small objects is faster than hashing them.

use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

#[derive(Debug, PartialEq)]
pub struct JsonError {
    /// Byte offset into the text.
    pub at: usize,
    pub what: &'static str,
}

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.what, self.at)
    }
}

impl Json {
    pub fn parse(text: &str) -> Result<Json, JsonError> {
        let mut p = Parser {
            s: text.as_bytes(),
            i: 0,
        };
        p.ws();
        let v = p.value()?;
        p.ws();
        if p.i != p.s.len() {
            return Err(p.err("trailing characters"));
        }
        Ok(v)
    }

    /// A member of an object; `None` for a missing key or a non-object.
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// An element of an array; `None` past the end or for a non-array.
    pub fn at(&self, i: usize) -> Option<&Json> {
        match self {
            Json::Array(a) => a.get(i),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_f32(&self) -> Option<f32> {
        self.as_f64().map(|n| n as f32)
    }

    /// A non-negative whole number — an index or a count.
    pub fn as_usize(&self) -> Option<usize> {
        match self {
            Json::Number(n) if *n >= 0.0 && n.fract() == 0.0 => Some(*n as usize),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Array(a) => Some(a),
            _ => None,
        }
    }

    /// An array of numbers as `f32`s; `None` if any element isn't one.
    pub fn f32s(&self) -> Option<Vec<f32>> {
        self.as_array()?.iter().map(Json::as_f32).collect()
    }
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn err(&self, what: &'static str) -> JsonError {
        JsonError { at: self.i, what }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }

    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn value(&mut self) -> Result<Json, JsonError> {
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Json::String(self.string()?)),
            Some(b't') => self.literal("true", Json::Bool(true)),
            Some(b'f') => self.literal("false", Json::Bool(false)),
            Some(b'n') => self.literal("null", Json::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.err("unexpected character")),
            None => Err(self.err("unexpected end")),
        }
    }

    fn literal(&mut self, word: &'static str, v: Json) -> Result<Json, JsonError> {
        if self.s[self.i..].starts_with(word.as_bytes()) {
            self.i += word.len();
            Ok(v)
        } else {
            Err(self.err("bad literal"))
        }
    }

    fn object(&mut self) -> Result<Json, JsonError> {
        self.i += 1;
        let mut members = Vec::new();
        self.ws();
        if self.eat(b'}') {
            return Ok(Json::Object(members));
        }
        loop {
            self.ws();
            if self.peek() != Some(b'"') {
                return Err(self.err("expected a key"));
            }
            let key = self.string()?;
            self.ws();
            if !self.eat(b':') {
                return Err(self.err("expected ':'"));
            }
            self.ws();
            let v = self.value()?;
            members.push((key, v));
            self.ws();
            if self.eat(b',') {
                continue;
            }
            if self.eat(b'}') {
                return Ok(Json::Object(members));
            }
            return Err(self.err("expected ',' or '}'"));
        }
    }

    fn array(&mut self) -> Result<Json, JsonError> {
        self.i += 1;
        let mut items = Vec::new();
        self.ws();
        if self.eat(b']') {
            return Ok(Json::Array(items));
        }
        loop {
            self.ws();
            items.push(self.value()?);
            self.ws();
            if self.eat(b',') {
                continue;
            }
            if self.eat(b']') {
                return Ok(Json::Array(items));
            }
            return Err(self.err("expected ',' or ']'"));
        }
    }

    fn string(&mut self) -> Result<String, JsonError> {
        self.i += 1;
        let mut out = String::new();
        loop {
            let Some(c) = self.peek() else {
                return Err(self.err("unterminated string"));
            };
            self.i += 1;
            match c {
                b'"' => return Ok(out),
                b'\\' => {
                    let Some(e) = self.peek() else {
                        return Err(self.err("unterminated escape"));
                    };
                    self.i += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let mut cp = self.hex4()?;
                            // A surrogate pair is two escapes back to back.
                            if (0xD800..0xDC00).contains(&cp) {
                                if !(self.eat(b'\\') && self.eat(b'u')) {
                                    return Err(self.err("lone surrogate"));
                                }
                                let lo = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&lo) {
                                    return Err(self.err("bad surrogate pair"));
                                }
                                cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                            }
                            out.push(char::from_u32(cp).ok_or_else(|| self.err("bad code point"))?);
                        }
                        _ => return Err(self.err("bad escape")),
                    }
                }
                _ => {
                    // Copy a run of plain bytes at once — strings are UTF-8
                    // already, and the text came in as a `&str`.
                    let start = self.i - 1;
                    while matches!(self.peek(), Some(b) if b != b'"' && b != b'\\') {
                        self.i += 1;
                    }
                    out.push_str(std::str::from_utf8(&self.s[start..self.i]).map_err(|_| self.err("bad UTF-8"))?);
                }
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, JsonError> {
        let end = self.i + 4;
        let hex = self.s.get(self.i..end).ok_or_else(|| self.err("short \\u escape"))?;
        let s = std::str::from_utf8(hex).map_err(|_| self.err("bad \\u escape"))?;
        let v = u32::from_str_radix(s, 16).map_err(|_| self.err("bad \\u escape"))?;
        self.i = end;
        Ok(v)
    }

    fn number(&mut self) -> Result<Json, JsonError> {
        let start = self.i;
        self.eat(b'-');
        if !self.digits() {
            return Err(self.err("expected a digit"));
        }
        if self.eat(b'.') && !self.digits() {
            return Err(self.err("expected a digit after '.'"));
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.i += 1;
            if !self.eat(b'+') {
                self.eat(b'-');
            }
            if !self.digits() {
                return Err(self.err("expected an exponent"));
            }
        }
        let text = std::str::from_utf8(&self.s[start..self.i]).map_err(|_| self.err("bad number"))?;
        text.parse()
            .map(Json::Number)
            .map_err(|_| JsonError {
                at: start,
                what: "bad number",
            })
    }

    /// Consume a run of digits; whether there was at least one.
    fn digits(&mut self) -> bool {
        let start = self.i;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.i += 1;
        }
        self.i > start
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_every_kind() {
        let j = Json::parse(r#" { "a": [1, -2.5, 3e2, 4E-1], "b": true, "c": null, "d": "x", "e": {} } "#)
            .unwrap();
        assert_eq!(j.get("a").unwrap().f32s().unwrap(), vec![1.0, -2.5, 300.0, 0.4]);
        assert_eq!(j.get("b").unwrap().as_bool(), Some(true));
        assert_eq!(j.get("c"), Some(&Json::Null));
        assert_eq!(j.get("d").unwrap().as_str(), Some("x"));
        assert_eq!(j.get("e"), Some(&Json::Object(vec![])));
        assert_eq!(j.get("missing"), None);
    }

    #[test]
    fn indices_are_whole_and_positive() {
        let j = Json::parse("[3, 3.5, -1]").unwrap();
        assert_eq!(j.at(0).unwrap().as_usize(), Some(3));
        assert_eq!(j.at(1).unwrap().as_usize(), None);
        assert_eq!(j.at(2).unwrap().as_usize(), None);
        assert_eq!(j.at(3), None);
    }

    #[test]
    fn escapes_decode() {
        let j = Json::parse(r#""a\"b\\c\/d\n\t\u00e9\ud83d\ude80""#).unwrap();
        assert_eq!(j.as_str(), Some("a\"b\\c/d\n\té🚀"));
    }

    #[test]
    fn nesting_and_whitespace() {
        let j = Json::parse("\n\t[ [ [ ] ] , { \"k\" : [ { } ] } ]\r\n").unwrap();
        assert_eq!(j.at(0).unwrap().at(0).unwrap().as_array().unwrap().len(), 0);
        assert!(j.at(1).unwrap().get("k").unwrap().at(0).is_some());
    }

    #[test]
    fn errors_say_where() {
        assert_eq!(Json::parse("[1, 2").unwrap_err().what, "expected ',' or ']'");
        assert_eq!(Json::parse("[").unwrap_err().what, "unexpected end");
        assert_eq!(Json::parse("{\"a\" 1}").unwrap_err().what, "expected ':'");
        assert_eq!(Json::parse("[1] x").unwrap_err(), JsonError { at: 4, what: "trailing characters" });
        assert_eq!(Json::parse("\"abc").unwrap_err().what, "unterminated string");
        assert_eq!(Json::parse("-").unwrap_err().what, "expected a digit");
        assert_eq!(Json::parse("1.").unwrap_err().what, "expected a digit after '.'");
        assert_eq!(Json::parse("tru").unwrap_err().what, "bad literal");
        assert_eq!(Json::parse("\"\\ud83d\"").unwrap_err().what, "lone surrogate");
    }

    #[test]
    fn a_padded_glb_chunk_parses() {
        // GLB pads its JSON chunk with spaces to a four-byte boundary.
        let j = Json::parse("{\"asset\":{\"version\":\"2.0\"}}   ").unwrap();
        assert_eq!(j.get("asset").unwrap().get("version").unwrap().as_str(), Some("2.0"));
    }
}
