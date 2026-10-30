//! The PAC generator (ADR-0011 decision 5): structure, limits, and, with Node, agreement with the
//! product's own matching rules on tens of thousands of hosts.

use std::fmt::Write as _;
use std::process::Command;

use hopswitch::core::{
    ListFormat, PacError, PacPlan, PacRoute, ProxyConfig, ProxyScheme, Rule, RuleList,
    RulesProfile, Target, ValidationError, PAC_MAX_BYTES,
};

fn socks(host: &str, port: u16) -> PacRoute {
    PacRoute::Proxy(ProxyConfig::new(ProxyScheme::Socks5, host, port).unwrap())
}

fn list(text: &str) -> RuleList {
    RuleList::import(text, ListFormat::Plain).0
}

// ---------- the answers a PAC script gives ----------

#[test]
fn routes_become_the_answers_a_pac_engine_understands() {
    let cases = [
        (PacRoute::Direct, "DIRECT"),
        (socks("127.0.0.1", 10808), "SOCKS5 127.0.0.1:10808"),
        (
            PacRoute::Proxy(ProxyConfig::new(ProxyScheme::Http, "proxy.test", 3128).unwrap()),
            "PROXY proxy.test:3128",
        ),
        (
            PacRoute::Proxy(ProxyConfig::new(ProxyScheme::Https, "proxy.test", 443).unwrap()),
            "HTTPS proxy.test:443",
        ),
        (
            PacRoute::Proxy(ProxyConfig::new(ProxyScheme::Socks4, "proxy.test", 1080).unwrap()),
            "SOCKS4 proxy.test:1080",
        ),
        (socks("::1", 1080), "SOCKS5 [::1]:1080"),
    ];
    for (route, answer) in cases {
        assert_eq!(route.answer(), answer);
    }
}

// ---------- which routes a PAC engine may not understand ----------

fn http(port: u16) -> PacRoute {
    PacRoute::Proxy(ProxyConfig::new(ProxyScheme::Http, "127.0.0.1", port).unwrap())
}

#[test]
fn a_plan_of_direct_and_http_routes_does_not_use_socks() {
    let plan = PacPlan::new(http(1))
        .with_rule("a.com", PacRoute::Direct)
        .unwrap()
        .with_list(list("b.com\n"), http(2));
    assert!(!plan.uses_socks());
}

#[test]
fn a_socks_route_is_found_in_the_default_a_rule_or_a_list() {
    let in_default = PacPlan::new(socks("127.0.0.1", 1));
    let in_rule = PacPlan::new(PacRoute::Direct)
        .with_rule("a.com", socks("127.0.0.1", 1))
        .unwrap();
    let in_list = PacPlan::new(PacRoute::Direct).with_list(list("a.com\n"), socks("127.0.0.1", 1));
    let socks4 = PacPlan::new(PacRoute::Proxy(
        ProxyConfig::new(ProxyScheme::Socks4, "127.0.0.1", 1).unwrap(),
    ));
    for (name, plan) in [
        ("default", in_default),
        ("rule", in_rule),
        ("list", in_list),
        ("socks4", socks4),
    ] {
        assert!(plan.uses_socks(), "{name}");
    }
}

// ---------- structure and limits ----------

#[test]
fn a_plan_with_nothing_in_it_answers_the_default_for_every_dotted_host() {
    let script = PacPlan::new(socks("127.0.0.1", 1080)).generate().unwrap();
    assert!(script.contains("function FindProxyForURL(url, host)"));
    assert!(script.contains("SOCKS5 127.0.0.1:1080"));
}

#[test]
fn the_script_is_plain_ascii_and_old_javascript() {
    let plan = PacPlan::new(PacRoute::Direct)
        .with_rule("a.com", socks("127.0.0.1", 1))
        .unwrap()
        .with_list(list("b.com\nc.org\nir\n"), socks("127.0.0.1", 2));
    let script = plan.generate().unwrap();

    assert!(script.is_ascii());
    for modern in [
        "let ", "const ", "=>", "`", "class ", ",]", ",}", ",)", "...", "**",
    ] {
        assert!(!script.contains(modern), "found {modern:?} in:\n{script}");
    }
}

#[test]
fn the_same_plan_always_gives_the_same_script() {
    let make = || {
        PacPlan::new(PacRoute::Direct)
            .with_list(list("b.com\na.com\nc.org\n"), socks("127.0.0.1", 2))
            .generate()
            .unwrap()
    };
    assert_eq!(make(), make());
    let reordered = PacPlan::new(PacRoute::Direct)
        .with_list(list("c.org\na.com\nb.com\n"), socks("127.0.0.1", 2))
        .generate()
        .unwrap();
    assert_eq!(make(), reordered);
}

#[test]
fn a_rule_with_an_invalid_domain_is_refused() {
    let err = PacPlan::new(PacRoute::Direct)
        .with_rule("bad host", PacRoute::Direct)
        .unwrap_err();
    assert_eq!(err, ValidationError::InvalidRuleDomain("bad host".into()));
}

#[test]
fn a_script_over_the_size_limit_is_refused_instead_of_produced() {
    let mut x = 12345_u64;
    let mut text = String::new();
    while text.len() < 2 * PAC_MAX_BYTES {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        writeln!(text, "{x:x}.example.com").unwrap();
    }
    let plan = PacPlan::new(PacRoute::Direct).with_list(list(&text), socks("127.0.0.1", 1));

    match plan.generate() {
        Err(PacError::TooLarge { bytes, max }) => {
            assert!(bytes > max);
            assert_eq!(max, PAC_MAX_BYTES);
        }
        other => panic!("expected TooLarge, got {:?}", other.map(|s| s.len())),
    }
}

// ---------- from a rules profile ----------

fn profile() -> RulesProfile {
    RulesProfile::new(
        "rp",
        "RP",
        vec![
            Rule::new("a.com", Target::Direct).unwrap(),
            Rule::new("b.com", Target::Proxy("p1".into())).unwrap(),
        ],
        Target::Proxy("p2".into()),
    )
    .unwrap()
}

fn resolve(target: &Target) -> Option<PacRoute> {
    match target {
        Target::Direct => Some(PacRoute::Direct),
        Target::Proxy(id) if id == "p1" => Some(socks("127.0.0.1", 1)),
        Target::Proxy(id) if id == "p2" => Some(socks("127.0.0.1", 2)),
        Target::Proxy(_) => None,
    }
}

#[test]
fn a_profile_becomes_a_plan_through_a_resolver() {
    let script = PacPlan::from_profile(&profile(), resolve)
        .unwrap()
        .generate()
        .unwrap();
    for answer in ["DIRECT", "SOCKS5 127.0.0.1:1", "SOCKS5 127.0.0.1:2"] {
        assert!(script.contains(answer), "{answer} missing:\n{script}");
    }
}

#[test]
fn a_target_the_resolver_cannot_answer_names_itself_in_the_error() {
    let err = PacPlan::from_profile(&profile(), |t| match t {
        Target::Proxy(id) if id == "p2" => None,
        other => resolve(other),
    })
    .unwrap_err();
    assert_eq!(err, PacError::UnresolvedTarget("p2".into()));
}

// ---------- behaviour in a real JavaScript engine (Node) ----------

const HARNESS: &str = r#"
var fs = require("fs"), vm = require("vm");
var src = fs.readFileSync(process.argv[2], "utf8");
var hosts = JSON.parse(fs.readFileSync(process.argv[3], "utf8"));
var ctx = { isPlainHostName: function (h) { return h.indexOf(".") == -1; } };
vm.createContext(ctx);
vm.runInContext(src, ctx);
var out = [];
for (var i = 0; i < hosts.length; i++) {
  out.push(ctx.FindProxyForURL("http://" + hosts[i] + "/", hosts[i]));
}
fs.writeFileSync(process.argv[4], JSON.stringify(out));
"#;

/// Runs `script` in Node for each host. `None` when Node is missing and not required; CI sets
/// `HOPSWITCH_REQUIRE_NODE=1` so a missing Node fails there instead of skipping the check.
fn in_node(script: &str, hosts: &[String]) -> Option<Vec<String>> {
    if Command::new("node").arg("--version").output().is_err() {
        assert!(
            std::env::var_os("HOPSWITCH_REQUIRE_NODE").is_none(),
            "Node is required here but was not found"
        );
        eprintln!("skipped: Node is not installed");
        return None;
    }
    let dir = tempfile::tempdir().unwrap();
    let (harness, pac, input, output) = (
        dir.path().join("harness.js"),
        dir.path().join("p.pac"),
        dir.path().join("hosts.json"),
        dir.path().join("out.json"),
    );
    std::fs::write(&harness, HARNESS).unwrap();
    std::fs::write(&pac, script).unwrap();
    std::fs::write(&input, serde_json::to_vec(hosts).unwrap()).unwrap();
    let run = Command::new("node")
        .args([&harness, &pac, &input, &output])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "node failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    Some(serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap())
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % n as u64).unwrap()
    }

    fn label(&mut self) -> String {
        const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
        let len = 1 + self.below(5);
        (0..len)
            .map(|_| char::from(CHARS[self.below(CHARS.len())]))
            .collect()
    }

    fn domain(&mut self, tlds: &[&str]) -> String {
        let depth = 1 + usize::from(self.below(5) == 0);
        let mut parts: Vec<String> = (0..depth).map(|_| self.label()).collect();
        parts.push(tlds[self.below(tlds.len())].to_owned());
        parts.join(".")
    }
}

fn lines(domains: &[String]) -> String {
    domains.join("\n")
}

/// What the PAC must answer, worked out with the product's own matching code and none of the
/// generator's.
fn expected(
    host: &str,
    profile: &RulesProfile,
    lists: &[(RuleList, PacRoute)],
    default: &PacRoute,
) -> String {
    if !host.contains('.') {
        return "DIRECT".to_owned();
    }
    for rule in profile.rules() {
        if list(rule.domain()).matches(host) {
            return resolve(rule.target()).unwrap().answer();
        }
    }
    for (rule_list, route) in lists {
        if rule_list.matches(host) {
            return route.answer();
        }
    }
    default.answer()
}

fn hosts_for(rng: &mut Rng, listed: &[String], tlds: &[&str]) -> Vec<String> {
    let mut hosts: Vec<String> = Vec::new();
    for domain in listed {
        hosts.push(domain.clone());
        hosts.push(format!("www.{domain}"));
        hosts.push(format!("x-{domain}"));
        hosts.push(format!("{domain}.evil.org"));
        hosts.push(domain[1..].to_owned());
        hosts.push(domain.to_ascii_uppercase());
        hosts.push(format!("{domain}."));
    }
    for _ in 0..20_000 {
        hosts.push(rng.domain(tlds));
    }
    for odd in [
        "constructor.com",
        "__proto__.com",
        "hasOwnProperty.net",
        "toString.ir",
        "valueOf.de",
        "a.constructor",
        "x.y.__proto__",
        "a.toString",
        "a.hasOwnProperty",
        "constructor",
        "localhost",
        "intranet",
        "ir",
        "com",
        "",
        ".",
        "..",
        "a..com",
        ".com",
        "a.b.c.d.e.f.g.com",
        "127.0.0.1",
        "10.0.0.1",
        "xn--mgbaam7a8h.com",
    ] {
        hosts.push(odd.to_owned());
    }
    hosts
}

#[test]
fn node_agrees_with_the_model_on_a_large_mixed_setup() {
    let tlds = ["com", "net", "org", "io", "ir", "de"];
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let first: Vec<String> = (0..25_000).map(|_| rng.domain(&tlds)).collect();
    let second: Vec<String> = (0..4_000).map(|_| rng.domain(&tlds)).collect();
    let (mut first_list, _) = RuleList::import(&lines(&first), ListFormat::Plain);
    first_list.add_suffix("ir").unwrap();
    let (second_list, _) = RuleList::import(&lines(&second), ListFormat::Plain);

    let profile = profile();
    let default = socks("127.0.0.1", 2);
    let lists = vec![
        (first_list.clone(), PacRoute::Direct),
        (second_list.clone(), socks("127.0.0.1", 3)),
    ];
    let plan = lists.iter().cloned().fold(
        PacPlan::from_profile(&profile, resolve).unwrap(),
        |plan, (l, route)| plan.with_list(l, route),
    );
    let script = plan.generate().unwrap();

    let mut sample: Vec<String> = first.iter().step_by(5).cloned().collect();
    sample.extend(second.iter().step_by(3).cloned());
    sample.extend(["a.com", "www.a.com", "b.com", "x.b.com", "nota.com"].map(String::from));
    let hosts = hosts_for(&mut rng, &sample, &tlds);

    let Some(answers) = in_node(&script, &hosts) else {
        return;
    };

    assert_eq!(answers.len(), hosts.len());
    let mut wrong = Vec::new();
    for (host, got) in hosts.iter().zip(&answers) {
        let want = expected(host, &profile, &lists, &default);
        if *got != want {
            wrong.push(format!("{host:?}: got {got}, want {want}"));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of {} hosts wrong, first ones:\n{}",
        wrong.len(),
        hosts.len(),
        wrong
            .iter()
            .take(15)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn user_rules_beat_every_list_and_the_first_rule_wins() {
    let plan = PacPlan::new(socks("127.0.0.1", 9))
        .with_rule("shop.example.ir", socks("127.0.0.1", 1))
        .unwrap()
        .with_rule("example.ir", PacRoute::Direct)
        .unwrap()
        .with_rule("shop.example.ir", socks("127.0.0.1", 5))
        .unwrap()
        .with_list(list("ir\nlisted.com\n"), socks("127.0.0.1", 2))
        .with_list(list("listed.com\nsecond.com\n"), socks("127.0.0.1", 3));
    let hosts: Vec<String> = [
        "shop.example.ir",
        "www.shop.example.ir",
        "example.ir",
        "other.ir",
        "listed.com",
        "second.com",
        "none.org",
    ]
    .map(String::from)
    .to_vec();

    let Some(answers) = in_node(&plan.generate().unwrap(), &hosts) else {
        return;
    };

    assert_eq!(
        answers,
        [
            "SOCKS5 127.0.0.1:1",
            "SOCKS5 127.0.0.1:1",
            "DIRECT",
            "SOCKS5 127.0.0.1:2",
            "SOCKS5 127.0.0.1:2",
            "SOCKS5 127.0.0.1:3",
            "SOCKS5 127.0.0.1:9",
        ]
    );
}

#[test]
fn a_whole_tld_entry_covers_every_host_under_it() {
    let plan = PacPlan::new(socks("127.0.0.1", 9)).with_list(list("ir\n"), PacRoute::Direct);
    let hosts: Vec<String> = ["a.ir", "x.y.z.ir", "ir.com", "notir", "com"]
        .map(String::from)
        .to_vec();

    let Some(answers) = in_node(&plan.generate().unwrap(), &hosts) else {
        return;
    };

    assert_eq!(
        answers,
        ["DIRECT", "DIRECT", "SOCKS5 127.0.0.1:9", "DIRECT", "DIRECT"]
    );
}
