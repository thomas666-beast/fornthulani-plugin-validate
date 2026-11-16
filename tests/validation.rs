use fornthulani_plugin_validate::{Rule, Rules, Validator, extend, rules};
use serde_json::{Value, json};

fn errs(input: Value, rules: impl Into<Rules>) -> Vec<(String, Vec<String>)> {
    let e = Validator::make(input, rules).errors();
    e.fields()
        .map(|f| (f.to_string(), e.get(f).unwrap().to_vec()))
        .collect()
}

fn fails_on(input: Value, rules: impl Into<Rules>, field: &str) -> bool {
    Validator::make(input, rules).errors().has(field)
}

#[test]
fn required_and_blank_values() {
    let r = [("name", "required")];
    assert!(fails_on(json!({}), r, "name"));
    assert!(fails_on(json!({"name": null}), r, "name"));
    assert!(fails_on(json!({"name": "   "}), r, "name"));
    assert!(fails_on(json!({"name": []}), r, "name"));
    assert!(!fails_on(json!({"name": "x"}), r, "name"));
    assert!(!fails_on(json!({"name": 0}), r, "name"));
    assert!(!fails_on(json!({"name": false}), r, "name"));
}

#[test]
fn default_message_matches_laravel() {
    let e = errs(json!({}), [("first_name", "required")]);
    assert_eq!(e[0].1[0], "The first name field is required.");
}

#[test]
fn optional_fields_are_skipped_when_absent_or_empty() {
    let r = [("email", "email|max:5")];
    assert!(Validator::make(json!({}), r).validate().is_ok());
    assert!(Validator::make(json!({"email": ""}), r).validate().is_ok());
}

#[test]
fn nullable() {
    let r = [("age", "nullable|integer")];
    assert!(Validator::make(json!({"age": null}), r).validate().is_ok());
    let r = [("age", "integer")];
    assert!(Validator::make(json!({"age": null}), r).validate().is_err());
}

#[test]
fn sometimes_only_validates_when_present() {
    let r = [("x", "sometimes|required|integer")];
    assert!(Validator::make(json!({}), r).validate().is_ok());
    assert!(Validator::make(json!({"x": ""}), r).validate().is_err());
}

#[test]
fn types() {
    assert!(fails_on(json!({"a": 5}), [("a", "string")], "a"));
    assert!(!fails_on(json!({"a": "5"}), [("a", "string")], "a"));
    assert!(!fails_on(json!({"a": "5"}), [("a", "integer")], "a"));
    assert!(fails_on(json!({"a": 5.5}), [("a", "integer")], "a"));
    assert!(!fails_on(json!({"a": 5.5}), [("a", "numeric")], "a"));
    assert!(fails_on(json!({"a": "abc"}), [("a", "numeric")], "a"));
    assert!(!fails_on(json!({"a": "true"}), [("a", "boolean")], "a"));
    assert!(fails_on(json!({"a": "yes"}), [("a", "boolean")], "a"));
    assert!(!fails_on(json!({"a": [1]}), [("a", "array")], "a"));
    assert!(fails_on(json!({"a": "x"}), [("a", "array")], "a"));
}

#[test]
fn size_rules_are_type_aware() {
    // strings: characters
    assert!(fails_on(json!({"s": "ab"}), [("s", "min:3")], "s"));
    assert!(!fails_on(json!({"s": "héé"}), [("s", "min:3|max:3")], "s"));
    // numbers: value (when `integer`/`numeric` is declared, even as a string)
    assert!(fails_on(json!({"n": "5"}), [("n", "integer|min:10")], "n"));
    assert!(!fails_on(
        json!({"n": "50"}),
        [("n", "integer|min:10")],
        "n"
    ));
    assert!(!fails_on(json!({"n": 18}), [("n", "between:18,120")], "n"));
    assert!(fails_on(json!({"n": 17}), [("n", "between:18,120")], "n"));
    // arrays: items
    assert!(fails_on(json!({"a": [1]}), [("a", "array|min:2")], "a"));
    assert!(!fails_on(
        json!({"a": [1, 2]}),
        [("a", "array|size:2")],
        "a"
    ));

    let e = errs(json!({"s": "ab"}), [("s", "min:3")]);
    assert_eq!(e[0].1[0], "The s field must be at least 3 characters.");
    let e = errs(json!({"n": 5}), [("n", "min:10")]);
    assert_eq!(e[0].1[0], "The n field must be at least 10.");
    let e = errs(json!({"a": [1]}), [("a", "array|min:2")]);
    assert_eq!(e[0].1[0], "The a field must have at least 2 items.");
}

#[test]
fn malformed_rule_arguments_fail_closed() {
    assert!(fails_on(json!({"s": "abc"}), [("s", "min:abc")], "s"));
    assert!(fails_on(json!({"s": "abc"}), [("s", "min")], "s"));
    assert!(fails_on(json!({"s": "abc"}), [("s", "between:1")], "s"));
    assert!(fails_on(json!({"s": "abc"}), [("s", "regex:(")], "s"));
}

#[test]
fn formats() {
    assert!(!fails_on(json!({"e": "a@b.co"}), [("e", "email")], "e"));
    assert!(fails_on(json!({"e": "nope"}), [("e", "email")], "e"));
    assert!(!fails_on(
        json!({"u": "https://x.io/p"}),
        [("u", "url")],
        "u"
    ));
    assert!(fails_on(
        json!({"u": "javascript:alert(1)"}),
        [("u", "url")],
        "u"
    ));
    assert!(!fails_on(
        json!({"id": "123e4567-e89b-12d3-a456-426614174000"}),
        [("id", "uuid")],
        "id"
    ));
    assert!(!fails_on(json!({"d": "2024-02-29"}), [("d", "date")], "d"));
    assert!(fails_on(json!({"d": "2023-02-29"}), [("d", "date")], "d"));
    assert!(!fails_on(json!({"i": "::1"}), [("i", "ip")], "i"));
    assert!(fails_on(json!({"i": "::1"}), [("i", "ipv4")], "i"));
    assert!(!fails_on(json!({"a": "abc"}), [("a", "alpha")], "a"));
    assert!(fails_on(json!({"a": "ab1"}), [("a", "alpha")], "a"));
    assert!(!fails_on(json!({"a": "ab1"}), [("a", "alpha_num")], "a"));
    assert!(!fails_on(json!({"a": "a-b_1"}), [("a", "alpha_dash")], "a"));
    assert!(!fails_on(json!({"a": "12345"}), [("a", "digits:5")], "a"));
    assert!(fails_on(json!({"a": "1234"}), [("a", "digits:5")], "a"));
    assert!(!fails_on(json!({"a": "{\"k\":1}"}), [("a", "json")], "a"));
    assert!(fails_on(json!({"a": "{k"}), [("a", "json")], "a"));
}

#[test]
fn in_not_in_regex_start_end() {
    assert!(!fails_on(
        json!({"r": "admin"}),
        [("r", "in:admin,user")],
        "r"
    ));
    assert!(fails_on(
        json!({"r": "root"}),
        [("r", "in:admin,user")],
        "r"
    ));
    assert!(fails_on(
        json!({"r": "admin"}),
        [("r", "not_in:admin,user")],
        "r"
    ));
    assert!(fails_on(
        json!({"r": ["admin", "root"]}),
        [("r", "in:admin,user")],
        "r"
    ));
    assert!(!fails_on(
        json!({"c": "AB-12"}),
        [("c", "regex:^[A-Z]{2}-\\d{2}$")],
        "c"
    ));
    assert!(fails_on(
        json!({"c": "ab-12"}),
        [("c", "regex:^[A-Z]{2}-\\d{2}$")],
        "c"
    ));
    assert!(!fails_on(
        json!({"c": "ACME-1"}),
        [("c", "starts_with:ACME-,X-")],
        "c"
    ));
    assert!(fails_on(
        json!({"c": "NOPE-1"}),
        [("c", "starts_with:ACME-,X-")],
        "c"
    ));
    // regex with `|` via the typed constructor
    let r = Rules::new().field("c", rules::regex("^(a|b)$"));
    assert!(
        Validator::make(json!({"c": "a"}), r.clone())
            .validate()
            .is_ok()
    );
    assert!(Validator::make(json!({"c": "c"}), r).validate().is_err());
}

#[test]
fn confirmed_same_different() {
    let r = [("password", "required|confirmed")];
    assert!(
        Validator::make(json!({"password": "x", "password_confirmation": "x"}), r)
            .validate()
            .is_ok()
    );
    assert!(fails_on(
        json!({"password": "x", "password_confirmation": "y"}),
        r,
        "password"
    ));
    assert!(fails_on(json!({"password": "x"}), r, "password"));
    assert!(fails_on(json!({"a": 1, "b": 2}), [("a", "same:b")], "a"));
    assert!(fails_on(
        json!({"a": 1, "b": 1}),
        [("a", "different:b")],
        "a"
    ));
}

#[test]
fn comparisons_with_fields_and_literals() {
    assert!(fails_on(
        json!({"a": 1, "b": 5}),
        [("a", "numeric|gt:b")],
        "a"
    ));
    assert!(!fails_on(
        json!({"a": 9, "b": 5}),
        [("a", "numeric|gt:b")],
        "a"
    ));
    assert!(!fails_on(
        json!({"a": 5}),
        [("a", "numeric|gte:5|lte:5")],
        "a"
    ));
    assert!(fails_on(json!({"a": 5}), [("a", "numeric|lt:5")], "a"));
}

#[test]
fn date_comparisons() {
    assert!(!fails_on(
        json!({"d": "2025-01-02"}),
        [("d", "after:2025-01-01")],
        "d"
    ));
    assert!(fails_on(
        json!({"d": "2025-01-01"}),
        [("d", "after:2025-01-01")],
        "d"
    ));
    assert!(!fails_on(
        json!({"d": "2025-01-01"}),
        [("d", "after_or_equal:2025-01-01")],
        "d"
    ));
    assert!(fails_on(
        json!({"start": "2025-05-01", "end": "2025-04-01"}),
        [("end", "date|after:start")],
        "end"
    ));
}

#[test]
fn conditional_requirements() {
    let r = [("reason", "required_if:status,rejected,pending")];
    assert!(fails_on(json!({"status": "rejected"}), r, "reason"));
    assert!(!fails_on(json!({"status": "ok"}), r, "reason"));
    assert!(!fails_on(
        json!({"status": "rejected", "reason": "x"}),
        r,
        "reason"
    ));

    let r = [("reason", "required_unless:status,ok")];
    assert!(fails_on(json!({"status": "bad"}), r, "reason"));
    assert!(!fails_on(json!({"status": "ok"}), r, "reason"));

    let r = [("phone", "required_without:email")];
    assert!(fails_on(json!({}), r, "phone"));
    assert!(!fails_on(json!({"email": "a@b.co"}), r, "phone"));

    let r = [("zip", "required_with:city,street")];
    assert!(fails_on(json!({"city": "x"}), r, "zip"));
    assert!(!fails_on(json!({}), r, "zip"));
}

#[test]
fn accepted_prohibited_present_filled() {
    assert!(fails_on(json!({}), [("tos", "accepted")], "tos"));
    assert!(!fails_on(
        json!({"tos": true}),
        [("tos", "accepted")],
        "tos"
    ));
    assert!(!fails_on(
        json!({"tos": "yes"}),
        [("tos", "accepted")],
        "tos"
    ));
    assert!(fails_on(json!({"x": 1}), [("x", "prohibited")], "x"));
    assert!(fails_on(json!({}), [("x", "present")], "x"));
    assert!(!fails_on(json!({"x": null}), [("x", "present")], "x"));
    assert!(fails_on(json!({"x": ""}), [("x", "filled")], "x"));
    assert!(!fails_on(json!({}), [("x", "filled")], "x"));
}

#[test]
fn wildcards_and_nesting() {
    let rules = [
        ("items.*.sku", "required|string"),
        ("items.*.qty", "required|integer|min:1"),
        ("address.city", "required"),
    ];
    let ok = json!({"items": [{"sku": "a", "qty": 2}, {"sku": "b", "qty": 1}], "address": {"city": "X"}});
    assert!(Validator::make(ok, rules).validate().is_ok());

    let bad = json!({"items": [{"sku": "a", "qty": 2}, {"qty": 0}], "address": {}});
    let e = Validator::make(bad, rules).errors();
    assert!(e.has("items.1.sku"));
    assert!(e.has("items.1.qty"));
    assert!(!e.has("items.0.sku"));
    assert!(e.has("address.city"));

    // missing array: nothing to iterate, nothing to fail
    assert!(
        !Validator::make(json!({"address": {"city": "x"}}), rules)
            .errors()
            .has("items.0.sku")
    );
}

#[test]
fn array_elements_and_distinct() {
    let r = [
        ("tags", "required|array|max:3"),
        ("tags.*", "string|max:3|distinct"),
    ];
    assert!(
        Validator::make(json!({"tags": ["a", "b"]}), r)
            .validate()
            .is_ok()
    );
    let e = Validator::make(json!({"tags": ["a", "a", "toolong"]}), r).errors();
    assert!(e.has("tags.0") && e.has("tags.1"), "{:?}", e);
    assert!(e.has("tags.2"));
}

#[test]
fn bail_stops_at_first_failure() {
    let e = errs(json!({"x": "a"}), [("x", "bail|integer|min:5")]);
    assert_eq!(e[0].1.len(), 1);
    let e = errs(json!({"x": "a"}), [("x", "integer|min:5")]);
    assert_eq!(e[0].1.len(), 2);
}

#[test]
fn custom_messages_and_attributes() {
    let r = Rules::new()
        .field("email", "required|email")
        .field("dob", "required|date")
        .message("email.required", "We need your email.")
        .message("date", ":attribute is not a real date")
        .attribute("dob", "date of birth");
    let e = Validator::make(json!({"dob": "zzz"}), r).errors();
    assert_eq!(e.get("email").unwrap()[0], "We need your email.");
    assert_eq!(e.get("dob").unwrap()[0], "date of birth is not a real date");
}

#[test]
fn custom_rule_closures() {
    let r = Rules::new().field(
        "code",
        vec![
            Rule::parse("required"),
            Rule::custom(|v| match v.as_str() {
                Some(s) if s.starts_with("ACME-") => Ok(()),
                _ => Err("The :attribute must start with ACME-.".into()),
            }),
        ],
    );
    assert!(
        Validator::make(json!({"code": "ACME-1"}), r.clone())
            .validate()
            .is_ok()
    );
    let e = Validator::make(json!({"code": "X"}), r).errors();
    assert_eq!(e.get("code").unwrap()[0], "The code must start with ACME-.");
}

#[test]
fn extension_rules() {
    extend(
        "even",
        |ctx| ctx.value.as_i64().is_some_and(|n| n % 2 == 0),
        "The :attribute must be even.",
    );
    assert!(!fails_on(json!({"n": 4}), [("n", "even")], "n"));
    assert!(fails_on(json!({"n": 3}), [("n", "even")], "n"));
}

#[test]
fn unknown_rules_fail_closed() {
    let e = errs(json!({"x": 1}), [("x", "frobnicate")]);
    assert!(e[0].1[0].contains("undefined rule"));
}

#[test]
fn validated_keeps_only_declared_fields_and_coerces() {
    let input = json!({"name": "Ada", "age": "36", "admin": true, "ok": "1", "extra": {"x": 1}});
    let v = Validator::make(
        input,
        [
            ("name", "required|string"),
            ("age", "integer"),
            ("ok", "boolean"),
        ],
    )
    .validate()
    .unwrap();
    assert_eq!(v.str("name"), Some("Ada"));
    assert_eq!(v.i64("age"), Some(36));
    assert_eq!(v.bool("ok"), Some(true));
    assert!(
        v.get("admin").is_none(),
        "undeclared fields must be dropped"
    );
    assert!(v.get("extra").is_none());
}

#[test]
fn validated_preserves_nested_shape() {
    let input = json!({"items": [{"sku": "a", "qty": "2", "evil": 1}], "address": {"city": "X", "evil": 1}});
    let v = Validator::make(
        input,
        [
            ("items.*.sku", "required"),
            ("items.*.qty", "required|integer"),
            ("address.city", "required"),
        ],
    )
    .validate()
    .unwrap();
    assert_eq!(
        v.as_value(),
        &json!({"items": [{"sku": "a", "qty": 2}], "address": {"city": "X"}})
    );
}

#[test]
fn error_json_shape_matches_laravel() {
    let e = Validator::make(json!({}), [("a", "required"), ("b", "required|email")]).errors();
    let j = e.to_json();
    assert_eq!(j["message"], "The a field is required. (and 1 more error)");
    assert_eq!(j["errors"]["a"][0], "The a field is required.");
    assert_eq!(j["errors"]["b"][0], "The b field is required.");

    let e = Validator::make(json!({}), [("a", "required")]).errors();
    assert_eq!(e.to_json()["message"], "The a field is required.");
}
