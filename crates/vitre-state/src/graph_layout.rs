//! Deterministic, bounded force layout in normalized viewport coordinates.
pub fn layout(count: usize, edges: &[(usize, usize)]) -> Vec<(f32, f32)> {
    if count == 0 {
        return Vec::new();
    }
    let mut points: Vec<_> = (0..count)
        .map(|i| {
            let angle = i as f32 * 2.399_963_1;
            let radius = 0.38 * ((i + 1) as f32 / count as f32).sqrt();
            (0.5 + radius * angle.cos(), 0.5 + radius * angle.sin())
        })
        .collect();
    for step in 0..100 {
        let mut forces = vec![(0f32, 0f32); count];
        for i in 0..count {
            for j in 0..i {
                let dx = points[i].0 - points[j].0;
                let dy = points[i].1 - points[j].1;
                let d2 = (dx * dx + dy * dy).max(0.0001);
                let f = 0.001 / (count as f32).sqrt() / d2;
                forces[i].0 += dx * f;
                forces[i].1 += dy * f;
                forces[j].0 -= dx * f;
                forces[j].1 -= dy * f;
            }
        }
        for &(a, b) in edges {
            if a >= count || b >= count || a == b {
                continue;
            }
            let dx = points[b].0 - points[a].0;
            let dy = points[b].1 - points[a].1;
            forces[a].0 += dx * 0.07;
            forces[a].1 += dy * 0.07;
            forces[b].0 -= dx * 0.07;
            forces[b].1 -= dy * 0.07;
        }
        let cooling = 1. - step as f32 / 120.;
        for (p, f) in points.iter_mut().zip(forces) {
            p.0 =
                (p.0 + ((0.5 - p.0) * 0.01 + f.0).clamp(-0.025, 0.025) * cooling).clamp(0.07, 0.93);
            p.1 =
                (p.1 + ((0.5 - p.1) * 0.01 + f.1).clamp(-0.025, 0.025) * cooling).clamp(0.08, 0.92);
        }
    }
    // Fit the settled layout to the available canvas; dense stars should not
    // collapse into a tiny dot cluster in the centre of a wide side panel.
    if count > 1 {
        let min_x = points.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
        let max_x = points.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
        let min_y = points.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
        let max_y = points.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
        for p in &mut points {
            p.0 = 0.12 + (p.0 - min_x) / (max_x - min_x).max(0.001) * 0.76;
            p.1 = 0.12 + (p.1 - min_y) / (max_y - min_y).max(0.001) * 0.76;
        }
    }
    points
}
#[cfg(test)]
mod tests {
    #[test]
    fn deterministic_bounded_even_for_disconnected_or_invalid_edges() {
        let edges = [(0, 1), (1, 2), (2, 2), (500, 1)];
        let a = super::layout(120, &edges);
        assert_eq!(a, super::layout(120, &edges));
        assert!(a.iter().all(|(x, y)| x.is_finite()
            && y.is_finite()
            && (0.0..=1.).contains(x)
            && (0.0..=1.).contains(y)));
        assert!(super::layout(0, &edges).is_empty());
    }
}
