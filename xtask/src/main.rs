#![forbid(unsafe_code)]

use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use flate2::read::GzDecoder;
use roxmltree::Document;
use serde::Serialize;
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Generate(GenerateArgs),
    Verify(GenerateArgs),
}

#[derive(Debug, Clone, Parser)]
struct GenerateArgs {
    #[arg(long, default_value = "data/vendor/kanjidic2.xml.gz")]
    input: PathBuf,
    #[arg(long, default_value = "src/generated/kanji_oo_db.rs")]
    output: PathBuf,
    #[arg(long, default_value = "data/generated/kanji_oo_db_meta.json")]
    metadata_out: PathBuf,
    #[arg(long, default_value_t = true)]
    include_ja_on: bool,
}

#[derive(Debug, Clone, Serialize)]
struct GenerationMetadata {
    source_name: &'static str,
    source_sha256: String,
    include_ja_on: bool,
    total_chars: usize,
    ja_kun_hits: usize,
    nanori_hits: usize,
    ja_on_hits: usize,
    sample_characters: Vec<String>,
}

#[derive(Debug, Clone)]
struct GeneratedArtifacts {
    rust_source: String,
    metadata_json: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Generate(args) => generate(args),
        Command::Verify(args) => verify(args),
    }
}

fn generate(args: GenerateArgs) -> Result<()> {
    let artifacts = build_artifacts(&args)?;
    write_if_changed(&args.output, &artifacts.rust_source)?;
    write_if_changed(&args.metadata_out, &artifacts.metadata_json)?;
    println!("generated {} and {}", args.output.display(), args.metadata_out.display());
    Ok(())
}

fn verify(args: GenerateArgs) -> Result<()> {
    let artifacts = build_artifacts(&args)?;
    verify_file_content(&args.output, &artifacts.rust_source)?;
    verify_file_content(&args.metadata_out, &artifacts.metadata_json)?;
    println!("verification succeeded");
    Ok(())
}

fn build_artifacts(args: &GenerateArgs) -> Result<GeneratedArtifacts> {
    let source_bytes = fs::read(&args.input)
        .with_context(|| format!("failed to read input: {}", args.input.display()))?;
    let source_sha256 = sha256_hex(&source_bytes);

    let xml = if args
        .input
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gz"))
    {
        decompress_gzip(&source_bytes).context("failed to decompress KANJIDIC2 gzip")?
    } else {
        String::from_utf8(source_bytes).context("KANJIDIC2 input is not valid UTF-8 XML")?
    };

    let extracted = extract_oo_kanji(&xml, args.include_ja_on)?;
    let metadata = GenerationMetadata {
        source_name: "KANJIDIC2",
        source_sha256,
        include_ja_on: args.include_ja_on,
        total_chars: extracted.all_hits.len(),
        ja_kun_hits: extracted.ja_kun_hits.len(),
        nanori_hits: extracted.nanori_hits.len(),
        ja_on_hits: extracted.ja_on_hits.len(),
        sample_characters: extracted.all_hits.iter().take(16).map(|ch| ch.to_string()).collect(),
    };

    let rust_source = render_generated_rust(&metadata, &extracted.all_hits);
    let metadata_json =
        serde_json::to_string_pretty(&metadata).context("failed to encode metadata json")?;

    Ok(GeneratedArtifacts { rust_source, metadata_json: format!("{metadata_json}\n") })
}

fn verify_file_content(path: &Path, expected: &str) -> Result<()> {
    let current = fs::read_to_string(path)
        .with_context(|| format!("failed to read generated file: {}", path.display()))?;
    if current != expected {
        bail!("{} is out of date. run: cargo run -p xtask -- generate", path.display());
    }
    Ok(())
}

fn write_if_changed(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory: {}", parent.display()))?;
    }

    match fs::read_to_string(path) {
        Ok(existing) if existing == content => return Ok(()),
        Ok(_) | Err(_) => {}
    }

    fs::write(path, content).with_context(|| format!("failed to write: {}", path.display()))
}

#[derive(Debug, Clone)]
struct Extracted {
    all_hits: BTreeSet<char>,
    ja_kun_hits: BTreeSet<char>,
    nanori_hits: BTreeSet<char>,
    ja_on_hits: BTreeSet<char>,
}

fn extract_oo_kanji(xml: &str, include_ja_on: bool) -> Result<Extracted> {
    let sanitized = strip_doctype(xml);
    let doc = Document::parse(&sanitized).context("failed to parse KANJIDIC2 XML")?;

    let mut all_hits = BTreeSet::new();
    let mut ja_kun_hits = BTreeSet::new();
    let mut nanori_hits = BTreeSet::new();
    let mut ja_on_hits = BTreeSet::new();

    let mut character_count = 0usize;

    for character in doc.descendants().filter(|n| n.has_tag_name("character")) {
        character_count += 1;

        let literal = character
            .children()
            .find(|n| n.has_tag_name("literal"))
            .and_then(|n| n.text())
            .and_then(single_char)
            .context("character entry is missing valid <literal>")?;

        for reading in character.descendants().filter(|n| n.has_tag_name("reading")) {
            let Some(kind) = reading.attribute("r_type") else {
                continue;
            };
            let Some(raw) = reading.text() else {
                continue;
            };
            let normalized = normalize_reading(raw);
            if !normalized.contains("おお") {
                continue;
            }

            match kind {
                "ja_kun" => {
                    ja_kun_hits.insert(literal);
                    all_hits.insert(literal);
                }
                "ja_on" if include_ja_on => {
                    ja_on_hits.insert(literal);
                    all_hits.insert(literal);
                }
                _ => {}
            }
        }

        for nanori in character.descendants().filter(|n| n.has_tag_name("nanori")) {
            let Some(raw) = nanori.text() else {
                continue;
            };
            let normalized = normalize_reading(raw);
            if normalized.contains("おお") {
                nanori_hits.insert(literal);
                all_hits.insert(literal);
            }
        }
    }

    if character_count == 0 {
        bail!("KANJIDIC2 input has zero <character> entries");
    }

    Ok(Extracted { all_hits, ja_kun_hits, nanori_hits, ja_on_hits })
}

fn strip_doctype(xml: &str) -> String {
    let Some(start) = xml.find("<!DOCTYPE") else {
        return xml.to_string();
    };

    let bytes = xml.as_bytes();
    let mut i = start;
    let mut subset_depth = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'[' => subset_depth += 1,
            b']' => {
                subset_depth = subset_depth.saturating_sub(1);
            }
            b'>' if subset_depth == 0 => {
                i += 1;
                break;
            }
            _ => {}
        }
        i += 1;
    }

    let mut out = String::with_capacity(xml.len());
    out.push_str(&xml[..start]);
    if i < xml.len() {
        out.push_str(&xml[i..]);
    }
    out
}

fn render_generated_rust(metadata: &GenerationMetadata, chars: &BTreeSet<char>) -> String {
    let mut codepoints = String::new();
    for ch in chars {
        let cp = *ch as u32;
        codepoints.push_str(&format!("    0x{cp:04X},\n"));
    }

    let mut out = String::new();
    out.push_str("use crate::domain::kanji_matcher::{KanjiOoDb, KanjiOoDbMetadata};\n\n");
    out.push_str(&format!("pub const SOURCE_NAME: &str = \"{}\";\n", metadata.source_name));
    out.push_str(&format!("pub const SOURCE_SHA256: &str = \"{}\";\n", metadata.source_sha256));
    out.push_str(&format!("pub const INCLUDE_JA_ON: bool = {};\n\n", metadata.include_ja_on));
    out.push_str("pub const OO_KANJI_CODEPOINTS: &[u32] = &[\n");
    out.push_str(&codepoints);
    out.push_str("];\n\n");
    out.push_str("pub const KANJI_OO_DB: KanjiOoDb = KanjiOoDb::new(\n");
    out.push_str("    OO_KANJI_CODEPOINTS,\n");
    out.push_str("    KanjiOoDbMetadata {\n");
    out.push_str("        source_name: SOURCE_NAME,\n");
    out.push_str("        source_sha256: SOURCE_SHA256,\n");
    out.push_str(&format!("        total_chars: {},\n", metadata.total_chars));
    out.push_str(&format!("        ja_kun_hits: {},\n", metadata.ja_kun_hits));
    out.push_str(&format!("        nanori_hits: {},\n", metadata.nanori_hits));
    out.push_str(&format!("        ja_on_hits: {},\n", metadata.ja_on_hits));
    out.push_str("    },\n");
    out.push_str(");\n");

    out
}

fn single_char(text: &str) -> Option<char> {
    let mut chars = text.chars();
    let ch = chars.next()?;
    if chars.next().is_none() {
        Some(ch)
    } else {
        None
    }
}

fn decompress_gzip(input: &[u8]) -> Result<String> {
    let mut decoder = GzDecoder::new(input);
    let mut buf = String::new();
    decoder.read_to_string(&mut buf).context("invalid gzip stream")?;
    Ok(buf)
}

fn sha256_hex(input: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input);
    format!("{:x}", hasher.finalize())
}

fn normalize_reading(input: &str) -> String {
    let normalized = input.nfkc().collect::<String>();
    let trimmed = normalized.trim();

    let mut out = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if matches!(
            ch,
            '.' | '-'
                | '‐'
                | '‑'
                | '–'
                | '—'
                | '―'
                | '・'
                | '･'
                | ' '
                | '　'
                | '\t'
                | '\n'
                | '\r'
        ) {
            continue;
        }
        out.push(katakana_to_hiragana(ch));
    }
    out.nfkc().collect()
}

fn katakana_to_hiragana(ch: char) -> char {
    let code = ch as u32;
    if (0x30A1..=0x30F6).contains(&code) {
        char::from_u32(code - 0x60).unwrap_or(ch)
    } else {
        ch
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_oo_kanji, render_generated_rust};

    #[test]
    fn extraction_works_on_fixture() {
        let xml = include_str!("../tests/fixtures/kanjidic2-mini.xml");
        let extracted = extract_oo_kanji(xml, true).expect("extract should succeed");
        assert!(extracted.all_hits.contains(&'大'));
        assert!(extracted.all_hits.contains(&'狼'));
        assert!(!extracted.all_hits.contains(&'小'));
    }

    #[test]
    fn generated_code_is_deterministic() {
        let xml = include_str!("../tests/fixtures/kanjidic2-mini.xml");
        let extracted = extract_oo_kanji(xml, true).expect("extract should succeed");
        let meta = super::GenerationMetadata {
            source_name: "KANJIDIC2",
            source_sha256: "test-sha".to_string(),
            include_ja_on: true,
            total_chars: extracted.all_hits.len(),
            ja_kun_hits: extracted.ja_kun_hits.len(),
            nanori_hits: extracted.nanori_hits.len(),
            ja_on_hits: extracted.ja_on_hits.len(),
            sample_characters: vec![],
        };
        let actual = render_generated_rust(&meta, &extracted.all_hits);
        let expected = include_str!("../tests/golden/kanji_oo_db.rs.golden");
        assert_eq!(actual, expected);
    }
}
