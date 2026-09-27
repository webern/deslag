# Feature Specification: Interactive Terminal Support

This feature implements robust interactive terminal support in `cderun`, ensuring a seamless and transparent user experience when executing interactive shells or Terminal User Interface (TUI) applications within ephemeral containers.

The implementation is designed following the technical architectural patterns documented in [go-cli-container-interaction.md](../references/go-cli-container-interaction.md).

## Feature Overview

### 1. Terminal Raw Mode

When a pseudo-TTY is allocated (`--tty`), `cderun` uses the `golang.org/x/term` library to transition the host's terminal file descriptor into "Raw Mode". This disables local character echoing and line buffering, forwarding all keypresses (including terminal escape and control sequences) directly to the containerized process. Terminal properties are safely and automatically restored on host process termination.

### 2. Signal Trapping and Forwarding

`cderun` traps standard OS lifecycle signals (such as `SIGINT`, `SIGTERM`, and `SIGHUP`) received on the host and forwards them directly to the containerized process via the container engine APIs. This ensures that pressing `Ctrl+C` or terminating the host process triggers proper signal-handling and graceful resource cleanup within the container. A two-phase termination (SIGINT followed by SIGKILL with grace periods) handles stuck containers.

### 3. Window Resize Synchronization (`SIGWINCH` on Unix)

On Unix platforms, the execution engine monitors terminal window resize events (`SIGWINCH`) on the host. When a resize occurs, the active terminal dimension (rows and columns) is dynamically synced with the containerized TTY. This prevents interface layout distortion when executing rich console utilities or text editors (such as `vim`, `nano`, or `htop`).

### 4. Robust I/O Lifecycle and Goroutine Cleanup

To prevent background goroutine leaks, all standard I/O streams are strictly monitored. Upon container process termination, stream pipes are closed to ensure that background stream copier goroutines return and terminate cleanly.

### 5. Windows ConPTY Support (Future Roadmap)

To deliver a consistent interactive console experience on Windows hosts, native Windows Pseudo Console (ConPTY) support is planned for a future release.

## Implementation Details

- **Terminal Raw Mode**: Configured via `term.MakeRaw(int(os.Stdin.Fd()))`.
- **Signal Forwarding**: Orchestrated via standard Go `os/signal` channels and the abstract `Runtime.SignalContainer` API.
- **Resize Tracking**: On Unix systems, resize tracking is trapped via `SIGWINCH` (handled in `signals_unix.go`) and applied to the container using the abstract `Runtime.ResizeContainerTTY` API. Windows terminal resizing is not handled via the same signal-trapping mechanism.
