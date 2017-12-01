extern crate rand;

use std::sync::{Mutex, Arc};
use std::thread;
use rand::distributions::{IndependentSample, Range};

const NUM_RUNS: i32 = 100000;
const NUM_PEOPLE: usize = 10;

const SHARES: i32 = 100;
const PEOPLE: [i32; NUM_PEOPLE] = [0; NUM_PEOPLE];

fn main() {
    let stock = Arc::new(Mutex::new(SHARES));
    let people = Arc::new(Mutex::new(PEOPLE));

    let mut handles = vec![];

    for i in 0..PEOPLE.len() {
        let people = people.clone();
        let stock = stock.clone();
        let handle = thread::spawn(move || {
            for _ in 0..NUM_RUNS {
                let mut shares = stock.lock().unwrap();
                let mut p = people.lock().unwrap();

                let between = Range::new(-p[i], *shares);
                let mut rng = rand::thread_rng();

                let amount = between.ind_sample(&mut rng);

                if amount < 0 {
                    println!("Person {} sells {}", i, -amount);
                } else {
                    println!("Person {} buys {}", i, amount);
                }

                *shares -= amount;
                p[i] += amount;
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    let mut total: i32 = *stock.lock().unwrap();

    println!("Stock: {}", total);

    let p = people.lock().unwrap();

    for i in 0..PEOPLE.len() {
        println!("Person {}: {}", i, p[i]);
        total += p[i];
    }

    println!("Total: {}", total);
}
