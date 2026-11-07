//! Circular two-body propagation and a rotating spherical Earth; fictional elements.
use std::f64::consts::PI;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pass {
    pub aos: u64,
    pub los: u64,
    pub peak: f64,
}
pub fn elevation(ms: u64) -> f64 {
    let t = ms as f64 / 1000.0;
    let lat = 51.5f64.to_radians();
    let lon = 7.4f64.to_radians();
    let inc = 97.6f64.to_radians();
    let radius = 7051.0f64;
    let n = (398600.4418 / radius.powi(3)).sqrt();
    let center = (lat.sin() / inc.sin()).asin();
    let node = lon - (inc.cos() * center.sin()).atan2(center.cos());
    let u = center - 0.40 + n * t;
    let s = [
        radius * (node.cos() * u.cos() - node.sin() * u.sin() * inc.cos()),
        radius * (node.sin() * u.cos() + node.cos() * u.sin() * inc.cos()),
        radius * u.sin() * inc.sin(),
    ];
    let theta = lon + 2.0 * PI * t / 86164.0905;
    let unit = [lat.cos() * theta.cos(), lat.cos() * theta.sin(), lat.sin()];
    let d = [
        s[0] - 6371.0 * unit[0],
        s[1] - 6371.0 * unit[1],
        s[2] - 6371.0 * unit[2],
    ];
    let dot = d[0] * unit[0] + d[1] * unit[1] + d[2] * unit[2];
    (dot / (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt())
        .asin()
        .to_degrees()
}
pub fn visible(t: u64) -> bool {
    elevation(t) >= 10.0
}
fn boundary(mut lo: u64, mut hi: u64, state: bool) -> u64 {
    while hi - lo > 1 {
        let m = lo + (hi - lo) / 2;
        if visible(m) == state {
            hi = m;
        } else {
            lo = m;
        }
    }
    hi
}
pub fn next_pass(t: u64) -> Pass {
    let mut cursor = t;
    let start;
    if visible(cursor) {
        let mut a = cursor;
        while a > 0 && visible(a) {
            a = a.saturating_sub(10000);
        }
        start = if a == 0 && visible(0) {
            0
        } else {
            boundary(a, (a + 10000).min(cursor), true)
        };
    } else {
        loop {
            let next = cursor + 10000;
            if visible(next) {
                start = boundary(cursor, next, true);
                break;
            }
            cursor = next;
        }
    }
    cursor = start;
    let mut peak = elevation(start);
    loop {
        let next = cursor + 1000;
        peak = peak.max(elevation(next));
        if !visible(next) {
            return Pass {
                aos: start,
                los: boundary(cursor, next, false),
                peak,
            };
        }
        cursor = next;
    }
}
pub fn predictions(mut t: u64, count: usize) -> Vec<Pass> {
    let mut out = vec![];
    for _ in 0..count {
        let p = next_pass(t);
        out.push(p);
        t = p.los + 1;
    }
    out
}
