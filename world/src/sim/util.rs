use super::WORLD_SIZE;
use common::{terrain::TerrainChunkSize, vol::RectVolSize};
use vek::*;

/// Computes the cumulative distribution function of the weighted sum of k independent,
/// uniformly distributed random variables between 0 and 1.  For each variable i, we use weights[i]
/// as the weight to give samples[i] (the weights should all be positive).
///
/// If the precondition is met, the distribution of the result of calling this function will be
/// uniformly distributed while preserving the same information that was in the original average.
///
/// For N > 33 the function will no longer return correct results since we will overflow u32.
///
/// NOTE:
///
/// Per [1], the problem of determing the CDF of
/// the sum of uniformly distributed random variables over *different* ranges is considerably more
/// complicated than it is for the same-range case.  Fortunately, it also provides a reference to
/// [2], which contains a complete derivation of an exact rule for the density function for
/// this case.  The CDF is just the integral of the cumulative distribution function [3],
/// which we use to convert this into a CDF formula.
///
/// This allows us to sum weighted, uniform, independent random variables.
///
/// At some point, we should probably contribute this back to stats-rs.
///
/// 1. https://www.r-bloggers.com/sums-of-random-variables/,
/// 2. Sadooghi-Alvandi, S., A. Nematollahi, & R. Habibi, 2009.
///    On the Distribution of the Sum of Independent Uniform Random Variables.
///    Statistical Papers, 50, 171-175.
/// 3. hhttps://en.wikipedia.org/wiki/Cumulative_distribution_function
pub fn cdf_irwin_hall<const N: usize>(weights: &[f32; N], samples: [f32; N]) -> f32 {
    // Let J_k = {(j_1, ... , j_k) : 1 ≤ j_1 < j_2 < ··· < j_k ≤ N }.
    //
    // Let A_N = Π{k = 1 to n}a_k.
    //
    // The density function for N ≥ 2 is:
    //
    //   1/(A_N * (N - 1)!) * (x^(N-1) + Σ{k = 1 to N}((-1)^k *
    //   Σ{(j_1, ..., j_k) ∈ J_k}(max(0, x - Σ{l = 1 to k}(a_(j_l)))^(N - 1))))
    //
    // So the cumulative distribution function is its integral, i.e. (I think)
    //
    // 1/(product{k in A}(k) * N!) * (x^N + sum(k in 1 to N)((-1)^k *
    // sum{j in Subsets[A, {k}]}(max(0, x - sum{l in j}(l))^N)))
    //
    // which is also equivalent to
    //
    //   (letting B_k = { a in Subsets[A, {k}] : sum {l in a} l }, B_(0,1) = 0 and
    //            H_k = { i : 1 ≤ 1 ≤ N! / (k! * (N - k)!) })
    //
    //   1/(product{k in A}(k) * N!) * sum(k in 0 to N)((-1)^k *
    //   sum{l in H_k}(max(0, x - B_(k,l))^N))
    //
    // We should be able to iterate through the whole power set
    // instead, and figure out K by calling count_ones(), so we can compute the result in O(2^N)
    // iterations.
    let x: f32 = weights
        .iter()
        .zip(samples.iter())
        .map(|(weight, sample)| weight * sample)
        .sum();

    let mut y = 0.0f32;
    for subset in 0u32..(1 << N) {
        // Number of set elements
        let k = subset.count_ones();
        // Add together exactly the set elements to get B_subset
        let z = weights
            .iter()
            .enumerate()
            .filter(|(i, _)| subset & (1 << i) as u32 != 0)
            .map(|(_, k)| k)
            .sum::<f32>();
        // Compute max(0, x - B_subset)^N
        let z = (x - z).max(0.0).powi(N as i32);
        // The parity of k determines whether the sum is negated.
        y += if k & 1 == 0 { z } else { -z };
    }

    // Divide by the product of the weights.
    y /= weights.iter().product::<f32>();

    // Remember to multiply by 1 / N! at the end.
    y / (1..=N as i32).product::<i32>() as f32
}

/// First component of each element of the vector is the computed CDF of the noise function at this
/// index (i.e. its position in a sorted list of value returned by the noise function applied to
/// every chunk in the game).  Second component is the cached value of the noise function that
/// generated the index.
///
/// NOTE: Length should always be WORLD_SIZE.x * WORLD_SIZE.y.
pub type InverseCdf = Box<[(f32, f32)]>;

/// Computes the position Vec2 of a SimChunk from an index, where the index was generated by
/// uniform_noise.
pub fn uniform_idx_as_vec2(idx: usize) -> Vec2<i32> {
    Vec2::new((idx % WORLD_SIZE.x) as i32, (idx / WORLD_SIZE.x) as i32)
}

/// Computes the index of a Vec2 of a SimChunk from a position, where the index is generated by
/// uniform_noise.  NOTE: Both components of idx should be in-bounds!
pub fn vec2_as_uniform_idx(idx: Vec2<i32>) -> usize {
    (idx.y as usize * WORLD_SIZE.x + idx.x as usize) as usize
}

/// Compute inverse cumulative distribution function for arbitrary function f, the hard way.  We
/// pre-generate noise values prior to worldgen, then sort them in order to determine the correct
/// position in the sorted order.  That lets us use `(index + 1) / (WORLDSIZE.y * WORLDSIZE.x)` as
/// a uniformly distributed (from almost-0 to 1) regularization of the chunks.  That is, if we
/// apply the computed "function" F⁻¹(x, y) to (x, y) and get out p, it means that approximately
/// (100 * p)% of chunks have a lower value for F⁻¹ than p.  The main purpose of doing this is to
/// make sure we are using the entire range we want, and to allow us to apply the numerous results
/// about distributions on uniform functions to the procedural noise we generate, which lets us
/// much more reliably control the *number* of features in the world while still letting us play
/// with the *shape* of those features, without having arbitrary cutoff points / discontinuities
/// (which tend to produce ugly-looking / unnatural terrain).
///
/// As a concrete example, before doing this it was very hard to tweak humidity so that either most
/// of the world wasn't dry, or most of it wasn't wet, by combining the billow noise function and
/// the computed altitude.  This is because the billow noise function has a very unusual
/// distribution that is heavily skewed towards 0.  By correcting for this tendency, we can start
/// with uniformly distributed billow noise and altitudes and combine them to get uniformly
/// distributed humidity, while still preserving the existing shapes that the billow noise and
/// altitude functions produce.
///
/// f takes an index, which represents the index corresponding to this chunk in any any SimChunk
/// vector returned by uniform_noise, and (for convenience) the float-translated version of those
/// coordinates.
/// f should return a value with no NaNs.  If there is a NaN, it will panic.  There are no other
/// conditions on f.  If f returns None, the value will be set to 0.0, and will be ignored for the
/// purposes of computing the uniform range.
///
/// Returns a vec of (f32, f32) pairs consisting of the percentage of chunks with a value lower than
/// this one, and the actual noise value (we don't need to cache it, but it makes ensuring that
/// subsequent code that needs the noise value actually uses the same one we were using here
/// easier).
pub fn uniform_noise(f: impl Fn(usize, Vec2<f64>) -> Option<f32>) -> InverseCdf {
    let mut noise = (0..WORLD_SIZE.x * WORLD_SIZE.y)
        .filter_map(|i| {
            (f(
                i,
                (uniform_idx_as_vec2(i) * TerrainChunkSize::RECT_SIZE.map(|e| e as i32))
                    .map(|e| e as f64),
            )
            .map(|res| (i, res)))
        })
        .collect::<Vec<_>>();

    // sort_unstable_by is equivalent to sort_by here since we include a unique index in the
    // comparison.  We could leave out the index, but this might make the order not
    // reproduce the same way between different versions of Rust (for example).
    noise.sort_unstable_by(|f, g| (f.1, f.0).partial_cmp(&(g.1, g.0)).unwrap());

    // Construct a vector that associates each chunk position with the 1-indexed
    // position of the noise in the sorted vector (divided by the vector length).
    // This guarantees a uniform distribution among the samples (excluding those that returned
    // None, which will remain at zero).
    let mut uniform_noise = vec![(0.0, 0.0); WORLD_SIZE.x * WORLD_SIZE.y].into_boxed_slice();
    let total = noise.len() as f32;
    for (noise_idx, (chunk_idx, noise_val)) in noise.into_iter().enumerate() {
        uniform_noise[chunk_idx] = ((1 + noise_idx) as f32 / total, noise_val);
    }
    uniform_noise
}
