use mido::{cli, process::SystemRunner, style::Style};

fn main() {
    let args = cli::parse();
    let mut out = std::io::stdout();
    let mut err = std::io::stderr();
    let code = cli::main_with(&args, &SystemRunner, &mut out, &mut err, Style::detect());
    std::process::exit(code);
}
