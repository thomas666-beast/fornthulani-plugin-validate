//! The built-in rule implementations. Everything here is pure and
//! panic-free: malformed rule arguments make a rule *fail* (closed).

use crate::messages::Kind;
use crate::rule::Rule;
use serde_json::Value;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub(crate) struct Ctx<'a> {
    pub input: &'a Value,
    pub path: &'a str,
    /// The field also carries `integer` / `numeric`: compare as numbers.
    pub numeric: bool,
}

// ---------------------------------------------------------------------
// path access
// ---------------------------------------------------------------------

pub(crate) fn get_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = root;
    for seg in path.split('.') {
        cur = match cur {
            Value::Object(m) => m.get(seg)?,
            Value::Array(a) => a.get(seg.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(cur)
}

// ---------------------------------------------------------------------
// value helpers
// ---------------------------------------------------------------------

pub(crate) fn scalar_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

pub(crate) fn is_blank(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => true,
        Some(Value::String(s)) => s.trim().is_empty(),
        Some(Value::Array(a)) => a.is_empty(),
        Some(Value::Object(o)) => o.is_empty(),
        Some(_) => false,
    }
}

pub(crate) fn as_number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64().filter(|f| f.is_finite()),
        Value::String(s) => s.parse::<f64>().ok().filter(|f| f.is_finite()),
        _ => None,
    }
}

pub(crate) fn is_integer(v: &Value) -> bool {
    match v {
        Value::Number(n) => n.is_i64() || n.is_u64(),
        Value::String(s) => {
            let digits = s.strip_prefix(['+', '-']).unwrap_or(s);
            !digits.is_empty()
                && digits.bytes().all(|b| b.is_ascii_digit())
                && s.parse::<i64>().is_ok()
        }
        _ => false,
    }
}

pub(crate) fn as_bool(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        Value::Number(n) => match n.as_i64() {
            Some(1) => Some(true),
            Some(0) => Some(false),
            _ => None,
        },
        Value::String(s) => match s.as_str() {
            "1" | "true" => Some(true),
            "0" | "false" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn kind_of(v: Option<&Value>, numeric: bool) -> Kind {
    match v {
        Some(Value::Number(_)) => Kind::Num,
        Some(Value::Array(_)) | Some(Value::Object(_)) => Kind::Arr,
        _ if numeric => Kind::Num,
        _ => Kind::Str,
    }
}

/// "Size" in Laravel's sense: the number for numbers, the character count
/// for strings, the item count for arrays.
fn size_of(v: &Value, numeric: bool) -> Option<f64> {
    if numeric || v.is_number() {
        return as_number(v);
    }
    match v {
        Value::String(s) => Some(s.chars().count() as f64),
        Value::Array(a) => Some(a.len() as f64),
        Value::Object(o) => Some(o.len() as f64),
        _ => None,
    }
}

fn arg_num(rule: &Rule, i: usize) -> Option<f64> {
    rule.args
        .get(i)
        .and_then(|a| a.parse::<f64>().ok())
        .filter(|f| f.is_finite())
}

fn same_value(a: &Value, b: &Value) -> bool {
    a == b
        || match (scalar_string(a), scalar_string(b)) {
            (Some(x), Some(y)) => x == y,
            _ => false,
        }
}

fn in_list(v: &Value, list: &[String]) -> bool {
    match v {
        Value::Array(items) => items.iter().all(|i| in_list(i, list)),
        other => scalar_string(other).is_some_and(|s| list.contains(&s)),
    }
}

// ---------------------------------------------------------------------
// dispatcher
// ---------------------------------------------------------------------

/// `Some(true)` pass, `Some(false)` fail, `None` unknown rule.
pub(crate) fn run(rule: &Rule, v: &Value, c: &Ctx<'_>) -> Option<bool> {
    let name = rule.name.as_str();
    let a0 = rule.args.first().map(String::as_str).unwrap_or("");
    let text = scalar_string(v);
    let text_ref = text.as_deref();

    Some(match name {
        // ---- types ----
        "string" => v.is_string(),
        "integer" => is_integer(v),
        "numeric" => as_number(v).is_some(),
        "boolean" => as_bool(v).is_some(),
        "array" => v.is_array() || v.is_object(),
        "json" => matches!(v, Value::String(s) if serde_json::from_str::<Value>(s).is_ok()),

        // ---- formats ----
        "email" => text_ref.is_some_and(is_email),
        "url" => text_ref.is_some_and(is_url),
        "uuid" => text_ref.is_some_and(is_uuid),
        "ip" => text_ref.is_some_and(|s| s.parse::<IpAddr>().is_ok()),
        "ipv4" => text_ref.is_some_and(|s| s.parse::<Ipv4Addr>().is_ok()),
        "ipv6" => text_ref.is_some_and(|s| s.parse::<Ipv6Addr>().is_ok()),
        "date" => text_ref.is_some_and(|s| parse_date(s).is_some()),
        "alpha" => text_ref.is_some_and(|s| !s.is_empty() && s.chars().all(char::is_alphabetic)),
        "alpha_num" => {
            text_ref.is_some_and(|s| !s.is_empty() && s.chars().all(char::is_alphanumeric))
        }
        "alpha_dash" => text_ref.is_some_and(|s| {
            !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        }),
        "ascii" => text_ref.is_some_and(|s| s.is_ascii()),
        "lowercase" => text_ref.is_some_and(|s| s == s.to_lowercase()),
        "uppercase" => text_ref.is_some_and(|s| s == s.to_uppercase()),
        "regex" => text_ref.is_some_and(|s| regex_is_match(a0, s).unwrap_or(false)),
        "not_regex" => text_ref.is_some_and(|s| regex_is_match(a0, s).map(|m| !m).unwrap_or(false)),
        "digits" => text_ref.is_some_and(|s| {
            s.bytes().all(|b| b.is_ascii_digit()) && Some(s.len() as f64) == arg_num(rule, 0)
        }),
        "digits_between" => text_ref.is_some_and(|s| {
            let n = s.len() as f64;
            s.bytes().all(|b| b.is_ascii_digit())
                && !s.is_empty()
                && matches!((arg_num(rule, 0), arg_num(rule, 1)), (Some(lo), Some(hi)) if n >= lo && n <= hi)
        }),
        "starts_with" => text_ref.is_some_and(|s| rule.args.iter().any(|p| s.starts_with(p.as_str()))),
        "ends_with" => text_ref.is_some_and(|s| rule.args.iter().any(|p| s.ends_with(p.as_str()))),

        // ---- membership / comparison with other fields ----
        "in" => in_list(v, &rule.args),
        "not_in" => !in_list(v, &rule.args),
        "same" => get_path(c.input, a0).is_some_and(|o| same_value(v, o)),
        "different" => !get_path(c.input, a0).is_some_and(|o| same_value(v, o)),
        "confirmed" => {
            let other = format!("{}_confirmation", c.path);
            get_path(c.input, &other).is_some_and(|o| same_value(v, o))
        }
        "distinct" => is_distinct(c, v),

        // ---- sizes ----
        "min" => cmp_arg(rule, v, c, |s, m| s >= m),
        "max" => cmp_arg(rule, v, c, |s, m| s <= m),
        "size" => cmp_arg(rule, v, c, |s, m| (s - m).abs() < f64::EPSILON),
        "between" => match (size_of(v, c.numeric), arg_num(rule, 0), arg_num(rule, 1)) {
            (Some(s), Some(lo), Some(hi)) => s >= lo && s <= hi,
            _ => false,
        },
        "gt" => cmp_field_or_literal(a0, v, c, |s, o| s > o),
        "gte" => cmp_field_or_literal(a0, v, c, |s, o| s >= o),
        "lt" => cmp_field_or_literal(a0, v, c, |s, o| s < o),
        "lte" => cmp_field_or_literal(a0, v, c, |s, o| s <= o),

        // ---- dates (date part only) ----
        "after" => cmp_date(a0, text_ref, c, |a, b| a > b),
        "after_or_equal" => cmp_date(a0, text_ref, c, |a, b| a >= b),
        "before" => cmp_date(a0, text_ref, c, |a, b| a < b),
        "before_or_equal" => cmp_date(a0, text_ref, c, |a, b| a <= b),

        _ => return None,
    })
}

fn cmp_arg(rule: &Rule, v: &Value, c: &Ctx<'_>, f: impl Fn(f64, f64) -> bool) -> bool {
    match (size_of(v, c.numeric), arg_num(rule, 0)) {
        (Some(s), Some(m)) => f(s, m),
        _ => false,
    }
}

fn cmp_field_or_literal(arg: &str, v: &Value, c: &Ctx<'_>, f: impl Fn(f64, f64) -> bool) -> bool {
    let mine = match size_of(v, c.numeric) {
        Some(s) => s,
        None => return false,
    };
    let theirs = match arg.parse::<f64>() {
        Ok(n) if n.is_finite() => Some(n),
        _ => get_path(c.input, arg).and_then(|o| size_of(o, c.numeric)),
    };
    theirs.is_some_and(|t| f(mine, t))
}

fn cmp_date(
    arg: &str,
    mine: Option<&str>,
    c: &Ctx<'_>,
    f: impl Fn((i32, u32, u32), (i32, u32, u32)) -> bool,
) -> bool {
    let a = match mine.and_then(parse_date) {
        Some(d) => d,
        None => return false,
    };
    let b = match parse_date(arg) {
        Some(d) => Some(d),
        None => get_path(c.input, arg)
            .and_then(scalar_string)
            .and_then(|s| parse_date(&s)),
    };
    b.is_some_and(|b| f(a, b))
}

/// For a path like `tags.2`, check no sibling in `tags` equals the value.
fn is_distinct(c: &Ctx<'_>, v: &Value) -> bool {
    let (parent, idx) = match c.path.rsplit_once('.') {
        Some(p) => p,
        None => return true,
    };
    let siblings = match get_path(c.input, parent) {
        Some(Value::Array(a)) => a,
        _ => return true,
    };
    let me = idx.parse::<usize>().ok();
    siblings
        .iter()
        .enumerate()
        .all(|(i, other)| Some(i) == me || !same_value(v, other))
}

fn regex_is_match(pattern: &str, text: &str) -> Option<bool> {
    if pattern.len() > 2048 {
        return None;
    }
    let re = regex::RegexBuilder::new(pattern)
        .size_limit(1 << 20)
        .build()
        .ok()?;
    Some(re.is_match(text))
}

// ---------------------------------------------------------------------
// formats
// ---------------------------------------------------------------------

pub(crate) fn is_hostname(h: &str) -> bool {
    if h.is_empty() || h.len() > 253 {
        return false;
    }
    h.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    })
}

/// Practical (not RFC-exhaustive) email check: ASCII only, one `@`, sane
/// lengths, a dotted domain whose last label is not all digits.
pub(crate) fn is_email(s: &str) -> bool {
    if s.len() > 254 || !s.is_ascii() {
        return false;
    }
    let (local, domain) = match s.rsplit_once('@') {
        Some(p) => p,
        None => return false,
    };
    if local.is_empty() || local.len() > 64 {
        return false;
    }
    if local.starts_with('.') || local.ends_with('.') || local.contains("..") {
        return false;
    }
    let local_ok = local
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b".!#$%&'*+/=?^_`{|}~-".contains(&b));
    if !local_ok || !is_hostname(domain) || !domain.contains('.') {
        return false;
    }
    let tld = domain.rsplit('.').next().unwrap_or("");
    !tld.bytes().all(|b| b.is_ascii_digit())
}

pub(crate) fn is_url(s: &str) -> bool {
    url_inner(s).unwrap_or(false)
}

fn url_inner(s: &str) -> Option<bool> {
    if s.len() > 2048 || s.bytes().any(|b| b <= 0x20 || b == 0x7f) {
        return Some(false);
    }
    let rest = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    let hostport = authority
        .rsplit_once('@')
        .map(|(_, h)| h)
        .unwrap_or(authority);

    let (host, port) = if let Some(v6) = hostport.strip_prefix('[') {
        let (h, after) = v6.split_once(']')?;
        if h.parse::<Ipv6Addr>().is_err() {
            return Some(false);
        }
        (h, after.strip_prefix(':'))
    } else {
        match hostport.split_once(':') {
            Some((h, p)) => (h, Some(p)),
            None => (hostport, None),
        }
    };

    if !host.contains(':') && host.parse::<Ipv4Addr>().is_err() && !is_hostname(host) {
        return Some(false);
    }
    if let Some(p) = port {
        if p.is_empty() || p.len() > 5 || !p.bytes().all(|b| b.is_ascii_digit()) {
            return Some(false);
        }
        if p.parse::<u32>().ok()? > 65535 {
            return Some(false);
        }
    }
    Some(true)
}

pub(crate) fn is_uuid(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    let lens = [8usize, 4, 4, 4, 12];
    parts.len() == 5
        && parts
            .iter()
            .zip(lens)
            .all(|(p, n)| p.len() == n && p.bytes().all(|b| b.is_ascii_hexdigit()))
}

// ---------------------------------------------------------------------
// dates
// ---------------------------------------------------------------------

fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        _ => 0,
    }
}

fn two_digits(s: &str) -> Option<u32> {
    if s.len() == 2 && s.bytes().all(|b| b.is_ascii_digit()) {
        s.parse().ok()
    } else {
        None
    }
}

/// `YYYY-MM-DD`, optionally followed by `T`/space and `HH:MM[:SS[.fff]]`
/// and `Z` / `±HH:MM`. Returns the calendar date.
pub(crate) fn parse_date(s: &str) -> Option<(i32, u32, u32)> {
    let date = s.get(..10)?;
    let mut it = date.split('-');
    let (ys, ms, ds) = (it.next()?, it.next()?, it.next()?);
    if it.next().is_some() || ys.len() != 4 || !ys.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let y: i32 = ys.parse().ok()?;
    let m = two_digits(ms)?;
    let d = two_digits(ds)?;
    if !(1..=12).contains(&m) || d == 0 || d > days_in_month(y, m) {
        return None;
    }
    let rest = &s[10..];
    if rest.is_empty() {
        return Some((y, m, d));
    }
    let time = rest.strip_prefix('T').or_else(|| rest.strip_prefix(' '))?;
    if parse_time(time) {
        Some((y, m, d))
    } else {
        None
    }
}

fn parse_time(t: &str) -> bool {
    let b = t.as_bytes();
    if b.len() < 5 || b[2] != b':' {
        return false;
    }
    let ok = |a: usize, max: u32| {
        t.get(a..a + 2)
            .and_then(two_digits)
            .is_some_and(|v| v <= max)
    };
    if !ok(0, 23) || !ok(3, 59) {
        return false;
    }
    let mut i = 5;
    if b.get(i) == Some(&b':') {
        if !ok(i + 1, 60) {
            return false;
        }
        i += 3;
        if b.get(i) == Some(&b'.') {
            let start = i + 1;
            let mut j = start;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if j == start {
                return false;
            }
            i = j;
        }
    }
    match b.get(i) {
        None => true,
        Some(b'Z') => i + 1 == b.len(),
        Some(b'+') | Some(b'-') => {
            i + 6 == b.len() && b[i + 3] == b':' && ok(i + 1, 23) && ok(i + 4, 59)
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emails() {
        for good in [
            "a@b.co",
            "first.last+tag@sub.example.org",
            "x_y@ex-ample.com",
        ] {
            assert!(is_email(good), "{good}");
        }
        for bad in [
            "",
            "plain",
            "@x.com",
            "a@",
            "a@b",
            "a@b.",
            "a@.com",
            "a..b@x.com",
            ".a@x.com",
            "a b@x.com",
            "a@x_y.com",
            "a@-x.com",
            "a@127.0.0.1",
            "é@x.com",
            "a@@x.com",
        ] {
            assert!(!is_email(bad), "{bad}");
        }
        assert!(!is_email(&format!("{}@x.com", "a".repeat(65))));
    }

    #[test]
    fn urls() {
        for good in [
            "http://example.com",
            "https://example.com/path?q=1#f",
            "http://localhost:8080/x",
            "https://[::1]:443/",
            "http://127.0.0.1",
        ] {
            assert!(is_url(good), "{good}");
        }
        for bad in [
            "example.com",
            "ftp://example.com",
            "javascript:alert(1)",
            "http://",
            "http:// space.com",
            "http://exa mple.com",
            "http://example.com:99999",
            "http://example.com:",
            "https://-bad.com",
        ] {
            assert!(!is_url(bad), "{bad}");
        }
    }

    #[test]
    fn uuids() {
        assert!(is_uuid("123e4567-e89b-12d3-a456-426614174000"));
        assert!(!is_uuid("123e4567e89b12d3a456426614174000"));
        assert!(!is_uuid("123e4567-e89b-12d3-a456-42661417400g"));
    }

    #[test]
    fn dates() {
        assert_eq!(parse_date("2024-02-29"), Some((2024, 2, 29)));
        assert!(parse_date("2023-02-29").is_none());
        assert!(parse_date("2024-13-01").is_none());
        assert!(parse_date("2024-00-10").is_none());
        assert!(parse_date("2024-1-10").is_none());
        assert!(parse_date("2024-01-10T12:30").is_some());
        assert!(parse_date("2024-01-10 12:30:45").is_some());
        assert!(parse_date("2024-01-10T12:30:45.123Z").is_some());
        assert!(parse_date("2024-01-10T12:30:45+02:00").is_some());
        assert!(parse_date("2024-01-10T25:00").is_none());
        assert!(parse_date("2024-01-10T12:30:45+2:00").is_none());
        assert!(parse_date("2024-01-10junk").is_none());
        assert!(parse_date("not a date").is_none());
        assert!(parse_date("日本語日本語日本語日").is_none());
    }

    #[test]
    fn integers_and_numbers() {
        use serde_json::json;
        assert!(is_integer(&json!(5)));
        assert!(is_integer(&json!("-12")));
        assert!(!is_integer(&json!(5.5)));
        assert!(!is_integer(&json!("5.5")));
        assert!(!is_integer(&json!("")));
        assert!(!is_integer(&json!("+")));
        assert!(as_number(&json!("1e3")).is_some());
        assert!(as_number(&json!("inf")).is_none());
        assert!(as_number(&json!("NaN")).is_none());
    }
}
