//! Behaviour of `RuleList`: importing public domain lists (ADR-0011).

use hopswitch::core::{ListFormat, RuleList, RuleListError};

fn plain(text: &str) -> (RuleList, hopswitch::core::ImportReport) {
    RuleList::import(text, ListFormat::Plain)
}

fn dotted(text: &str) -> (RuleList, hopswitch::core::ImportReport) {
    RuleList::import(text, ListFormat::Dotted)
}

#[test]
fn plain_format_reads_one_domain_per_line() {
    let (list, report) = plain("example.com\nexample.org\n");
    assert_eq!(list.domains(), ["example.com", "example.org"]);
    assert_eq!(report.kept, 2);
}

#[test]
fn dotted_format_strips_the_leading_dot() {
    let (list, _) = dotted(".example.com\n.example.org\n");
    assert_eq!(list.domains(), ["example.com", "example.org"]);
}

#[test]
fn plain_format_rejects_a_leading_dot() {
    let (list, report) = plain(".example.com\nexample.org\n");
    assert_eq!(list.domains(), ["example.org"]);
    assert_eq!(report.invalid, 1);
}

#[test]
fn entries_are_lower_cased() {
    let (list, _) = plain("Example.COM\n");
    assert_eq!(list.domains(), ["example.com"]);
}

#[test]
fn blank_lines_comments_and_windows_line_endings_are_ignored_without_counting() {
    let (list, report) = plain("# header\r\n\r\nexample.com\r\n   \r\n# end\r\n");
    assert_eq!(list.domains(), ["example.com"]);
    assert_eq!(report.invalid, 0);
}

#[test]
fn surrounding_whitespace_and_a_trailing_dot_are_removed() {
    let (list, _) = plain("  example.com.  \n");
    assert_eq!(list.domains(), ["example.com"]);
}

#[test]
fn exact_duplicates_are_dropped_and_counted() {
    let (list, report) = plain("example.com\nEXAMPLE.com\nexample.org\n");
    assert_eq!(list.domains(), ["example.com", "example.org"]);
    assert_eq!(report.duplicates, 1);
}

#[test]
fn entries_covered_by_a_parent_are_dropped_and_counted() {
    let (list, report) = plain("a.b.example.com\nexample.com\nb.example.com\nother.org\n");
    assert_eq!(list.domains(), ["example.com", "other.org"]);
    assert_eq!(report.covered, 2);
}

#[test]
fn a_look_alike_is_not_covered_by_a_shorter_name() {
    let (list, report) = plain("example.com\nnotexample.com\n");
    assert_eq!(list.domains(), ["example.com", "notexample.com"]);
    assert_eq!(report.covered, 0);
}

#[test]
fn invalid_lines_are_dropped_and_counted() {
    let text = "ok.com\nhas space.com\nunder_score.com\n-lead.com\ntrail-.com\n\
                double..dot.com\nüber.de\nhttp://scheme.com\nhost:80\n";
    let (list, report) = plain(text);
    assert_eq!(list.domains(), ["ok.com"]);
    assert_eq!(report.invalid, 8);
}

#[test]
fn a_label_longer_than_63_characters_is_invalid() {
    let long = "a".repeat(64);
    let (list, report) = plain(&format!("{long}.com\n"));
    assert!(list.is_empty());
    assert_eq!(report.invalid, 1);
}

#[test]
fn a_name_longer_than_253_characters_is_invalid() {
    let label = "a".repeat(60);
    let name = format!("{label}.{label}.{label}.{label}.{label}.com");
    assert!(name.len() > 253);
    let (list, report) = plain(&name);
    assert!(list.is_empty());
    assert_eq!(report.invalid, 1);
}

#[test]
fn punycode_names_are_accepted() {
    let (list, _) = plain("xn--mgbaam7a8h.com\n");
    assert_eq!(list.domains(), ["xn--mgbaam7a8h.com"]);
}

#[test]
fn a_single_label_such_as_a_tld_is_a_valid_rule() {
    let (list, _) = plain("ir\n");
    assert!(list.matches("shop.example.ir"));
}

#[test]
fn the_report_adds_up_to_every_valid_line() {
    let (_, report) = plain("a.com\nA.com\nb.a.com\nc.org\nbad host\n");
    assert_eq!(report.kept + report.duplicates + report.covered, 4);
    assert_eq!(report.invalid, 1);
}

#[test]
fn domains_are_sorted_so_the_output_is_deterministic() {
    let (one, _) = plain("b.com\na.com\nc.com\n");
    let (two, _) = plain("c.com\nb.com\na.com\n");
    assert_eq!(one, two);
    assert_eq!(one.domains(), ["a.com", "b.com", "c.com"]);
}

#[test]
fn a_domain_matches_itself_and_its_subdomains() {
    let (list, _) = plain("example.com\n");
    assert!(list.matches("example.com"));
    assert!(list.matches("www.example.com"));
    assert!(list.matches("a.b.example.com"));
}

#[test]
fn matching_ignores_case_and_a_trailing_dot() {
    let (list, _) = plain("example.com\n");
    assert!(list.matches("WWW.Example.COM."));
}

#[test]
fn matching_respects_label_boundaries() {
    let (list, _) = plain("example.com\n");
    assert!(!list.matches("notexample.com"));
    assert!(!list.matches("example.com.evil.org"));
    assert!(!list.matches("com"));
    assert!(!list.matches(""));
}

#[test]
fn adding_a_suffix_removes_the_entries_it_now_covers() {
    let (mut list, _) = plain("a.ir\nb.ir\nexample.com\n");
    let removed = list.add_suffix("ir").expect("valid domain");
    assert_eq!(removed, 2);
    assert_eq!(list.domains(), ["example.com", "ir"]);
}

#[test]
fn adding_a_suffix_that_is_already_covered_changes_nothing() {
    let (mut list, _) = plain("example.com\n");
    let removed = list.add_suffix("www.example.com").expect("valid domain");
    assert_eq!(removed, 0);
    assert_eq!(list.domains(), ["example.com"]);
}

#[test]
fn adding_an_invalid_suffix_is_refused() {
    let (mut list, _) = plain("example.com\n");
    let err = list.add_suffix("bad host").unwrap_err();
    assert_eq!(err, RuleListError::InvalidDomain("bad host".to_owned()));
    assert_eq!(list.domains(), ["example.com"]);
}

#[test]
fn empty_input_gives_an_empty_list() {
    let (list, report) = plain("");
    assert!(list.is_empty());
    assert_eq!(list.len(), 0);
    assert_eq!(report.kept, 0);
}
