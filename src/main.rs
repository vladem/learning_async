use crate::client::Client;

use clap::{arg, Parser};
use executor::{new_executor_and_spawner, Spawner};
use log::info;

mod client;
mod executor;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    run_as_one_task: bool,
}

fn run_as_separate_tasks(client: &Client, spawner: &Spawner) {
    let r1 = client.new_request(1, 10).unwrap();
    let r2 = client.new_request(2, 20).unwrap();
    spawner.spawn(r1);
    spawner.spawn(r2);
}

fn run_as_one_task(client: &Client, spawner: &Spawner) {
    let r1 = client.new_request(1, 10).unwrap();
    let r2 = client.new_request(2, 20).unwrap();
    spawner.spawn(async {
        futures::join!(r1, r2);
    });
}

fn main() {
    env_logger::init();
    let args = Args::parse();

    let (executor, spawner) = new_executor_and_spawner();

    let client = Client::new();
    if args.run_as_one_task {
        info!("running as one task with joined futures");
        run_as_one_task(&client, &spawner);
    } else {
        info!("running as two tasks each executing its feature");
        run_as_separate_tasks(&client, &spawner);
    }

    executor.run_forever();
}
