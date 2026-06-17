pub fn json_bool(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

pub fn json_string(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        escape_char(ch, &mut out);
    }
    out.push('"');
    out
}

fn escape_char(ch: char, out: &mut String) {
    match ch {
        '"' => out.push_str("\\\""),
        '\\' => out.push_str("\\\\"),
        '\n' => out.push_str("\\n"),
        '\r' => out.push_str("\\r"),
        '\t' => out.push_str("\\t"),
        ch if ch.is_control() => out.push_str(&format!("\\u{:04x}", ch as u32)),
        ch => out.push(ch),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_json_string() {
        assert_eq!(json_string("a\n\"b\""), "\"a\\n\\\"b\\\"\"");
    }
}
