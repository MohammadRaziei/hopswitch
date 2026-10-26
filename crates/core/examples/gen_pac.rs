//! Builds the PAC scripts of the Rust generator from the two public lists, for the Windows PAC
//! benchmark (`.github/workflows/pac-bench.yml`): the same lists, the same sample hosts and the same
//! expected answers as the Python spike, so the real product output is what `WinHTTP` is asked about.
//!
//! ```text
//! cargo run -p hopswitch-core --example gen_pac -- --iran iran.txt --fod fod.txt --out build/
//! ```
//!
//! Reads `build/samples.json` (written by `tools/pac-bench/make_pac.py`), adds the variants
//! `rust-bucket-ir` and `rust-fod` to it, and writes their `.pac` files next to it.

use std::error::Error;
use std::fs;
use std::path::PathBuf;

use hopswitch_core::{ListFormat, PacPlan, PacRoute, ProxyConfig, ProxyScheme, RuleList};
use serde_json::Value;

/// The benchmark never connects through it; `PROXY` is the one keyword whose answer the harness reads.
fn placeholder_proxy() -> Result<PacRoute, Box<dyn Error>> {
    Ok(PacRoute::Proxy(ProxyConfig::new(
        ProxyScheme::Http,
        "127.0.0.1",
        9,
    )?))
}

fn arg(args: &[String], name: &str) -> Result<PathBuf, Box<dyn Error>> {
    let at = args
        .iter()
        .position(|a| a == name)
        .ok_or_else(|| format!("missing {name}"))?;
    Ok(PathBuf::from(
        args.get(at + 1)
            .ok_or_else(|| format!("{name} needs a value"))?,
    ))
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (iran, fod, out) = (
        arg(&args, "--iran")?,
        arg(&args, "--fod")?,
        arg(&args, "--out")?,
    );

    let (mut iran_list, _) = RuleList::import(&fs::read_to_string(iran)?, ListFormat::Plain);
    iran_list.add_suffix("ir")?;
    let (fod_list, _) = RuleList::import(&fs::read_to_string(fod)?, ListFormat::Dotted);

    let iran_pac = PacPlan::new(placeholder_proxy()?)
        .with_list(iran_list, PacRoute::Direct)
        .generate()?;
    let fod_pac = PacPlan::new(PacRoute::Direct)
        .with_list(fod_list, placeholder_proxy()?)
        .generate()?;
    fs::write(out.join("rust-bucket-ir.pac"), &iran_pac)?;
    fs::write(out.join("rust-fod.pac"), &fod_pac)?;

    // Same lists and semantics as the Python variants, so they have the same expected answers.
    let samples_path = out.join("samples.json");
    let mut samples: Value = serde_json::from_slice(&fs::read(&samples_path)?)?;
    for (new, old) in [("rust-bucket-ir", "bucket-ir"), ("rust-fod", "fod")] {
        let expected = samples["expect"][old].clone();
        samples["expect"][new] = expected;
    }
    fs::write(&samples_path, serde_json::to_vec(&samples)?)?;

    println!(
        "rust-bucket-ir: {} bytes, rust-fod: {} bytes",
        iran_pac.len(),
        fod_pac.len()
    );
    Ok(())
}
