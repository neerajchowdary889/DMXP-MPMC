# DMXP-MPMC Documentation

## Quick Start

```bash
# Terminal 1: Rust producer
cargo run --example producer 4 1000

# Terminal 2: Python consumer
python3 tests_py/consumer.py 4 1000

# TUI monitoring (requires tui_demo running first)
cargo run --example tui_demo   # Terminal 1
cargo run -p dmxp-tui          # Terminal 2
```

## Documentation Index

| File | Contents |
|---|---|
| [ARCHITECTURE.md](./ARCHITECTURE.md) | System overview, design decisions, mermaid diagrams, performance characteristics |
| [MEMORY_LAYOUT.md](./MEMORY_LAYOUT.md) | Exact byte offsets for every struct; Rust/Python/C definitions; validation checklist |
| [BUILDING_CONSUMERS.md](./BUILDING_CONSUMERS.md) | Full working examples in Rust, Python, C, and Go |
| [SFU_PROBLEM.md](./SFU_PROBLEM.md) | Why the shared-memory overflow backend is needed; problem analysis and fix history |
| [SHARED_BACKEND_INTEGRATION.md](./SHARED_BACKEND_INTEGRATION.md) | Implementation details of the SFU shared backend; deployment and monitoring guide |
| [TUI_DEMO_GUIDE.md](./TUI_DEMO_GUIDE.md) | Running the large-payload demo, TUI metrics explained, testing commands, limitations |
| [walkthrough.md](./walkthrough.md) | Code map with inline file/line references for both DMXP-MPMC and stable-fragmented-buffer |

## Key Facts

- **SHM region**: `/dev/shm/dmxp_alloc` (128 MB)
- **Overflow store**: `/dev/shm/dmxp_ovf_ctrl` + `/dev/shm/dmxp_ovf_data_N` (32 MB chunks)
- **Inline limit**: `MSG_INLINE` = 1024 bytes; overflow threshold = 921 bytes (90%); larger messages spill to SFU
- **Max channels**: 256
- **Magic number**: `0x444D58505F4D454D` ("DMXP_MEM")
