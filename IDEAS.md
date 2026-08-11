# Forge — Ideas for Improvement and Projects

## Compiler Improvements

### Completed
- **Hosted runtime `argc`/`argv` passing** — `_start` forwards argc/argv to
  `_forge_main` in both x86_64 and x86_32 backends; `pub def main(argc, argv)`
  can now read the command line.  0-param mains ignore the extra args.
- **Pointer arithmetic scaling** — `ptr + index` now scales the index by the
  pointee size (shift, since all scalar sizes are powers of two); fixes
  multi-element `argv[i]` access and slice/field indexing.
- **Conservative garbage collection** — `std.gc`/`std.alloc` replace the old
  bump arena with a first-fit free-list allocator over a 4 MiB heap plus a
  conservative mark-and-sweep collector (x86_64 hosted target).  Collection
  runs automatically on heap exhaustion or via `collect()`; `leak_check()`
  reports unreachable bytes without reclaiming them.  The collector scans the
  stack and read-only data as root sets; dead frames are zeroed on function
  return so dropped references are detected precisely.

### Optimizer
- **Constant folding** — evaluate constant expressions at compile time
- **Dead code elimination** — remove unreachable code and unused functions/consts
- **Common subexpression elimination (CSE)** — reuse computed values
- **Register allocation** — graph-coloring or linear-scan register allocator
- **Instruction selection** — pattern-match IR to optimal instruction sequences

### Error Messages & Diagnostics
- **Source spans** — attach line/column ranges to AST nodes for precise errors
- **"Did you mean?"** — suggest similar identifiers on unknown name errors
- **Multi-line context** — show several lines of context around errors
- **Type mismatch hints** — explain why two types are incompatible
- **Warning system** — unused variables, unreachable code, shadowed names

### Memory Management
- ~~Precise garbage collection~~ *(done)* — conservative mark-and-sweep GC in
  the hosted x86_64 runtime (`std.gc`), with automatic collection on heap
  exhaustion, explicit `collect()`, and `leak_check()` diagnostics.  Still
  open: precise (non-conservative) collection.
- **Ownership types** — borrow-checker-style memory safety without GC
- **Region-based memory** — arena allocators with automatic lifetimes
- **Smart pointers** — `Box<T>`, `Rc<T>`, `Arc<T>` patterns

### Language Features
- **String type** — proper heap-allocated strings with concatenation, slicing
- **Vec<T> / Array type** — growable arrays with push/pop/insert/remove
- **HashMap<K, V>** — hash table data structure in stdlib
- **Pattern matching** — exhaustive match with binding, slice patterns
- **Iterators** — lazy iterator protocol with `for` loop integration
- **Async/await** — cooperative async I/O with event loops
- **Modules with visibility** — `pub` / private exports, re-exports
- **Macros / Procedural macros** — compile-time code generation
- **Generics with trait bounds** — parametric polymorphism with interfaces

### Standard Library Expansion
- **DateTime** — parsing, formatting, timezone support
- **URL encoding / decoding** — percent-encoding, query string parsing
- **HTTP client** — outbound HTTP requests with connection pooling
- **JSON parsing** — deserialize JSON into structs
- **CSV parsing** — structured data reading
- **Regex** — string pattern matching with capture groups
- **TLS / Crypto** — secure sockets, hashing (SHA-256), random generation
- **Compression** — gzip/zlib inflate/deflate
- **SQLite bindings** — embedded database access via FFI
- **Threading** — POSIX threads, channels, mutexes

### Backend Targets
- **ARM64 (AArch64)** — mobile and server targets
- **RISC-V** — educational and embedded targets
- **WebAssembly (WASM)** — run in the browser
- **Windows PE** — MSVC-compatible or MinGW output
- **macOS Mach-O** — native macOS binaries
- **Bare-metal ARM** — Cortex-M microcontrollers

### Developer Tooling
- **DWARF debug info** — GDB / LLDB debugging support
- **Language Server Protocol (LSP)** — IDE features (autocomplete, go-to-def, etc.)
- **Package manager** — `forge pkg install`, dependency resolution, `Package.toml`
- **Build system** — `forge build`, `forge run`, `forge test` commands
- **Formatting tool** — `forge fmt` for consistent code style
- **Test framework** — `#[test]` attribute, assertion macros
- **Documentation generator** — extract docs from comments into HTML
- **Profiling** — CPU and memory profiling of generated programs

## Project Ideas Buildable with Forge

### Operating Systems & Firmware
- **Minimal OS kernel** — using the `x86_16-boot` target for boot sector
- **Multiboot kernel** — 32-bit x86 kernel with paging, interrupts, scheduler
- **BIOS extensions** — EBDA manipulation, hardware probing
- **Bootloader with UEFI support** — modern firmware boot
- **Embedded RTOS** — real-time OS for microcontrollers (ARM Cortex-M)

### System Utilities
- **Build system** — like `make`, with dependency tracking and parallelism
- **File synchronizer** — like `rsync`, with delta transfer algorithm
- **Log analyzer** — parse, filter, aggregate log files in real-time
- **Network scanner** — port scanner, service fingerprinting
- **Process monitor** — like `top`/`htop`, per-process resource usage
- **File recovery tool** — undelete deleted files from ext4 filesystem
- **System profiler** — hardware info, benchmarking, performance counters

### Web & Network Services
- **Static file CDN** — with LRU caching, range requests, HTTP caching
- **Reverse proxy** — load balancing, connection pooling, health checks
- **WebSocket server** — real-time bidirectional communication
- **HTTP/2 server** — binary protocol, header compression, multiplexing
- **DNS server** — recursive resolver, authoritative zone serving
- **SMTP server** — email receiving with spam filtering
- **IRC server / client** — chat protocol implementation
- **BitTorrent client** — P2P file sharing protocol
- **Chat application** — end-to-end encrypted messaging server + client

### Games & Interactive Media
- **2D game engine** — sprite rendering, collision detection, audio
- **Roguelike** — procedural dungeon generation, turn-based gameplay
- **Text adventures** — interactive fiction engine with parser
- **Terminal-based games** — Snake, Tetris, Pong in the console
- **Pixel art editor** — drawing tool with layers and export formats
- **Music tracker** — sequenced audio synthesis, pattern-based composition
- **ASCII art renderer** — convert images to ANSI/Unicode terminal art

### Development Tools
- **Self-hosting compiler** — compile Forge with a Forge-compiled compiler
- **Bytecode VM** — implement a virtual machine for a custom instruction set
- **Disassembler** — x86/x86_64 binary analysis and disassembly
- **Static analyzer** — detect bugs, security vulnerabilities, style issues
- **Code formatter** — for Forge and other languages
- **Diff tool** — like `diff`/`git diff`, with syntax highlighting
- **Terminal multiplexer** — like `tmux`, with panes and sessions
- **Line editor** — readline-like library for CLI apps

### Data Processing & Scientific Computing
- **CSV/JSON processor** — filter, transform, aggregate structured data
- **Log correlation** — join events across multiple log sources
- **Time series database** — store and query metrics data
- **Statistical analyzer** — compute distributions, regression, hypothesis testing
- **Image processor** — PPM/PGM manipulation, filters, transformations
- **Compression tool** — implement DEFLATE, LZ77, Huffman coding
- **Database engine** — SQL-like query language, B-tree indexes, transactions
- **Search engine** — inverted index, TF-IDF ranking, full-text search

### Security & Cryptography
- **Password manager** — encrypted storage, password generation, breach checking
- **TLS client/server** — implement SSL/TLS handshake, certificate validation
- **Blockchain node** — simple cryptocurrency, mining, wallet
- **Password cracker** — dictionary attacks, brute force (educational)
- **Firewall** — packet filtering, NAT, connection tracking
- **Intrusion detection** — signature-based and anomaly-based detection
- **Secure shell** — SSH-like remote terminal with encryption

### Education & Research
- **Brainfuck interpreter** *(done)* — `examples/brainfuck/`; reads program
  from `argv[1]`, handles `><+-.,[]`, 30k-cell tape.  Ships integration tests
  and runs on both x86_64 and x86_32.
### Education & Research
- **Snake** *(done)* — `examples/snake/`; terminal Snake via ANSI escapes, non-blocking stdin, `rand`. Runs on x86_64 and x86_32.
- **Assembly learning environment** — visualize registers, memory, execution flow
- **Compiler textbook projects** — brainfuck compiler, Lisp interpreter
- **Distributed systems simulator** — model consensus algorithms (Raft, Paxos)
- **Formal verification tool** — model checking, theorem proving basics
- **Plagiarism detector** — compare source code similarity across files
- **Automated grader** — compile and test student programming assignments

### Creative & Miscellaneous
- **Static site generator** — Markdown to HTML, with template engine
- **Markdown renderer** — parse and render Markdown to HTML/terminal
- **Email client** — read/send emails with encryption support
- **Torrent indexer** — track and serve torrent metadata
- **RSS reader** — fetch, parse, and display RSS feeds
- **Personal finance tracker** — budgeting, expense tracking, reporting
- **Recipe manager** — store, search, scale recipes
- **Habit tracker** — daily streaks, progress visualization

## Next Steps for the Website Example

1. **Add TLS/HTTPS** — integrate mbedTLS or implement a minimal TLS stack
2. **HTTP/1.1 keep-alive** — handle multiple requests per connection
3. **POST support** — read request bodies, handle forms
4. **Content templates** — simple template engine for HTML
5. **Session management** — cookies, session IDs, state
6. **URL routing** — regex-based route matching with path parameters
7. **File uploads** — handle multipart/form-data
8. **Compression** — gzip/deflate response compression
9. **Static asset pipeline** — minify CSS/JS, generate thumbnails
10. **Logging** — structured request logging with timestamps
11. **Rate limiting** — prevent abuse with token bucket / sliding window
12. **Metrics** — Prometheus-style metrics endpoint for monitoring
