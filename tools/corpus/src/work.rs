//! Work split over the machine's threads, with results in a fixed order so that every run prints
//! the same thing.

use std::sync::atomic::{AtomicUsize, Ordering};

/// `run` over each chunk of `items`, `size` at a time, on as many threads as the machine has; the
/// results come back in the order of the chunks, whichever thread ran each.
pub fn in_chunks<T: Sync, R: Send>(
    items: &[T],
    size: usize,
    run: impl Fn(&[T]) -> R + Sync,
) -> Vec<R> {
    let chunks: Vec<&[T]> = items.chunks(size.max(1)).collect();
    let next = AtomicUsize::new(0);
    let threads = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(chunks.len())
        .max(1);
    let mut done: Vec<(usize, R)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let at = next.fetch_add(1, Ordering::Relaxed);
                        let Some(chunk) = chunks.get(at) else {
                            return done;
                        };
                        done.push((at, run(chunk)));
                    }
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("a worker panicked"))
            .collect()
    });
    done.sort_by_key(|(at, _)| *at);
    done.into_iter().map(|(_, result)| result).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_come_back_in_the_order_of_the_chunks() {
        let items: Vec<u32> = (0..1000).collect();
        let sums = in_chunks(&items, 7, |chunk| chunk.iter().sum::<u32>());
        let expected: Vec<u32> = items.chunks(7).map(|chunk| chunk.iter().sum()).collect();
        assert_eq!(sums, expected);
        assert!(in_chunks(&[] as &[u32], 7, |chunk| chunk.len()).is_empty());
    }
}
