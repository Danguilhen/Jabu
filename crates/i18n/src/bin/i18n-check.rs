//! Checks the shipped catalogues and reports their status.
//!
//! Run with `cargo run -p jabu-i18n --bin i18n-check`. The command exits
//! non-zero when a catalogue has missing or extra keys, which makes it usable
//! in review scripts and by translators adding a locale.

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    if let Some(flag) = args.next() {
        if flag == "--template" {
            let tag = args.next().unwrap_or_else(|| "xx".to_string());
            println!(
                "## {tag} catalogue for Jabu. Translate every value and keep the\n\
## message ids, placeholders and plural selectors unchanged.\n\
## Source: English. Do not copy Movim catalogues.\n"
            );
            print!("{}", jabu_i18n::catalogue("en"));
            return ExitCode::SUCCESS;
        }
        eprintln!("unknown argument: {flag}");
        eprintln!("usage: i18n-check [--template <tag>]");
        return ExitCode::FAILURE;
    }

    let english = jabu_i18n::keys("en");
    println!("source: en ({} messages)", english.len());

    let mut failed = false;
    for language in jabu_i18n::SUPPORTED
        .iter()
        .filter(|language| language.tag != "en")
    {
        let keys = jabu_i18n::keys(language.tag);
        let missing: Vec<_> = english.difference(&keys).cloned().collect();
        let extra: Vec<_> = keys.difference(&english).cloned().collect();
        println!(
            "{}: {} messages, {} missing, {} extra ({})",
            language.tag,
            keys.len(),
            missing.len(),
            extra.len(),
            language.name
        );
        for key in &missing {
            println!("  missing: {key}");
        }
        for key in &extra {
            println!("  extra:   {key}");
        }
        failed |= !missing.is_empty() || !extra.is_empty();
    }

    if failed {
        println!("catalogues are out of sync");
        ExitCode::FAILURE
    } else {
        println!("catalogues are in sync");
        ExitCode::SUCCESS
    }
}
