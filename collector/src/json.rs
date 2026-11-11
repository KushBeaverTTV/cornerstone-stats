//! A small streaming JSON writer. Frames are built straight into one reused
//! `String`, so a tick allocates almost nothing, and numbers are written with
//! the precision each unit needs instead of seventeen digits.

use std::fmt::Write as _;

pub struct Json {
    pub out: String,
    /// One entry per open object/array: true until its first member is written.
    first: Vec<bool>,
}

impl Json {
    pub fn new() -> Self {
        Json { out: String::with_capacity(16 * 1024), first: Vec::with_capacity(16) }
    }

    pub fn clear(&mut self) {
        self.out.clear();
        self.first.clear();
    }

    fn comma(&mut self) {
        if let Some(first) = self.first.last_mut() {
            if *first {
                *first = false;
            } else {
                self.out.push(',');
            }
        }
    }

    fn key(&mut self, key: &str) {
        self.comma();
        self.out.push('"');
        self.out.push_str(key);
        self.out.push_str("\":");
    }

    // ---- containers -------------------------------------------------------

    pub fn begin_obj(&mut self) {
        self.comma();
        self.out.push('{');
        self.first.push(true);
    }

    pub fn obj(&mut self, key: &str) {
        self.key(key);
        self.out.push('{');
        self.first.push(true);
    }

    pub fn end_obj(&mut self) {
        self.first.pop();
        self.out.push('}');
    }

    pub fn begin_arr(&mut self) {
        self.comma();
        self.out.push('[');
        self.first.push(true);
    }

    pub fn arr(&mut self, key: &str) {
        self.key(key);
        self.out.push('[');
        self.first.push(true);
    }

    pub fn end_arr(&mut self) {
        self.first.pop();
        self.out.push(']');
    }

    // ---- members ----------------------------------------------------------

    pub fn null(&mut self, key: &str) {
        self.key(key);
        self.out.push_str("null");
    }

    pub fn bool(&mut self, key: &str, value: bool) {
        self.key(key);
        self.out.push_str(if value { "true" } else { "false" });
    }

    pub fn uint(&mut self, key: &str, value: u64) {
        self.key(key);
        let _ = write!(self.out, "{value}");
    }

    /// A number rounded to `decimals` places; NaN and infinities become null.
    pub fn num(&mut self, key: &str, value: f64, decimals: u8) {
        self.key(key);
        self.raw_num(value, decimals);
    }

    pub fn opt(&mut self, key: &str, value: Option<f64>, decimals: u8) {
        match value {
            Some(v) => self.num(key, v, decimals),
            None => self.null(key),
        }
    }

    pub fn opt_uint(&mut self, key: &str, value: Option<u64>) {
        match value {
            Some(v) => self.uint(key, v),
            None => self.null(key),
        }
    }

    pub fn str(&mut self, key: &str, value: &str) {
        self.key(key);
        self.raw_str(value);
    }

    pub fn opt_str(&mut self, key: &str, value: Option<&str>) {
        match value {
            Some(v) => self.str(key, v),
            None => self.null(key),
        }
    }

    /// Splice in text that is already valid JSON.
    pub fn raw(&mut self, key: &str, json: &str) {
        self.key(key);
        self.out.push_str(json);
    }

    // ---- array items ------------------------------------------------------

    pub fn item_num(&mut self, value: f64, decimals: u8) {
        self.comma();
        self.raw_num(value, decimals);
    }

    pub fn item_uint(&mut self, value: u64) {
        self.comma();
        let _ = write!(self.out, "{value}");
    }

    pub fn item_str(&mut self, value: &str) {
        self.comma();
        self.raw_str(value);
    }

    // ---- scalars ----------------------------------------------------------

    fn raw_num(&mut self, value: f64, decimals: u8) {
        if !value.is_finite() {
            self.out.push_str("null");
            return;
        }
        if decimals == 0 {
            let _ = write!(self.out, "{}", value.round() as i64);
            return;
        }
        let scale = 10f64.powi(decimals as i32);
        let rounded = (value * scale).round() / scale;
        if rounded == rounded.trunc() && rounded.abs() < 1e15 {
            let _ = write!(self.out, "{}", rounded as i64);
        } else {
            let _ = write!(self.out, "{:.*}", decimals as usize, rounded);
            // Trim trailing zeros: 12.50 -> 12.5
            while self.out.ends_with('0') {
                self.out.pop();
            }
            if self.out.ends_with('.') {
                self.out.pop();
            }
        }
    }

    pub fn raw_str(&mut self, value: &str) {
        self.out.push('"');
        for c in value.chars() {
            match c {
                '"' => self.out.push_str("\\\""),
                '\\' => self.out.push_str("\\\\"),
                '\n' => self.out.push_str("\\n"),
                '\r' => self.out.push_str("\\r"),
                '\t' => self.out.push_str("\\t"),
                c if (c as u32) < 0x20 || c == '\u{2028}' || c == '\u{2029}' => {
                    let _ = write!(self.out, "\\u{:04x}", c as u32);
                }
                c => self.out.push(c),
            }
        }
        self.out.push('"');
    }
}

/// Text from another process or a device, cut to `max` characters.
pub fn bounded(text: &str, max: usize) -> &str {
    match text.char_indices().nth(max) {
        Some((at, _)) => &text[..at],
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_nested_values_with_commas() {
        let mut j = Json::new();
        j.begin_obj();
        j.num("a", 1.25, 1);
        j.arr("b");
        j.item_num(2.0, 1);
        j.item_str("x\"y\n");
        j.end_arr();
        j.obj("c");
        j.opt("d", None, 1);
        j.num("e", f64::NAN, 1);
        j.end_obj();
        j.end_obj();
        assert_eq!(j.out, r#"{"a":1.3,"b":[2,"x\"y\n"],"c":{"d":null,"e":null}}"#);
        let v: serde_json::Value = serde_json::from_str(&j.out).unwrap();
        assert_eq!(v["b"][1], "x\"y\n");
    }

    #[test]
    fn numbers_keep_only_needed_digits() {
        let mut j = Json::new();
        j.begin_arr();
        j.item_num(12.50, 2);
        j.item_num(0.004, 2);
        j.item_num(3.14159, 2);
        j.item_num(-1.0, 0);
        j.end_arr();
        assert_eq!(j.out, "[12.5,0,3.14,-1]");
    }

    #[test]
    fn bounded_cuts_on_characters() {
        assert_eq!(bounded("héllo", 2), "hé");
        assert_eq!(bounded("ab", 5), "ab");
    }
}
