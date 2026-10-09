use std::fmt;

use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ProductSessionId(pub [u8; 16]);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct AttachmentGeneration(pub u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct PacketSeq(pub u64);

#[derive(Clone, Eq, PartialEq, Zeroize, ZeroizeOnDrop)]
pub struct ResumeToken(pub [u8; 32]);

impl fmt::Debug for ResumeToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ResumeToken([REDACTED])")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionState {
    New,
    Authenticated,
    Attached,
    Detached,
    Resuming,
    Closed,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum SessionError {
    #[error("invalid transition from {from:?} to {to:?}")]
    InvalidTransition { from: SessionState, to: SessionState },
    #[error("attachment generation overflow")]
    GenerationOverflow,
    #[error("packet sequence overflow")]
    SequenceOverflow,
}

#[derive(Debug)]
pub struct ProductSessionCore {
    pub id: ProductSessionId,
    pub state: SessionState,
    generation: AttachmentGeneration,
    tx_seq: PacketSeq,
}

impl ProductSessionCore {
    pub fn new(id: ProductSessionId) -> Self {
        Self {
            id,
            state: SessionState::New,
            generation: AttachmentGeneration(0),
            tx_seq: PacketSeq(0),
        }
    }

    pub fn mark_authenticated(&mut self) -> Result<(), SessionError> {
        self.transition(SessionState::Authenticated)
    }

    pub fn attach_initial(&mut self) -> Result<AttachmentGeneration, SessionError> {
        if self.state != SessionState::Authenticated {
            return Err(SessionError::InvalidTransition {
                from: self.state,
                to: SessionState::Attached,
            });
        }
        self.generation = AttachmentGeneration(1);
        self.state = SessionState::Attached;
        Ok(self.generation)
    }

    pub fn detach(&mut self) -> Result<(), SessionError> {
        self.transition(SessionState::Detached)
    }

    pub fn begin_resume(&mut self) -> Result<(), SessionError> {
        self.transition(SessionState::Resuming)
    }

    pub fn commit_resume(&mut self) -> Result<AttachmentGeneration, SessionError> {
        if self.state != SessionState::Resuming {
            return Err(SessionError::InvalidTransition {
                from: self.state,
                to: SessionState::Attached,
            });
        }
        self.generation.0 = self
            .generation
            .0
            .checked_add(1)
            .ok_or(SessionError::GenerationOverflow)?;
        self.state = SessionState::Attached;
        Ok(self.generation)
    }

    pub fn close(&mut self) {
        self.state = SessionState::Closed;
    }

    pub fn generation(&self) -> AttachmentGeneration {
        self.generation
    }

    pub fn accepts_generation(&self, generation: AttachmentGeneration) -> bool {
        self.state == SessionState::Attached && generation == self.generation
    }

    pub fn next_packet_seq(&mut self) -> Result<PacketSeq, SessionError> {
        let current = self.tx_seq;
        self.tx_seq.0 = self
            .tx_seq
            .0
            .checked_add(1)
            .ok_or(SessionError::SequenceOverflow)?;
        Ok(current)
    }

    fn transition(&mut self, to: SessionState) -> Result<(), SessionError> {
        let valid = matches!(
            (self.state, to),
            (SessionState::New, SessionState::Authenticated)
                | (SessionState::Attached, SessionState::Detached)
                | (SessionState::Detached, SessionState::Resuming)
        );
        if !valid {
            return Err(SessionError::InvalidTransition {
                from: self.state,
                to,
            });
        }
        self.state = to;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_attachment_is_rejected_after_resume() {
        let mut s = ProductSessionCore::new(ProductSessionId([7; 16]));
        s.mark_authenticated().unwrap();
        let g1 = s.attach_initial().unwrap();
        assert!(s.accepts_generation(g1));
        s.detach().unwrap();
        s.begin_resume().unwrap();
        let g2 = s.commit_resume().unwrap();
        assert!(g2 > g1);
        assert!(!s.accepts_generation(g1));
        assert!(s.accepts_generation(g2));
    }
}
