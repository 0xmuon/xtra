#![allow(unused)]
// Target: 1535b-style stdin program; fuzzable via run_from_input.
// solution to the question
use std::io::stdin;

fn take_int() -> usize {
    let mut input = String::new();
    stdin().read_line(&mut input).unwrap();
    input.trim().parse().unwrap()
}

fn take_vector() -> Vec<usize> {
    let mut input = String::new();
    stdin().read_line(&mut input).unwrap();
    input
        .trim()
        .split_whitespace()
        .map(|x| x.parse().unwrap())
        .collect()
}

fn gcd(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        let temp = a % b;
        a = b;
        b = temp;
    }
    a
}

fn gcd2x() {
    let n = take_int();
    let mut a = take_vector();

    a.sort_by_key(|&x| x % 2);

    let mut cnt = 0;
    for i in 0..n {
        for j in i + 1..n {
            if gcd(a[i], 2 * a[j]) > 1 {
                cnt += 1;
            }
        }
    }

    println!("{}", cnt);
}

/// Run the same logic as the program, but reading from a byte slice (e.g. fuzzer input).
/// Calls `on_signal(idx)` at coverage points for the fuzzer.
/// Returns `Ok(())` on success, `Err(())` on parse error.
pub fn run_from_input(input: &[u8], on_signal: impl Fn(usize)) -> Result<(), ()> {
    on_signal(0); // always set so every run has some coverage (avoids empty corpus)
    let s = std::str::from_utf8(input).map_err(|_| ())?;
    let mut lines = s.lines().map(str::trim).filter(|l| !l.is_empty());
    let t: usize = lines.next().ok_or(())?.parse().map_err(|_| ())?;
    on_signal(5);
    for _ in 0..t {
        let n: usize = lines.next().ok_or(())?.parse().map_err(|_| ())?;
        let line = lines.next().ok_or(())?;
        let a: Vec<usize> = line
            .split_whitespace()
            .filter_map(|x| x.parse().ok())
            .collect();
        if a.len() != n {
            return Err(());
        }
        on_signal(1);
        let mut a = a;
        a.sort_by_key(|&x| x % 2);
        let mut cnt = 0usize;
        for i in 0..n {
            for j in i + 1..n {
                on_signal(2);
                if gcd(a[i], 2 * a[j]) > 1 {
                    cnt += 1;
                    on_signal(3);
                }
            }
        }
        on_signal(4);
    }
    Ok(())
}

pub fn main() {
    let t = take_int();
    for _ in 0..t {
        gcd2x();
    }
}
