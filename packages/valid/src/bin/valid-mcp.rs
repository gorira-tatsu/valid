use std::{env, process};

use valid::mcp::{serve_stdio, ServerConfig};

fn main() {
    let mut config = ServerConfig::default();
    let mut args = env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--registry-binary" => {
                config.default_registry_binary = Some(next_arg(&mut args, "--registry-binary"))
            }
            "--model-file" => config.default_model_file = Some(next_arg(&mut args, "--model-file")),
            "--name" => config.server_name = next_arg(&mut args, "--name"),
            "--help" | "-h" => usage_exit(0),
            _ => usage_exit(3),
        }
    }

    if let Err(message) = serve_stdio(config) {
        eprintln!("{message}");
        process::exit(1);
    }
}

fn next_arg(args: &mut impl Iterator<Item = String>, flag: &str) -> String {
    args.next().unwrap_or_else(|| {
        eprintln!("missing value for {flag}");
        process::exit(3);
    })
}

fn usage_exit(code: i32) -> ! {
    eprintln!(
        "usage: valid-mcp [--registry-binary <path>] [--model-file <path>] [--name <server-name>]"
    );
    process::exit(code);
}
