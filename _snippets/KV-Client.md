Below is a minimal standalone Rust CLI that talks to your TCP cache service. It uses `clap` for argument parsing and a blocking `TcpStream` to send one command and print the reply.

```toml
# Cargo.toml (add these deps)
[dependencies]
clap = { version = "4.0", features = ["derive"] }
```

```rust
// src/bin/kv_client.rs
use clap::{Parser, Subcommand};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

#[derive(Parser)]
#[command(name = "kv_client", about = "CLI for the KV cache service")]
struct Cli {
    /// Service address, e.g. 127.0.0.1:6379
    #[arg(short, long, default_value = "127.0.0.1:6379")]
    addr: String,

    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// PUT key value
    Put {
        key: String,
        value: String,
    },
    /// GET key
    Get {
        key: String,
    },
    /// DELETE key
    Delete {
        key: String,
    },
}

fn main() -> std::io::Result<()> {
    let cli = Cli::parse();
    let mut stream = TcpStream::connect(&cli.addr)?;
    let line = match cli.cmd {
        Commands::Put { key, value } => format!("PUT {} {}", key, value),
        Commands::Get { key }         => format!("GET {}", key),
        Commands::Delete { key }      => format!("DELETE {}", key),
    };
    // send command
    writeln!(stream, "{}", line)?;
    // read one-line response
    let mut reader = BufReader::new(stream);
    let mut resp = String::new();
    reader.read_line(&mut resp)?;
    print!("{}", resp);
    Ok(())
}
```

**Usage examples:**

```bash
# put a value
$ kv_client put foo bar
OK

# get it back
$ kv_client get foo
bar

# delete it
$ kv_client delete foo
OK

# missing key
$ kv_client get foo
nil
```

This simple client can be dropped into your workspace under `src/bin/kv_client.rs` and built alongside the server.
