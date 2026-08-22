use std::time::Instant;

use danie_llm::{check, Config, LlmError};
use tokio::runtime::Runtime;

const EXAMPLE_CONFIG: &str = "\
[default]
provider = \"openai-compat\"
model = \"llama3.1\"

[providers.openai_compat]
base_url = \"http://localhost:11434/v1\"";

pub fn effective_model(config: &Config) -> String {
    if config.default.provider == "openai-compat" {
        config
            .providers
            .openai_compat
            .model
            .clone()
            .unwrap_or_else(|| config.default.model.clone())
    } else {
        config.default.model.clone()
    }
}

pub fn print_setup_guidance(error: &LlmError) {
    println!();
    println!("Setup guidance");
    println!("--------------");
    if let LlmError::MissingKey { env_var } = error {
        println!("Set the `{env_var}` environment variable to your API key, for example:");
        println!("  export {env_var}=sk-...        (bash)");
        println!("  $env:{env_var} = \"sk-...\"    (PowerShell)");
    } else {
        println!("Fix the problem reported above.");
    }
    println!(
        "Or point danie at a local OpenAI-compatible server such as Ollama by creating this file:"
    );
    println!("{}", Config::path().display());
    for line in EXAMPLE_CONFIG.lines() {
        println!("  {line}");
    }
}

pub fn run(rt: &Runtime) -> u8 {
    println!("danie doctor");
    let config_path = Config::path();
    let state = if config_path.exists() {
        "found"
    } else {
        "not found (using defaults)"
    };
    println!("Config file : {} [{state}]", config_path.display());

    let config = match Config::load() {
        Ok(config) => config,
        Err(error) => {
            println!("FAIL could not load config: {error}");
            return 1;
        }
    };

    let model = effective_model(&config);
    println!("Provider    : {}", config.default.provider);
    println!("Model       : {model}");
    println!();

    println!("Creating provider...");
    let provider = match config.create_provider() {
        Ok(provider) => provider,
        Err(error) => {
            println!("FAIL {error}");
            print_setup_guidance(&error);
            return 1;
        }
    };
    println!("Provider ready.");

    println!("Pinging the model...");
    let started = Instant::now();
    match rt.block_on(check(provider.as_ref())) {
        Ok(reply) => {
            let elapsed_ms = started.elapsed().as_millis();
            println!("PASS reply {reply:?} received in {elapsed_ms} ms");
            println!();
            println!("Everything looks good. Run `danie teach` to start learning.");
            0
        }
        Err(error) => {
            println!("FAIL {error}");
            print_setup_guidance(&LlmError::Config(
                "the provider could not be reached; see the error above".into(),
            ));
            1
        }
    }
}
