#![allow(unused)]
//i know the complexity if f*ckedup
use std::io::stdin;

fn take_int() -> usize {
    let mut input = String::new();
    stdin().read_line(&mut input).unwrap();
    input.trim().parse().unwrap()
}

fn take_vector() -> Vec<usize> {
    let mut input = String::new();
    stdin().read_line(&mut input).unwrap();
    input.trim().split_whitespace().map(|x| x.parse().unwrap()).collect()
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
        for j in i+1..n {
            if gcd(a[i], 2 * a[j]) > 1 {
                cnt += 1;
            }
        }
    }

    println!("{}", cnt);
}

fn main() {
    let t = take_int();
    for _ in 0..t {
        gcd2x();
    }
}
