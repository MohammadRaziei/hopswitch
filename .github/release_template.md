## Hopswitch VERSION_STRING

BUILD_NOTE

Switch the system proxy and DNS from the command line. Download the file for your system from the
assets below, or from the [release page](https://github.com/MohammadRaziei/hopswitch/releases/tag/RELEASE_NAME).

| System | File |
| :--- | :--- |
| **Linux** (any distribution, static) | `hopswitch-VERSION_STRING-x86_64-unknown-linux-musl.tar.gz` |
| **Debian / Ubuntu** | `hopswitch_VERSION_STRING-1_amd64.deb` |
| **Fedora / RHEL / openSUSE** | `hopswitch-VERSION_STRING-1.x86_64.rpm` |
| **macOS** (Apple silicon) | `hopswitch-VERSION_STRING-aarch64-apple-darwin.tar.gz` |
| **macOS** (Intel) | `hopswitch-VERSION_STRING-x86_64-apple-darwin.tar.gz` |
| **Windows** | `hopswitch-VERSION_STRING-x86_64-pc-windows-msvc.zip` or the `.msi` installer |

```bash
# Debian / Ubuntu
sudo dpkg -i hopswitch_VERSION_STRING-1_amd64.deb

# Fedora / RHEL
sudo rpm -i hopswitch-VERSION_STRING-1.x86_64.rpm

# Any Linux or macOS: unpack and put `hopswitch` somewhere on your PATH
tar xzf hopswitch-VERSION_STRING-<system>.tar.gz
```

Check a download with `sha256sum -c SHA256SUMS`. The programs are not code-signed yet, so Windows
and macOS may warn about a file downloaded with a browser.
