//! Exec subcommand - execute TCL scripts

use anyhow::{Context, Result};
use std::path::Path;
use rtcl_core::Interp;

/// Convert an rtcl error to anyhow by stringifying it.
fn tcl_err(e: rtcl_core::Error) -> anyhow::Error {
    anyhow::anyhow!("{}", e)
}

/// Run the exec subcommand
pub fn run(script: &str, args: &[String]) -> Result<()> {
    let script_path = Path::new(script);

    if !script_path.exists() {
        anyhow::bail!("Script not found: {}", script);
    }

    // Create TCL interpreter
    let mut interp = Interp::new();

    // Set up argv global variable
    let argv_list = rtcl_core::Value::from_list_cached(
        args.iter().map(|a| rtcl_core::Value::from_str(a)).collect()
    );
    interp.set_var("argv", argv_list).map_err(tcl_err)?;

    // Source and execute the script
    interp.eval(&format!("source {{{}}}", script_path.display()))
        .map_err(tcl_err)
        .with_context(|| format!("Failed to execute script: {}", script))?;

    Ok(())
}
