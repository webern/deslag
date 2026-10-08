//! `deslag-gold silver`: the silver set, labels that models made, assembled into a batch that the
//! training reader may read and CI can check without any model.
//!
//! A batch is `silver/NAME/` in the big tier's image. It carries everything it is checked
//! against, in `record/`, so a batch that passes [`check`] once passes for as long as the image
//! holds it: the checkout's `voters.json`, the gold set and the corpus may all change after it.
//! The two rules that never lapse are the ones [`standing`] holds a live batch to against
//! today's checkout.
//!
//! - [`live`]: which batches are live, and the repositories and texts they hold, which `rank`,
//!   `queue` and `draw` leave out.
//! - [`part`]: one labelled part and the preflight that runs at each gate.
//! - [`build`]: the assembler.
//! - [`check`]: the rules a batch is checked by, frozen once a batch is live.
//! - [`standing`]: a live batch against today's gold and corpus.
//! - [`datasheet`]: the numbers of a batch and the sheet rendered from them.

pub mod build;
pub mod check;
pub mod datasheet;
pub mod kit;
pub mod layout;
pub mod live;
pub mod part;
pub mod runs;
pub mod score;
pub mod standing;
pub mod table;
