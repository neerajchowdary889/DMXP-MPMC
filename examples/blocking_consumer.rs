use dmxp_kvcache::MPMC::ChannelBuilder;
use std::env;

fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let channel_id = if args.len() > 1 {
        args[1].parse().unwrap_or(0)
    } else {
        0
    };

    println!("Blocking Consumer: Connecting to channel {}", channel_id);

    let consumer = ChannelBuilder::new()
        .with_channel_id(channel_id)
        .build_consumer()?;

    println!("Blocking Consumer: Waiting for messages...");

    loop {
        match consumer.receive_blocking() {
            Ok(data) => {
                let msg = String::from_utf8_lossy(&data);
                println!("Received: {}", msg);
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                break;
            }
        }
    }

    Ok(())
}
