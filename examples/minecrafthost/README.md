# minecrafthost - Minimal Minecraft Server in Forge

A minimal Minecraft Java Edition server written in the Forge programming language.

## Features

- Listens on port 25565 (default Minecraft port)
- Responds to server list pings (status requests) with dynamic online player count
- Handles login sequence: Login Start → Login Success → Join Game
- Sends superflat world configuration (bedrock, dirt, grass, full sky light)
- Fork-based concurrency (each connection handled in a child process)
- 10-second receive timeout on handshake/login; 1-second timeout in play
- Per-byte syscalls eliminated via buffered `SocketReader`
- SIGPIPE ignored by the runtime; writes to dead peers return `-EPIPE`
- In-game commands: `/help`, `/seed`, `/spawn`, `/time`, `/gamemode`, `/tp`, `/list`

## Protocol

The server implements the Minecraft protocol for version 1.20.2 (protocol 764):

- **Handshake**: Client sends initial connection with `next_state` field
- **Status** (next_state=1): Server list ping flow
  1. Handshake (state=1)
  2. Status Request → Status Response (JSON with `"online": N`)
  3. Ping → Pong (payload echoed verbatim)
- **Login** (next_state=2): Login flow
  1. Handshake (state=2)
  2. Login Start → Login Success (UUID as 16 bytes, username, properties)
  3. Login Acknowledged → Enter Configuration state
  4. Registry Data (packet 0x05) → Dimension/biome/chat codec
  5. Finish Configuration (config state 0x02) → Player finishes config → Play state
  6. Join Game (play state)
  7. Keep Alive
  8. Player Info

**Important**: Minecraft protocol state values:
- 0 = Handshaking (not used for connection handling)
- 1 = Status (server list ping)
- 2 = Login (joining the server)

**Packet IDs for 1.20.2 (protocol 764)**:
- Status Response: 0x00
- Pong: 0x01
- Login Success: 0x02 (login state)
- Login Acknowledged: 0x03 (config state toServer)
- Finish Configuration: 0x02 (config state)
- Registry Data: 0x05 (config state)
- Join Game: 0x29 (play state)
- Keep Alive: 0x24 (play state)
- Player Info: 0x3C (play state)

**Notes**:
- No compression: the server never sends Set Compression. Vanilla servers with
  compression disabled omit the packet entirely; clients stay in plain framing
  and everything keeps working. (Sending Set Compression is only useful when
  actually compressing, since clients enable their compression pipeline after
  receiving it.)
- `registry.nbt` already includes the registry_data packet ID byte (0x05) as
  its first byte, so it is forwarded verbatim. The codec is baked into the
  binary at compile time with `embed REGISTRY = "../registry.nbt"` in
  `net/registry.dev` (data lives in `.rodata`; no file I/O at runtime).
- All multi-byte integers are written big-endian, as required by the protocol.
- Online player count is tracked via a file at `/tmp/minecrafthost.count`
  (race-tolerant, cosmetic precision).

## Building

```sh
make build   # compiles minecrafthost
make run     # starts the server on port 25565
make test    # runs the integration test (port 25565 must be free)
make clean   # removes the binary
```

Or manually with the Forge compiler:
```sh
../../target/release/forgec minecrafthost.dev -o minecrafthost --target x86_64-unknown-linux-gnu
./minecrafthost
```

## Testing

The integration test in `tests/integration.rs` validates:
1. `minecrafthost_dev_compiles_and_responds` - Server responds to status pings (with pong payload echo verification) and handles the login sequence

Run with:
```sh
make test
# or directly:
cargo test --test integration minecrafthost_dev_compiles_and_responds
```

## Running

```sh
./minecrafthost
```

Connect with any Minecraft 1.20.2 client at `localhost:25565`.

In-game commands (prefix with `/`):
- `/help` — show command list
- `/seed` — show world seed (0)
- `/spawn` — teleport to spawn (8.5, -60, 8.5)
- `/time <0-24000>` — set time of day
- `/gamemode <0|1|2|3>` — survival/creative/adventure/spectator
- `/tp <x> <y> <z>` — teleport to coordinates
- `/list` — show online player count and your name

## Architecture

The server is a single-process, fork-based server:
- Main process listens on port 25565 and accepts connections
- Each accepted connection forks a child process
- Child processes handle the status or login protocol flow
- Child processes exit after the client disconnects

The server uses a fixed superflat world with:
- Dimension: minecraft:overworld (ID 0)
- World type: 1 (default)
- Seed: 0
- Block count in floor section: 1024 (256 bedrock + 512 dirt + 256 grass)
- Full sky light (15) in all 24 sections

In play state the server answers keep-alives every 10 seconds (using
SO_RCVTIMEO=1s so reads never block forever) and handles movement,
chat, and block interactions.