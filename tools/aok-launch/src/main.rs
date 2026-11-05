//! aok-launch CLI dispatch. All logic lives in the library; this file only
//! routes subcommands and maps usage errors to exit 64.

#![deny(unsafe_code)]

use aok_launch::{EXIT_USAGE, identity, lifecycle, nest, parse_nest, parse_run, run};

const USAGE: &str = "usage: aok-launch run --name N [--on-behalf-of H] [--grant SPEC]... -- CMD...
       aok-launch nest --name BASE --depth N [--grant SPEC]... -- CMD...
       aok-launch freeze|thaw|kill AGENT
       aok-launch whois --pid P | --pidfd FD | --listen SOCK";

fn usage(msg: &str) -> i32 {
    eprintln!("aok-launch: usage: {msg}\n{USAGE}");
    EXIT_USAGE
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.split_first() {
        None => usage("no subcommand"),
        Some((cmd, rest)) => match cmd.as_str() {
            "run" => match parse_run(rest) {
                Ok(spec) => run(&spec),
                Err(m) => usage(&m),
            },
            "nest" => match parse_nest(rest) {
                Ok(spec) => nest(&spec),
                Err(m) => usage(&m),
            },
            "freeze" | "thaw" | "kill" => lifecycle(cmd, rest),
            "whois" => identity::whois(rest),
            other => usage(&format!("unknown subcommand {other:?}")),
        },
    };
    std::process::exit(code);
}
