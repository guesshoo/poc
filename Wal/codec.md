You can cleanly swap in “text” vs “CBOR” serialization by factoring out your payload encoder/decoder behind a small `trait` (i.e. the Strategy pattern), and then wiring your reader/writer to use whatever implementation you’ve configured. Here’s a sketch of how you might do it:

```rust
use std::{io, fmt::Debug};

/// Your plain old WAL record as before…
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct WalRecord { /* … */ }

/// A small trait for encoding/decoding a WalRecord payload.
pub trait WalCodec: Send + Sync + Debug + 'static {
    /// Encode a record to bytes (payload only).
    fn encode(&self, record: &WalRecord) -> io::Result<Vec<u8>>;

    /// Decode from payload bytes back into a record.
    fn decode(&self, data: &[u8]) -> io::Result<WalRecord>;
}

/// CBOR implementation using serde_cbor
#[derive(Debug)]
pub struct CborCodec;

impl WalCodec for CborCodec {
    fn encode(&self, record: &WalRecord) -> io::Result<Vec<u8>> {
        serde_cbor::to_vec(record)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
    fn decode(&self, data: &[u8]) -> io::Result<WalRecord> {
        serde_cbor::from_slice(data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

/// “Text” implementation—e.g. JSON lines—for easy human-inspection
#[derive(Debug)]
pub struct JsonCodec;

impl WalCodec for JsonCodec {
    fn encode(&self, record: &WalRecord) -> io::Result<Vec<u8>> {
        // Serialize to JSON + a newline so each record is on its own line
        let mut s = serde_json::to_string(record)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        s.push('\n');
        Ok(s.into_bytes())
    }
    fn decode(&self, data: &[u8]) -> io::Result<WalRecord> {
        // data includes trailing newline
        let s = std::str::from_utf8(data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        serde_json::from_str(s.trim_end())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}
```

Then make your writer/reader generic over `C: WalCodec` (or store a `Box<dyn WalCodec>` if you prefer dynamic dispatch):

```rust
pub struct AsyncWalWriter<C: WalCodec> {
    codec: C,
    inner: BufWriter<tokio::fs::File>,
}

impl<C: WalCodec> AsyncWalWriter<C> {
    pub fn with_codec(codec: C, file: tokio::fs::File) -> Self {
        Self { codec, inner: BufWriter::new(file) }
    }

    pub async fn append(&mut self, record: &WalRecord) -> io::Result<()> {
        let payload = self.codec.encode(record)?;
        let len = payload.len() as u64;
        self.inner.write_all(&FILE_IDENTIFIER).await?;
        self.inner.write_all(&len.to_be_bytes()).await?;
        self.inner.write_all(&payload).await?;
        self.inner.flush().await?;
        Ok(())
    }
}

// And similarly for AsyncWalReader<C>…
```

### Usage

```rust
// Text mode (e.g. in development):
let file = tokio::fs::OpenOptions::new().create(true).append(true).open(path).await?;
let mut writer = AsyncWalWriter::with_codec(JsonCodec, file);

// Real-world (binary) mode:
let file = tokio::fs::OpenOptions::new()…open(path).await?;
let mut writer = AsyncWalWriter::with_codec(CborCodec, file);
```

Because both codecs implement the same `WalCodec` trait, the rest of your WAL framing logic stays 100% unchanged. You can even switch at runtime by holding a `Box<dyn WalCodec>` in your `WriteAheadLog`.

---

#### Compile-time feature flags (optional)

If you only ever need one or the other in a given build, you can also gate the implementations on Cargo features:

```toml
[features]
text = []
cbor = []

[dependencies]
serde_cbor = { version="…", optional = true }
serde_json = { version="…", optional = true }
```

```rust
#[cfg(feature = "cbor")]
pub type DefaultCodec = CborCodec;
#[cfg(feature = "text")]
pub type DefaultCodec = JsonCodec;

pub type DefaultWalWriter = AsyncWalWriter<DefaultCodec>;
```

This way `cargo build --features text` gives you JSON mode, and `--features cbor` gives you CBOR mode, with no runtime overhead for unused code.
