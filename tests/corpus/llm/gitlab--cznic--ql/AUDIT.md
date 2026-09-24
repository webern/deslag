# Correctness audit — modernc.org/ql

**Date:** 2026-07-06
**Scope:** the whole engine (root package `ql`), the `database/sql` driver, and the
introspection / httpfs / CLI helpers.
**Method:** read-only review of every subsystem, cross-checked against the language
spec in `doc.go`, followed by reproduction of each candidate defect against the public
API on all three storage backends (in-RAM, V1 file, V2 file). No engine files were
changed during the audit.

Every defect listed below was **reproduced**, either directly against `modernc.org/ql`
(`OpenMem` / `OpenFile` with `FileFormat: 0` and `2`) or through `database/sql`. Findings
that are code-evident but need an injected I/O fault to trigger are marked *(code trace)*.

The baseline suite (`go test -vet=off ./...`) and a `-race` run both pass. These bugs are
invisible to it because `testdata.ql` runs the entire case battery inside **one outer
transaction**, so top-level `ROLLBACK`, single-statement automatic transactions, and
several backend-specific paths are never exercised.

## Running the reproductions

The reproductions live in `audit_repro_test.go`, gated behind a build tag so they never
affect the normal build, `go test`, or CI:

```sh
go test -vet=off -tags auditrepro -run TestAudit -v
```

Each `TestAudit_*` is a **red regression test**: it asserts the *correct* behavior and
therefore fails while the bug is present. A test turns green when its bug is fixed. The
IDs below (A1, A2, …) match the test names (`TestAudit_A1_…`).

## Resolution (2026-07-14)

Every finding was worked through individually; each fix was verified against its
reproduction and the full four-backend `go test -vet=off ./...` suite, then
committed on its own. All findings are fixed except for the judgment calls noted
here:

- **A3 — statement atomicity:** fixed with a per-statement savepoint, and
  **on by default everywhere as of 2026-07-28**. It was initially opt-in on the
  file backends via `Options.StatementAtomicity` (default off), on the grounds
  that it roughly doubles their write cost. That estimate was never measured and
  was too high: `BenchmarkStatementAtomicity` puts the overhead at +4–14% on V1
  and +28–67% on V2 (`PERFORMANCE.md` P7). For a Tier-1 durable-corruption
  finding that is worth paying, so the default was flipped;
  `Options.NoStatementAtomicity` opts out and the old field is deprecated and
  ignored. `TestStatementAtomicity` pins both the default and the opt-out.
- **A16 — constant contexts:** fully fixed. Both the `x == true` / `x != false`
  type check and the constant-conversion range check: `int8(200)` (etc.) now
  errors, per doc.go, Go and the INSERT path. `testdata.ql` cases 88/96/98/102
  were changed from the (spec-violating) truncated results to the overflow error.
- **A48 — time zone name on the file backends:** fixed. V1 stays
  format-compatible; V2 uses a new `tag2timeNamed` tag, so V2 files written after
  this change are not readable by ql releases predating it (old files still
  read).
- **A33 — driver `Rows.Close` deadlock:** initially left as a documented known
  limitation; **fixed on 2026-07-28** (see below).

The gated reproduction suite (`go test -vet=off -tags auditrepro -run TestAudit`)
passes for every finding.

## Resolution addendum (2026-07-28) — A33

A33 was reopened and fixed. `driverRows.Close` no longer waits for the row-
streaming goroutine, so it never blocks.

The 2026-07-14 decision to keep the wait assumed that dropping it would
reintroduce the nil-store panic that commit 1026b9c had fixed. It does not: that
commit predates the **A31** fix, which moved the same protection into the engine,
where it belongs. `DB.Close` now takes the write lock before nil-ing
`store`/`root`, and every read path re-checks for a closed DB after acquiring the
read lock, so a straggling goroutine either completes before the teardown or
fails cleanly with `errClosedDB`. The driver-level wait had become redundant —
and it was the sole reason `Close` could block.

A33 also turned out to be deterministically reproducible, contrary to the note
above: under `GOMAXPROCS(1)` the goroutine spawned by `Query` stays runnable but
unscheduled until the caller blocks, so a write transaction opened on another
connection always takes the exclusive lock first and parks it. The reproduction
is now `TestAudit_A33_RowsCloseDeadlock` (20/20 before the fix, 0/20 after).

**id() note (not an audit finding):** a follow-up investigation found that
CREATE INDEX (and other DDL that writes the `__Index2` / `__Column2` system
tables) consumes values from the global, DB-wide id() counter, so user rows
inserted after such DDL get offset id() values (e.g. 11,12,13 rather than
1,2,3). This is spec-compliant — doc.go defines id() as "table-unique" and "not
reused unless the DB becomes completely empty", i.e. a global counter that
permits gaps — so it is a surprising quirk, not a bug, and is left as-is;
isolating system-table ids would need a persistent on-disk format change.

---

## Tier 1 — Durable data corruption / loss

### A1. V2 backend: ROLLBACK is not honored; rolled-back writes are committed later
`file2.go` — `storage2.Rollback` → `pop()` never calls the WAL's `Rollback()`, so a
rolled-back top-level transaction's journal pages stay mapped and are flushed by the next
commit.
Repro: `INSERT (10),(20); COMMIT;` → `BEGIN; UPDATE i=999 WHERE i==10; ROLLBACK;` → any
unrelated `BEGIN; …; COMMIT;` leaves the table as `[999,20]`, and it **persists across
reopen**. Silent, durable corruption on the `"ql2"` driver.

### A2. V1 backend: UPDATE corrupts the index of an untouched indexed blob column
`file.go` `UpdateRow` frees the chunk chains of *all* blob columns
(`//LATER detect which blobs are actually affected`) and reallocates them, but
`updateStmt` (`stmt.go:280-310`) refreshes only `touched` indexes.
Repro: blob columns `a` (indexed) and `b`; `UPDATE a=…`, then `UPDATE i=…` (a,b untouched)
→ `SELECT i FROM t WHERE a == <value>` returns 0 rows. Committed and permanent; can also
surface as a decode panic in `file.collate`.

### A3. Statement-level operations are not atomic
`updateStmt` / `insertIntoStmt` (`stmt.go:249-320`, `961-1003`) mutate rows and indexes in
place with no per-statement rollback; `Execute` only rolls back whole nesting levels.
Repro: unique index on `a`; `UPDATE t SET a = 4-a` on `(1),(3)` fails midway and commits
a table with duplicate `1`s while the index shows a single `1`. Multi-row INSERT and
`INSERT…SELECT` leave phantom index entries the same way. Amplified by the driver, which
runs BEGIN / each Exec / COMMIT as separate `Execute` calls, so the partial work survives
into the still-open transaction.

### A4. INSERT INTO … SELECT with a blob-like DEFAULT writes unreadable garbage (file backends)
`execSelect` reuses one `data0` buffer across rows and `file.Create` flattens it in place
(`stmt.go:904-1003`, `file.go:1131`), so from row 2 on an unlisted `time`/`blob`/`bigint`
DEFAULT column stores previously-encoded bytes.
Repro: `CREATE TABLE t (a int64, ts time DEFAULT now()); INSERT INTO t (a) SELECT a FROM
src;` succeeds, then `SELECT a, ts` fails permanently with
`cannot convert [169 39 …] (type []uint8) to type time`.

### A5. DROP TABLE leaves __Column2 rows behind
`dropTableStmt` (`stmt.go:549`) deletes `__Index2` rows but not `__Column2`, so a later
same-name `CREATE TABLE` silently inherits the dropped table's constraints and defaults.
Repro: create `t (a int a>0 DEFAULT 99)`, drop it, `CREATE TABLE t (a int)` →
`INSERT VALUES(-5)` is rejected and `INSERT VALUES(NULL)` silently stores 99.

### A6. ALTER TABLE DROP/ADD COLUMN misaligns t.constraints
`constraintsAndDefaults` rebuilds against the stale pre-ALTER `t.cols`
(`storage.go:143-204`, `stmt.go:626,713`).
Repro: `t2 (a int, b int NOT NULL, c string)`, `ALTER … DROP COLUMN a` → **NULL is
accepted into the NOT NULL column b** while a valid insert into nullable `c` is rejected.
`ALTER … ADD d int NOT NULL` likewise fails to enforce the new constraint. This is also
the root cause of the driver's wrong `ColumnType.Nullable()` after ALTER.

### A7. DROP TABLE + ROLLBACK produces duplicate id() values
`store.ResetID()` (`storage.go:999`) is not undo-logged in any backend.
Repro: table with ids 1,2; `BEGIN; DROP TABLE t; ROLLBACK; INSERT (30)` → two rows with
`id()==1`. Breaks every id()-keyed lookup, index, and join. (mem + V1.)

### A8. mem backend: nested COMMIT drops the child undo log
`mem.Commit` does `s.rollback = s.rollback.parent` (`mem.go:568`), discarding the
committed level's undo; V1/V2 merge it into the parent.
Repro: `BEGIN; BEGIN; INSERT(42); COMMIT; ROLLBACK;` → table scan is empty (correct) but
`WHERE i==42` via index returns `[42]` — a phantom row plus a storage leak.

---

## Tier 2 — Silent wrong query results

### A9. Open-open range scan returns rows above the upper bound when it is absent from the index
`plan.go` `doInterval00:195` stops on `collate1(val, hval) == 0` instead of `>= 0`.
Repro: indexed values `1,3,5,6`; `WHERE i > 1 && i < 4` → `3,5,6` instead of `3`. Only on
indexed columns (index-vs-scan divergence); `EXPLAIN` still prints the correct predicate.

### A10. WHERE id() >= k returns all rows for every k ≥ 1
Inverted condition in `whereRset.planBinOp` (`ql.go:372`, `rv >= 1` should be `rv <= 1`);
the predicate is dropped from the plan.
Repro: 4 rows, `WHERE id() >= 4` → 4 rows instead of 1.

### A11. SELECT * cross / LEFT / FULL join emits duplicate rows
The join builders do `append(prefix, in…)` reusing one backing array; retained rows alias
the last inner row when capacity slack exists.
Repro: a 17-column table × a 3-row table → three identical rows. `rightJoin` and explicit
column lists are safe.

### A12. Blob `>` computes equality, not ordering
`expr.go:786` returns `bytes.Equal` for `'>'` while `<,<=,>=` use `bytes.Compare`.
`WHERE v > blob("b")` returns the *equal* row on a scan and the *correct* row via index.

### A13. Ordering operators on a bool column diverge by plan
Scan path errors (`operator < not defined on bool`); index path executes and returns rows
(`plan.go` `doFalse`/`doTrue` + `filterIdent`). Adding an index turns an error into a
silent result.

### A14. Constant-NULL folding breaks three-valued logic for &&/||
`newBinaryOperation0` (`expr.go:392-419`) folds a static-NULL operand to NULL for all
operators. `SELECT NULL || true` → NULL (should be true); `SELECT false && NULL` → NULL
(should be false). In a WHERE clause this silently drops rows.

### A15. Empty GROUP BY fabricates a bogus all-zero row
`ident.eval` under `$agg0` returns `int64(0)` and `selectFieldsGroupPlan.do` runs it even
with no groups (`expr.go:3107`, `plan.go:2075`).
Repro: empty table, `SELECT c FROM t GROUP BY c` → `[0]` (also the wrong type).

### A16. Type checks skipped in several constant contexts
`x == true` / `x != false` bypasses type checking (`SELECT true == 1` → `1`); constant
conversions do not range-check (`int8(200)` → `-56`, `uint8(-1)` → `255`) though the
INSERT path rejects the same values.

### A17. MaxInt64 literal is typed unsigned; unary minus on it wraps
`lexer.go:115` uses `n < math.MaxInt64` (off by one); unary `-` on idealUint wraps mod 2⁶⁴
(`expr.go:3588`). `SELECT bigint(-9223372036854775807)` → `9223372036854775809`; the
MinInt64 literal is unusable in an int64 column.

### A18. Rune literals yield the first UTF-8 byte, not the code point
`scanner.l:175` takes `string[0]`. `int32('ä')` → 195 (doc says 228), `int32('π')` → 207
(should be 960), `'ä'` → 195.

### A19. IN / NOT IN mishandle a NULL element
`expr.go` `pIn.eval`. `SELECT 1 NOT IN (2, NULL)` → `true` and `SELECT 1 IN (2, NULL)` →
`false`; the spec (`e==a || e==b`, incl. NULL handling) makes both NULL.

### A20. Negative shift count silently yields 0
`expr.go` rsh/lsh — `cnt = uint64(y)` wraps `-1` to a huge count. `SELECT 1 << -1` → `0`
instead of an error.

### A21. avg accumulates the sum in the column's narrow type
`builtin.go` — `avg` of `int8` values `100,100` → `-28` (int8 sum wraps) though the true
average `100` is representable.

### A22. Multi-row INSERT with a column subset + expression DEFAULT reuses the row buffer
`stmt.go:1039`. `CREATE TABLE t (a int, b int DEFAULT a+100); INSERT INTO t (a) VALUES
(1),(2),(3)` stores `b = 101,101,101` instead of `101,102,103`. Same for `now()` defaults.

### A23. A reused Recordset returns stale data and can panic
`beginTransaction` clones table structs (`ql.go:1488`), so a recordset from an earlier
`Run` keeps the old `*table`. After an `INSERT` the recordset still returns the old rows;
after a `DELETE` `rs.Rows` **panics** (`interface conversion: nil, not int64`).
Contradicts the documented "every invocation of Do will see the current data".

### A51. FULL OUTER JOIN drops every right row when the left side is empty
`fullJoinDefaultPlan.do` (`plan.go`) visits the right side only from inside the
left side's callback, and records unmatched right rows there. An empty left side
therefore never visits the right side at all, leaves the bookkeeping empty, and
returns nothing — where a FULL OUTER JOIN must return every right row, padded,
all of them being unmatched by definition.

Repro: `SELECT * FROM e FULL OUTER JOIN u ON e.k == u.k` with `e` empty and `u`
holding rows returns 0 rows instead of `len(u)`. Silent wrong results on all
backends. An empty *right* side is handled correctly, as is RIGHT OUTER JOIN,
which reverses its inputs and so puts the non-empty side outermost.

*Found 2026-07-29 while merge-joining the outer forms: the merge implementation
returned the right rows and the differential test flagged the disagreement.*
Fixed for both plans. `TestFullJoinEmptyLeft` guards it and uses a non-equi ON
clause so it exercises the nested loop, which remains the plan for every shape
the merge join does not take.

---

## Tier 3 — Panics on ordinary input (process crash; no recover on the exec path)

### A24. id() range with an id() index → nil-pointer dereference
`SELECT … WHERE id() > 1 && id() < 3` when an `id()` index exists (`plan.go:1294` →
`col.clone` on nil).

### A25. Slice with low > high → panic escapes
`SELECT s[2:1] FROM t` → `slice bounds out of range` out of `Rows` (`expr.go`/`etc.go`).

### A26. ORDER BY / DISTINCT over cross-typed values → panic
`SELECT * FROM t ORDER BY coalesce(intCol, strCol)` → `internal error 024` in `collate1`.

### A27. Aggregates over cross-typed values → panic
`SELECT min(coalesce(intCol, strCol)) FROM t` → `interface conversion` panic (`builtin.go`).

### A28. id() of a resolvable expression in a join → panic
`SELECT id(ta.i) FROM ta, tb` → `interface conversion` panic (`builtin.go` `builtinID`).

### A29. INSERT INTO narrow SELECT wide → panic
`INSERT INTO t SELECT * FROM u` with `u` wider than `t` → `index out of range`
(`stmt.go:915`, missing column-count check).

### A30. V1 INSERT … IF NOT EXISTS on a non-empty unique blob index → panic + DB wedge
`file.go` `fileIndex.Exists` builds the probe key without flattening; the decode panics
(`DecodeScalars: corrupted data`) **while holding the write lock**, so the next statement
deadlocks.

---

## Tier 4 — Concurrency / documented-contract violations

### A31. DB.Close() crashes a concurrent reader and returns nil
`Close` nils `store`/`root` under only `db.mu`, not `rwmu` (`ql.go:1442`); a reader
documented as concurrency-safe panics on the nil store — every run.

### A32. tx.Query of >500 rows then tx.Exec on the same tx deadlocks
`db.do` holds `db.mu` for the whole iteration (`ql.go:1466`) while the driver's 500-slot
streaming goroutine blocks.

### A33. Driver Rows.Close can deadlock (introduced by commit 1026b9c)
`Close` waits for the streaming goroutine, which may be parked in `rwmu.RLock` behind
another connection's open transaction — typically held by the very caller that is now
blocked in `Close`. *(deterministic under `GOMAXPROCS(1)`; see the 2026-07-28 addendum)*

### A34. ALTER TABLE … ADD shares the statement's *col across executions of one List
`stmt.go:713` appends `s.c` without cloning. Running the same compiled ALTER on two DBs
with different column counts corrupts `col.index` → later INSERT panics; also a data race.

### A35. Execute(nil, …) concurrent with another context's transaction panics
The rollback branch registers `defer func(){ pc.LastInsertID = … }()` before checking
`pc != nil` (`ql.go:1387`). *(timing-dependent; not in the deterministic repro suite)*

### A36. pLike caches its compiled regexp on the shared expression node
`expr.go:294` — a data race when one compiled statement runs from multiple goroutines
(idempotent values, low practical harm). *(code trace)*

### A49. V2: concurrent reads race on the storage's shared scratch buffers
`dbStorage` (`file2.go`) kept four fixed scratch buffers as struct fields — for a
record, a B-tree key, a value and a varint — and reused them for every call.
`DB.rwmu` is a read/write lock, so QL runs any number of readers at once: two
concurrent `SELECT`s had one copying a record into the shared buffer via
`internal/file.(*file).ReadAt` while the other decoded that same memory in
`decode2` → `binary.Uvarint`.

Repro: four connections, four goroutines, read-only `SELECT`s, every result set
fully drained — no writes, no prepared statements, no undrained `Close`. Races on
`ql2` under `-race`; `ql-mem` and `ql` are unaffected. A torn decode is not merely
a detector warning, it is a silently wrong row.

*Found 2026-07-28 while benchmarking the driver, not by the original audit.*

### A50. BEGIN TRANSACTION enters the storage before taking the write lock
`run1` (`ql.go`) called `db.store.BeginTransaction()` and only then
`db.rwmu.Lock()`. Beginning a transaction mutates storage-level state — the V2
backend swaps the `File` readers read through, the V1 one installs an
in-transaction page cache — and readers hold only `rwmu.RLock`, not `db.mu`,
while they iterate, so `db.mu` gave them no protection. Every read in flight
raced with the start of a write transaction.

The already-in-a-transaction branch a few lines below has always taken the lock
in the correct order, so this was an inconsistency within one function.

Repro: readers querying while other connections open write transactions. Races on
both file backends under `-race`.

*Found 2026-07-28, same occasion as A49.*

---

## Tier 5 — database/sql driver

### A37. Prepared statements bind named parameters by position, not name
`driver1.8.go:140-154`. `db.Prepare("… $b, $a"); stmt.Query(Named("a",1), Named("b",2))`
binds them swapped — silent wrong data. The non-prepared path binds correctly.

### A38. A repeated named parameter breaks argument counting
`filterNamedArgs` renumbers every occurrence, so `"… $x + $x"` with one `Named("x", …)`
fails with `expected 2 arguments, got 1`.

### A39. Open rejects names starting with "file" or "memory"
`driver.go:186-212`. `sql.Open("ql", "files.db")` → `unexpected/unsupported scheme`.

### A40. Context cancellation is ignored
`QueryContext` / `ExecContext` / `BeginTx` never consult `ctx` (`driver1.8.go`); a blocked
call cannot be timed out and the connection is lost to the pool. *(code trace)*

### A41. A failed COMMIT/ROLLBACK desyncs driverConn and poisons the pooled connection
`driver.go:333-363` returns early on a storage-level commit error without clearing
`c.ctx` / `c.tnl`. *(code trace; needs a storage failure)*

---

## Tier 6 — Metadata / API correctness

### A42. DB.Execute never sets its documented `index` return
Always 0, even when a later statement in the list fails.

### A43. Internal system-table DML pollutes TCtx.LastInsertID / RowsAffected
The driver surfaces these as the `sql.Result` of a pure-DDL statement.

### A44. CREATE INDEX is permanently broken after a rolled-back CREATE INDEX
`db.hasIndex2` is set to 2 when `__Index2` is auto-created but ROLLBACK does not restore
the flag (`ql.go:966`), so every later CREATE INDEX fails until the DB is reopened.

### A45. Unmarshal can never populate a big.Int / big.Rat field
`introspection.go:246-260,604` — the schema type is set but never `check`ed, so
marshalling / unmarshalling a `big.Int`/`big.Rat` field fails.

### A46. Schema interpolates names into DDL unquoted
`introspection.go:387,399` — a `name` struct tag with spaces breaks compilation, and a
crafted tag injects arbitrary DDL.

### A47. HTTPFile.Seek rejects negative offsets and mis-implements SeekEnd
`httpfs.go:131-178` — violates `io.Seeker`; `http.FileServer` survives only because it
uses `Seek(0, SeekEnd)` and `SeekStart` exclusively. *(not in the repro suite)*

### A48. time.Time zone name is lost on file backends
`blob.go` / `encode2.go` — the offset is preserved and the instant compares equal, but the
location name differs from what was stored and from the mem backend.

---

## Checked and found solid

Scalar round-trip fidelity for every type including min/max/NaN/±Inf/large big values; V1
blob chunking across the 64 KB boundary with shrink/grow; V1 WAL durability (correct lldb
usage, no torn-write window); format sniffing (neither driver overwrites the other's
format); LIMIT / OFFSET / DESC semantics; aggregate empty-set and NULL semantics;
unique-index NULL handling; single-operator index scans and AND-interval *planning* (only
the open-open *executor*, A9, is wrong); runtime three-valued logic; division-by-zero
handling; non-transaction reader isolation (serialized, as documented); httpfs `..`
traversal protection; CLI transaction wrapping.

**Concurrency pass (2026-07-28).** A49 and A50 were both found by accident, while
benchmarking, which said little for how well this area was covered. A focused
pass followed, driving each backend through `database/sql` from several
goroutines at once under `-race` and `-cpu 1,4,24`: every read plan shape
(index point and range lookups, scan, GROUP BY, DISTINCT, ORDER BY with LIMIT,
count, merge join), a prepared statement shared between goroutines, autocommit
writes, explicit transactions both committed and rolled back, result sets
abandoned undrained, and index and table DDL running against a table while it is
being read. Nothing further turned up. The tests are
`driver/{concurrent,compiled,stress}_test.go` and they run in the normal suite;
they are only meaningful under `-race`.

Two properties are worth carrying forward. The `mem` backend has its own
`sync.RWMutex` and was clean through both bugs; the file backends have no
equivalent and rely entirely on the engine's `DB.rwmu`, which is a *read/write*
lock — so any storage-level state a file backend mutates while serving a read is
unprotected by construction. That is the shape of A49 and the first place to look
for more. And a store-mutating operation must take `rwmu.Lock` before entering
the storage, not after; that is A50.

**Correction (2026-07-28).** "Non-transaction reader isolation" above was checked
as an *isolation* property — what a reader is allowed to observe — and that
conclusion stands. It was not a check of the storage layer's thread-safety, and
reading it as one would be wrong: `DB.rwmu` is a read/write lock, so readers do
run concurrently, and two of the paths they take were not safe for that. See A49
and A50, both found afterwards and both capable of returning silently wrong rows.
A concurrency claim in this document should be read as covering only what it
literally says.

## Priority

The most urgent are **A1** (V2 rollback resurrection), **A2 / A3** (blob-index corruption
and non-atomic statements), and **A6** (NOT NULL bypass via ALTER) — these silently corrupt
or lose committed data.
