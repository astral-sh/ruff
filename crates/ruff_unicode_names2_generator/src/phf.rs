//! Computes a perfect hash table using [the CHD
//! algorithm](http://cmph.sourceforge.net/papers/esa09.pdf).
//!
//! Strongly inspired by https://github.com/sfackler/rust-phf

use rand::prelude::{Rng, SeedableRng, SliceRandom, StdRng};
use std::iter::repeat;

static NOVAL: char = '\0';

/// FNV
fn hash(s: &str, h: u64) -> u64 {
    let mut g = 0xcbf29ce484222325 ^ h;
    for b in s.bytes() {
        g ^= b as u64;
        g = g.wrapping_mul(0x100000001b3);
    }
    g
}

pub fn displace(f1: u32, f2: u32, d1: u32, d2: u32) -> u32 {
    d2.wrapping_add(f1.wrapping_mul(d1)).wrapping_add(f2)
}
fn split(hash: u64) -> Hash {
    let bits = 21;
    let mask = (1 << bits) - 1;
    Hash {
        g: (hash & mask) as u32,
        f1: ((hash >> bits) & mask) as u32,
        f2: ((hash >> (2 * bits)) & mask) as u32,
    }
}

#[derive(Copy, Clone)]
struct Hash {
    g: u32,
    f1: u32,
    f2: u32,
}

#[allow(clippy::type_complexity)]
fn try_phf_table(
    values: &[(char, String)],
    lambda: usize,
    seed: u64,
    rng: &mut StdRng,
) -> Option<(Vec<(u32, u32)>, Vec<char>)> {
    let hashes: Vec<_> = values
        .iter()
        .map(|(n, s)| (split(hash(s, seed)), *n))
        .collect();

    let table_len = hashes.len();
    let buckets_len = (table_len + lambda - 1) / lambda;

    // group the elements into buckets of lambda (on average, for a
    // good hash) based on the suffix of their hash.
    let mut buckets = (0..buckets_len).map(|i| (i, vec![])).collect::<Vec<_>>();
    for &(h, cp) in hashes.iter() {
        buckets[h.g as usize % buckets_len].1.push((h, cp))
    }

    // place the large buckets first.
    buckets.sort_by_key(|(_index, keys)| std::cmp::Reverse(keys.len()));

    // this stores the final computed backing vector, i.e. getting the
    // value for `foo` is "just" `map[displace(hash(foo))]`, where
    // `displace` uses the pair of displacements that we computed
    // (stored in `disps`).
    let mut map = repeat(NOVAL).take(table_len).collect::<Vec<_>>();
    let mut disps = repeat((0, 0)).take(buckets_len).collect::<Vec<_>>();

    // the set of index -> value mappings for the next bucket to be
    // placed; we need it separate because it may not work, so we may
    // have to roll back.
    //
    // it works by storing a map from index -> generation, so we can
    // check if the index is taken by a previously-placed element of
    // the current bucket cheaply (just an array lookup) without
    // having to clear the whole the whole array each time (just
    // compare against the generation). A u64 won't overflow.
    let mut generation = 0;
    let mut try_map = repeat(0u64).take(table_len).collect::<Vec<_>>();
    // the placed (index, codepoint) pairs of the current bucket, to
    // be placed into the main map if the whole bucket fits.
    let mut values_to_add = vec![];

    // heuristically avoiding doing everything in the same order seems
    // good? I dunno; but anyway, we get vectors of indices and
    // shuffle them.
    let mut d1s = (0..(table_len as u32)).collect::<Vec<_>>();
    let mut d2s = d1s.clone();
    d1s.shuffle(rng);
    d2s.shuffle(rng);

    // run through each bucket and try to fit the elements into the
    // array by choosing appropriate adjusting factors
    // ("displacements") that allow the other two parts of the hash to
    // be combined into an empty index.
    'next_bucket: for &(bkt_idx, ref bkt_keys) in buckets.iter() {
        // exhaustively search for a pair of displacements that work.
        for &d1 in d1s.iter() {
            'next_disp: for &d2 in d2s.iter() {
                generation += 1;
                values_to_add.clear();

                // run through the elements to see if they all fit
                for &(h, cp) in bkt_keys.iter() {
                    // adjust the index slightly using the
                    // displacements, hoping that this will allow us
                    // to avoid collisions.
                    let idx = (displace(h.f1, h.f2, d1, d2) % table_len as u32) as usize;

                    if map[idx] != NOVAL || try_map[idx] == generation {
                        // nope, this one is taken, so this pair of
                        // displacements doesn't work.
                        continue 'next_disp;
                    }
                    try_map[idx] = generation;
                    values_to_add.push((idx, cp));
                }

                // everything works! let's lock it in and go to the
                // next bucket.
                disps[bkt_idx] = (d1, d2);
                for &(idx, cp) in values_to_add.iter() {
                    map[idx] = cp
                }
                continue 'next_bucket;
            }
        }

        // if we're here, we ran through all displacements for a
        // bucket and didn't find one that worked, so we can't make
        // the hash table.
        return None;
    }

    Some((disps, map))
}

pub fn create_phf(
    data: &[(char, String)],
    lambda: usize,
    max_tries: usize,
) -> (u64, Vec<(u32, u32)>, Vec<char>) {
    let mut rng = StdRng::seed_from_u64(0xf0f0f0f0);
    #[cfg(feature = "timing")]
    let start = time::Instant::now();

    for i in 0..(max_tries) {
        #[cfg(feature = "timing")]
        let my_start = time::Instant::now();
        #[cfg(feature = "timing")]
        println!("PHF #{}: starting {:.2}", i, my_start - start);
        #[cfg(not(feature = "timing"))]
        println!("PHF #{}", i);

        let seed = rng.gen();
        if let Some((disp, map)) = try_phf_table(data, lambda, seed, &mut rng) {
            #[cfg(feature = "timing")]
            println!(
                "PHF took: total {:.2} s, successive {:.2} s",
                start.elapsed(),
                my_start.elapsed()
            );
            return (seed, disp, map);
        }
    }
    panic!(
        "could not create a length {} PHF with {}, {}",
        data.len(),
        lambda,
        max_tries
    );
}
