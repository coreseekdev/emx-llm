//! emx-llm binary entry point

use anyhow::Result;

mod cli;
mod chat;
mod dev;
mod env;
mod exec;
mod test_cmd;
mod tools;

use clap::Parser;
use cli::{Cli, Commands};
use env::MetadataOptions;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Chat {
            session,
            prompt,
            model,
            api_base,
            stream,
            no_stream,
            system,
            dry_run,
            token_stats,
            attach,
            tools,
            raw,
        } => {
            chat::run(
                session,
                prompt,
                model,
                api_base,
                stream,
                no_stream,
                system,
                dry_run,
                token_stats,
                attach,
                tools,
                raw,
            )?;
        }
        Commands::Test { provider } => {
            test_cmd::run(provider)?;
        }
        Commands::Env {
            format,
            files,
            git,
            env_vars,
            all,
            size,
            mtime,
            ctime,
            full,
            verbose,
        } => {
            let include_files = files || all || verbose;
            let include_git = git || all || verbose;
            let include_env = env_vars || all || verbose;
            let meta_opts = MetadataOptions {
                show_size: size || full || verbose,
                show_mtime: mtime || full || verbose,
                show_ctime: ctime || full || verbose,
            };
            env::run(format, include_files, include_git, include_env, meta_opts, verbose)?;
        }
        Commands::Dev { all, format } => {
            dev::run(all, format)?;
        }
        Commands::Tools {
            info,
            json,
            args,
        } => {
            tools::run(info, json, args)?;
        }
        Commands::Exec { script, args } => {
            exec::run(&script, &args)?;
        }
    }

    Ok(())
}
