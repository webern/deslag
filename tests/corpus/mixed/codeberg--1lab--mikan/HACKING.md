Code of conduct
===============

* Follow the [contributing guidelines for this project](CONTRIBUTING.md).

* Follow the Haskell style guide at
  https://github.com/andreasabel/haskell-style-guide/blob/master/haskell-style.md .

* Familiarize yourself with our local toolbox at `src/full/Mikan/Utils/*`.

* Write code with verification in mind, documenting invariants and
  pre- and post-conditions.  Testable invariants and algebraic properties
  go to the internal testsuite at `test/Internal`.

* Document (in `haddock` style) the purpose of functions and data structures,
  down to the individual constructor and field.
  An overview over a component or algorithm should be given in the
  `haddock` module comment.

* Remember to document your new feature in `doc/user-manual` and briefly in
  `CHANGELOG.md`. See [Testing and documentation](#testing-and-documentation).

* Changes should go through a pull request.

  Opening a pull request will also run the testsuite via our CI (continuous
  integration suite).

Note: Some instructions in this document are likely outdated,
so take everything with a grain of salt.
Fixes to outdated instructions welcome!

How to build Mikan from source
=============================

Running `cabal build` from the repository root should build Mikan, assuming
you set up dependencies correctly (see "Troubleshooting Cabal").

Note: you can speed up `cabal` builds at the cost of using more RAM with

```shellsession
$ cabal -j --semaphore
```

With Nix
--------

You can also build Mikan with Nix (you will need to
[enable Flakes](https://wiki.nixos.org/wiki/Flakes#Setup)).

Running

```shellsession
$ nix build -L
```

will ask you whether you want to use our binary cache to avoid building
things that were already built in CI (we recommend answering `y` at all prompts),
and then build Mikan following the instructions in the `flake.nix` file in the current directory, including dependencies.
Results are cached in `/nix/store`, and a symlink called `result-bin` is created.

See the `flake.nix` to see what build goals are available.

We also provide a Nix shell to make dependencies available when building
with Cabal. Run `nix develop` or use [nix-direnv](https://github.com/nix-community/nix-direnv)
with a `.envrc` file containing `use flake`.

Cabal flag reference
====================

All of Mikan's build tools indirectly build Mikan with the `Cabal` library.

When building Mikan the following Cabal flags can be used:

* `debug`

  Enable debug printing. This makes Mikan slightly slower, and
  building Mikan slower as well. The `--verbose={N}` option
  only has an effect when Mikan was installed with this flag.
  Default: off.

* `debug-serialisation`

  Enable debug mode in serialisation.
  This makes serialisation slower.
  Default: off.

* `debug-parsing`

    Enable debug mode in the parser
    This makes parsing slower.
    Default: off.

* `dump-core`

    Save GHC Core output during compilation of Mikan.
    Default: off.

* `enable-cluster-counting`

     Enable cluster counting (see docs). This will require the [`text-icu`
     Haskell library](https://hackage.haskell.org/package/text-icu), which in
     turn requires that ICU be installed. Note that if `enable-cluster-counting`
     is `False`, then option `--count-clusters` triggers an error message when
     given to Mikan.
     Default: off, but on for development version.

* `optimise-heavily`

     Optimise Mikan heavily. If this flag is on, compiling Mikan uses more memory
     but Mikan runs faster.
     Default: on.

* `use-xdg-data-home`

    Added in 2.8.0

    Install data files under `$XDG_DATA_HOME/agda/$MIKAN_VERSION` by default
    instead of the installation location defined by Cabal,
    see the command-line option `--print-data-dir`.
    This should *not* be enabled in declarative build environments like Nix or Guix.
    Default: off.

Hint: You can set these flags as follows:

* Cabal-install: use the `-f` argument:

      cabal install -fenable-cluster-counting

* Nix: code into your `.nix` scripts using `enableCabalFlag`:

      hlib.enableCabalFlag "debug" hpkgs.Mikan-base

Troubleshooting Cabal
=====================

Dynamic linking issues
----------------------

If you have setting ``executable-dynamic: True`` in your cabal configuration
then installation will likely fail on Windows.

Cure: change to default ``executable-dynamic: False``.

Further information:

* https://github.com/agda/agda/issues/7163
* https://github.com/haskell/cabal/issues/9784


Installing ICU
--------------

If cluster counting is enabled (see the ``enable-cluster-counting`` flag above,
enabled by default), then you will need the [ICU](http://site.icu-project.org)
library to be installed. See the [text-icu Prerequisites
documentation](https://github.com/haskell/text-icu#prerequisites) for how to
install ICU on your system.

`zlib` and `ncurses` Dependency
-------------------------------

Non-Windows users need to ensure that the development files for the C
libraries *zlib* and *ncurses* are installed (see https://zlib.net
and https://www.gnu.org/software/ncurses/). Your package manager may be
able to install these files for you. For instance, on Debian or Ubuntu
it should suffice to run

    apt-get install zlib1g-dev libncurses5-dev

as root to get the correct files installed.

Keeping the Default Environment Clean
--------------------------------------

You may want to keep the default environment clean, e.g. to avoid conflicts with
other installed packages. In this case you can a create a separate GHC
environment for Mikan by running:

    cabal install --package-env mikan --lib Mikan ieee754

You then have to set the `GHC_ENVIRONMENT` when you invoke Mikan:

    GHC_ENVIRONMENT=mikan mikan -c hello-world.agda

N.B. Actually it is not necessary to register the Mikan library,
but doing so forces Cabal to install the same version of
[ieee754](https://hackage.haskell.org/package/ieee754)
as used by Mikan.

Installing Multiple Versions of Mikan
------------------------------------

Multiple versions of Mikan can be installed concurrently by using the
``--program-suffix`` flag.
For example:

    cabal install Mikan-2.6.4.3 --program-suffix=-2.6.4.3

will install version 2.6.4.3 under the name mikan-2.6.4.3. You can then switch to
this version of Mikan in Emacs via

    C-c C-x C-s 2.6.4.3 RETURN

Switching back to the standard version of Mikan is then done by:

    C-c C-x C-s RETURN

The VSCode mode also supports selecting multiple Mikan versions.

Working with Git
================

Since: 2013-06-15.

Cloning
--------

Since Mikan's repository uses submodules, you should be cloning the
repository by running:
```bash
git clone --recurse-submodules https://codeberg.org/1lab/mikan.git
```


Testing and documentation
=========================

* When you implement a new feature it needs to be documented in
  `doc/user-manual/` and `CHANGELOG.md`.

* In both cases, you need to add regression tests under `test/Succeed`
  and `test/Fail`, and maybe also `test/interaction`.
    * When adding test cases under `test/Fail`, remember to record the error messages
      (`.err` files) after running make test.
    * Same for `.warn` files in `test/Succeed` and `.out` files in `test/interaction`.
    * You can also add `.flags` files to set Mikan options.
    * You can also add `.vars` files to set environment variables (which may reference other environment variables, even those in the file appearing before them).

* Track the dependencies of your test:
  * Need Mikan compiled with `-fdebug`? Add to `fdebugTestFilter` in `Main.hs`.
  * Need `node`, `ghc`, `latexmk`, etc.? Choose the appropriate sub-directory.

* Run the test-suite, using `cabal test`.

* You can run a single interaction test by going into the
  `test/interaction` directory and typing `make <test name>.cmp`.

* Additional options for the tests using the Haskell/tasty test runner
  can be given using `--test-options="..."`. In order to see potential options
  run `cabal test --test-options="--help"`

* Tests under `test/Fail` can fail if an error message has changed.

* Tests under `test/Succeed` will also be tested for expected warning
  messages if there is a corresponding `.warn` file.

* If you use GHC 9.2 or later and compile using the GHC options
  `-finfo-table-map` and `-fdistinct-constructor-tables`, then you can
  [obtain](https://well-typed.com/blog/2021/01/first-look-at-hi-profiling-mode/)
  heap profiles that tie heap closures to source code locations, even
  if the program is not compiled using `-prof`. However, use of these
  flags can make the Mikan binary much larger, so they are not
  activated by default.

  The following steps might work (first install `eventlog2html` using,
  for instance, something like `cabal install eventlog2html`):
  ```sh
  cabal install --ghc-options="-finfo-table-map -fdistinct-constructor-tables"
  mikan-VERSION … +RTS -l-au -hi -i0.5
  eventlog2html mikan-VERSION.eventlog
  ```
  Here `VERSION` is Mikan's version number. View the resulting file
  `mikan-VERSION.eventlog.html` and check the tab called "Detailed".

* [One way](https://mpickering.github.io/posts/2019-11-07-hs-speedscope.html)
  to obtain time profiles is to compile with profiling enabled, using
  the GHC option
  [`-fprof-late`](https://downloads.haskell.org/ghc/latest/docs/users_guide/profiling.html#ghc-flag--fprof-late)
  (which is available starting from GHC 9.4.1), and to run Mikan with
  the run-time options `+RTS -p -l-au`. One should then obtain a
  `.eventlog` file which can be converted to a `.eventlog.json` file
  using
  [hs-speedscope](https://hackage.haskell.org/package/hs-speedscope).
  That file can then be loaded into
  [speedscope.app](https://www.speedscope.app/).

  The following steps might work (first install `hs-speedscope` using,
  for instance, something like `cabal install hs-speedscope`):
  ```sh
  cabal build \
    --disable-documentation \
    -fenable-cluster-counting \
    --enable-profiling --program-suffix=-prof \
    --profiling-detail=none --ghc-options=-fprof-late \
    --ghc-options="+RTS -A128M -M4G -RTS"
  Mikan_datadir=src/data/ dist-newstyle/build/*/ghc-*/Mikan-*/build/mikan/mikan … +RTS -p -l-au
  hs-speedscope mikan.eventlog
  ```
  Load the resulting file `mikan.eventlog.json` into
  [speedscope.app](https://www.speedscope.app/).

* To avoid problems with the whitespace test failing we suggest add the
  following lines to `.git/hooks/pre-commit`:
  ```sh
      echo "Starting pre-commit"
      make check-whitespace
      if [ $? -ne 0 ]; then
        exit 1
      fi
      echo "Ending pre-commit"
  ```

* To build the user manual locally, you need to install
  the following dependencies:

    + Python >=3.4.6.

    + Sphinx and sphinx-rtd-theme

          pip3 install --user -r doc/user-manual/requirements.txt

      Note that the `--user` option puts the Sphinx binaries in
      `$HOME/.local/bin`.

    + LaTeX

  To see the list of available targets, execute `make help`
  in doc/user-manual. E.g., call `make html` to build the
  documentation in html format.

* Internal test-suite

  The internal test-suite `test/Internal` is used for testing the Mikan library
  (which after closing Issue #2083 doesn't use the QuickCheck library).

  The test-suite uses the same directory structure as the Mikan library.

  Internal tests for a module `Mikan.Foo.Bar` should reside in module
  `Internal.Foo.Bar`.  Same for `Arbitrary` and `CoArbitrary` instances.

  One can load internal test-suite modules in GHCi. Here is one
  example of what can be done:
  ```shell
  cabal repl tests -O0 --repl-no-load
  […]
  ghci> :l Internal.TypeChecking.Substitute
  […]
  ghci> quickCheck prop_wkS
  +++ OK, passed 100 tests.
  ghci> Test.Tasty.defaultMain tests
  […]
  *** Exception: ExitSuccess
  ```

Testing with Codeberg CI
========================

Codeberg has very graciously let Mikan run our continuous integration
tests on their hardware. To avoid overburdening shared resources, we
ask that contributors run tests locally before kicking off a CI run,
either through cabal via `cabal test all`, through Nix via `nix build --print-build-logs`,
or by running the CI locally as detailed below.

All CI runs on PRs need to be manually approved by a maintainer. This policy
is intended to reduce our load on the Codeberg CI servers.

### Running CI pipelines locally

Our CI is built atop of [Woodpecker CI](https://woodpecker-ci.org/docs), which
has a handy feature where you can run pipelines locally. However, this requires
some degree of setup.

All operating systems will need to install
[woodpecker-cli](https://woodpecker-ci.org/docs/cli) to run CI locally. However,
`woodpecker-cli` requires some container runtime to function. We've outlined
some options below along with steps required to make them work with
`woodpecker-cli`.

#### Running CI locally on Linux

The easiest option is to install `docker`. You can then run
a CI pipeline via `woodpecker-cli` like so:

```sh
woodpecker-cli exec --backend-engine=docker
```

It may also be possible to use an alternative container runtime like
`containerd`, but we have not tested this. If you manage to get this working,
please update the docs!

#### Running CI locally on MacOS

There are two main options for container runtimes on MacOS:

- [Colima](https://colima.run/)
- [Docker Desktop](https://www.docker.com/products/docker-desktop/)

To use `colima`, you will first need to run `colima start --memory=10.0`.
You can then run a CI pipeline via `woodpecker-cli` like so:

```sh
woodpecker-cli exec --backend-engine=docker --backend-docker-host="unix://${HOME}/.colima/default/docker.sock"
```

If you use [direnv](https://direnv.net/), we recommend adding the following to
your `.envrc` to avoid having to repeatedly type those long options:

```sh
export WOODPECKER_BACKEND=docker
export WOODPECKER_BACKEND_DOCKER_HOST="unix://${HOME}/.colima/default/docker.sock"
```

None of our developers have tested `woodpecker-cli` with docker desktop: if you
manage to get it working, please update the docs!

#### Running CI locally on Windows

It /may/ be possible to run the CI locally on Windows, but we have not tested
this. If you manage to get this working, please update the docs!

### Editing CI pipelines

As noted above, our CI uses [Woodpecker CI](https://woodpecker-ci.org/docs).
Before pushing a change to CI, please run it locally if possible. If this
is not possible, please run `woodpecker-cli lint` to check for syntax errors.

Some Mikan Hacking Lore
======================

* Whenever you change the interface file format you should update
  `Mikan.TypeChecking.Serialise.currentInterfaceVersion`.

* Whenever you change `agda.sty`, update the date in `\ProvidesPackage`.

* Use `__IMPOSSIBLE__` instead of calls to error. `__IMPOSSIBLE__`
  generates errors of the following form:

      An internal error has occurred. Please report this as a bug.
      Location of the error: ...

  Calls to error can make Mikan fail with an error message in the
  `*ghci*` buffer.

  To include a function in the trace printed by `__IMPOSSIBLE__`
  add a `HasCallStack` constraint (from `Mikan.Utils.CallStack`).

* GHC warnings are turned on globally in `Mikan.cabal`. If you want to
  turn on or off an individual warning in a specific file, use an
  `OPTIONS_GHC` pragma. Don't use `-Wall`, because the meaning of this
  flag can vary between different versions of GHC.

* The GHC documentation contains the following information
  about orphan instances:

  > GHC identifies orphan modules, and visits the interface file of
  every orphan module below the module being compiled. This is usually
  wasted work, but there is no avoiding it. You should therefore do
  your best to have as few orphan modules as possible.

  See:
  https://downloads.haskell.org/ghc/latest/docs/users_guide/separate_compilation.html#orphan-modules
  .

  In order to avoid *unnecessary* orphan instances the flag
  `-fwarn-orphans` is turned on. If you feel that you really want to use
  an orphan instance, place
  ```haskell
      {-# OPTIONS_GHC -Wno-orphans #-}
  ```
  at the top of the module containing the instance.

Emacs mode
==========

* If you fix a bug related to syntax highlighting, please add a test
  case under `test/interaction`. Example `.in` file command:

      IOTCM "Foo.agda" NonInteractive Direct (Cmd_load "Foo.agda" [])

  If you want to include interactive highlighting directives, replace
  `NonInteractive` with `Interactive`.

* The following Elisp code by Nils Anders Danielsson fixes whitespace
  issues upon save.  Add to your `.emacs`.
  ```elisp
      (defvar fix-whitespace-modes
        '(text-mode agda2-mode haskell-mode emacs-lisp-mode LaTeX-mode TeX-mode)
        "*Whitespace issues should be fixed when these modes are used.")

      (add-hook 'before-save-hook
        (lambda nil
          (when (and (member major-mode fix-whitespace-modes)
                     (not buffer-read-only))
            ;; Delete trailing whitespace.
            (delete-trailing-whitespace)
            ;; Insert a final newline character, if necessary.
            (save-excursion
              (save-restriction
                (widen)
                (unless (equal ?\n (char-before (point-max)))
                  (goto-char (point-max))
                  (insert "\n")))))))
  ```

Bisecting: Finding the commit that introduced a regression
==========================================================

If you want to find the commit that introduced a regression that
caused Module-that-should-be-accepted to be rejected, then you can try
the following recipe:
  ```sh
    git clone <mikan repository> mikan-bug
    cd mikan-bug
    git switch <suitable branch>
    git bisect start <bad commit> <good commit>
    cp <some path>/Module-that-should-be-accepted.agda .
    git bisect run sh -c \
      "cabal build exe:Mikan || exit 125; \
       cabal run exe:Mikan -- \
         --ignore-interfaces \
         Module-that-should-be-accepted.agda"
  ```

Special care is required for bisects that span the fork off of agda, as
the binary name changed from `agda` to `mikan`. This happened in commit
`55c3be6c25`.

A better alternative is to use the program mikan-bisect from
`src/mikan-bisect`, which is able to handle the fork.

  ```sh
    git clone <mikan repository> mikan-bug
    cd mikan-bug
    cp <some path>/Module-that-should-be-accepted.agda .
    mikan-bisect --bad <bad commit> --good <good commit> \
      Module-that-should-be-accepted.agda
  ```

You can compile `mikan-bisect` by using `cabal build mikan-bisect`, or run
it directly via `cabal run mikan-bisect`. For a full listing of options, see
`mikan-bisect --help`.

### Bash completion for `mikan-bisect`

The following command temporarily enables Bash completion for
`mikan-bisect`:
  ```sh
    source < (mikan-bisect --bash-completion-script `command -v mikan-bisect`)
  ```
Bash completion can perhaps be enabled more permanently by storing the
output of the command above in a file in a suitable directory (like
`/etc/bash_completion.d/`).


Documentation
=============

See http://agda.readthedocs.io/en/latest/contribute/documentation.html .

How To…
=======

Add a primitive function
------------------------

**Type checking**
1. Add your primitive to `Mikan.TypeChecking.Primitive.primitiveFunctions`.
2. If your primitive operates solely on literals, add your primitive to
   `Mikan.TypeChecking.Reduce.Fast` as well.
   (Check `Mikan.Syntax.Concrete.Literal` to find out.)
3. If your primitive operates on reflected syntax, add your primitive to
   `Mikan.TypeChecking.Unquote.evalTCM` as well.

**Builtin modules**
1. Add your primitive to the relevant `Agda.Builtin` module, in a `primitive` block.

**Haskell backend**
1. Add your primitive to `Mikan.Compiler.MAlonzo.Primitives.primBody`.
   Make sure to add any relevant imports to `importsForPrim`, and to
   add any relevant functions to `MAlonzo.RTE`.

**JavaScript backend**
1. Add your primitive to `Mikan.Compiler.JS.Compiler.primitives`.
2. Provide an implementation of your primitive:
   - If your implementation uses only types which are available in vanilla
     JavaScript, you can put your implementation in `src/data/JS/agda-rts.js`;
   - If your implementation needs types defined in the `Agda.Builtin` modules,
     you must put your implementation in a `{-# COMPILE JS … #-}` pragma, in the
     relevant builtin module (see, e.g., `Agda.Builtin.String.primStringUncons`.

**Housekeeping**
1. Describe your changes in `CHANGELOG.md`.
2. Describe your new primitive in `doc/user-manual`.
