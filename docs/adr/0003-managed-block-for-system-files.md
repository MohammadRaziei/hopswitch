# ADR-0003: Edit foreign system files only through a managed block

* **Status:** accepted
* **Context:** On Linux, DNS lives in `/etc/resolv.conf`, which users and other tools also edit
  (extra nameservers, `search`, `options`, comments). A naive "write the file" approach could destroy
  that content. On modern distros the file is often a symlink owned by `systemd-resolved` or
  NetworkManager, so editing it is either pointless or harmful.
* **Decision:**
  1. Prefer the owning service's API (`resolvectl`, `nmcli`) when it manages DNS.
  2. When a plain static file must be edited, write **one marked block at the top** using
     `hopswitch_core::managed_block`. Everything outside the block is preserved byte-for-byte; nothing
     is ever deleted; deactivation removes only our block and restores the original exactly.
  3. Refuse to touch a file whose markers are damaged (unterminated, stray, duplicated).
  4. Keep a full backup copy before the first write; write atomically (temp file + rename in the same
     directory) and preserve permissions and ownership; never replace a symlink with a regular file.
* **Consequences:** a little more code than overwriting the file; in exchange, the worst case is a
  refused operation with a clear error, never lost user data. The property is covered by tests
  (`crates/core/tests/managed_block.rs`), including exact round-trip restoration.
* **Known limit:** glibc uses only the first **3** `nameserver` lines of `resolv.conf`. The Linux
  adapter must warn or truncate when a profile has more than 3 DNS servers (profiles allow up to 4
  because Windows and macOS accept more).
