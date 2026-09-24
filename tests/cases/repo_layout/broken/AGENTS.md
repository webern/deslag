# Agents

Read this before you change anything.

## Repository layout

```text
repo/
  Makefile      <- every build and test
  src/lib.rs    <- the library
   src/main.rs  <- the binary
  docs/        <- the design docs
  src/          <- the source: the library, the binary, a module for each lint, and the helpers they all share
  gone.rs       <- deleted last week
  Makefile/     <- the build, as a directory
  /etc/hosts    <- where the hosts live
  src tests     <- the code
  README.md     <-
                   what the project is
       and a line under nothing
```

## Build

Run `make`.
