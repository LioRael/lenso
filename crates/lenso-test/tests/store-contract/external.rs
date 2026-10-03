//! Typed external completions recorded from actual providers, never SQL matching.

use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Completion {
    pub request: Value,
    pub response: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Transcript {
    pub format: u8,
    pub provider: String,
    pub corpus_sha256: String,
    pub completions: Vec<Completion>,
}

impl Transcript {
    pub fn read(provider: &str) -> Self {
        let directory = std::env::var_os("LENSO_STORE_TRANSCRIPT_DIR").map_or_else(
            || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/store-contract"),
            PathBuf::from,
        );
        let path = directory.join(format!("{provider}.json"));
        let value: Self = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(value.format, 1);
        assert_eq!(value.provider, provider);
        assert_eq!(value.corpus_sha256, corpus_digest());
        assert!(
            !value.completions.is_empty(),
            "empty traces cannot qualify replay"
        );
        value
    }
}

pub fn corpus_digest() -> String {
    format!("{:x}", Sha256::digest(include_bytes!("corpus.json")))
}

#[derive(Debug)]
pub enum ExternalStore {
    Real(RealStore),
    Replay {
        remaining: VecDeque<Completion>,
        consumed: usize,
    },
}

impl ExternalStore {
    pub fn replay(transcript: &Transcript) -> Self {
        Self::Replay {
            remaining: transcript.completions.clone().into(),
            consumed: 0,
        }
    }

    pub fn real(provider: &str) -> Self {
        Self::Real(RealStore::start(provider))
    }

    pub fn call(&mut self, request: Value) -> Value {
        match self {
            Self::Real(real) => real.call(request),
            Self::Replay {
                remaining,
                consumed,
            } => {
                let completion = remaining
                    .pop_front()
                    .expect("external transcript exhausted");
                assert_eq!(
                    request, completion.request,
                    "first external completion divergence at index {consumed}"
                );
                *consumed += 1;
                completion.response
            }
        }
    }

    pub fn finish(&self) {
        match self {
            Self::Replay { remaining, .. } => {
                assert!(remaining.is_empty(), "unconsumed external completion");
            }
            Self::Real(real) => {
                if let Some(path) = std::env::var_os("LENSO_STORE_TRACE_OUT") {
                    std::fs::write(path, serde_json::to_vec_pretty(&real.transcript).unwrap())
                        .unwrap();
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct RealStore {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    transcript: Transcript,
}

impl RealStore {
    fn start(provider: &str) -> Self {
        assert!(matches!(provider, "postgres" | "d1"));
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.github/fixtures/durable-store/provider.mjs");
        let mut child = Command::new("node")
            .arg(script)
            .arg(provider)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("start the real provider fixture");
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        let mut real = Self {
            child,
            input,
            output,
            transcript: Transcript {
                format: 1,
                provider: provider.to_owned(),
                corpus_sha256: corpus_digest(),
                completions: vec![],
            },
        };
        assert_eq!(real.receive(), json!({"ready": true, "provider": provider}));
        real
    }

    fn receive(&mut self) -> Value {
        let mut line = String::new();
        self.output
            .by_ref()
            .take(8193)
            .read_line(&mut line)
            .unwrap();
        assert!(
            line.len() < 8193 && line.ends_with('\n'),
            "bounded complete provider frame required"
        );
        serde_json::from_str(&line).expect("typed provider JSON response")
    }

    fn call(&mut self, request: Value) -> Value {
        let bytes = serde_json::to_vec(&request).unwrap();
        assert!(bytes.len() < 8192);
        self.input.write_all(&bytes).unwrap();
        self.input.write_all(b"\n").unwrap();
        self.input.flush().unwrap();
        let response = self.receive();
        self.transcript.completions.push(Completion {
            request,
            response: response.clone(),
        });
        response
    }
}

impl Drop for RealStore {
    fn drop(&mut self) {
        if std::thread::panicking() {
            let _ = self.child.kill();
        } else {
            let _ = self.input.write_all(b"{\"action\":\"close\"}\n");
            let _ = self.input.flush();
        }
        let status = self.child.wait().expect("join provider process");
        if !std::thread::panicking() {
            assert!(status.success(), "real provider cleanup failed: {status}");
        }
    }
}
