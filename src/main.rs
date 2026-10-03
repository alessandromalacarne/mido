use mido::{cli, mcp, process::SystemRunner, style::Style};

fn main() {
    let args = cli::parse();

    if matches!(args.command, Some(cli::Command::Mcp)) {
        std::process::exit(mcp::serve_stdio());
    }

    let mut out = std::io::stdout();
    let mut err = std::io::stderr();
    let code = cli::main_with(&args, &SystemRunner, &mut out, &mut err, Style::detect());
    std::process::exit(code);
}
