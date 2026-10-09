# Parsing

- The reader works in steps.

  ```rust
  // The reader takes one record at a time from the input and checks its length against the
  // header before it reads the body, so that a record cut short is reported where it ends and
  // not where the next one begins. The reader takes one record at a time from the input and
  // checks its length against the header before it reads the body, so that a record cut short
  // is reported where it ends and not where the next one begins.
  fn next() {}

  // The reader takes one record at a time from the input and checks its length against the
  // header before it reads the body, so that a record cut short is reported where it ends and
  // not where the next one begins. The reader takes one record at a time from the input and
  // checks its length against the header before it reads the body, so that a record cut short
  // is reported where it ends and not where the next one begins. The reader takes one record
  // at a time from the input and checks its length against the header before it reads the
  // body, so that a record cut short is reported where it ends and not where the next one
  // begins.
  fn last() {}
  ```
