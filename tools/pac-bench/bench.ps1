<#
.SYNOPSIS
  Runs the generated PAC files through WinHTTP's real PAC engine and checks every answer.

.DESCRIPTION
  For each variant: serve build/<variant>.pac from 127.0.0.1, ask WinHttpGetProxyForUrl (with an
  explicit PAC URL, so the registry is not touched) for every host in samples.json, compare with
  the expected answer, and time it. The first lookup includes downloading and compiling the script.
  A variant that cannot be loaded (for example because it is too big) is reported, not hidden.

  Exit code 1 only if the variants we intend to ship (bucket-ir, fod) fail or give a wrong answer.
#>
param(
  [Parameter(Mandatory = $true)][string]$PacDir,
  [int]$Port = 8765,
  [string[]]$Variants = @('object-full', 'object-ir', 'bucket-ir', 'fod'),
  [string[]]$MustPass = @('bucket-ir', 'fod')
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
}
finally {
  Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue
}

$rows | Format-Table -AutoSize | Out-String | Write-Host

if ($env:GITHUB_STEP_SUMMARY) {
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
  Add-Content -Path $env:GITHUB_STEP_SUMMARY -Value ($lines -join "`n")
}

if ($failed) { Write-Error "A variant we intend to ship failed: see the table above."; exit 1 }
