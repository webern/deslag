# FiveWin track

A desktop GUI over OpenADS. The engine and the data access are exactly
the same as the console track (`rddads` + the `ADSCDX` RDD); the only new
thing is the user interface, drawn with **FiveWin** (FWH).

| Example | Level | Shows |
|---------|-------|-------|
| [`crud_browse`](crud_browse.prg) | intermediate | FWH **xBrowse** grid over an OpenADS table + a **CRUD form** (New / Edit / Delete) through a dialog; all writes land on the engine via the ordinary xBase verbs |

## Two ways to run it

`crud_browse` is written so its CRUD logic (`AddRow` / `EditRow` /
`DelRow`) is callable with **or** without a window:

* **interactive** — `crud_browse.exe`
  Opens the grid with a New / Edit / Delete button bar.

* **head-less smoke** — `crud_browse.exe /auto`
  Drives add → edit → delete through the same functions and verifies the
  results without opening a window. This is what the build script runs to
  confirm the example works against the engine on a machine with no
  display, and it prints `OK: FWH /auto ...` on success.

## Build

FiveWin is a commercial library you install separately, so — unlike the
console/ORM tracks — there is no single self-contained toolchain here.
`build.cmd` reads every path from the environment (no drive letters):

```cmd
set HB_INSTALL=...your Harbour root...
set FWDIR64=...your FiveWin install...
set MSVC_UCRT_X64=...Windows SDK ucrt x64 lib dir...
set MSVC_SETUP=...a vcvars x64 .bat...
build.cmd  <folder-with-the-OpenADS-DLL>  crud_browse.hbp
crud_browse.exe /auto
```

The `.hbp` is the stock FWH link recipe; only the last three lines
(`-lrddads` + the OpenADS import lib) point the app at OpenADS instead of
any other ACE-compatible engine. Drop the OpenADS DLL next to the `.exe`
(the build script copies it) so it is the one resolved at run time.

Data is invented (a tiny product list). No real-world data.
