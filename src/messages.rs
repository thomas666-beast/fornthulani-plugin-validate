//! Default (English) messages, modelled on Laravel's.

use crate::rule::Rule;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    Num,
    Str,
    Arr,
}

pub(crate) fn default_template(rule: &Rule, kind: Kind) -> &'static str {
    use Kind::*;
    match (rule.name.as_str(), kind) {
        ("required", _) => "The :attribute field is required.",
        ("present", _) => "The :attribute field must be present.",
        ("filled", _) => "The :attribute field must have a value.",
        ("accepted", _) => "The :attribute field must be accepted.",
        ("declined", _) => "The :attribute field must be declined.",
        ("prohibited", _) => "The :attribute field is prohibited.",
        ("string", _) => "The :attribute field must be a string.",
        ("integer", _) => "The :attribute field must be an integer.",
        ("numeric", _) => "The :attribute field must be a number.",
        ("boolean", _) => "The :attribute field must be true or false.",
        ("array", _) => "The :attribute field must be an array.",
        ("email", _) => "The :attribute field must be a valid email address.",
        ("url", _) => "The :attribute field must be a valid URL.",
        ("uuid", _) => "The :attribute field must be a valid UUID.",
        ("ip", _) => "The :attribute field must be a valid IP address.",
        ("ipv4", _) => "The :attribute field must be a valid IPv4 address.",
        ("ipv6", _) => "The :attribute field must be a valid IPv6 address.",
        ("date", _) => "The :attribute field must be a valid date.",
        ("json", _) => "The :attribute field must be a valid JSON string.",
        ("alpha", _) => "The :attribute field must only contain letters.",
        ("alpha_num", _) => "The :attribute field must only contain letters and numbers.",
        ("alpha_dash", _) => {
            "The :attribute field must only contain letters, numbers, dashes, and underscores."
        }
        ("ascii", _) => {
            "The :attribute field must only contain single-byte alphanumeric characters and symbols."
        }
        ("lowercase", _) => "The :attribute field must be lowercase.",
        ("uppercase", _) => "The :attribute field must be uppercase.",
        ("regex", _) | ("not_regex", _) => "The :attribute field format is invalid.",
        ("in", _) | ("not_in", _) => "The selected :attribute is invalid.",
        ("same", _) => "The :attribute field must match :other.",
        ("different", _) => "The :attribute field and :other must be different.",
        ("confirmed", _) => "The :attribute field confirmation does not match.",
        ("digits", _) => "The :attribute field must be :digits digits.",
        ("digits_between", _) => "The :attribute field must be between :min and :max digits.",
        ("starts_with", _) => "The :attribute field must start with one of the following: :values.",
        ("ends_with", _) => "The :attribute field must end with one of the following: :values.",
        ("distinct", _) => "The :attribute field has a duplicate value.",

        ("min", Num) => "The :attribute field must be at least :min.",
        ("min", Str) => "The :attribute field must be at least :min characters.",
        ("min", Arr) => "The :attribute field must have at least :min items.",
        ("max", Num) => "The :attribute field must not be greater than :max.",
        ("max", Str) => "The :attribute field must not be greater than :max characters.",
        ("max", Arr) => "The :attribute field must not have more than :max items.",
        ("between", Num) => "The :attribute field must be between :min and :max.",
        ("between", Str) => "The :attribute field must be between :min and :max characters.",
        ("between", Arr) => "The :attribute field must have between :min and :max items.",
        ("size", Num) => "The :attribute field must be :size.",
        ("size", Str) => "The :attribute field must be :size characters.",
        ("size", Arr) => "The :attribute field must contain :size items.",

        ("gt", Num) => "The :attribute field must be greater than :value.",
        ("gt", Str) => "The :attribute field must be greater than :value characters.",
        ("gt", Arr) => "The :attribute field must have more than :value items.",
        ("gte", Num) => "The :attribute field must be greater than or equal to :value.",
        ("gte", Str) => "The :attribute field must be greater than or equal to :value characters.",
        ("gte", Arr) => "The :attribute field must have :value items or more.",
        ("lt", Num) => "The :attribute field must be less than :value.",
        ("lt", Str) => "The :attribute field must be less than :value characters.",
        ("lt", Arr) => "The :attribute field must have less than :value items.",
        ("lte", Num) => "The :attribute field must be less than or equal to :value.",
        ("lte", Str) => "The :attribute field must be less than or equal to :value characters.",
        ("lte", Arr) => "The :attribute field must not have more than :value items.",

        ("after", _) => "The :attribute field must be a date after :date.",
        ("after_or_equal", _) => "The :attribute field must be a date after or equal to :date.",
        ("before", _) => "The :attribute field must be a date before :date.",
        ("before_or_equal", _) => "The :attribute field must be a date before or equal to :date.",

        ("required_if", _) => "The :attribute field is required when :other is :value.",
        ("required_unless", _) => "The :attribute field is required unless :other is in :values.",
        ("required_with", _) | ("required_with_all", _) => {
            "The :attribute field is required when :values is present."
        }
        ("required_without", _) | ("required_without_all", _) => {
            "The :attribute field is required when :values is not present."
        }
        _ => "The :attribute field is invalid.",
    }
}

/// Fill in `:attribute` and the rule's own placeholders.
pub(crate) fn render(template: &str, attribute: &str, rule: &Rule) -> String {
    let a0 = rule.args.first().cloned().unwrap_or_default();
    let a1 = rule.args.get(1).cloned().unwrap_or_default();
    let all = rule.args.join(", ");
    let rest = if rule.args.len() > 1 {
        rule.args[1..].join(", ")
    } else {
        String::new()
    };

    let pairs: Vec<(&str, String)> = match rule.name.as_str() {
        "min" => vec![(":min", a0)],
        "max" => vec![(":max", a0)],
        "size" => vec![(":size", a0)],
        "between" | "digits_between" => vec![(":min", a0), (":max", a1)],
        "digits" => vec![(":digits", a0)],
        "gt" | "gte" | "lt" | "lte" => vec![(":value", a0)],
        "same" | "different" => vec![(":other", a0)],
        "after" | "after_or_equal" | "before" | "before_or_equal" => vec![(":date", a0)],
        "required_if" => vec![(":other", a0), (":value", rest)],
        "required_unless" => vec![(":other", a0), (":values", rest)],
        "in"
        | "not_in"
        | "starts_with"
        | "ends_with"
        | "required_with"
        | "required_with_all"
        | "required_without"
        | "required_without_all" => {
            vec![(":values", all)]
        }
        _ => Vec::new(),
    };

    let mut out = template.replace(":attribute", attribute);
    for (k, v) in pairs {
        out = out.replace(k, &v);
    }
    out
}
