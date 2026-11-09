//! Translate one message with the embedded model.
//!
//! Usage:
//! `cargo run -p jabu-translate --features embedded --bin translate-once -- \
//!   --model ~/.local/share/jabu/models/Hy-MT2-1.8B-Q4_K_M.gguf \
//!   --from ko --to fr --text "안녕하세요"`

use std::path::PathBuf;
use std::time::Instant;

use jabu_translate::{
    EmbeddedGenerator, Language, PromptGenerator, TranslationRequest, Translator,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut model = None;
    let mut model_id = "hy-mt2-1.8b-q4-k-m".to_string();
    let mut gpu_layers = 0i32;
    let mut text = None;
    let mut source = None;
    let mut target = None;
    let mut raw = false;
    let mut threads = std::thread::available_parallelism().map_or(4, std::num::NonZeroUsize::get);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--model" => model = args.next().map(PathBuf::from),
            "--model-id" => {
                if let Some(value) = args.next() {
                    model_id = value;
                }
            }
            "--gpu-layers" => {
                if let Some(value) = args.next() {
                    gpu_layers = value.parse()?;
                }
            }
            "--text" => text = args.next(),
            "--from" => source = args.next(),
            "--to" => target = args.next(),
            "--raw" => raw = true,
            "--threads" => {
                if let Some(value) = args.next() {
                    threads = value.parse()?;
                }
            }
            other => return Err(format!("unknown argument: {other}").into()),
        }
    }
    let model = model.ok_or("--model is required")?;
    let text = text.ok_or("--text is required")?;
    let target = target.ok_or("--to is required")?;
    let target: Language = Language::from_tag(&target).ok_or("unknown target language")?;
    let source = source.as_deref().and_then(Language::from_tag);

    let started = Instant::now();
    let generator = EmbeddedGenerator::load(
        &model,
        model_id.clone(),
        i32::try_from(threads.max(1))?,
        4096,
        gpu_layers,
    )
    .await?;
    let loaded = started.elapsed();
    if raw {
        let started = Instant::now();
        let output = generator.generate(&text, 256).await?;
        println!("{output}");
        eprintln!("load {:.2?}, generate {:.2?}", loaded, started.elapsed());
        return Ok(());
    }
    let translator = Translator::new(generator);
    let started = Instant::now();
    let result = translator
        .translate(TranslationRequest {
            text,
            source,
            target,
        })
        .await?;
    let elapsed = started.elapsed();
    println!("{}", result.text);
    eprintln!(
        "load {:.2?}, translate {:.2?}, model {}",
        loaded, elapsed, result.engine
    );
    Ok(())
}
