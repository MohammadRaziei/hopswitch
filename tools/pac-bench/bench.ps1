<#
.SYNOPSIS
  Runs the generated PAC files through WinHTTP's real PAC engine and checks every answer.

.DESCRIPTION
  For each variant: serve build/<variant>.pac from 127.0.0.1, ask WinHttpGetProxyForUrl (with an
  explicit PAC URL, so the registry is not touched) for every host in samples.json, compare with
  the expected answer, and time it. The first lookup includes downloading and compiling the script.
  A variant that cannot be loaded (for example because it is too big) is reported, not hidden.

  Exit code 1 only if the variants we intend to ship (bucket-ir, fod, and the same two built by the Rust
  generator: rust-bucket-ir, rust-fod) fail or give a wrong answer.

  After the variants it asks what WinHTTP makes of the PAC keywords PROXY, HTTPS, SOCKS, SOCKS4 and
  SOCKS5. That table is information only and never fails the run.
#>
param(
  [Parameter(Mandatory = $true)][string]$PacDir,
  [int]$Port = 8765,
  [string[]]$Variants = @('object-full', 'object-ir', 'bucket-ir', 'fod', 'rust-bucket-ir', 'rust-fod'),
  [string[]]$MustPass = @('bucket-ir', 'fod', 'rust-bucket-ir', 'rust-fod')
)
$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @"
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;

public static class WinHttpPac
{
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    struct AutoProxyOptions
    {
        public uint Flags;
        public uint AutoDetectFlags;
        public string AutoConfigUrl;
        public IntPtr Reserved1;
        public uint Reserved2;
        [MarshalAs(UnmanagedType.Bool)] public bool AutoLogonIfChallenged;
    }

    [StructLayout(LayoutKind.Sequential)]
    struct ProxyInfo
    {
        public uint AccessType;
        public IntPtr Proxy;
        public IntPtr ProxyBypass;
    }

    [DllImport("winhttp.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern IntPtr WinHttpOpen(string agent, uint accessType, string proxy, string bypass, uint flags);

    [DllImport("winhttp.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    static extern bool WinHttpGetProxyForUrl(IntPtr session, string url, ref AutoProxyOptions options, out ProxyInfo info);

    [DllImport("winhttp.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    static extern bool WinHttpCloseHandle(IntPtr handle);

    [DllImport("kernel32.dll")]
    static extern IntPtr GlobalFree(IntPtr memory);

    const uint WINHTTP_ACCESS_TYPE_NO_PROXY = 1;
    const uint WINHTTP_AUTOPROXY_CONFIG_URL = 0x2;
    const uint WINHTTP_ACCESS_TYPE_NAMED_PROXY = 3;

    public static IntPtr Open()
    {
        IntPtr session = WinHttpOpen("hopswitch-pac-bench", WINHTTP_ACCESS_TYPE_NO_PROXY, null, null, 0);
        if (session == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
        return session;
    }

    public static void Close(IntPtr session) { WinHttpCloseHandle(session); }

    /// <summary>What WinHTTP returned, unmodified: the access type and the proxy string.</summary>
    public static string ResolveRaw(IntPtr session, string url, string pacUrl)
    {
        AutoProxyOptions options = new AutoProxyOptions();
        options.Flags = WINHTTP_AUTOPROXY_CONFIG_URL;
        options.AutoConfigUrl = pacUrl;
        options.AutoLogonIfChallenged = false;
        ProxyInfo info;
        if (!WinHttpGetProxyForUrl(session, url, ref options, out info))
            throw new Win32Exception(Marshal.GetLastWin32Error());
        string text = "type=" + info.AccessType + " proxy=" + (info.Proxy == IntPtr.Zero ? "" : Marshal.PtrToStringUni(info.Proxy));
        if (info.Proxy != IntPtr.Zero) GlobalFree(info.Proxy);
        if (info.ProxyBypass != IntPtr.Zero) GlobalFree(info.ProxyBypass);
        return text;
    }

    /// <summary>"DIRECT" or "PROXY host:port", as the PAC script decided.</summary>
    public static string Resolve(IntPtr session, string url, string pacUrl)
    {
        AutoProxyOptions options = new AutoProxyOptions();
        options.Flags = WINHTTP_AUTOPROXY_CONFIG_URL;
        options.AutoConfigUrl = pacUrl;
        options.AutoLogonIfChallenged = false;
        ProxyInfo info;
        if (!WinHttpGetProxyForUrl(session, url, ref options, out info))
            throw new Win32Exception(Marshal.GetLastWin32Error());
        string answer = info.AccessType == WINHTTP_ACCESS_TYPE_NAMED_PROXY
            ? "PROXY " + Marshal.PtrToStringUni(info.Proxy)
            : "DIRECT";
        if (info.Proxy != IntPtr.Zero) GlobalFree(info.Proxy);
        if (info.ProxyBypass != IntPtr.Zero) GlobalFree(info.ProxyBypass);
        return answer;
    }
}
"@

$samples = Get-Content -Raw -Path (Join-Path $PacDir 'samples.json') | ConvertFrom-Json
$hosts = @($samples.hosts)

$server = Start-Process -FilePath 'python' `
  -ArgumentList @('-m', 'http.server', "$Port", '--bind', '127.0.0.1', '--directory', $PacDir) `
  -PassThru -WindowStyle Hidden
$rows = @()
$probe = @()
$failed = $false
try {
  # wait until the server answers
  for ($i = 0; $i -lt 20; $i++) {
    try { Invoke-WebRequest -Uri "http://127.0.0.1:$Port/samples.json" -UseBasicParsing | Out-Null; break }
    catch { Start-Sleep -Milliseconds 250 }
  }

  foreach ($variant in $Variants) {
    $pacFile = Join-Path $PacDir "$variant.pac"
    $size = (Get-Item $pacFile).Length
    $pacUrl = "http://127.0.0.1:$Port/$variant.pac"
    $want = @($samples.expect.PSObject.Properties[$variant].Value)
    $row = [ordered]@{ variant = $variant; bytes = $size; first_ms = ''; lookup_us = ''; wrong = ''; status = '' }
    $session = [WinHttpPac]::Open()
    try {
      $sw = [Diagnostics.Stopwatch]::StartNew()
      [void][WinHttpPac]::Resolve($session, 'http://example.com/', $pacUrl)   # download + compile
      $row.first_ms = [math]::Round($sw.Elapsed.TotalMilliseconds, 1)

      $wrong = 0
      $ticks = 0
      for ($i = 0; $i -lt $hosts.Count; $i++) {
        $sw.Restart()
        $got = [WinHttpPac]::Resolve($session, "http://$($hosts[$i])/", $pacUrl)
        $ticks += $sw.ElapsedTicks
        if ($got -ne $want[$i]) {
          $wrong++
          if ($wrong -le 5) { Write-Host "  [$variant] $($hosts[$i]): expected $($want[$i]), got $got" }
        }
      }
      $row.lookup_us = [math]::Round(($ticks / [Diagnostics.Stopwatch]::Frequency) * 1e6 / $hosts.Count, 1)
      $row.wrong = $wrong
      $row.status = if ($wrong -eq 0) { 'ok' } else { 'WRONG ANSWERS' }
    }
    catch {
      $code = if ($_.Exception.InnerException -is [System.ComponentModel.Win32Exception]) { $_.Exception.InnerException.NativeErrorCode } else { '' }
      $row.status = "FAILED: $($_.Exception.Message) $code".Trim()
    }
    finally { [WinHttpPac]::Close($session) }

    if ($row.status -ne 'ok' -and $MustPass -contains $variant) { $failed = $true }
    $rows += [pscustomobject]$row
  }

  # Information only: what does WinHTTP return for each PAC keyword? (Hopswitch's proxies are mostly SOCKS5.)
  $probeDir = Join-Path $PacDir 'probe'
  New-Item -ItemType Directory -Force -Path $probeDir | Out-Null
  foreach ($keyword in @('PROXY', 'HTTPS', 'SOCKS', 'SOCKS4', 'SOCKS5')) {
    $file = "keyword-$($keyword.ToLower()).pac"
    $body = 'function FindProxyForURL(url, host) { return "' + $keyword + ' 127.0.0.1:9"; }'
    Set-Content -Path (Join-Path $probeDir $file) -Value $body -Encoding ascii
    $session = [WinHttpPac]::Open()
    try { $raw = [WinHttpPac]::ResolveRaw($session, 'http://example.com/', "http://127.0.0.1:$Port/probe/$file") }
    catch { $raw = "FAILED: $($_.Exception.Message)" }
    finally { [WinHttpPac]::Close($session) }
    $probe += [pscustomobject]@{ keyword = "$keyword 127.0.0.1:9"; winhttp_returned = $raw }
  }

  # Information only: what does WinHTTP say when the PAC address is dead, as after a crash of the
  # process that served it? (Nothing listens on port 1.)
  $session = [WinHttpPac]::Open()
  try { $dead = [WinHttpPac]::ResolveRaw($session, 'http://example.com/', 'http://127.0.0.1:1/hopswitch.pac') }
  catch { $dead = "FAILED: $($_.Exception.Message)" }
  finally { [WinHttpPac]::Close($session) }
  $probe += [pscustomobject]@{ keyword = 'a PAC address nothing listens on'; winhttp_returned = $dead }
}
finally {
  Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue
}

$rows | Format-Table -AutoSize | Out-String | Write-Host
$probe | Format-Table -AutoSize | Out-String | Write-Host

$lines = @(
  '### PAC scripts through the real WinHTTP engine',
  '',
  "$($hosts.Count) sample hosts per variant. 'first' includes downloading and compiling the script.",
  '',
  '| variant | bytes | first lookup (ms) | mean lookup (us) | wrong | status |',
  '| --- | ---: | ---: | ---: | ---: | --- |'
)
foreach ($r in $rows) {
  $lines += "| $($r.variant) | $('{0:N0}' -f $r.bytes) | $($r.first_ms) | $($r.lookup_us) | $($r.wrong) | $($r.status) |"
}
$lines += @('', '### What WinHTTP returns for each PAC keyword, and for a dead PAC address', '', '| keyword in the script | WinHTTP returned |', '| --- | --- |')
foreach ($p in $probe) { $lines += "| $($p.keyword) | $($p.winhttp_returned) |" }
$summary = $lines -join "`n"
# Also kept as a file: the workflow's report job publishes it (a step summary cannot be read by tools).
Set-Content -Path (Join-Path $PacDir 'summary.md') -Value $summary -Encoding utf8
if ($env:GITHUB_STEP_SUMMARY) { Add-Content -Path $env:GITHUB_STEP_SUMMARY -Value $summary }

if ($failed) { Write-Error "A variant we intend to ship failed: see the table above."; exit 1 }
