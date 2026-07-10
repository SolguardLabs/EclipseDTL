use crate::error::{EclipseError, Result};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope<T> {
    pub version: u32,
    pub kind: String,
    pub payload: T,
}

impl<T> Envelope<T> {
    pub fn new(kind: impl Into<String>, payload: T) -> Self {
        Self {
            version: 1,
            kind: kind.into(),
            payload,
        }
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Envelope<U> {
        Envelope {
            version: self.version,
            kind: self.kind,
            payload: f(self.payload),
        }
    }
}

pub fn to_json<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string_pretty(value).map_err(EclipseError::from)
}

pub fn from_json<T: DeserializeOwned>(input: &str) -> Result<T> {
    serde_json::from_str(input).map_err(EclipseError::from)
}

pub fn envelope_to_json<T: Serialize>(kind: impl Into<String>, value: T) -> Result<String> {
    to_json(&Envelope::new(kind, value))
}

pub fn envelope_from_json<T: DeserializeOwned>(input: &str, expected_kind: &str) -> Result<T> {
    let envelope: Envelope<T> = from_json(input)?;
    if envelope.kind != expected_kind {
        return Err(EclipseError::InvalidScenario(format!(
            "expected envelope kind {}, got {}",
            expected_kind, envelope.kind
        )));
    }
    Ok(envelope.payload)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Digest {
    pub algorithm: String,
    pub value: String,
}

impl Digest {
    pub fn new(algorithm: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            algorithm: algorithm.into(),
            value: value.into(),
        }
    }

    pub fn pseudo_blake3(input: &str) -> Self {
        let mut acc: u128 = 0x6a09_e667_f3bc_c908;
        for (idx, byte) in input.bytes().enumerate() {
            let shift = (idx % 16) * 8;
            acc ^= (byte as u128) << shift;
            acc = acc.rotate_left(7).wrapping_mul(0x1000_0000_01b3);
        }
        Self {
            algorithm: "eclipse-pseudo".to_owned(),
            value: format!("{acc:032x}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalFrame {
    pub sequence: u64,
    pub previous: Option<Digest>,
    pub digest: Digest,
    pub body: String,
}

impl JournalFrame {
    pub fn new(sequence: u64, previous: Option<Digest>, body: impl Into<String>) -> Self {
        let body = body.into();
        let seed = match &previous {
            Some(digest) => format!("{}:{}:{}", sequence, digest.value, body),
            None => format!("{}:{}", sequence, body),
        };
        let digest = Digest::pseudo_blake3(seed.as_str());
        Self {
            sequence,
            previous,
            digest,
            body,
        }
    }

    pub fn verify(&self) -> bool {
        let seed = match &self.previous {
            Some(digest) => format!("{}:{}:{}", self.sequence, digest.value, self.body),
            None => format!("{}:{}", self.sequence, self.body),
        };
        Digest::pseudo_blake3(seed.as_str()) == self.digest
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Journal {
    frames: Vec<JournalFrame>,
}

impl Journal {
    pub fn new() -> Self {
        Self { frames: Vec::new() }
    }

    pub fn append(&mut self, body: impl Into<String>) -> JournalFrame {
        let previous = self.frames.last().map(|frame| frame.digest.clone());
        let frame = JournalFrame::new(self.frames.len() as u64 + 1, previous, body);
        self.frames.push(frame.clone());
        frame
    }

    pub fn verify(&self) -> bool {
        let mut previous: Option<Digest> = None;
        for frame in &self.frames {
            if frame.previous != previous || !frame.verify() {
                return false;
            }
            previous = Some(frame.digest.clone());
        }
        true
    }

    pub fn frames(&self) -> &[JournalFrame] {
        &self.frames
    }
}
