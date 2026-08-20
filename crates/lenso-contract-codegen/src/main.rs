use std::{env, path::Path, process::ExitCode};

use lenso_contract_codegen::{check_generated, lint_compatibility, write_generated};

fn usage() -> &'static str {
    "usage:\n  lenso-contract-codegen generate <descriptor> <rust-output> <typescript-output>\n  lenso-contract-codegen check <descriptor> <rust-output> <typescript-output>\n  lenso-contract-codegen lint <old-descriptor> <new-descriptor>"
}

fn run(arguments: &[String]) -> Result<(), String> {
    let Some(command) = arguments.first().map(String::as_str) else {
        return Err(usage().to_owned());
    };
    match command {
        "generate" if arguments.len() == 4 => write_generated(
            Path::new(&arguments[1]),
            Path::new(&arguments[2]),
            Path::new(&arguments[3]),
        )
        .map_err(|error| error.to_string()),
        "check" if arguments.len() == 4 => check_generated(
            Path::new(&arguments[1]),
            Path::new(&arguments[2]),
            Path::new(&arguments[3]),
        )
        .map_err(|error| error.to_string()),
        "lint" if arguments.len() == 3 => {
            lint_compatibility(Path::new(&arguments[1]), Path::new(&arguments[2]))
                .map(|_| ())
                .map_err(|error| error.to_string())
        }
        _ => Err(usage().to_owned()),
    }
}

fn main() -> ExitCode {
    match run(&env::args().skip(1).collect::<Vec<_>>()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
