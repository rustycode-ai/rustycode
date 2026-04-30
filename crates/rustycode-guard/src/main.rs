use std::io::{self, Read};

mod codec;
mod permission;
mod post_tool;
mod pre_tool;
mod rules;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // Expect: rustycode-guard hook <subcommand>
    if args.len() < 3 || args[1] != "hook" {
        eprintln!("Usage: rustycode-guard hook <pre-tool|post-tool|permission>");
        std::process::exit(1);
    }

    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    let hook_input = codec::parse_input(&input)?;

    let result = match args[2].as_str() {
        "pre-tool" => pre_tool::evaluate(&hook_input),
        "post-tool" => post_tool::evaluate(&hook_input),
        "permission" => permission::evaluate(&hook_input),
        _ => {
            eprintln!("Unknown hook type: {}", args[2]);
            std::process::exit(1);
        }
    };

    // Print result to stdout
    let json = serde_json::to_string(&result)?;
    println!("{json}");

    Ok(())
}
