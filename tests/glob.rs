use fossic::glob::{matches, specificity_score, validate_pattern};

// ── matches ───────────────────────────────────────────────────────────────────

#[test]
fn exact_match_succeeds() {
    assert!(matches("policy-scout/audit", "policy-scout/audit"));
}

#[test]
fn exact_match_fails_different_segment() {
    assert!(!matches("policy-scout/audit", "policy-scout/other"));
}

#[test]
fn star_matches_single_segment() {
    assert!(matches("cerebra/lattice/*", "cerebra/lattice/abc"));
}

#[test]
fn star_does_not_match_multiple_segments() {
    assert!(!matches("cerebra/lattice/*", "cerebra/lattice/abc/sub"));
}

#[test]
fn star_does_not_match_wrong_prefix() {
    assert!(!matches("cerebra/lattice/*", "cerebra/other/abc"));
}

#[test]
fn double_star_matches_zero_segments() {
    assert!(matches("cerebra/**", "cerebra"));
}

#[test]
fn double_star_matches_one_segment() {
    assert!(matches("cerebra/**", "cerebra/lattice"));
}

#[test]
fn double_star_matches_two_segments() {
    assert!(matches("cerebra/**", "cerebra/lattice/abc"));
}

#[test]
fn double_star_does_not_match_wrong_prefix() {
    assert!(!matches("cerebra/**", "other/lattice"));
}

#[test]
fn double_star_only_matches_everything() {
    assert!(matches("**", "a/b/c/d"));
    assert!(matches("**", "single"));
    assert!(matches("**", ""));
}

#[test]
fn double_star_mid_pattern() {
    assert!(matches("a/**/z", "a/z"));
    assert!(matches("a/**/z", "a/b/z"));
    assert!(matches("a/**/z", "a/b/c/z"));
    assert!(!matches("a/**/z", "a/b/c/d"));
}

#[test]
fn multiple_stars() {
    assert!(matches("a/*/c/*", "a/b/c/d"));
    assert!(!matches("a/*/c/*", "a/b/c"));
    assert!(!matches("a/*/c/*", "a/b/d/e"));
}

// ── specificity_score ─────────────────────────────────────────────────────────

#[test]
fn specificity_exact_higher_than_single_star() {
    assert!(
        specificity_score("cerebra/lattice/abc") > specificity_score("cerebra/lattice/*")
    );
}

#[test]
fn specificity_longer_prefix_wins() {
    assert!(
        specificity_score("cerebra/lattice/*") > specificity_score("cerebra/*")
    );
}

#[test]
fn specificity_star_and_double_star_equal() {
    assert_eq!(
        specificity_score("cerebra/lattice/*"),
        specificity_score("cerebra/lattice/**")
    );
}

#[test]
fn specificity_bare_double_star_is_zero() {
    assert_eq!(specificity_score("**"), 0);
}

// ── validate_pattern ──────────────────────────────────────────────────────────

#[test]
fn validate_rejects_empty() {
    assert!(validate_pattern("").is_err());
}

#[test]
fn validate_rejects_leading_slash() {
    assert!(validate_pattern("/foo/bar").is_err());
}

#[test]
fn validate_rejects_trailing_slash() {
    assert!(validate_pattern("foo/bar/").is_err());
}

#[test]
fn validate_rejects_whitespace() {
    assert!(validate_pattern("foo bar").is_err());
    assert!(validate_pattern("foo/b ar").is_err());
}

#[test]
fn validate_rejects_double_quote() {
    assert!(validate_pattern("foo\"bar").is_err());
}

#[test]
fn validate_rejects_single_quote() {
    assert!(validate_pattern("foo'bar").is_err());
}

#[test]
fn validate_accepts_wildcards() {
    assert!(validate_pattern("foo/*").is_ok());
    assert!(validate_pattern("foo/**").is_ok());
    assert!(validate_pattern("**").is_ok());
    assert!(validate_pattern("a/*/b/**").is_ok());
}

#[test]
fn validate_accepts_exact() {
    assert!(validate_pattern("cerebra/lattice/abc").is_ok());
}
