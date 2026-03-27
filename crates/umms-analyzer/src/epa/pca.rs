//! Power Iteration PCA with deflation.
//!
//! Extracts principal semantic axes from weighted, centered point clouds
//! without any external linear algebra crate. Operates entirely on `Vec<f32>`.

use umms_core::tag::SemanticAxis;

/// Run Power Iteration PCA on a set of weighted, centered points.
///
/// # Arguments
/// - `points`: embeddings (each `&[f32]` of equal dimensionality).
/// - `weights`: per-point weights (e.g. similarity scores).
/// - `num_axes`: number of principal components to extract.
/// - `iterations`: power iterations per axis.
///
/// # Returns
/// `Vec<SemanticAxis>` with unit-length `direction` and `explained_variance`
/// as a fraction of total variance.
pub fn power_iteration_pca(
    points: &[&[f32]],
    weights: &[f32],
    num_axes: usize,
    iterations: usize,
) -> Vec<SemanticAxis> {
    if points.is_empty() || num_axes == 0 {
        return Vec::new();
    }

    let dim = points[0].len();
    let total_weight: f32 = weights.iter().sum();
    if total_weight <= 0.0 || dim == 0 {
        return Vec::new();
    }

    // Weighted mean
    let mean = weighted_mean(points, weights, dim, total_weight);

    // Center the points (materialize to avoid repeated subtraction)
    let mut centered: Vec<Vec<f32>> = points
        .iter()
        .map(|p| p.iter().zip(mean.iter()).map(|(a, m)| a - m).collect())
        .collect();

    // Total variance for normalization
    let total_variance = weighted_total_variance(&centered, weights, total_weight);
    if total_variance <= 0.0 {
        return Vec::new();
    }

    let mut axes = Vec::with_capacity(num_axes);
    let num_axes = num_axes.min(dim).min(points.len());

    for _ in 0..num_axes {
        // Power iteration: find dominant eigenvector of weighted covariance
        let (eigenvec, eigenvalue) =
            power_iteration(&centered, weights, total_weight, dim, iterations);

        if eigenvalue <= 0.0 {
            break;
        }

        let explained_variance = eigenvalue / total_variance;

        axes.push(SemanticAxis {
            direction: eigenvec.clone(),
            explained_variance,
        });

        // Deflation: remove component along found eigenvector
        deflate(&mut centered, &eigenvec);
    }

    axes
}

/// Compute the weighted mean of points.
fn weighted_mean(points: &[&[f32]], weights: &[f32], dim: usize, total_weight: f32) -> Vec<f32> {
    let mut mean = vec![0.0_f32; dim];
    for (p, &w) in points.iter().zip(weights.iter()) {
        for (d, val) in p.iter().enumerate() {
            mean[d] += val * w;
        }
    }
    for v in &mut mean {
        *v /= total_weight;
    }
    mean
}

/// Total weighted variance (sum of squared norms of centered points, weighted).
fn weighted_total_variance(centered: &[Vec<f32>], weights: &[f32], total_weight: f32) -> f32 {
    let mut var = 0.0_f32;
    for (p, &w) in centered.iter().zip(weights.iter()) {
        let sq_norm: f32 = p.iter().map(|v| v * v).sum();
        var += w * sq_norm;
    }
    var / total_weight
}

/// Single power iteration: extract dominant eigenvector and eigenvalue
/// from the weighted covariance of the (already centered) data.
fn power_iteration(
    centered: &[Vec<f32>],
    weights: &[f32],
    total_weight: f32,
    dim: usize,
    iterations: usize,
) -> (Vec<f32>, f32) {
    // Initialize with a non-zero vector: use the first centered point,
    // or a unit vector if that's zero.
    let mut v: Vec<f32> = centered
        .iter()
        .find(|p| p.iter().any(|&x| x != 0.0))
        .cloned()
        .unwrap_or_else(|| {
            let mut u = vec![0.0_f32; dim];
            if !u.is_empty() {
                u[0] = 1.0;
            }
            u
        });
    normalize(&mut v);

    for _ in 0..iterations {
        // Compute Cv = (1/N) * sum_i w_i * (x_i^T v) * x_i
        let mut new_v = vec![0.0_f32; dim];
        for (p, &w) in centered.iter().zip(weights.iter()) {
            let dot: f32 = p.iter().zip(v.iter()).map(|(a, b)| a * b).sum();
            let w_dot = w * dot;
            for (d, val) in p.iter().enumerate() {
                new_v[d] += w_dot * val;
            }
        }
        // Divide by total weight
        for val in &mut new_v {
            *val /= total_weight;
        }

        normalize(&mut new_v);
        v = new_v;
    }

    // Eigenvalue = v^T C v
    let eigenvalue = rayleigh_quotient(centered, weights, total_weight, &v);
    (v, eigenvalue)
}

/// Rayleigh quotient: v^T C v where C is the weighted covariance.
fn rayleigh_quotient(
    centered: &[Vec<f32>],
    weights: &[f32],
    total_weight: f32,
    v: &[f32],
) -> f32 {
    let mut result = 0.0_f32;
    for (p, &w) in centered.iter().zip(weights.iter()) {
        let dot: f32 = p.iter().zip(v.iter()).map(|(a, b)| a * b).sum();
        result += w * dot * dot;
    }
    result / total_weight
}

/// Deflation: project out the component along `eigenvec` from each centered point.
fn deflate(centered: &mut [Vec<f32>], eigenvec: &[f32]) {
    for p in centered.iter_mut() {
        let dot: f32 = p.iter().zip(eigenvec.iter()).map(|(a, b)| a * b).sum();
        for (d, val) in p.iter_mut().enumerate() {
            *val -= dot * eigenvec[d];
        }
    }
}

/// L2 normalize a vector in place. If zero-length, sets to unit along dim 0.
fn normalize(v: &mut Vec<f32>) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-12 {
        for val in v.iter_mut() {
            *val /= norm;
        }
    } else if !v.is_empty() {
        v.fill(0.0);
        v[0] = 1.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pca_on_known_2d_matrix() {
        // Points along the x-axis with some noise on y.
        // Dominant axis should be close to [1, 0].
        let p1 = [1.0_f32, 0.1];
        let p2 = [2.0, -0.1];
        let p3 = [3.0, 0.2];
        let p4 = [4.0, -0.2];
        let p5 = [5.0, 0.0];

        let points: Vec<&[f32]> = vec![&p1, &p2, &p3, &p4, &p5];
        let weights = vec![1.0; 5];

        let axes = power_iteration_pca(&points, &weights, 2, 100);
        assert_eq!(axes.len(), 2);

        // First axis should capture most variance and align with x-axis
        let ax0 = &axes[0];
        assert!(
            ax0.explained_variance > 0.9,
            "First axis should explain >90% of variance, got {}",
            ax0.explained_variance
        );
        // Direction should be close to [±1, 0]
        assert!(
            ax0.direction[0].abs() > 0.95,
            "First axis x-component should be ~1, got {}",
            ax0.direction[0]
        );

        // Second axis should explain the remainder
        let ax1 = &axes[1];
        assert!(ax1.explained_variance < 0.1);

        // Variance ratios should sum to ~1
        let total: f32 = axes.iter().map(|a| a.explained_variance).sum();
        assert!(
            (total - 1.0).abs() < 0.05,
            "Variance ratios should sum to ~1, got {}",
            total
        );
    }

    #[test]
    fn pca_equal_variance_2d() {
        // Points at 45 degrees: equal spread on x and y.
        let p1 = [1.0_f32, 1.0];
        let p2 = [-1.0, -1.0];
        let p3 = [2.0, 2.0];
        let p4 = [-2.0, -2.0];

        let points: Vec<&[f32]> = vec![&p1, &p2, &p3, &p4];
        let weights = vec![1.0; 4];

        let axes = power_iteration_pca(&points, &weights, 2, 100);
        assert_eq!(axes.len(), 2);

        // First axis should align with [1/√2, 1/√2]
        let dot = (axes[0].direction[0] + axes[0].direction[1]).abs() / 2.0_f32.sqrt();
        assert!(
            dot > 0.95,
            "First axis should align with diagonal, dot={}",
            dot
        );

        // All variance should be on first axis (points are collinear)
        assert!(axes[0].explained_variance > 0.99);
    }

    #[test]
    fn pca_weighted_points() {
        // Heavy weight on x-axis points should pull first axis toward x.
        let p1 = [5.0_f32, 0.0];
        let p2 = [-5.0, 0.0];
        let p3 = [0.0, 1.0];
        let p4 = [0.0, -1.0];

        let points: Vec<&[f32]> = vec![&p1, &p2, &p3, &p4];
        let weights = vec![10.0, 10.0, 1.0, 1.0];

        let axes = power_iteration_pca(&points, &weights, 2, 100);
        assert!(axes[0].direction[0].abs() > 0.95);
    }

    #[test]
    fn pca_single_point() {
        let p = [1.0_f32, 2.0, 3.0];
        let points: Vec<&[f32]> = vec![&p];
        let weights = vec![1.0];
        // Single point = zero variance after centering
        let axes = power_iteration_pca(&points, &weights, 1, 50);
        assert!(axes.is_empty());
    }

    #[test]
    fn pca_empty() {
        let points: Vec<&[f32]> = Vec::new();
        let weights: Vec<f32> = Vec::new();
        let axes = power_iteration_pca(&points, &weights, 3, 50);
        assert!(axes.is_empty());
    }

    #[test]
    fn pca_3d_known_spread() {
        // Points spread along z, tight along x and y.
        let pts: Vec<[f32; 3]> = vec![
            [0.0, 0.0, -10.0],
            [0.0, 0.0, -5.0],
            [0.1, 0.1, 0.0],
            [0.0, 0.0, 5.0],
            [0.0, 0.0, 10.0],
        ];
        let points: Vec<&[f32]> = pts.iter().map(|p| p.as_slice()).collect();
        let weights = vec![1.0; 5];

        let axes = power_iteration_pca(&points, &weights, 3, 100);
        assert!(!axes.is_empty());

        // Dominant axis should align with z
        assert!(
            axes[0].direction[2].abs() > 0.95,
            "First axis should be along z, got {:?}",
            axes[0].direction
        );
        assert!(axes[0].explained_variance > 0.95);
    }
}
