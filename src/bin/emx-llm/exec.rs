//! Exec subcommand - execute TCL scripts

use anyhow::{Context, Result};
use std::path::Path;
use rtcl_core::Interp;

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
    interp.set_var("argv", argv_list)?;

    // Source and execute the script
    interp.eval(&format!("source {{{}}}", script_path.display()))
        .with_context(|| format!("Failed to execute script: {}", script))?;

    Ok(())
}
