use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use tracing_subscriber::EnvFilter;
use xwayclip::Config;

const MIB: usize = 1024 * 1024;

#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// Maximum size accepted for a single clipboard format, in MiB
    #[arg(long, default_value_t = 256)]
    max_target_size_mib: usize,

    /// Maximum combined size accepted for one clipboard snapshot, in MiB
    #[arg(long, default_value_t = 512)]
    max_total_size_mib: usize,

    /// Maximum time to wait for each clipboard target transfer
    #[arg(long, default_value_t = 5_000)]
    transfer_timeout_ms: u64,
}

fn main() -> Result<()> {
    init_tracing();

    let args = Args::parse();
    let config = config_from_args(&args)?;

    xwayclip::run(config)
}

fn init_tracing() {
    let default_filter = if cfg!(debug_assertions) {
        "xwayclip=debug,warn"
    } else {
        "xwayclip=info,warn"
    };
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}

fn config_from_args(args: &Args) -> Result<Config> {
    let max_target_bytes = args
        .max_target_size_mib
        .checked_mul(MIB)
        .context("--max-target-size-mib is too large")?;
    let max_total_bytes = args
        .max_total_size_mib
        .checked_mul(MIB)
        .context("--max-total-size-mib is too large")?;

    Config::new(
        max_target_bytes,
        max_total_bytes,
        Duration::from_millis(args.transfer_timeout_ms),
    )
}
