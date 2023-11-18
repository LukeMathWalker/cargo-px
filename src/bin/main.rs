use anyhow::Context;
use cargo_px::Shell;
use std::process::{exit, Command};
use tracing_subscriber::{fmt::format::FmtSpan, EnvFilter};

/// The name of the environment variable that can be used to enable (and configure) `tracing`
/// output for `cargo px`.
static TRACING_ENV_VAR: &str = "CARGO_PX_LOG";

fn init_tracing() -> Result<(), anyhow::Error> {
    // We don't want to show `tracing` data to users as they go about their business, so we
    // require them to explicitly opt-in to it.
    if !std::env::var(TRACING_ENV_VAR).is_ok() {
        return Ok(());
    }
    let env_filter = EnvFilter::builder()
        .with_env_var(TRACING_ENV_VAR)
        .from_env()?;
    let timer = tracing_subscriber::fmt::time::uptime();
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_level(false)
        .with_timer(timer)
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .compact();
    subscriber.init();
    Ok(())
}

fn main() {
    let mut shell = Shell::new();
    if let Err(e) = init_tracing().context("Failed to initialize `tracing`'s subscriber") {
        let _ = display_error(&e, &mut shell);
        exit(1)
    }

    let cargo_path = std::env::var("CARGO").expect(
        "The `CARGO` environment variable was not set. \
        This is unexpected: it should always be provided by `cargo` when \
        invoking a custom sub-command, allowing `cargo-px` to correctly detect \
        which toolchain should be used. Please file a bug.",
    );
    // The first arg is always `cargo` and the second arg is always the name
    // of the sub-command, i.e. `px` in our case.
    let forwarded_args: Vec<_> = std::env::args().skip(2).collect();

    if let Some(cargo_command) = forwarded_args.first() {
        // If the user is invoking a command whose outcome might be affected by code generation,
        // we need to perform code generation first.
        if [
            "build", "b", "test", "t", "check", "c", "run", "r", "doc", "d", "bench", "publish",
        ]
        .contains(&cargo_command.as_str())
        {
            if let Err(errors) = cargo_px::codegen(&cargo_path, &mut shell) {
                for error in errors {
                    let _ = display_error(&error, &mut shell);
                }
                let _ = shell.error("Something went wrong during code generation");
                exit(1);
            }
        }
    }

    let mut cmd = Command::new(cargo_path);
    cmd.args(forwarded_args);
    let status = match cmd.status().context("Failed to execute `cargo` command") {
        Ok(status) => status,
        Err(e) => {
            let _ = display_error(&e, &mut shell);
            exit(1);
        }
    };

    exit(status.code().unwrap_or(1));
}

fn display_error(error: &anyhow::Error, shell: &mut Shell) -> Result<(), anyhow::Error> {
    shell.error(error)?;
    for cause in error.chain().skip(1) {
        writeln!(shell.err(), "\n  Caused by:")?;
        write!(
            shell.err(),
            "{}",
            textwrap::indent(&cause.to_string(), "    ")
        )?;
    }
    Ok(())
}
