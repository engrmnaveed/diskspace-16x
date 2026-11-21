//! A squarified treemap layout (Bruls, Huizing and van Wijk, 2000), shared by the HTML
//! report and the explorer's `m` view. Rectangles keep their sizes' proportions and stay
//! close to square, so their areas compare well.

/// A rectangle in any unit: percentages for HTML, terminal cells for the explorer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Lays out `sizes`, largest first, in `area`. Returns one rectangle per size, in the
/// same order. Zero sizes get empty rectangles.
pub fn layout(sizes: &[u64], area: Rect) -> Vec<Rect> {
    let total: f64 = sizes.iter().map(|&size| size as f64).sum();
    let mut rects = vec![
        Rect {
            x: area.x,
            y: area.y,
            width: 0.0,
            height: 0.0,
        };
        sizes.len()
    ];
    if total <= 0.0 || area.width <= 0.0 || area.height <= 0.0 {
        return rects;
    }
    let scale = area.width * area.height / total;
    let areas: Vec<f64> = sizes.iter().map(|&size| size as f64 * scale).collect();
    let mut free = area;
    let mut start = 0;
    while start < areas.len() && areas[start] > 0.0 {
        let side = free.width.min(free.height);
        // Grow the row while that makes its worst aspect ratio better.
        let mut end = start + 1;
        while end < areas.len()
            && areas[end] > 0.0
            && worst(&areas[start..=end], side) <= worst(&areas[start..end], side)
        {
            end += 1;
        }
        let row = &areas[start..end];
        let sum: f64 = row.iter().sum();
        if free.width >= free.height {
            // A column along the left edge.
            let width = if free.height > 0.0 {
                sum / free.height
            } else {
                0.0
            };
            let mut y = free.y;
            for (offset, &part) in row.iter().enumerate() {
                let height = if width > 0.0 { part / width } else { 0.0 };
                rects[start + offset] = Rect {
                    x: free.x,
                    y,
                    width,
                    height,
                };
                y += height;
            }
            free.x += width;
            free.width = (free.width - width).max(0.0);
        } else {
            // A row along the top edge.
            let height = if free.width > 0.0 {
                sum / free.width
            } else {
                0.0
            };
            let mut x = free.x;
            for (offset, &part) in row.iter().enumerate() {
                let width = if height > 0.0 { part / height } else { 0.0 };
                rects[start + offset] = Rect {
                    x,
                    y: free.y,
                    width,
                    height,
                };
                x += width;
            }
            free.y += height;
            free.height = (free.height - height).max(0.0);
        }
        start = end;
    }
    rects
}

/// The worst aspect ratio of a row of `areas` laid along a side of length `side`.
fn worst(areas: &[f64], side: f64) -> f64 {
    let sum: f64 = areas.iter().sum();
    let (smallest, largest) = areas
        .iter()
        .fold((f64::MAX, 0.0_f64), |(low, high), &area| {
            (low.min(area), high.max(area))
        });
    if sum <= 0.0 || smallest <= 0.0 {
        return f64::MAX;
    }
    let side = side * side;
    let sum = sum * sum;
    (side * largest / sum).max(sum / (side * smallest))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(width: f64, height: f64) -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            width,
            height,
        }
    }

    #[test]
    fn fills_the_area_in_proportion() {
        let sizes = [600, 600, 400, 300, 200, 200, 100, 0];
        let rects = layout(&sizes, area(6.0, 4.0));
        assert_eq!(rects.len(), sizes.len());
        let total: u64 = sizes.iter().sum();
        for (rect, &size) in rects.iter().zip(&sizes) {
            let expected = 24.0 * size as f64 / total as f64;
            assert!(
                (rect.width * rect.height - expected).abs() < 1e-9,
                "{rect:?}"
            );
            assert!(
                rect.x >= -1e-9 && rect.x + rect.width <= 6.0 + 1e-9,
                "{rect:?}"
            );
            assert!(
                rect.y >= -1e-9 && rect.y + rect.height <= 4.0 + 1e-9,
                "{rect:?}"
            );
        }
        // The classic example from the paper: the first two share a column.
        assert!((rects[0].width - 3.0).abs() < 1e-9 && (rects[1].x - rects[0].x).abs() < 1e-9);
        assert_eq!(rects[7].width * rects[7].height, 0.0);
        // No two rectangles overlap.
        for (index, a) in rects.iter().enumerate() {
            for b in &rects[index + 1..] {
                let overlap_x = (a.x + a.width).min(b.x + b.width) - a.x.max(b.x);
                let overlap_y = (a.y + a.height).min(b.y + b.height) - a.y.max(b.y);
                assert!(overlap_x <= 1e-9 || overlap_y <= 1e-9, "{a:?} {b:?}");
            }
        }
    }

    #[test]
    fn handles_empty_input() {
        assert!(layout(&[], area(10.0, 10.0)).is_empty());
        let zero = layout(&[0, 0], area(10.0, 10.0));
        assert!(zero.iter().all(|rect| rect.width == 0.0));
        let flat = layout(&[5, 5], area(0.0, 10.0));
        assert!(flat.iter().all(|rect| rect.width == 0.0));
        let one = layout(&[7], area(100.0, 50.0));
        assert_eq!(one[0], area(100.0, 50.0));
    }
}
