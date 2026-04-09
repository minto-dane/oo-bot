#![no_main]

use discord_oo_bot::app::replay::ReplayCase;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);

    let _ = serde_yaml::from_str::<ReplayCase>(&text);
    let _ = serde_yaml::from_str::<Vec<ReplayCase>>(&text);
    let _ = serde_json::from_str::<ReplayCase>(&text);
    let _ = serde_json::from_str::<Vec<ReplayCase>>(&text);
});
