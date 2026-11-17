//! `horizon-open-relay`: let Horizon Open Creative players join a vanilla or
//! Paper 26.3 server (docs/open-creative.md, "Joining a Java server through
//! the relay").
//!
//! ```text
//! horizon-open-relay vanilla-data --out DIR [--version 26.3] [--jar server.jar] [--java java]
//! horizon-open-relay serve --upstream HOST:PORT --open-resources DIR
//!                          (--vanilla-reports DIR | --vanilla-data DIR)
//!                          [--listen 0.0.0.0:25565] [--max-connections 50] [--debug]
//! horizon-open-relay mapping --open-resources DIR --vanilla-reports DIR
//! ```
//! Every flag also reads from the environment: `HORIZON_RELAY_<FLAG>` (for
//! example `HORIZON_RELAY_UPSTREAM`), so a container needs no arguments.
//! `--vanilla-data DIR` makes the reports in DIR on first start (Mojang's
//! server jar, checked against its SHA-1, and Java on the PATH) and reuses them.
use horizon_open_relay::{relay, tables::Tables, vanilla};
use std::{path::PathBuf, sync::Arc};

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == &format!("--{name}"))
        .and_then(|i| args.get(i + 1).cloned())
        .or_else(|| {
            let env = format!("HORIZON_RELAY_{}", name.to_uppercase().replace('-', "_"));
            std::env::var(env).ok().filter(|v| !v.is_empty())
        })
}
fn required(args: &[String], name: &str) -> Result<String, String> {
    flag(args, name).ok_or_else(|| format!("--{name} is required"))
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().cloned().unwrap_or_else(|| "serve".into());
    let reports = || -> Result<PathBuf, String> {
        if let Some(reports) = flag(&args, "vanilla-reports") {
            return Ok(PathBuf::from(reports));
        }
        let dir = PathBuf::from(
            flag(&args, "vanilla-data").ok_or("--vanilla-reports or --vanilla-data is required")?,
        );
        let reports = dir.join("reports");
        if reports.join("blocks.json").is_file() && reports.join("registries.json").is_file() {
            return Ok(reports);
        }
        let version =
            flag(&args, "version").unwrap_or_else(|| horizon_core::VANILLA_VERSION.into());
        eprintln!(
            "horizon-open-relay: deriving the {version} reports in {}",
            dir.display()
        );
        let jar = vanilla::fetch_server_jar(&version, &dir)?;
        vanilla::generate_reports(
            &jar,
            &dir,
            &flag(&args, "java").unwrap_or_else(|| "java".into()),
        )
    };
    let load = || {
        let tables = Tables::load(
            &reports()?,
            &PathBuf::from(required(&args, "open-resources")?),
        )?;
        eprintln!(
            "horizon-open-relay: {} vanilla block states ({} of them substitutes), {} items, brand {}",
            tables.states.len(),
            tables.substituted_blocks,
            tables.items.len(),
            tables.brand
        );
        Ok::<_, String>(tables)
    };
    match command.as_str() {
        "vanilla-data" => {
            let out = PathBuf::from(required(&args, "out")?);
            let version =
                flag(&args, "version").unwrap_or_else(|| horizon_core::VANILLA_VERSION.into());
            let java = flag(&args, "java").unwrap_or_else(|| "java".into());
            let jar = match flag(&args, "jar") {
                Some(jar) => PathBuf::from(jar),
                None => vanilla::fetch_server_jar(&version, &out)?,
            };
            let reports = vanilla::generate_reports(&jar, &out, &java)?;
            println!("{}", reports.display());
            Ok(())
        }
        "mapping" => {
            let tables = load()?;
            println!(
                "vanilla blocks shown as air: {}",
                tables.hidden_blocks.len()
            );
            for name in &tables.hidden_blocks {
                println!("  {name}");
            }
            Ok(())
        }
        "serve" => {
            let tables = Arc::new(load()?);
            let config = relay::Config {
                listen: flag(&args, "listen").unwrap_or_else(|| "0.0.0.0:25565".into()),
                upstream: required(&args, "upstream")?,
                max_connections: flag(&args, "max-connections")
                    .map(|v| {
                        v.parse()
                            .map_err(|_| "invalid --max-connections".to_owned())
                    })
                    .transpose()?
                    .unwrap_or(50),
                debug: args.iter().any(|a| a == "--debug")
                    || std::env::var_os("HORIZON_RELAY_DEBUG").is_some(),
            };
            relay::serve(config, tables)
        }
        other => Err(format!(
            "unknown command {other} (serve, vanilla-data, mapping)"
        )),
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("horizon-open-relay: {e}");
        std::process::exit(1);
    }
}
