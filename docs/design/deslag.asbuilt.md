---
updated: 2026-09-17
subsystems:
  - deslag
max_size_bytes: 16384
---
# deslag: as built

One package with two targets. The library holds the logic; the binary calls into it.

```
src/
  lib.rs   the library: `greeting()`, a placeholder
  main.rs  the binary: prints `greeting()`
```

Nothing else exists yet.
