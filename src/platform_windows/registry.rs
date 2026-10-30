//! Pure logic for the Windows proxy: reading the registry values into a
//! [`ProxyState`], planning the writes for a desired state, and rendering a
//! `PowerShell` script. No process is started here, so it is fully unit-testable.
//!
//! Windows keeps the user proxy under
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings`:
//! `ProxyEnable` (0/1), `ProxyServer` (`host:port` = HTTP for all protocols,
//! `socks=host:port` = SOCKS), `ProxyOverride` (`;`-separated bypass list),
//! `AutoConfigURL` (PAC script) and `AutoDetect` (WPAD).

use crate::core::{PacConfig, ProxyConfig, ProxyScheme, ProxyState, SystemError};
use serde::{Deserialize, Serialize};

/// The registry key that holds the user's proxy settings.
const KEY: &str = r"HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings";

/// The five registry values we care about; `None` = the value does not exist.
///
/// Also the payload of [`ProxyState::Other`], so a foreign state can be written
/// back value for value.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryValues {
    /// `ProxyEnable`: 1 when "use a proxy server" is switched on.
    #[serde(rename = "ProxyEnable", default)]
    pub proxy_enable: Option<u32>,
    /// `ProxyServer`: `host:port`, `socks=host:port` or per-protocol entries.
    #[serde(rename = "ProxyServer", default)]
    pub proxy_server: Option<String>,
    /// `ProxyOverride`: the bypass list, `;`-separated.
    #[serde(rename = "ProxyOverride", default)]
    pub proxy_override: Option<String>,
    /// `AutoConfigURL`: the address of a PAC script.
    #[serde(rename = "AutoConfigURL", default)]
    pub auto_config_url: Option<String>,
    /// `AutoDetect`: automatic proxy detection (WPAD).
    #[serde(rename = "AutoDetect", default)]
    pub auto_detect: Option<u32>,
}

/// One registry change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// Set a string value.
    SetString(&'static str, String),
    /// Set a DWORD value.
    SetDword(&'static str, u32),
    /// Delete a value (it is fine if it does not exist).
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
///
/// [`ProxyState::Disabled`] means nothing is switched on *and nothing is stored*. Windows keeps
/// the proxy address and bypass list when the proxy is switched off; that is a state of its own
/// (`Other { enabled: false }`), so applying a proxy and restoring never loses it.
pub fn parse_state(values: &RegistryValues) -> ProxyState {
    let foreign = |enabled: bool| ProxyState::Other {
        raw: serde_json::to_string(values).unwrap_or_default(),
        enabled,
    };
    let stored = |value: &Option<String>| value.as_deref().is_some_and(|s| !s.is_empty());

    if let Some(url) = values.auto_config_url.as_deref().filter(|s| !s.is_empty()) {
        // A PAC script is its own state only when no manual proxy is in use or stored, so that
        // writing the state back reproduces the registry. Like every other state, it leaves the
        // auto-detect value (`AutoDetect`) alone, whatever it is.
        let alone = values.proxy_enable.unwrap_or(0) == 0
            && !stored(&values.proxy_server)
            && !stored(&values.proxy_override);
        if alone {
            if let Ok(pac) = PacConfig::new(url) {
                return ProxyState::Pac(pac);
            }
        }
        return foreign(true);
    }
    match values.proxy_enable {
        None | Some(0) => {
            if stored(&values.proxy_server) || stored(&values.proxy_override) {
                foreign(false)
            } else {
                ProxyState::Disabled
            }
        }
        Some(1) => values
            .proxy_server
            .as_deref()
            .and_then(|server| parse_server(server, values.proxy_override.as_deref()))
            .map_or_else(|| foreign(true), ProxyState::Manual),
        Some(_) => foreign(true),
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
        // "Nothing switched on, nothing stored": also removes the address and bypass list a
        // proxy we applied left behind. A leftover that was there before is `Other` instead.
        ProxyState::Disabled => Ok(vec![
            Op::SetDword("ProxyEnable", 0),
            Op::Remove("AutoConfigURL"),
            Op::Remove("ProxyServer"),
            Op::Remove("ProxyOverride"),
        ]),
        ProxyState::Manual(config) => plan_manual(config),
        // The manual proxy is cleared first and the address is written last, so the script only
        // becomes active once no manual proxy is left to compete with it. `AutoDetect` is not
        // touched, as with a manual proxy, so a snapshot of any state restores it exactly.
        ProxyState::Pac(pac) => Ok(vec![
            Op::SetDword("ProxyEnable", 0),
            Op::Remove("ProxyServer"),
            Op::Remove("ProxyOverride"),
            Op::SetString("AutoConfigURL", pac.url().to_owned()),
        ]),
        ProxyState::Other { raw, .. } => {
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
