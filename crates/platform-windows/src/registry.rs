//! Pure logic for the Windows proxy: reading the registry values into a
//! [`ProxyState`], planning the writes for a desired state, and rendering a
//! `PowerShell` script. No process is started here, so it is fully unit-testable.
//!
//! Windows keeps the user proxy under
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings`:
//! `ProxyEnable` (0/1), `ProxyServer` (`host:port` = HTTP for all protocols,
//! `socks=host:port` = SOCKS), `ProxyOverride` (`;`-separated bypass list),
//! `AutoConfigURL` (PAC script) and `AutoDetect` (WPAD).

use hopswitch_core::{ProxyConfig, ProxyScheme, ProxyState, SystemError};
use serde::{Deserialize, Serialize};

const KEY: &str = r"HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings";

/// The five registry values we care about; `None` = the value does not exist.
///
/// Also the payload of [`ProxyState::Other`], so a foreign state can be written
/// back value for value.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryValues {
    #[serde(rename = "ProxyEnable", default)]
    pub proxy_enable: Option<u32>,
    #[serde(rename = "ProxyServer", default)]
    pub proxy_server: Option<String>,
    #[serde(rename = "ProxyOverride", default)]
    pub proxy_override: Option<String>,
    #[serde(rename = "AutoConfigURL", default)]
    pub auto_config_url: Option<String>,
    #[serde(rename = "AutoDetect", default)]
    pub auto_detect: Option<u32>,
}

/// One registry change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    SetString(&'static str, String),
    SetDword(&'static str, u32),
    Remove(&'static str),
}

/// `PowerShell` that prints the five values as compact JSON (UTF-8).
pub fn read_script() -> String {
    format!(
        "[Console]::OutputEncoding=[System.Text.Encoding]::UTF8; \
         $p=Get-ItemProperty -Path '{KEY}'; \
         [ordered]@{{ProxyEnable=$p.ProxyEnable;ProxyServer=$p.ProxyServer;\
         ProxyOverride=$p.ProxyOverride;AutoConfigURL=$p.AutoConfigURL;\
         AutoDetect=$p.AutoDetect}} | ConvertTo-Json -Compress"
    )
}

/// Classifies registry values. Anything Hopswitch cannot model exactly becomes
/// [`ProxyState::Other`] carrying every value, so it can be restored faithfully.
pub fn parse_state(values: &RegistryValues) -> ProxyState {
    let foreign = || ProxyState::Other {
        raw: serde_json::to_string(values).unwrap_or_default(),
    };

    if values
        .auto_config_url
        .as_deref()
        .is_some_and(|s| !s.is_empty())
    {
        return foreign();
    }
    match values.proxy_enable {
        None | Some(0) => ProxyState::Disabled,
        Some(1) => values
            .proxy_server
            .as_deref()
            .and_then(|server| parse_server(server, values.proxy_override.as_deref()))
            .map_or_else(foreign, ProxyState::Manual),
        Some(_) => foreign(),
    }
}

fn parse_server(server: &str, over: Option<&str>) -> Option<ProxyConfig> {
    let (scheme, endpoint) = match server.strip_prefix("socks=") {
        Some(rest) => (ProxyScheme::Socks5, rest),
        None => (ProxyScheme::Http, server),
    };
    // `a=b:1;c=d:2` (per-protocol proxies) and anything with several entries.
    if endpoint.is_empty() || endpoint.contains(['=', ';', ' ']) {
        return None;
    }

    let (host, port) = endpoint.rsplit_once(':')?;
    let host = match host.strip_prefix('[') {
        Some(inner) => inner.strip_suffix(']')?,
        None if host.contains(':') => return None, // bare IPv6 is ambiguous
        None => host,
    };
    let port: u16 = port.parse().ok()?;

    let bypass = over
        .unwrap_or_default()
        .split(';')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_owned)
        .collect();
    ProxyConfig::new(scheme, host, port)
        .ok()?
        .with_bypass(bypass)
        .ok()
}

/// The registry changes that make the system match `state`.
///
/// Order matters: the server is written before the proxy is enabled.
pub fn plan_write(state: &ProxyState) -> Result<Vec<Op>, SystemError> {
    match state {
        ProxyState::Disabled => Ok(vec![
            Op::SetDword("ProxyEnable", 0),
            Op::Remove("AutoConfigURL"),
        ]),
        ProxyState::Manual(config) => plan_manual(config),
        ProxyState::Other { raw } => {
            let values: RegistryValues = serde_json::from_str(raw)
                .map_err(|_| SystemError::Unsupported("not a Windows proxy snapshot".to_owned()))?;
            Ok(vec![
                string_op("ProxyServer", values.proxy_server),
                string_op("ProxyOverride", values.proxy_override),
                string_op("AutoConfigURL", values.auto_config_url),
                dword_op("AutoDetect", values.auto_detect),
                dword_op("ProxyEnable", values.proxy_enable),
            ])
        }
    }
}

fn string_op(name: &'static str, value: Option<String>) -> Op {
    value.map_or(Op::Remove(name), |v| Op::SetString(name, v))
}

fn dword_op(name: &'static str, value: Option<u32>) -> Op {
    value.map_or(Op::Remove(name), |v| Op::SetDword(name, v))
}

fn plan_manual(config: &ProxyConfig) -> Result<Vec<Op>, SystemError> {
    let prefix = match config.scheme() {
        ProxyScheme::Http => "",
        ProxyScheme::Socks5 => "socks=",
        other @ (ProxyScheme::Https | ProxyScheme::Socks4) => {
            return Err(SystemError::Unsupported(format!(
                "{other} proxies cannot be expressed in the Windows proxy settings"
            )));
        }
    };
    if let Some(bad) = config
        .bypass()
        .iter()
        .find(|entry| entry.contains(';') || entry.chars().any(char::is_control))
    {
        return Err(SystemError::Unsupported(format!(
            "bypass entry {bad:?} cannot be stored in the Windows proxy settings"
        )));
    }

    let host = config.host();
    let endpoint = if host.contains(':') {
        format!("{prefix}[{host}]:{}", config.port())
    } else {
        format!("{prefix}{host}:{}", config.port())
    };
    let over = (!config.bypass().is_empty()).then(|| config.bypass().join(";"));

    Ok(vec![
        Op::SetString("ProxyServer", endpoint),
        string_op("ProxyOverride", over),
        Op::Remove("AutoConfigURL"),
        Op::SetDword("ProxyEnable", 1),
    ])
}

/// `PowerShell` single-quoted literal. Besides `'`, `PowerShell` also treats the
/// typographic single quotes as quote characters; doubling escapes any of them.
fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('\'');
    for c in value.chars() {
        out.push(c);
        if matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}') {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

/// Renders the planned changes, followed by a best-effort `WinINet` refresh so
/// running applications pick the new settings up without a restart.
pub fn render_write(ops: &[Op]) -> String {
    let mut parts = vec![
        "$ErrorActionPreference='Stop'".to_owned(),
        format!("$k='{KEY}'"),
    ];
    for op in ops {
        parts.push(match op {
            Op::SetString(name, value) => format!(
                "Set-ItemProperty -Path $k -Name {} -Value {} -Type String",
                quote(name),
                quote(value)
            ),
            Op::SetDword(name, value) => format!(
                "Set-ItemProperty -Path $k -Name {} -Value {value} -Type DWord",
                quote(name)
            ),
            Op::Remove(name) => format!(
                "Remove-ItemProperty -Path $k -Name {} -ErrorAction SilentlyContinue",
                quote(name)
            ),
        });
    }
    // 39 = INTERNET_OPTION_SETTINGS_CHANGED, 37 = INTERNET_OPTION_REFRESH.
    parts.push(
        "try { $sig='[DllImport(\"wininet.dll\")] public static extern bool \
         InternetSetOption(IntPtr h,int o,IntPtr b,int l);'; \
         $w=Add-Type -MemberDefinition $sig -Name WinInet -Namespace Hopswitch -PassThru; \
         [void]$w::InternetSetOption([IntPtr]::Zero,39,[IntPtr]::Zero,0); \
         [void]$w::InternetSetOption([IntPtr]::Zero,37,[IntPtr]::Zero,0) } catch { }"
            .to_owned(),
    );
    parts.join("; ")
}
