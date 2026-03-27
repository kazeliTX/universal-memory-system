//! Weighted K-Means clustering with K-Means++ initialization.
//!
//! Pure Rust implementation operating on f32 slices — no external ML
//! dependencies. Designed for clustering activated tag embeddings in
//! the 3072-dimensional space.

/// A cluster produced by weighted K-Means.
#[derive(Debug, Clone)]
pub struct Cluster {
    /// Centroid vector (same dimensionality as input points).
    pub centroid: Vec<f32>,
    /// Sum of weights of all members assigned to this cluster.
    pub total_weight: f32,
    /// Indices into the original points array.
    pub member_indices: Vec<usize>,
}

/// Run weighted K-Means clustering.
///
/// # Arguments
/// - `points`: slice of point references, each a `&[f32]` of equal length.
/// - `weights`: per-point weights (e.g. similarity scores). Must have same
///   length as `points`.
/// - `k`: desired number of clusters. Clamped to `points.len()` if larger.
/// - `max_iterations`: convergence cutoff.
///
/// # Returns
/// A `Vec<Cluster>` of length ≤ `k` (empty clusters are dropped).
pub fn weighted_kmeans(
    points: &[&[f32]],
    weights: &[f32],
    k: usize,
    max_iterations: usize,
) -> Vec<Cluster> {
    assert_eq!(points.len(), weights.len(), "points and weights must match");
    if points.is_empty() || k == 0 {
        return Vec::new();
    }

    let n = points.len();
    let dim = points[0].len();
    let k = k.min(n);

    // --- K-Means++ initialization ---
    let mut centroids = kmeans_plus_plus(points, weights, k);

    let epsilon = 1e-6_f32;

    for _iter in 0..max_iterations {
        // Assignment step
        let assignments = assign(points, &centroids);

        // Update step: weighted centroid recomputation
        let new_centroids = update(points, weights, &assignments, k, dim);

        // Convergence check
        let max_shift = centroids
            .iter()
            .zip(new_centroids.iter())
            .map(|(old, new)| {
                old.iter()
                    .zip(new.iter())
                    .map(|(a, b)| (a - b) * (a - b))
                    .sum::<f32>()
                    .sqrt()
            })
            .fold(0.0_f32, f32::max);

        centroids = new_centroids;

        if max_shift < epsilon {
            break;
        }
    }

    // Build final clusters
    let assignments = assign(points, &centroids);
    build_clusters(centroids, weights, &assignments, k)
}

/// K-Means++ seeding: choose initial centroids proportional to
/// weighted squared distance from already-chosen centroids.
fn kmeans_plus_plus(points: &[&[f32]], weights: &[f32], k: usize) -> Vec<Vec<f32>> {
    let n = points.len();
    let mut centroids: Vec<Vec<f32>> = Vec::with_capacity(k);

    // Pick first centroid: weighted random (use weight-proportional selection).
    // Deterministic fallback: pick the point with highest weight.
    let first = weights
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i)
        .unwrap_or(0);
    centroids.push(points[first].to_vec());

    // Select remaining centroids
    for _ in 1..k {
        // Compute weighted squared distance to nearest existing centroid
        let mut dist_sq: Vec<f32> = Vec::with_capacity(n);
        for (i, p) in points.iter().enumerate() {
            let min_d2 = centroids
                .iter()
                .map(|c| squared_euclidean(p, c))
                .fold(f32::INFINITY, f32::min);
            dist_sq.push(min_d2 * weights[i]);
        }

        // Pick the point with maximum weighted distance (deterministic D2).
        let next = dist_sq
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
            .unwrap_or(0);
        centroids.push(points[next].to_vec());
    }

    centroids
}

/// Assign each point to its nearest centroid. Returns assignment indices.
fn assign(points: &[&[f32]], centroids: &[Vec<f32>]) -> Vec<usize> {
    points
        .iter()
        .map(|p| {
            centroids
                .iter()
                .enumerate()
                .map(|(ci, c)| (ci, squared_euclidean(p, c)))
                .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(ci, _)| ci)
                .unwrap_or(0)
        })
        .collect()
}

/// Recompute centroids from weighted assignments.
fn update(
    points: &[&[f32]],
    weights: &[f32],
    assignments: &[usize],
    k: usize,
    dim: usize,
) -> Vec<Vec<f32>> {
    let mut sums = vec![vec![0.0_f32; dim]; k];
    let mut weight_sums = vec![0.0_f32; k];

    for (i, &cluster) in assignments.iter().enumerate() {
        let w = weights[i];
        weight_sums[cluster] += w;
        for (d, val) in points[i].iter().enumerate() {
            sums[cluster][d] += val * w;
        }
    }

    sums.into_iter()
        .zip(weight_sums.iter())
        .map(|(s, &ws)| {
            if ws > 0.0 {
                s.into_iter().map(|v| v / ws).collect()
            } else {
                s
            }
        })
        .collect()
}

/// Build final `Cluster` structs, dropping empty clusters.
fn build_clusters(
    centroids: Vec<Vec<f32>>,
    weights: &[f32],
    assignments: &[usize],
    k: usize,
) -> Vec<Cluster> {
    let mut clusters: Vec<Cluster> = centroids
        .into_iter()
        .map(|c| Cluster {
            centroid: c,
            total_weight: 0.0,
            member_indices: Vec::new(),
        })
        .collect();

    for (i, &cluster) in assignments.iter().enumerate() {
        if cluster < k {
            clusters[cluster].total_weight += weights[i];
            clusters[cluster].member_indices.push(i);
        }
    }

    // Drop empty clusters
    clusters.retain(|c| !c.member_indices.is_empty());
    clusters
}

/// Squared Euclidean distance between two slices.
fn squared_euclidean(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| {
            let d = x - y;
            d * d
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_clusters_2d() {
        // Two tight clusters in 2D
        let a1 = [0.0_f32, 0.0];
        let a2 = [0.1, 0.1];
        let a3 = [0.2, 0.0];
        let b1 = [10.0, 10.0];
        let b2 = [10.1, 10.1];
        let b3 = [10.2, 10.0];

        let points: Vec<&[f32]> = vec![&a1, &a2, &a3, &b1, &b2, &b3];
        let weights = vec![1.0; 6];

        let clusters = weighted_kmeans(&points, &weights, 2, 50);
        assert_eq!(clusters.len(), 2);

        // Each cluster should have 3 members
        let mut sizes: Vec<usize> = clusters.iter().map(|c| c.member_indices.len()).collect();
        sizes.sort();
        assert_eq!(sizes, vec![3, 3]);

        // Members {0,1,2} and {3,4,5} should be in different clusters
        let c0_members = &clusters[0].member_indices;
        let c1_members = &clusters[1].member_indices;
        let all_low = c0_members.iter().all(|&i| i < 3) || c1_members.iter().all(|&i| i < 3);
        assert!(all_low, "Points near origin should cluster together");
    }

    #[test]
    fn three_clusters_3d() {
        let p0 = [0.0_f32, 0.0, 0.0];
        let p1 = [0.1, 0.0, 0.0];
        let p2 = [5.0, 5.0, 5.0];
        let p3 = [5.1, 5.0, 5.0];
        let p4 = [10.0, 0.0, 10.0];
        let p5 = [10.1, 0.0, 10.0];

        let points: Vec<&[f32]> = vec![&p0, &p1, &p2, &p3, &p4, &p5];
        let weights = vec![1.0; 6];

        let clusters = weighted_kmeans(&points, &weights, 3, 50);
        assert_eq!(clusters.len(), 3);

        for c in &clusters {
            assert_eq!(c.member_indices.len(), 2);
        }
    }

    #[test]
    fn weights_affect_centroid() {
        let p0 = [0.0_f32, 0.0];
        let p1 = [10.0, 10.0];

        let points: Vec<&[f32]> = vec![&p0, &p1];
        let weights = vec![9.0, 1.0];

        let clusters = weighted_kmeans(&points, &weights, 1, 50);
        assert_eq!(clusters.len(), 1);

        // Centroid should be pulled toward p0 due to higher weight
        let c = &clusters[0].centroid;
        assert!(c[0] < 5.0, "centroid should be closer to origin: {}", c[0]);
        assert!(c[1] < 5.0, "centroid should be closer to origin: {}", c[1]);
    }

    #[test]
    fn single_point() {
        let p = [1.0_f32, 2.0, 3.0];
        let points: Vec<&[f32]> = vec![&p];
        let weights = vec![1.0];
        let clusters = weighted_kmeans(&points, &weights, 3, 10);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].member_indices, vec![0]);
    }

    #[test]
    fn empty_input() {
        let points: Vec<&[f32]> = Vec::new();
        let weights: Vec<f32> = Vec::new();
        let clusters = weighted_kmeans(&points, &weights, 3, 10);
        assert!(clusters.is_empty());
    }
}
