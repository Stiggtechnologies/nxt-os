//! WebTax 4B `.cor` export: one field per line, a 3-digit field code followed by
//! the value (e.g. `002` + business number). Blank lines are ignored.
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CorField { pub line: usize, pub code: String, pub value: String }

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CorReturn { pub fields: Vec<CorField>, pub business_number: Option<String> }

impl CorReturn {
    pub fn get(&self, code: &str) -> Vec<&str> {
        self.fields.iter().filter(|f| f.code == code).map(|f| f.value.as_str()).collect()
    }
}

pub fn parse(text: &str) -> Result<CorReturn, String> {
    let mut fields = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let l = raw.trim_end_matches('\r');
        if l.trim().is_empty() { continue; }
        if l.len() < 3 || !l.is_char_boundary(3) || !l[..3].chars().all(|c| c.is_ascii_digit()) {
            return Err(format!("line {}: expected 3-digit field code", i + 1));
        }
        fields.push(CorField { line: i + 1, code: l[..3].to_string(), value: l[3..].to_string() });
    }
    if fields.is_empty() { return Err("empty .cor file".into()); }
    let business_number = fields.iter().map(|f| f.value.as_str())
        .find(|v| v.len() == 15 && v[9..11].eq_ignore_ascii_case("RC") && v[..9].chars().all(|c| c.is_ascii_digit()))
        .map(str::to_string);
    Ok(CorReturn { fields, business_number })
}

#[cfg(test)]
mod tests {
    use super::*;
    const SAMPLE: &str = "0012\r\n002MC01\r\n0032025.1\r\n004000000000RC0001\r\n005Example Co Ltd\r\n\r\n0061\r\n";
    #[test] fn parses_synthetic() {
        let r = parse(SAMPLE).unwrap();
        assert_eq!(r.fields.len(), 6);
        assert_eq!(r.get("005"), vec!["Example Co Ltd"]);
        assert_eq!(r.business_number.as_deref(), Some("000000000RC0001"));
    }
    #[test] fn rejects_bad_line() { assert!(parse("xx1\n").is_err()); assert!(parse("").is_err()); }
}
