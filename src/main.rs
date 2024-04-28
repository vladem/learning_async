use crate::client::Client;
use futures::executor::block_on;

mod client;

async fn async_main() {
    let client = Client::new();
    let r1 = client.new_request(1, 10).unwrap();
    let r2 = client.new_request(2, 20).unwrap();
    futures::join!(r1, r2);
}

fn main() {
    block_on(async_main());
}
