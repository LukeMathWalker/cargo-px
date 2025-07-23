use anyhow::Context;
use cargo_px::{Shell, Verbosity};
use std::process::{exit, Command};
use tracing_subscriber::{fmt::format::FmtSpan, EnvFilter};

/// The name of the environment variable that can be used to enable (and configure) `tracing`
/// output for `cargo px`.
static TRACING_ENV_VAR: &str = "CARGO_PX_LOG";

fn init_tracing() -> Result<(), anyhow::Error> {
    // We don't want to show `tracing` data to users as they go about their business, so we
    // require them to explicitly opt-in to it.
    if std::env::var(TRACING_ENV_VAR).is_err() {
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
        which toolchain should be used. \n\
        Make sure that you are invoking `cargo-px` as a `cargo` sub-command: `cargo px [...]` rather \
        than `cargo-px [...]` (notice the missing dash in the first one!). \n
        If you're invoking it as expected but it's showing this error message, please file a bug.",
    );
    let mut args = std::env::args();
    args.next(); // Skip the first argument, since it's always `cargo`
    let args: Vec<_> = args.collect();
    // Skip the `px` argument.
    let forwarded_args = &args[1..];

    let be_quiet = forwarded_args
        .iter()
        .any(|arg| arg == "--quiet" || arg == "-q");
    if be_quiet {
        shell.set_verbosity(Verbosity::Quiet);
    }

    let mut has_codegened = false;
    let cwd = std::env::current_dir().expect("Failed to get current working directory");
    if let Some(cargo_command) = forwarded_args.first() {
        // This is not a proxy for a `cargo` command, it is a `cargo-px` command.
        if "verify-freshness" == cargo_command.as_str() {
            if let Err(errors) = cargo_px::verify(&cargo_path, &cwd, &args, &mut shell) {
                for error in errors {
                    let _ = display_error(&error, &mut shell);
                }
                exit(1);
            }

            exit(0);
        }

        // If the user is invoking a command whose outcome might be affected by code generation,
        // we need to perform code generation first.
        if [
            "build", "b", "test", "t", "check", "c", "run", "r", "doc", "d", "bench", "publish",
        ]
        .contains(&cargo_command.as_str())
        {
            if let Err(errors) = cargo_px::codegen(&cargo_path, &cwd, &args, &mut shell) {
                for error in errors {
                    let _ = display_error(&error, &mut shell);
                }
                exit(1);
            }
            has_codegened = true;
        }
    }

    if has_codegened {
        if let Some(cargo_command) = forwarded_args.first() {
            let _ = shell.status("Invoking", format!("`cargo {cargo_command}`"));
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
