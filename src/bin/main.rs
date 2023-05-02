use std::process::{exit, Command};

use tracing::metadata::LevelFilter;
use tracing_subscriber::{fmt::format::FmtSpan, EnvFilter};

fn init_tracing() {
    let env_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy();
    let timer = tracing_subscriber::fmt::time::uptime();
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_level(false)
        .with_timer(timer)
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .compact();
    subscriber.init();
}

fn main() {
    init_tracing();

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
            if let Err(errors) = cargo_px::codegen(&cargo_path) {
                for error in errors {
                    eprintln!("Something went wrong during code generation.\n{}", error);
                }
                exit(1);
            }
        }
    }

    let mut cmd = Command::new(cargo_path);
    cmd.args(forwarded_args);
    let status = match cmd.status() {
        Ok(status) => status,
        Err(e) => {
            eprintln!("Error executing command: {:?}", e);
            exit(1);
        }
    };

    exit(status.code().unwrap_or(1));
}
