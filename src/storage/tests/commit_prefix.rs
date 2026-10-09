use super::*;
fn source(yaml: &str) -> Source {
    Source {
        path: "/fixture/prefix.yml".into(),
        explicitly_chosen_global: false,
        yaml: yaml.into(),
    }
}
fn rules(pattern: &str, replace: &str) -> CommitPrefixes {
    let yaml = serde_yaml::to_string(
        &serde_json::json!({"git": {"commitPrefix": [{"pattern": pattern, "replace": replace}]}}),
    )
    .unwrap();
    Settings::load_m1(&[source(&yaml)])
        .unwrap()
        .m1()
        .commit_prefixes
        .clone()
}
#[test]
fn go_replacements_captures_literals_missing_maximal_names_and_unmatched_text() {
    for (pattern, replace, branch, expected) in [
        (r"^\w+\/(\w+-\w+).*", "[$1] ", "feature/AB-123", "[AB-123] "),
        (
            r"^(?P<issue>AB-123)$",
            "${issue}: $0 $$ $9 $unknown",
            "AB-123",
            "AB-123: AB-123 $  ",
        ),
        (
            r"^(a)(b)?$",
            "$2 $1x ${1}x $01 $999 ${bad} $- ${} ${1",
            "a",
            "  ax    $- ${} ${1",
        ),
        ("AB", "literal", "xAB-ABtail", "xliteral-literaltail"),
        ("AB", "", "ABAB", ""),
        ("a*", "x", "baa", "xbx"),
        (".*", "x", "branch", "x"),
        ("^", "x", "branch", "xbranch"),
        ("$", "x", "branch", "branchx"),
        ("^.*$", "£ dollars $$ ", "branch", "£ dollars $ "),
        (r"\bAB\b", "x", "éABé", "éxé"),
        (r"[\d]+", "x", "12-٣", "x-٣"),
        (r"\s", "x", "a\u{b} b", "a\u{b}xb"),
        (r"a{2,3}", "x", "aaaa", "xa"),
        (r"\x{1234}", "x", "aሴb", "axb"),
        (r"(?:a|b)+?", "x", "ab", "xx"),
        (r"(?m)^a$", "x", "a\na", "x\nx"),
    ] {
        assert_eq!(
            rules(pattern, replace).prefill("repo", branch).as_deref(),
            Some(expected),
            "{pattern}, {replace}"
        );
    }
    assert_eq!(rules(r"^\w+$", "x").prefill("repo", "é"), None);
    assert_eq!(rules(r"^\d+$", "x").prefill("repo", "٣"), None);
    assert_eq!(rules("", "ignored").prefill("repo", "branch"), None);
}
#[test]
fn source_override_order_repository_exact_keys_and_empty_lists_fall_back() {
    let first = source(
        "git: {commitPrefix: [{pattern: '^.*$', replace: old}], commitPrefixes: {repo: [{pattern: '^feature', replace: repo}], other: []}}",
    );
    let mut second = source(
        "git: {commitPrefix: [{pattern: '^.*$', replace: global}], commitPrefixes: {repo: [], 'case-sensitive': [{pattern: '^.*$', replace: special}]}}",
    );
    second.path = "/fixture/override.yml".into();
    let settings = Settings::load_m1(&[first, second]).unwrap();
    let rules = &settings.m1().commit_prefixes;
    assert_eq!(
        rules.prefill("repo", "feature/x").as_deref(),
        Some("global")
    );
    assert_eq!(
        rules.prefill("other", "feature/x").as_deref(),
        Some("global")
    );
    assert_eq!(
        rules.prefill("case-sensitive", "feature/x").as_deref(),
        Some("special")
    );
    assert_eq!(
        rules.prefill("CASE-SENSITIVE", "feature/x").as_deref(),
        Some("global")
    );
    assert!(
        settings
            .diagnostics
            .iter()
            .filter(|d| d.path.starts_with("git.commitPrefix"))
            .all(|d| d.reason == "Supported M1 setting.")
    );
}
#[test]
fn invalid_or_incompatible_regex_reload_is_source_aware_transactional() {
    let mut settings = Settings::load_m1(&[source(
        "git: {commitPrefix: [{pattern: '^.*$', replace: retained}]}",
    )])
    .unwrap();
    for pattern in [
        "[",
        "(?=x)",
        r"(x)\1",
        r"\p{Greek}",
        r"\Qx\E",
        "(?x)a b",
        "[a&&b]",
        "[a[b]",
        "a{1001}",
        "a{01}",
        "a**",
        "a*??",
        "a{2}*",
        "(a{2}){501}",
        "(?i)branch",
        "(?P<same>a)(?P<same>b)",
    ] {
        let yaml = serde_yaml::to_string(&serde_json::json!({"git": {"commitPrefixes": {"repo": [{"pattern": pattern, "replace": "new"}]}}})).unwrap();
        let error = settings
            .reload(DEFAULTS, &[source(&yaml)])
            .unwrap_err()
            .to_string();
        assert!(error.contains("/fixture/prefix.yml"), "{error}");
        assert!(
            error.contains("git.commitPrefixes.repo[0].pattern"),
            "{error}"
        );
        assert_eq!(
            settings
                .m1()
                .commit_prefixes
                .prefill("repo", "branch")
                .as_deref(),
            Some("retained")
        );
    }
    for yaml in [
        "git: {commitPrefix: nope}",
        "git: {commitPrefixes: []}",
        "git: {commitPrefixes: {repo: [{pattern: 7}]}}",
    ] {
        assert!(
            settings
                .reload(DEFAULTS, &[source(yaml)])
                .unwrap_err()
                .to_string()
                .contains("/fixture/prefix.yml")
        );
    }
}
