#![no_main]

use discord_ooh_bot::sandbox::abi::ActionProposal;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    for chunk in data.chunks(8) {
        let mut bytes = [0u8; 8];
        let len = chunk.len();
        bytes[..len].copy_from_slice(chunk);

        let wire = i64::from_le_bytes(bytes);
        if let Ok(proposal) = ActionProposal::decode_wire(wire) {
            let rewire = proposal.encode_wire();
            let _ = ActionProposal::decode_wire(rewire);
        }
    }
});
