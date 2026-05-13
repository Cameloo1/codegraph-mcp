#![forbid(unsafe_code)]

fn main() {
    let output = codegraph_cli::run(std::env::args());

    print!("{}", output.stdout);
    eprint!("{}", output.stderr);

    std::process::exit(output.exit_code);
}
