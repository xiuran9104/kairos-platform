pub fn evaluate_run(completed: u32, total: u32, findings: u32, cost: f64, breach: bool) -> f64 {
    let mut s = completed as f64 / total.max(1) as f64;
    if findings > 0 {
        s += 0.2;
    }
    if cost > 0.0 {
        s -= (cost / 100.0).min(0.3);
    }
    if breach {
        s = s.min(0.3);
    }
    s.clamp(0.0, 1.0)
}
