use eclipsedtl::scenario::{report_to_pretty_json, run_scenario_file};
use eclipsedtl::{EclipseError, Result};

fn main() {
    if let Err(error) = run() {
        eprintln!(
            "{}",
            serde_json::json!({
                "error": error.to_string(),
                "code": error.code()
            })
        );
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let first = args.next().unwrap_or_else(|| "--help".to_owned());
    match first.as_str() {
        "--scenario" | "-s" => {
            let path = args
                .next()
                .ok_or_else(|| EclipseError::InvalidScenario("missing scenario path".to_owned()))?;
            let report = run_scenario_file(path)?;
            println!("{}", report_to_pretty_json(&report)?);
            Ok(())
        }
        "--help" | "-h" => {
            println!("Usage: eclipsedtl --scenario <path>");
            Ok(())
        }
        value => Err(EclipseError::InvalidScenario(format!(
            "unknown argument {value}"
        ))),
    }
}
