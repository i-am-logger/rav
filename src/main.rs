const SKEW: f32 = 0.57;
fn ramp_index(row: u16, height: u16, bands: usize) -> usize {
    let last = bands.saturating_sub(1);
    if height <= 1 {
        return last / 2;
    }
    let t = row.min(height - 1) as f32 / (height - 1) as f32;
    let t = if (height as usize) < bands {
        t.powf(SKEW)
    } else {
        t
    };
    ((t * last as f32).round() as usize).min(last)
}
// Ramp::at sampled at the cell centre: height_above_floor/screen_height
fn ramp_at(row: u16, height: u16, bands: usize) -> usize {
    let last = bands - 1;
    if last == 0 {
        return 0;
    }
    let frac = (row as f32 + 0.5) / height as f32;
    ((frac * last as f32).round() as usize).min(last)
}
fn main() {
    let (mut dis, mut tot) = (0u32, 0u32);
    for h in 16u16..=200 {
        for r in 0..h {
            tot += 1;
            if ramp_index(r, h, 16) != ramp_at(r, h, 16) {
                dis += 1;
            }
        }
    }
    println!("disagreements {dis} / {tot}");
    let a: Vec<usize> = (0..16).map(|r| ramp_index(r, 16, 16)).collect();
    let b: Vec<usize> = (0..16).map(|r| ramp_at(r, 16, 16)).collect();
    println!("rows=16 stretched {a:?}");
    println!("rows=16 positional {b:?}");
    println!("equal at 16: {}", a == b);
    // short display: does positional ever reach the top stop?
    for h in [4u16, 8, 12] {
        let s: Vec<usize> = (0..h).map(|r| ramp_index(r, h, 16)).collect();
        let p: Vec<usize> = (0..h).map(|r| ramp_at(r, h, 16)).collect();
        println!(
            "h={h} stretched_top={} positional_top={}",
            s[s.len() - 1],
            p[p.len() - 1]
        );
    }
}
