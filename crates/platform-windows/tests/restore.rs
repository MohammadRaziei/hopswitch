//! The promise of the whole adapter: apply a proxy, restore the snapshot, and the registry is
//! back to what it was. Checked on the pure registry logic with a tiny in-memory registry.

use hopswitch_core::{ProxyConfig, ProxyScheme, ProxyState};
use hopswitch_platform_windows::registry::{parse_state, plan_write, Op, RegistryValues};

fn values(
    enable: Option<u32>,
    server: Option<&str>,
    over: Option<&str>,
    pac: Option<&str>,
    detect: Option<u32>,
) -> RegistryValues {
    RegistryValues {
        proxy_enable: enable,
        proxy_server: server.map(str::to_owned),
        proxy_override: over.map(str::to_owned),
        auto_config_url: pac.map(str::to_owned),
        auto_detect: detect,
    }
}

/// What the writes do to a registry.
fn apply(mut registry: RegistryValues, ops: &[Op]) -> RegistryValues {
    for op in ops {
        match op {
            Op::SetString("ProxyServer", v) => registry.proxy_server = Some(v.clone()),
            Op::SetString("ProxyOverride", v) => registry.proxy_override = Some(v.clone()),
            Op::SetString("AutoConfigURL", v) => registry.auto_config_url = Some(v.clone()),
            Op::SetDword("ProxyEnable", v) => registry.proxy_enable = Some(*v),
            Op::SetDword("AutoDetect", v) => registry.auto_detect = Some(*v),
            Op::Remove("ProxyServer") => registry.proxy_server = None,
            Op::Remove("ProxyOverride") => registry.proxy_override = None,
            Op::Remove("AutoConfigURL") => registry.auto_config_url = None,
            Op::Remove("ProxyEnable") => registry.proxy_enable = None,
            Op::Remove("AutoDetect") => registry.auto_detect = None,
            other => panic!("unexpected write {other:?}"),
        }
    }
    registry
}

/// Windows treats "absent", "empty" and (for the enable flag) "0" alike.
fn meaning(
    r: &RegistryValues,
) -> (
    bool,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<u32>,
) {
    let text = |v: &Option<String>| v.clone().filter(|s| !s.is_empty());
    (
        r.proxy_enable.unwrap_or(0) != 0,
        text(&r.proxy_server),
        text(&r.proxy_override),
        text(&r.auto_config_url),
        r.auto_detect,
    )
}

fn proxy_to_apply() -> ProxyState {
    let config = ProxyConfig::new(ProxyScheme::Socks5, "127.0.0.1", 10808)
        .unwrap()
        .with_bypass(vec!["<local>".into(), "*.example.com".into()])
        .unwrap();
    ProxyState::Manual(config)
}

#[test]
fn applying_then_restoring_brings_every_kind_of_original_back() {
    let originals = [
        ("nothing stored", values(None, None, None, None, None)),
        ("switched off", values(Some(0), None, None, None, None)),
        (
            "empty strings",
            values(Some(0), Some(""), Some(""), None, None),
        ),
        (
            "off with automatic detection",
            values(Some(0), None, None, None, Some(1)),
        ),
        (
            "off with a leftover server",
            values(Some(0), Some("corp:3128"), None, None, None),
        ),
        (
            "off with a leftover bypass list",
            values(Some(0), None, Some("<local>"), None, None),
        ),
        (
            "off with both leftovers",
            values(Some(0), Some("corp:3128"), Some("<local>"), None, Some(1)),
        ),
        (
            "a leftover and no enable flag",
            values(None, Some("corp:3128"), None, None, None),
        ),
        (
            "an http proxy",
            values(Some(1), Some("corp:3128"), Some("<local>"), None, None),
        ),
        (
            "a socks proxy",
            values(Some(1), Some("socks=127.0.0.1:1080"), None, None, None),
        ),
        (
            "a PAC script",
            values(Some(0), None, None, Some("http://wpad/p.pac"), Some(1)),
        ),
        (
            "a PAC script and a proxy",
            values(
                Some(1),
                Some("corp:3128"),
                None,
                Some("http://wpad/p.pac"),
                None,
            ),
        ),
        (
            "per-protocol proxies",
            values(Some(1), Some("http=a:1;https=b:2"), None, None, None),
        ),
        (
            "enabled without a server",
            values(Some(1), None, None, None, None),
        ),
    ];

    for (name, original) in originals {
        let snapshot = parse_state(&original);

        let applied = apply(original.clone(), &plan_write(&proxy_to_apply()).unwrap());
        assert_eq!(
            parse_state(&applied),
            proxy_to_apply(),
            "{name}: what we wrote reads back"
        );

        let restored = apply(applied, &plan_write(&snapshot).unwrap());
        assert_eq!(
            meaning(&restored),
            meaning(&original),
            "{name}: restored registry differs"
        );
    }
}

#[test]
fn a_clean_registry_stays_clean_so_nothing_of_ours_is_left() {
    let original = values(None, None, None, None, None);
    let applied = apply(original.clone(), &plan_write(&proxy_to_apply()).unwrap());

    let restored = apply(applied, &plan_write(&parse_state(&original)).unwrap());

    assert_eq!(restored.proxy_server, None);
    assert_eq!(restored.proxy_override, None);
    assert_eq!(restored.auto_config_url, None);
}
