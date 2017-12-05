extern crate rand;

use std::sync::{Mutex, Arc};
use std::thread;
use std::time::Duration;
use rand::distributions::{IndependentSample, Range};
use std::collections::VecDeque;
use std::sync::RwLock;

const STOCK_END_TIME: u64 = 5000;
const NUM_ITERATIONS: usize = 1000;
const NUM_PEOPLE: usize = 100;
const NUM_STOCKS: usize = 20;
const NUM_STARTING_SHARES_IN_STOCK_EXCHANGE: i32 = 100;
const NUM_STARTING_SHARES_PERSON: i32 = 50;

struct PurchaseRequest {
    person: thread::Thread,
    amount: i32,
}

struct Stock {
    shares: i32,
    queue: VecDeque<PurchaseRequest>,
}

/**
 * Main problem was people waiting on queues that they could never get off of.
 * 1) If everyone is finished except one person that buys, then that person will never get off the
 *    queue assuming insufficient shares available
 *    - To solve this, I initially slept the main thread for some number of seconds before
 *      unparking every thread and joining (to simulate "end of trading day"). This isn't sufficient
 *      because after unparking the thread, they might still loop and get stuck on a queue again.
 *      I guess you can hackishly unpark the thread NUM_ITERATIONS times with enough delay in
 *      between... the workaround I used is to use a RwLock (reader-writer lock) that each person
 *      reads before beginning another iteration. The main thread sets this to true after the
 *      trading day ends so all threads, after being unparked if necessary, will exit the loop
 *      on the next iteration.
 * 2) Everyone can get stuck on a queue. It isn't sufficient to limit size of the queue, because
 *    whatever N you set it to, if everyone except the last N people are finished, all those
 *    N people can end up on the same queue. Workaround: Use the RwLock from above to kill
 *    any stuck processes and ignore the issue completely. I guess another solution is to keep
 *    track of how many people are left/on queues, and adjust the limit dynamically.
 * 3) It is very easy to deadlock by holding the mutex while parking. The scopes have to be
 *    handled very carefully, to ensure the mutex is dropped at the right time.
 *
 * Instead of using another thread for each stock to check when buyers can be removed off the
 * queue, I took advantage of the fact that the only time something like that can occur
 * is if someone sells. So all sellers (unrealistically) wakes up people off the queue
 * as a way to check when this event occurs.
 */
fn main() {
    let mut stocks = Vec::with_capacity(NUM_STOCKS);
    for _ in 0..NUM_STOCKS {
        stocks.push(Arc::new(Mutex::new(Stock {
            shares: NUM_STARTING_SHARES_IN_STOCK_EXCHANGE,
            queue: VecDeque::new(),
        })));
    }

    let mut handles = vec![];
    let should_finish = Arc::new(RwLock::new(false));

    for i in 0..NUM_PEOPLE {
        let mut shares_of_each_stock = vec![NUM_STARTING_SHARES_PERSON as i32; NUM_STOCKS];
        let stocks = stocks.clone();
        let should_finish = should_finish.clone();
        let handle = thread::Builder::new()
            .name(format!("Person {}", i).into())
            .spawn(move || {
                let mut rng = rand::thread_rng();

                for j in 0..NUM_ITERATIONS {
                    if *should_finish.read().unwrap() {
                        println!("Stoppping iteration! Got to iteration {}", j);
                        break;
                    }

                    // Amount of stocks to buy/sell
                    // +ve: buy, -ve: sell
                    let between = Range::new(-10, 10);
                    let mut amount = between.ind_sample(&mut rng);

                    // Which stock to buy/sell from
                    let stock_range = Range::new(0, stocks.len());
                    let stock_index = stock_range.ind_sample(&mut rng);

                    if amount < 0 {
                        // Cap the amount they can sell to how much they have
                        if -amount > shares_of_each_stock[stock_index] {
                            amount = -shares_of_each_stock[stock_index];
                        }

                        println!(
                            "{} attempting to sell {} shares of stock {} on iteration {}",
                            thread::current().name().unwrap(),
                            -amount,
                            stock_index,
                            j
                        );

                        let mut stock = stocks[stock_index].lock().unwrap();
                        stock.shares -= amount;
                        shares_of_each_stock[stock_index] += amount;

                        // Then check if this enables any buyer to get off the queue
                        // If so, remove all the buyers from the queue
                        // that satisfy the change in shares
                        // This doesn't guarantee they'll get them,
                        // since another thread could swoop in at that exact moment
                        // Alternatively, we could modify the woken up buyer's shares here?
                        let mut estimated_stocks_left = stock.shares;
                        while stock.queue.len() > 0 && estimated_stocks_left > 0 {
                            let r = stock.queue.pop_front().unwrap();
                            // Is there enough shares for this person?
                            if r.amount <= stock.shares {
                                estimated_stocks_left -= r.amount;
                                // Let them try and buy it now
                                r.person.unpark();
                            } else {
                                // Back on the queue it goes
                                stock.queue.push_front(r);
                                break;
                            }
                        }
                    } else if amount > 0 {
                        println!(
                            "{} attempting to buy {} shares of stock {} on iteration {}",
                            thread::current().name().unwrap(),
                            amount,
                            stock_index,
                            j
                        );
                        // Buying: going on queue
                        // Runs logic to check if there's something on queue
                        // If nothing on queue, jump the queue and buy
                        // If insufficient number of stocks,
                        // then wait on queue with what's left to buy
                        // Get on queue
                        let mut should_park = false;
                        {
                            let mut stock = stocks[stock_index].lock().unwrap();
                            if stock.queue.len() > 0 {
                                // Someone is ahead of line. Wait
                                println!(
                                    "Placing {} on queue (wait time: {}, available: {})",
                                    thread::current().name().unwrap(),
                                    stock.queue.len(),
                                    stock.shares
                                );
                                stock.queue.push_back(PurchaseRequest {
                                    person: thread::current(),
                                    amount,
                                });
                                should_park = true;
                            } else {
                                // No line, so buy if possible... otherwise, get on queue
                                if stock.shares < amount {
                                    println!(
                                        "\t{} has to wait ({} available)",
                                        thread::current().name().unwrap(),
                                        stock.shares
                                    );

                                    // Wait until more is available
                                    stock.queue.push_back(PurchaseRequest {
                                        person: thread::current(),
                                        amount,
                                    });
                                    should_park = true;
                                } else {
                                    stock.shares -= amount;
                                    shares_of_each_stock[stock_index] += amount;
                                    println!(
                                        "{} purchased {} shares of stock {} (current count: {})",
                                        thread::current().name().unwrap(),
                                        amount,
                                        stock_index,
                                        shares_of_each_stock[stock_index]
                                    );
                                }
                            }
                        }

                        if should_park {
                            thread::park();
                            println!("\t{} is now awake!", thread::current().name().unwrap());

                            let mut stock = stocks[stock_index].lock().unwrap();

                            // Repeated code from above...
                            // Did not want to re-obtain the mutex
                            if stock.shares >= amount {
                                stock.shares -= amount;
                                shares_of_each_stock[stock_index] += amount;
                                println!(
                                    "{} purchased {} shares of stock {} (current count: {})",
                                    thread::current().name().unwrap(),
                                    amount,
                                    stock_index,
                                    shares_of_each_stock[stock_index]
                                );
                            } else {
                                // Okay, too bad, they waited and didn't get anything, they'll
                                // just need to deal with it and try to buy/sell something else!
                                println!("Giving up!");
                            }
                        }
                    }
                }

                shares_of_each_stock
            })
            .unwrap();

        handles.push(handle);
    }

    thread::sleep(Duration::from_millis(STOCK_END_TIME));

    // Signal that all threads should finish
    let mut should_finish = should_finish.write().unwrap();
    *should_finish = true;

    let mut sum = 0;
    for handle in handles {
        handle.thread().unpark();
        let vals = handle.join().unwrap();
        println!("Stock count for: {:?}", vals);
        for val in vals {
            sum += val;
        }
    }

    for stock in stocks.iter() {
        let shares = stock.lock().unwrap().shares;
        println!("Stock value: {}", shares);
        sum += shares;
    }

    println!(
        "Program finished. Total sum = {} (expected {})",
        sum,
        (NUM_STOCKS * (NUM_STARTING_SHARES_IN_STOCK_EXCHANGE as usize)) +
            (NUM_PEOPLE * NUM_STOCKS * (NUM_STARTING_SHARES_PERSON as usize))
    );
}
