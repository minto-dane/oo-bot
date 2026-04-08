use serde::{Deserialize, Serialize};

pub const SANDBOX_ABI_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum ProposalKind {
    Noop = 0,
    ReactOnce = 1,
    SendStamped = 2,
    SpecialPhrase = 3,
    SuspiciousInput = 4,
    Defer = 5,
    Reject = 6,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum RejectReasonCode {
    InvalidAbi = 1,
    InvalidProposal = 2,
    SandboxTrap = 3,
    Timeout = 4,
    ResourceLimit = 5,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionProposal {
    Noop,
    ReactOnce,
    SendStamped { count: u8 },
    SpecialPhrase,
    SuspiciousInput,
    Defer,
    Reject { reason: RejectReasonCode },
}

impl ActionProposal {
    pub fn encode_wire(&self) -> i64 {
        let (kind, aux) = match self {
            Self::Noop => (ProposalKind::Noop as u32, 0u32),
            Self::ReactOnce => (ProposalKind::ReactOnce as u32, 1u32),
            Self::SendStamped { count } => (ProposalKind::SendStamped as u32, (*count).into()),
            Self::SpecialPhrase => (ProposalKind::SpecialPhrase as u32, 1u32),
            Self::SuspiciousInput => (ProposalKind::SuspiciousInput as u32, 0u32),
            Self::Defer => (ProposalKind::Defer as u32, 0u32),
            Self::Reject { reason } => (ProposalKind::Reject as u32, *reason as u32),
        };
        ((kind as i64) << 32) | i64::from(aux)
    }

    pub fn decode_wire(value: i64) -> Result<Self, RejectReasonCode> {
        let kind = ((value >> 32) & 0xFFFF_FFFF) as u32;
        let aux = (value & 0xFFFF_FFFF) as u32;

        match kind {
            x if x == ProposalKind::Noop as u32 => Ok(Self::Noop),
            x if x == ProposalKind::ReactOnce as u32 => Ok(Self::ReactOnce),
            x if x == ProposalKind::SendStamped as u32 => {
                Ok(Self::SendStamped { count: aux.clamp(1, u8::MAX as u32) as u8 })
            }
            x if x == ProposalKind::SpecialPhrase as u32 => Ok(Self::SpecialPhrase),
            x if x == ProposalKind::SuspiciousInput as u32 => Ok(Self::SuspiciousInput),
            x if x == ProposalKind::Defer as u32 => Ok(Self::Defer),
            x if x == ProposalKind::Reject as u32 => Ok(Self::Reject {
                reason: match aux {
                    1 => RejectReasonCode::InvalidAbi,
                    2 => RejectReasonCode::InvalidProposal,
                    3 => RejectReasonCode::SandboxTrap,
                    4 => RejectReasonCode::Timeout,
                    5 => RejectReasonCode::ResourceLimit,
                    _ => RejectReasonCode::InvalidProposal,
                },
            }),
            _ => Err(RejectReasonCode::InvalidProposal),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnalyzerRequest<'a> {
    pub content: &'a str,
    pub kanji_count: usize,
    pub special_phrase_hit: bool,
}

#[derive(Debug, Clone)]
pub enum AnalyzerError {
    AbiMismatch { expected: u32, actual: u32 },
    Trap(String),
    ResourceLimit(String),
    Timeout,
    InvalidWire(i64),
}

pub trait ProposalAnalyzer: Send {
    fn abi_version(&self) -> u32;
    fn propose(&mut self, req: &AnalyzerRequest<'_>) -> Result<ActionProposal, AnalyzerError>;
}

#[cfg(test)]
mod tests {
    use super::{ActionProposal, RejectReasonCode};

    #[test]
    fn wire_roundtrip() {
        let proposal = ActionProposal::SendStamped { count: 12 };
        let wire = proposal.encode_wire();
        let decoded = ActionProposal::decode_wire(wire).expect("decode should work");
        assert_eq!(proposal, decoded);

        let reject = ActionProposal::Reject { reason: RejectReasonCode::SandboxTrap };
        let reject_wire = reject.encode_wire();
        let reject_decoded = ActionProposal::decode_wire(reject_wire).expect("decode should work");
        assert_eq!(reject, reject_decoded);
    }
}
