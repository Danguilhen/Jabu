//! Translation helper process for the desktop sandbox experiment.
//!
//! The parent sends one JSON request per line and reads one JSON response per
//! line. Prompts and translations stay on the inherited stdin/stdout pipes;
//! no network listener is opened.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};

use jabu_translate::{EmbeddedGenerator, PromptGenerator};

#[derive(Debug, Deserialize)]
struct Request {
    id: u64,
    prompt: String,
    max_tokens: usize,
}

#[derive(Debug, Serialize)]
struct Response {
    id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut model = None;
    let mut model_id = "hy-mt2-1.8b-q4-k-m".to_string();
    let mut threads = std::thread::available_parallelism().map_or(4, std::num::NonZeroUsize::get);
    let mut gpu_layers = 0i32;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--model" => model = args.next().map(PathBuf::from),
            "--model-id" => {
                if let Some(value) = args.next() {
                    model_id = value;
                }
            }
            "--threads" => {
                if let Some(value) = args.next() {
                    threads = value.parse()?;
                }
            }
            "--gpu-layers" => {
                if let Some(value) = args.next() {
                    gpu_layers = value.parse()?;
                }
            }
            other => return Err(format!("unknown argument: {other}").into()),
        }
    }
    let model = model.ok_or("--model is required")?;
    let generator = EmbeddedGenerator::load(
        model,
        model_id,
        i32::try_from(threads.max(1))?,
        4096,
        gpu_layers,
    )
    .await?;

    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();
    let mut stdout = BufWriter::new(tokio::io::stdout());
    while let Some(line) = lines.next_line().await? {
        let request: Request = match serde_json::from_str(&line) {
            Ok(request) => request,
            Err(error) => {
                let response = Response {
                    id: 0,
                    text: None,
                    error: Some(error.to_string()),
                };
                stdout
                    .write_all(format!("{}\n", serde_json::to_string(&response)?).as_bytes())
                    .await?;
                stdout.flush().await?;
                continue;
            }
        };
        let response = match generator
            .generate(&request.prompt, request.max_tokens)
            .await
        {
            Ok(text) => Response {
                id: request.id,
                text: Some(text),
                error: None,
            },
            Err(error) => Response {
                id: request.id,
                text: None,
                error: Some(error.to_string()),
            },
        };
        stdout
            .write_all(format!("{}\n", serde_json::to_string(&response)?).as_bytes())
            .await?;
        stdout.flush().await?;
    }
    Ok(())
}
