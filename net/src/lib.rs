//! Peer-to-peer sync transport for Lifetime (Phase B).
//!
//! A thin, portable layer over the Phase A engine: it establishes an
//! authenticated, encrypted [Noise](https://noiseprotocol.org/) channel between
//! two peers that share a pre-shared key (derived from the vault's master key by
//! the caller), then runs one sync round by reusing the store's
//! `version_vector` / `records_since` / `ingest` primitives. No discovery, no
//! crypto-key management, no platform coupling — the caller supplies a connected
//! stream and the PSK, so this crate is reusable by desktop and (later) Android.

use std::io::{Read, Write};

use serde::{Deserialize, Serialize};

use lifetime_core::storage::{StorageError, Store};
use lifetime_core::sync::{SyncRecord, VersionVector};

/// Noise handshake pattern: no static keys, mutual authentication via the PSK.
const PARAMS: &str = "Noise_NNpsk0_25519_ChaChaPoly_SHA256";
/// Max Noise transport message is 65535 bytes incl. the 16-byte tag.
const MAX_CHUNK: usize = 65535 - 16;
/// Records per `Batch` message (keeps each batch comfortably streamable).
const BATCH: usize = 256;

#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("noise: {0}")]
    Noise(#[from] snow::Error),
    #[error("serialization: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("storage: {0}")]
    Storage(#[from] StorageError),
    #[error("protocol: {0}")]
    Protocol(&'static str),
}

pub type Result<T> = std::result::Result<T, NetError>;

/// Records moved during one sync round, from the local node's perspective.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncOutcome {
    pub sent: usize,
    pub received: usize,
}

#[derive(Debug, Serialize, Deserialize)]
enum SyncMessage {
    Vector(VersionVector),
    Batch(Vec<SyncRecord>),
    Done,
}

/// Run a full bidirectional sync round over `stream`. The two peers must pass
/// the same `psk` (or the handshake fails and nothing is exchanged). Exactly one
/// side passes `initiator = true`. Reuses the Phase A engine for the merge.
pub fn run_session<S: Read + Write>(
    store: &Store,
    stream: S,
    psk: &[u8],
    initiator: bool,
) -> Result<SyncOutcome> {
    let mut ch = Channel::establish(stream, psk, initiator)?;
    let my_vv = store.version_vector()?;

    // Strict ordering (initiator acts first in each phase) avoids a write/write
    // deadlock on the single stream.
    let (sent, received) = if initiator {
        ch.send(&SyncMessage::Vector(my_vv))?;
        let peer = expect_vector(ch.recv()?)?;
        let sent = send_records(&mut ch, store, &peer)?;
        let received = recv_records(&mut ch, store)?;
        (sent, received)
    } else {
        let peer = expect_vector(ch.recv()?)?;
        ch.send(&SyncMessage::Vector(my_vv))?;
        let received = recv_records(&mut ch, store)?;
        let sent = send_records(&mut ch, store, &peer)?;
        (sent, received)
    };

    Ok(SyncOutcome { sent, received })
}

fn expect_vector(msg: SyncMessage) -> Result<VersionVector> {
    match msg {
        SyncMessage::Vector(v) => Ok(v),
        _ => Err(NetError::Protocol("expected version vector")),
    }
}

fn send_records<S: Read + Write>(
    ch: &mut Channel<S>,
    store: &Store,
    peer: &VersionVector,
) -> Result<usize> {
    let records = store.records_since(peer)?;
    let total = records.len();
    for batch in records.chunks(BATCH) {
        ch.send(&SyncMessage::Batch(batch.to_vec()))?;
    }
    ch.send(&SyncMessage::Done)?;
    Ok(total)
}

fn recv_records<S: Read + Write>(ch: &mut Channel<S>, store: &Store) -> Result<usize> {
    let mut applied = 0;
    loop {
        match ch.recv()? {
            SyncMessage::Batch(records) => applied += store.ingest(&records)?,
            SyncMessage::Done => break,
            SyncMessage::Vector(_) => {
                return Err(NetError::Protocol("unexpected vector mid-stream"));
            }
        }
    }
    Ok(applied)
}

/// An encrypted message channel: a Noise transport plus a stream, with
/// length-prefixed framing. An application message is one or more encrypted
/// chunks terminated by an empty (tag-only) chunk.
struct Channel<S: Read + Write> {
    stream: S,
    noise: snow::TransportState,
}

impl<S: Read + Write> Channel<S> {
    fn establish(mut stream: S, psk: &[u8], initiator: bool) -> Result<Self> {
        let params = PARAMS.parse().expect("static noise params are valid");
        let builder = snow::Builder::new(params).psk(0, psk);
        let mut hs = if initiator {
            builder.build_initiator()?
        } else {
            builder.build_responder()?
        };

        let mut buf = vec![0u8; u16::MAX as usize];
        if initiator {
            let n = hs.write_message(&[], &mut buf)?;
            write_raw(&mut stream, &buf[..n])?;
            let msg = read_raw(&mut stream)?;
            hs.read_message(&msg, &mut buf)?;
        } else {
            let msg = read_raw(&mut stream)?;
            hs.read_message(&msg, &mut buf)?;
            let n = hs.write_message(&[], &mut buf)?;
            write_raw(&mut stream, &buf[..n])?;
        }

        Ok(Self {
            stream,
            noise: hs.into_transport_mode()?,
        })
    }

    fn send(&mut self, msg: &SyncMessage) -> Result<()> {
        let data = serde_json::to_vec(msg)?;
        for chunk in data.chunks(MAX_CHUNK) {
            self.write_chunk(chunk)?;
        }
        self.write_chunk(&[])?; // empty terminator chunk
        Ok(())
    }

    fn write_chunk(&mut self, plaintext: &[u8]) -> Result<()> {
        let mut buf = vec![0u8; plaintext.len() + 16];
        let n = self.noise.write_message(plaintext, &mut buf)?;
        write_raw(&mut self.stream, &buf[..n])
    }

    fn recv(&mut self) -> Result<SyncMessage> {
        let mut data = Vec::new();
        loop {
            let ct = read_raw(&mut self.stream)?;
            let mut pt = vec![0u8; ct.len()];
            let n = self.noise.read_message(&ct, &mut pt)?;
            if n == 0 {
                break; // terminator
            }
            data.extend_from_slice(&pt[..n]);
        }
        Ok(serde_json::from_slice(&data)?)
    }
}

fn write_raw<W: Write>(w: &mut W, data: &[u8]) -> Result<()> {
    w.write_all(&(data.len() as u16).to_be_bytes())?;
    w.write_all(data)?;
    Ok(())
}

fn read_raw<R: Read>(r: &mut R) -> Result<Vec<u8>> {
    let mut len = [0u8; 2];
    r.read_exact(&mut len)?;
    let mut buf = vec![0u8; u16::from_be_bytes(len) as usize];
    r.read_exact(&mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lifetime_core::model::{IdleSample, Observation, ObservationKind};
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use time::OffsetDateTime;
    use uuid::Uuid;

    fn obs(device: Uuid) -> Observation {
        Observation {
            id: Uuid::now_v7(),
            device_id: device,
            recorded_at: OffsetDateTime::now_utc(),
            kind: ObservationKind::Idle(IdleSample { idle_seconds: 1 }),
        }
    }

    /// Run one sync round between two owned stores over a loopback socket,
    /// returning the stores (so callers can re-sync) and both outcomes.
    fn sync_pair(
        a: Store,
        b: Store,
        psk_a: Vec<u8>,
        psk_b: Vec<u8>,
    ) -> (Store, Store, Result<SyncOutcome>, Result<SyncOutcome>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let responder = thread::spawn(move || {
            let (sock, _) = listener.accept().unwrap();
            let out = run_session(&b, sock, &psk_b, false);
            (b, out)
        });
        let sock = TcpStream::connect(addr).unwrap();
        let out_a = run_session(&a, sock, &psk_a, true);
        let (b, out_b) = responder.join().unwrap();
        (a, b, out_a, out_b)
    }

    #[test]
    fn converges_over_loopback_and_is_idempotent() {
        let a = Store::open_in_memory().unwrap();
        let b = Store::open_in_memory().unwrap();
        a.insert_observation(&obs(Uuid::now_v7())).unwrap();
        b.insert_observation(&obs(Uuid::now_v7())).unwrap();
        let psk = vec![7u8; 32];

        let (a, b, oa, ob) = sync_pair(a, b, psk.clone(), psk.clone());
        oa.unwrap();
        ob.unwrap();
        assert_eq!(a.all_observations().unwrap().len(), 2);
        assert_eq!(b.all_observations().unwrap().len(), 2);
        assert_eq!(a.version_vector().unwrap(), b.version_vector().unwrap());

        // Second round: nothing new to apply.
        let (a, b, oa, ob) = sync_pair(a, b, psk.clone(), psk);
        assert_eq!(oa.unwrap().received, 0);
        assert_eq!(ob.unwrap().received, 0);
        assert_eq!(a.all_observations().unwrap().len(), 2);
        assert_eq!(b.all_observations().unwrap().len(), 2);
    }

    #[test]
    fn wrong_psk_rejected_and_no_data_transferred() {
        let a = Store::open_in_memory().unwrap();
        let b = Store::open_in_memory().unwrap();
        a.insert_observation(&obs(Uuid::now_v7())).unwrap();
        b.insert_observation(&obs(Uuid::now_v7())).unwrap();

        let (a, b, oa, ob) = sync_pair(a, b, vec![1u8; 32], vec![2u8; 32]);
        // The handshake must fail (mismatched PSK) on at least one side...
        assert!(oa.is_err() || ob.is_err());
        // ...and neither store received the peer's record.
        assert_eq!(a.all_observations().unwrap().len(), 1);
        assert_eq!(b.all_observations().unwrap().len(), 1);
    }
}
