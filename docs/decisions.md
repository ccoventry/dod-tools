# Technical Decisions

## Decision Log

### Threading over Async UI
- **Context:** The `egui` framework requires a highly responsive main thread.
- **Decision:** All heavy parsing and HLAE process executions are offloaded to `std::thread::Builder` rather than `tokio` async tasks.
- **Rationale:** Prevents immediate-mode UI freezing during heavy blocking I/O (binary patching).
- **Consequences:** Requires passing state back to the UI via `mpsc` channels and explicitly requesting repaints (`ctx.request_repaint()`).
