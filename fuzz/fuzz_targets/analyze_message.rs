#![no_main]

use discord_oo_bot::{
    app::analyze_message::{analyze_message, BotConfig},
    generated::kanji_oo_db::KANJI_OO_DB,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let cfg = BotConfig::default();
    let _ = analyze_message(&text, false, &cfg, &KANJI_OO_DB);
});
